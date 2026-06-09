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
//!   EU_BOOT_AGENT        random | cfr3                       (default cfr3)
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
//!   EU_BOOT_GAMES        games to play                        (default 5000)
//!   EU_BOOT_THREADS      data-collection worker threads       (default 8)
//!   EU_BOOT_EPOCHS       training epochs over collected data  (default 20)
//!   EU_BOOT_BATCH        training batch size                  (default 256)
//!   EU_BOOT_LR           learning rate                        (default 5e-4)
//!   EU_BOOT_CONFIG       smoke|medium|paper                   (default paper)
//!   EU_BOOT_OUT          output safetensors path
//!                        (default /tmp/euchre_gomcts_bootstrap/bootstrap.safetensors)
//!   EU_BOOT_SEED         base RNG seed                        (default 0)
//!   EUCHRE_CFR3_WEIGHTS  cfr3 weights dir (only used by cfr3 agent)
//!                        (default /home/steven/card_platypus/infostate.three_card_played_f32)

use card_platypus::{
    agents::Agent,
    algorithms::{
        cfres::EuchreCfres,
        gomcts_transformer::{
            euchre::EuchreTokenizer, train_tch_with_callback, GoMctsTransformerTch, TrainExample,
            TransformerConfig,
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

fn parse<T: std::str::FromStr>(name: &str, default: T) -> T {
    std::env::var(name).ok().and_then(|s| s.parse().ok()).unwrap_or(default)
}
fn parse_path(name: &str, default: &str) -> PathBuf {
    std::env::var(name).map(PathBuf::from).unwrap_or_else(|_| PathBuf::from(default))
}

fn pick_config() -> TransformerConfig {
    let v = EuchreTokenizer::VOCAB_SIZE;
    let c = EuchreTokenizer::MAX_CONTEXT;
    match std::env::var("EU_BOOT_CONFIG").as_deref() {
        Ok("smoke") => TransformerConfig::euchre_smoke(v, c),
        Ok("medium") => TransformerConfig::euchre_medium(v, c),
        _ => TransformerConfig::paper_default(v, c),
    }
}

#[derive(Clone, Copy, Debug)]
enum BootAgent {
    Random,
    Cfr3,
}

fn main() {
    let agent_kind = match std::env::var("EU_BOOT_AGENT").as_deref() {
        Ok("random") => BootAgent::Random,
        _ => BootAgent::Cfr3,
    };
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
    let out_path: PathBuf = parse_path(
        "EU_BOOT_OUT",
        "/tmp/euchre_gomcts_bootstrap/bootstrap.safetensors",
    );

    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent).expect("create out dir");
    }

    if matches!(agent_kind, BootAgent::Cfr3) {
        assert!(cfr3_path.exists(), "cfr3 weights not found at {}", cfr3_path.display());
    }

    let cfg = pick_config();
    println!(
        "Bootstrap: agent={:?}, games={}, threads={}, epochs={}, batch={}, lr={}, out={}",
        agent_kind,
        n_games,
        n_threads,
        n_epochs,
        batch_size,
        lr,
        out_path.display(),
    );
    println!(
        "transformer: d={}, layers={}, heads={}, d_ff={}, vocab={}, max_ctx={}",
        cfg.d_model, cfg.n_layers, cfg.n_heads, cfg.d_ff, cfg.vocab_size, cfg.max_context,
    );

    // --- Phase 1: parallel cfr3-vs-cfr3 data collection ---
    let t0 = Instant::now();
    let games_per_thread = (n_games + n_threads - 1) / n_threads;
    let shared: Arc<Mutex<Vec<TrainExample>>> = Arc::new(Mutex::new(Vec::with_capacity(n_games * 80)));
    let progress: Arc<Mutex<usize>> = Arc::new(Mutex::new(0));

    std::thread::scope(|s| {
        for t in 0..n_threads {
            let cfr3_path = cfr3_path.clone();
            let shared = Arc::clone(&shared);
            let progress = Arc::clone(&progress);
            s.spawn(move || {
                // Per-worker agent state. cfr3 carries the mmap-backed
                // CFR weight table (cheap to construct per thread —
                // they all share the file via mmap). Random has no
                // state.
                let mut cfr3 = match agent_kind {
                    BootAgent::Cfr3 => Some(EuchreCfres::new_euchre(
                        StdRng::seed_from_u64(base_seed.wrapping_add(1_000 + t as u64)),
                        3,
                        Some(&cfr3_path),
                    )),
                    BootAgent::Random => None,
                };
                let start = t * games_per_thread;
                let end = ((t + 1) * games_per_thread).min(n_games);
                let mut local: Vec<TrainExample> = Vec::with_capacity((end - start) * 80);
                let mut last_log = Instant::now();
                for game_idx in start..end {
                    let mut rng: StdRng =
                        SeedableRng::seed_from_u64(base_seed.wrapping_add(100 + game_idx as u64));
                    let exs = match agent_kind {
                        BootAgent::Cfr3 => play_one_cfr3_game(cfr3.as_mut().unwrap(), &mut rng),
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
                let _ = progress;
                let mut s = shared.lock().expect("lock");
                let added = local.len();
                s.extend(local);
                eprintln!("thread {} done: contributed {} examples", t, added);
            });
        }
    });

    let mut examples: Vec<TrainExample> =
        Arc::try_unwrap(shared).ok().unwrap().into_inner().unwrap();
    let collect_secs = t0.elapsed().as_secs_f64();
    println!(
        "collected {} examples from {} {:?}-vs-self games in {:.1}s ({:.1} games/s, \
         {:.1} examples/s)",
        examples.len(),
        n_games,
        agent_kind,
        collect_secs,
        n_games as f64 / collect_secs,
        examples.len() as f64 / collect_secs,
    );

    // --- Phase 2: train transformer on collected examples ---
    let device = tch::Device::cuda_if_available();
    println!("training device: {:?}", device);
    let mut net = GoMctsTransformerTch::new(cfg, device).expect("build");
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
