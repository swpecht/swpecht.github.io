//! Bootstrap a Euchre GO-MCTS transformer from cfr3 supervised data.
//!
//! Pure-CPU data generation: play N games where all 4 seats use the
//! cfr3-trained agent, recording every per-seat (observation history,
//! chosen action, terminal payoff) tuple. Then train a fresh
//! transformer on that data with the existing `train()` (one-hot policy
//! targets + two-position value MSE). Save to disk for self-play to
//! resume from via `EU_INIT_WEIGHTS=`.
//!
//! Run:
//!   cargo run -p card_platypus --release --example euchre_gomcts_bootstrap
//!
//! Knobs:
//!   EU_BOOT_AGENT        random | cfr3 | cfr3_eps | cfr3_eps_vs_cfr0  (default cfr3_eps)
//!                        cfr3_eps_vs_cfr0: recorded team = cfr3+ε,
//!                        opponents = cfr0; outcomes teach the value
//!                        head what beats cfr0 (exploiter bootstrap).
//!                        - random: pure uniform-random play.
//!                          Paper-faithful (Hearts used 4M random
//!                          games). Cheap to generate, gives the
//!                          value head proper counterfactual
//!                          coverage at the cost of weak LM-head
//!                          targets.
//!                        - cfr3: all four seats play cfr3.
//!                          Strong LM-head target but the value
//!                          head sees ~zero counterfactual data
//!                          because cfr3 is sharply peaked → V
//!                          extrapolates poorly to off-policy
//!                          actions → ArgmaxVal\* breaks.
//!                        - cfr3_eps: cfr3 seats with ε-greedy
//!                          uniform exploration. Exploration moves
//!                          are recorded with policy_weight=0 so
//!                          the LM head still imitates pure cfr3
//!                          while the V head gets counterfactual
//!                          outcome coverage at cfr3-reachable
//!                          states. Designed to fix BOTH failure
//!                          modes above.
//!   EU_BOOT_EPS          exploration prob for cfr3_eps        (default 0.15)
//!   EU_BOOT_GAMES        games to play                        (default 5000)
//!   EU_BOOT_THREADS      data-collection worker threads       (default 8)
//!   EU_BOOT_EPOCHS       training epochs over collected data  (default 20)
//!   EU_BOOT_BATCH        training batch size                  (default 256)
//!   EU_BOOT_LR           learning rate                        (default 5e-4)
//!   EU_BOOT_CONFIG       smoke|medium|paper                   (default paper)
//!   EU_BOOT_VHEAD        scalar | outcome                     (default scalar)
//!                        outcome = paper Eq. 1 categorical head over
//!                        Euchre's 6 discrete payoffs, CE loss.
//!   EU_BOOT_OUT          output safetensors path
//!                        (default /home/steven/card_platypus/gomcts/bootstrap.safetensors)
//!   EU_BOOT_DATA         dataset cache path (rmp). If the file exists,
//!                        collection is skipped and examples are loaded
//!                        from it; otherwise collected examples are
//!                        saved to it before training. Lets hyperparam
//!                        re-runs skip the collection phase entirely.
//!                        (default <EU_BOOT_OUT dir>/dataset_<agent>_<games>.rmp)
//!   EU_BOOT_INIT         warm-start checkpoint to fine-tune from (default unset)
//!   EU_BOOT_SEED         base RNG seed                        (default 0)
//!   EUCHRE_CFR3_WEIGHTS  cfr3 weights dir (only used by cfr3 agents)
//!                        (default /home/steven/card_platypus/infostate.three_card_played_f32)

use card_platypus::{
    agents::Agent,
    algorithms::{
        cfres::EuchreCfres,
        gomcts_transformer::{
            enable_tf32,
            euchre::{EuchreTokenizer, OUTCOME_VALUES as EUCHRE_OUTCOME_VALUES},
            parse_env as parse, parse_env_path as parse_path, train_tch_with_callback,
            GoMctsTransformerTch, TrainExample, TransformerConfig,
        },
    },
};
use games::{
    gamestates::euchre::Euchre,
    istate::IStateKey,
    Action, GameState,
};
use rand::{rngs::StdRng, seq::IndexedRandom, SeedableRng};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Instant,
};

fn pick_config() -> TransformerConfig {
    TransformerConfig::from_env(
        "EU_BOOT_CONFIG",
        "paper",
        EuchreTokenizer::VOCAB_SIZE,
        EuchreTokenizer::MAX_CONTEXT,
    )
}

#[derive(Clone, Copy, Debug)]
enum BootAgent {
    Random,
    Cfr3,
    Cfr3Eps,
    /// Exploiter data: the recorded team plays cfr3 with ε-exploration,
    /// the OPPONENT team plays cfr0. Outcomes therefore measure what
    /// works *against cfr0* — the value head becomes an approximate
    /// best-response evaluator vs cfr0 (entry 38: imitation alone caps
    /// at ~49% pts because cfr3 ≈ cfr0 head-to-head).
    Cfr3EpsVsCfr0,
}

impl BootAgent {
    fn slug(self) -> &'static str {
        match self {
            BootAgent::Random => "random",
            BootAgent::Cfr3 => "cfr3",
            BootAgent::Cfr3Eps => "cfr3_eps",
            BootAgent::Cfr3EpsVsCfr0 => "cfr3_eps_vs_cfr0",
        }
    }
}

fn main() {
    let agent_kind = match std::env::var("EU_BOOT_AGENT").as_deref() {
        Ok("random") => BootAgent::Random,
        Ok("cfr3") => BootAgent::Cfr3,
        Ok("cfr3_eps_vs_cfr0") => BootAgent::Cfr3EpsVsCfr0,
        _ => BootAgent::Cfr3Eps,
    };
    let eps: f64 = parse("EU_BOOT_EPS", 0.15);
    let n_games: usize = parse("EU_BOOT_GAMES", 5000);
    let n_threads: usize = parse("EU_BOOT_THREADS", 8).max(1);
    let n_epochs: usize = parse("EU_BOOT_EPOCHS", 20);
    let batch_size: usize = parse("EU_BOOT_BATCH", 256);
    let lr: f64 = parse("EU_BOOT_LR", 5e-4);
    let base_seed: u64 = parse("EU_BOOT_SEED", 0);
    let cfr3_path: PathBuf = parse_path(
        "EUCHRE_CFR3_WEIGHTS",
        "/home/steven/card_platypus/infostate.three_card_played_f32",
    );
    let cfr0_path: PathBuf = parse_path(
        "EUCHRE_CFR0_WEIGHTS",
        "/home/steven/card_platypus/infostate.baseline",
    );
    // Optional warm start: fine-tune from an existing checkpoint
    // (e.g. the cfr3_eps bootstrap) instead of a fresh random init.
    let init_weights: Option<PathBuf> =
        std::env::var("EU_BOOT_INIT").ok().map(PathBuf::from);
    // NOTE: keep these off /tmp — a reboot on 2026-06-10 wiped every
    // checkpoint of the project's first week of training runs.
    let out_path: PathBuf = parse_path(
        "EU_BOOT_OUT",
        "/home/steven/card_platypus/gomcts/bootstrap.safetensors",
    );
    let data_path: PathBuf = std::env::var("EU_BOOT_DATA").map(PathBuf::from).unwrap_or_else(|_| {
        out_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .join(format!("dataset_{}_{}.rmp", agent_kind.slug(), n_games))
    });

    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent).expect("create out dir");
    }
    if let Some(parent) = data_path.parent() {
        std::fs::create_dir_all(parent).expect("create data dir");
    }

    if matches!(
        agent_kind,
        BootAgent::Cfr3 | BootAgent::Cfr3Eps | BootAgent::Cfr3EpsVsCfr0
    ) {
        assert!(cfr3_path.exists(), "cfr3 weights not found at {}", cfr3_path.display());
    }
    if matches!(agent_kind, BootAgent::Cfr3EpsVsCfr0) {
        assert!(cfr0_path.exists(), "cfr0 weights not found at {}", cfr0_path.display());
    }

    let cfg = pick_config();
    println!(
        "Bootstrap: agent={:?}, eps={}, games={}, threads={}, epochs={}, batch={}, lr={}, out={}, data={}",
        agent_kind,
        eps,
        n_games,
        n_threads,
        n_epochs,
        batch_size,
        lr,
        out_path.display(),
        data_path.display(),
    );
    println!(
        "transformer: d={}, layers={}, heads={}, d_ff={}, vocab={}, max_ctx={}",
        cfg.d_model, cfg.n_layers, cfg.n_heads, cfg.d_ff, cfg.vocab_size, cfg.max_context,
    );

    // --- Phase 1: load cached dataset(s), or run parallel self-play collection ---
    // EU_BOOT_DATA also accepts a comma-separated list of cache files;
    // they are concatenated and collection is skipped (for training on
    // merged datasets, e.g. cfr3_eps + exploiter).
    let t0 = Instant::now();
    let multi_paths: Option<Vec<PathBuf>> = std::env::var("EU_BOOT_DATA")
        .ok()
        .filter(|v| v.contains(','))
        .map(|v| v.split(',').map(PathBuf::from).collect());
    let mut examples: Vec<TrainExample> = if let Some(paths) = multi_paths {
        let mut exs: Vec<TrainExample> = Vec::new();
        for path in &paths {
            assert!(path.exists(), "dataset cache {} not found", path.display());
            let bytes = std::fs::read(path).expect("read dataset cache");
            let part: Vec<TrainExample> =
                rmp_serde::from_slice(&bytes).expect("decode dataset cache");
            println!("loaded {} cached examples from {}", part.len(), path.display());
            exs.extend(part);
        }
        println!(
            "merged {} examples from {} caches in {:.1}s",
            exs.len(),
            paths.len(),
            t0.elapsed().as_secs_f64(),
        );
        exs
    } else if data_path.exists() {
        let bytes = std::fs::read(&data_path).expect("read dataset cache");
        let exs: Vec<TrainExample> =
            rmp_serde::from_slice(&bytes).expect("decode dataset cache");
        println!(
            "loaded {} cached examples from {} in {:.1}s (collection skipped)",
            exs.len(),
            data_path.display(),
            t0.elapsed().as_secs_f64(),
        );
        exs
    } else {
        let games_per_thread = (n_games + n_threads - 1) / n_threads;
        let shared: Arc<Mutex<Vec<TrainExample>>> =
            Arc::new(Mutex::new(Vec::with_capacity(n_games * 80)));

        std::thread::scope(|s| {
            for t in 0..n_threads {
                let cfr3_path = cfr3_path.clone();
                let cfr0_path = cfr0_path.clone();
                let shared = Arc::clone(&shared);
                s.spawn(move || {
                    // Per-worker agent state. cfr3 carries the mmap-backed
                    // CFR weight table (cheap to construct per thread —
                    // they all share the file via mmap). Random has no
                    // state.
                    let mut cfr3 = match agent_kind {
                        BootAgent::Cfr3 | BootAgent::Cfr3Eps | BootAgent::Cfr3EpsVsCfr0 => {
                            Some(EuchreCfres::new_euchre(
                                StdRng::seed_from_u64(base_seed.wrapping_add(1_000 + t as u64)),
                                3,
                                Some(&cfr3_path),
                            ))
                        }
                        BootAgent::Random => None,
                    };
                    let mut cfr0 = match agent_kind {
                        BootAgent::Cfr3EpsVsCfr0 => Some(EuchreCfres::new_euchre(
                            StdRng::seed_from_u64(base_seed.wrapping_add(2_000 + t as u64)),
                            0,
                            Some(&cfr0_path),
                        )),
                        _ => None,
                    };
                    let start = t * games_per_thread;
                    let end = ((t + 1) * games_per_thread).min(n_games);
                    let mut local: Vec<TrainExample> = Vec::with_capacity((end - start) * 80);
                    let mut last_log = Instant::now();
                    for game_idx in start..end {
                        let mut rng: StdRng = SeedableRng::seed_from_u64(
                            base_seed.wrapping_add(100 + game_idx as u64),
                        );
                        let exs = match agent_kind {
                            BootAgent::Cfr3 => {
                                play_one_cfr3_game(cfr3.as_mut().unwrap(), &mut rng)
                            }
                            BootAgent::Cfr3Eps => play_one_cfr3_eps_game(
                                cfr3.as_mut().unwrap(),
                                eps,
                                &mut rng,
                            ),
                            BootAgent::Cfr3EpsVsCfr0 => play_one_cfr3_eps_vs_cfr0_game(
                                cfr3.as_mut().unwrap(),
                                cfr0.as_mut().unwrap(),
                                eps,
                                game_idx % 2,
                                &mut rng,
                            ),
                            BootAgent::Random => play_one_random_game(&mut rng),
                        };
                        local.extend(exs);
                        if last_log.elapsed().as_secs() > 5 {
                            let done = game_idx + 1 - start;
                            let total = end - start;
                            eprintln!(
                                "thread {}: {}/{} games done ({}%), {} examples buffered",
                                t,
                                done,
                                total,
                                done * 100 / total.max(1),
                                local.len()
                            );
                            last_log = Instant::now();
                        }
                    }
                    let mut s = shared.lock().expect("lock");
                    let added = local.len();
                    s.extend(local);
                    eprintln!("thread {} done: contributed {} examples", t, added);
                });
            }
        });

        let exs: Vec<TrainExample> =
            Arc::try_unwrap(shared).ok().unwrap().into_inner().unwrap();
        let bytes = rmp_serde::to_vec(&exs).expect("encode dataset cache");
        std::fs::write(&data_path, &bytes).expect("write dataset cache");
        println!(
            "cached {} examples ({:.1} MB) to {}",
            exs.len(),
            bytes.len() as f64 / 1e6,
            data_path.display(),
        );
        exs
    };
    let collect_secs = t0.elapsed().as_secs_f64();
    let n_explore = examples.iter().filter(|e| e.policy_weight == 0.0).count();
    println!(
        "dataset ready: {} examples ({} exploration / {} on-policy) from {} {:?} games in {:.1}s \
         ({:.1} games/s, {:.1} examples/s)",
        examples.len(),
        n_explore,
        examples.len() - n_explore,
        n_games,
        agent_kind,
        collect_secs,
        n_games as f64 / collect_secs,
        examples.len() as f64 / collect_secs,
    );

    // --- Phase 2: train transformer on collected examples ---
    if parse::<usize>("TF32", 1) == 1 {
        enable_tf32();
    }
    let device = tch::Device::cuda_if_available();
    println!("training device: {:?}", device);
    let outcome_head = std::env::var("EU_BOOT_VHEAD").as_deref() == Ok("outcome");
    println!("value head: {}", if outcome_head { "categorical outcome (paper Eq. 1)" } else { "scalar" });
    let mut net = if outcome_head {
        GoMctsTransformerTch::new_with_outcomes(cfg, device, EUCHRE_OUTCOME_VALUES.to_vec())
            .expect("build")
    } else {
        GoMctsTransformerTch::new(cfg, device).expect("build")
    };
    if let Some(init) = init_weights.as_ref() {
        assert!(init.exists(), "EU_BOOT_INIT={} not found", init.display());
        net.load_safetensors(init).expect("load EU_BOOT_INIT");
        println!("fine-tuning from {}", init.display());
    }
    let tokenizer = EuchreTokenizer;
    let mut rng: StdRng = SeedableRng::seed_from_u64(base_seed.wrapping_add(7));

    let t1 = Instant::now();
    let loss_before = std::cell::Cell::new(f32::NAN);
    let last_loss = std::cell::Cell::new(f32::NAN);
    // Single train() call with an epoch-end callback. AdamW state
    // (moment buffers) persists across epochs since the optimizer is
    // constructed once inside train_tch_with_callback.
    let _final = train_tch_with_callback(
        &mut net,
        &tokenizer,
        &examples,
        n_epochs,
        batch_size,
        lr,
        &mut rng,
        |epoch, loss| {
            if epoch == 1 {
                loss_before.set(loss);
            }
            last_loss.set(loss);
            let elapsed = t1.elapsed().as_secs_f64();
            println!(
                "  epoch {:>3}/{}: loss={:.4}  cum_secs={:.1}",
                epoch, n_epochs, loss, elapsed
            );
            println!(
                "kestrel: step={} phase=train_epoch epoch_loss={:.6} \
                 cum_train_secs={:.4} examples={} batch_size={}",
                epoch,
                loss,
                elapsed,
                n_epochs,  // ignored placeholder for kestrel parser shape
                batch_size,
            );
        },
    )
    .expect("training");
    let loss_before = loss_before.get();
    let l_after = last_loss.get();
    let train_secs = t1.elapsed().as_secs_f64();
    println!(
        "trained for {} epochs over {} examples in {:.1}s. loss: {:.4} → {:.4}",
        n_epochs,
        examples.len(),
        train_secs,
        loss_before,
        l_after,
    );

    // --- Phase 3: save weights ---
    net.save_safetensors(&out_path).expect("save");
    println!("saved bootstrap weights to {}", out_path.display());
    println!(
        "kestrel: step=1 phase=bootstrap n_games={} examples={} collect_secs={:.4} \
         train_secs={:.4} loss_before={:.6} loss_after={:.6}",
        n_games,
        examples.len(),
        collect_secs,
        train_secs,
        loss_before,
        l_after,
    );

    // Avoid the unused variable warning if `examples` ends up small enough
    // that the compiler complains; explicit drop also frees memory before
    // the binary exits.
    examples.clear();
}

/// Play one full Euchre game with uniform-random play at all 4 seats.
/// Returns per-seat (history, action_taken, terminal_value) tuples.
/// Paper-faithful bootstrap data: random play gives the value head
/// the **counterfactual coverage** it needs to discriminate alternative
/// actions at the same history (every legal action gets sampled across
/// many games), at the cost of weak LM-head action targets.
fn play_one_random_game(rng: &mut StdRng) -> Vec<TrainExample> {
    let mut gs = Euchre::new_state();
    let mut buf = Vec::new();

    while gs.is_chance_node() {
        buf.clear();
        gs.legal_actions(&mut buf);
        let a = *buf.choose(rng).expect("non-empty chance");
        gs.apply_action(a);
    }

    let n_players = gs.num_players();
    let mut per_player: Vec<Vec<(IStateKey, Action)>> = vec![Vec::new(); n_players];

    while !gs.is_terminal() {
        let p = gs.cur_player();
        let history = gs.istate_key(p);
        buf.clear();
        gs.legal_actions(&mut buf);
        let action = *buf.choose(rng).expect("non-empty legal");
        per_player[p].push((history, action));
        gs.apply_action(action);
    }

    let mut out = Vec::new();
    for p in 0..n_players {
        let v = gs.evaluate(p) as f32;
        for (h, a) in per_player[p].drain(..) {
            out.push(TrainExample::hard(h, a, v));
        }
    }
    out
}

/// Play one full Euchre game where every seat plays cfr3 but, with
/// probability ε per decision, takes a uniform-random legal action
/// instead. Exploration moves are recorded with `TrainExample::explore`
/// (policy_weight = 0): they train the value head on the *real outcome*
/// of deviating at a cfr3-reachable state — exactly the counterfactual
/// coverage ArgmaxVal\* needs — without teaching the LM head to play
/// randomly. cfr3 moves are recorded as normal hard targets.
fn play_one_cfr3_eps_game(
    cfr3: &mut EuchreCfres,
    eps: f64,
    rng: &mut StdRng,
) -> Vec<TrainExample> {
    use rand::RngExt;
    let mut gs = Euchre::new_state();
    let mut buf = Vec::new();

    while gs.is_chance_node() {
        buf.clear();
        gs.legal_actions(&mut buf);
        let a = *buf.choose(rng).expect("non-empty chance");
        gs.apply_action(a);
    }

    let n_players = gs.num_players();
    let mut per_player: Vec<Vec<(IStateKey, Action, bool)>> = vec![Vec::new(); n_players];

    while !gs.is_terminal() {
        let p = gs.cur_player();
        let history = gs.istate_key(p);
        let explore = rng.random::<f64>() < eps;
        let action = if explore {
            buf.clear();
            gs.legal_actions(&mut buf);
            *buf.choose(rng).expect("non-empty legal")
        } else {
            cfr3.step(&gs)
        };
        per_player[p].push((history, action, explore));
        gs.apply_action(action);
    }

    let mut out = Vec::new();
    for p in 0..n_players {
        let v = gs.evaluate(p) as f32;
        for (h, a, explore) in per_player[p].drain(..) {
            out.push(if explore {
                TrainExample::explore(h, a, v)
            } else {
                TrainExample::hard(h, a, v)
            });
        }
    }
    out
}

/// Exploiter data: `hero_team` (0 → seats 0+2, 1 → seats 1+3) plays
/// cfr3 with ε-exploration and is recorded; the other team plays cfr0
/// and is NOT recorded. Outcomes are the hero seats' terminal payoffs —
/// i.e. results achieved *against cfr0's actual play*.
fn play_one_cfr3_eps_vs_cfr0_game(
    cfr3: &mut EuchreCfres,
    cfr0: &mut EuchreCfres,
    eps: f64,
    hero_team: usize,
    rng: &mut StdRng,
) -> Vec<TrainExample> {
    use rand::RngExt;
    let mut gs = Euchre::new_state();
    let mut buf = Vec::new();

    while gs.is_chance_node() {
        buf.clear();
        gs.legal_actions(&mut buf);
        let a = *buf.choose(rng).expect("non-empty chance");
        gs.apply_action(a);
    }

    let n_players = gs.num_players();
    let mut per_player: Vec<Vec<(IStateKey, Action, bool)>> = vec![Vec::new(); n_players];

    while !gs.is_terminal() {
        let p = gs.cur_player();
        if p % 2 == hero_team {
            let history = gs.istate_key(p);
            let explore = rng.random::<f64>() < eps;
            let action = if explore {
                buf.clear();
                gs.legal_actions(&mut buf);
                *buf.choose(rng).expect("non-empty legal")
            } else {
                cfr3.step(&gs)
            };
            per_player[p].push((history, action, explore));
            gs.apply_action(action);
        } else {
            let action = cfr0.step(&gs);
            gs.apply_action(action);
        }
    }

    let mut out = Vec::new();
    for p in 0..n_players {
        if p % 2 != hero_team {
            continue;
        }
        let v = gs.evaluate(p) as f32;
        for (h, a, explore) in per_player[p].drain(..) {
            out.push(if explore {
                TrainExample::explore(h, a, v)
            } else {
                TrainExample::hard(h, a, v)
            });
        }
    }
    out
}

/// Play one full Euchre game with cfr3 at all 4 seats, returning a
/// per-seat list of training tuples. Each tuple's value is that seat's
/// terminal payoff (team score for Euchre's team payoff).
fn play_one_cfr3_game(cfr3: &mut EuchreCfres, rng: &mut StdRng) -> Vec<TrainExample> {
    let mut gs = Euchre::new_state();
    let mut buf = Vec::new();

    // Resolve chance nodes (deal + face-up) with the rng.
    while gs.is_chance_node() {
        buf.clear();
        gs.legal_actions(&mut buf);
        let a = *buf.choose(rng).expect("non-empty chance");
        gs.apply_action(a);
    }

    let n_players = gs.num_players();
    let mut per_player: Vec<Vec<(IStateKey, Action)>> = vec![Vec::new(); n_players];

    while !gs.is_terminal() {
        let p = gs.cur_player();
        let history = gs.istate_key(p);
        let action = cfr3.step(&gs);
        per_player[p].push((history, action));
        gs.apply_action(action);
    }

    let mut out = Vec::new();
    for p in 0..n_players {
        let v = gs.evaluate(p) as f32;
        for (h, a) in per_player[p].drain(..) {
            out.push(TrainExample::hard(h, a, v));
        }
    }
    out
}
