//! Bootstrap an Oh Hell GO-MCTS transformer from PIMCTS supervised data.
//!
//! Mirrors `euchre_gomcts_bootstrap` with the recipe that reached
//! expert-parity on Euchre (experiment log entries 29-43): all seats
//! play the expert (here PIMCTS + OpenHandSolver::new_oh_hell) with
//! ε-greedy uniform exploration; exploration moves are recorded with
//! `policy_weight = 0` so the LM head imitates pure PIMCTS while the
//! value head gets counterfactual outcome coverage.
//!
//! Each game samples `n_tricks` uniformly from
//! [OH_MIN_TRICKS, OH_MAX_TRICKS] so one model covers every hand size.
//!
//! Knobs:
//!   OH_BOOT_GAMES        games to play                        (default 2000)
//!   OH_BOOT_THREADS      data-collection worker threads       (default 24)
//!   OH_BOOT_EPS          exploration prob                     (default 0.15)
//!   OH_BOOT_ROLLOUTS     PIMCTS rollouts per decision         (default 50)
//!   OH_PLAYERS           players per game                     (default 3)
//!   OH_MIN_TRICKS        min n_tricks                         (default 1)
//!   OH_MAX_TRICKS        max n_tricks                         (default 10)
//!   OH_BOOT_EPOCHS       training epochs                      (default 6)
//!   OH_BOOT_BATCH        training batch size                  (default 256)
//!   OH_BOOT_LR           learning rate                        (default 1e-4)
//!   OH_BOOT_CONFIG       smoke|medium|paper                   (default paper)
//!   OH_BOOT_INIT         warm-start checkpoint                (default unset)
//!   OH_BOOT_OUT          output safetensors path
//!                        (default /home/steven/card_platypus/gomcts/oh_hell/bootstrap.safetensors)
//!   OH_BOOT_DATA         dataset cache (rmp); comma-separated list to
//!                        merge; skips collection when present
//!   OH_BOOT_SEED         base RNG seed                        (default 0)
//!   OH_COLLECT_ONLY=1    stop after writing the dataset cache

use card_platypus::{
    agents::Agent,
    algorithms::{
        gomcts_transformer::{
            enable_tf32, oh_hell::OhHellTokenizer, parse_env as parse,
            parse_env_path as parse_path, train_tch_with_callback, GoMctsTransformerTch,
            TrainExample, TransformerConfig,
        },
        open_hand_solver::OpenHandSolver,
        pimcts::PIMCTSBot,
    },
};
use games::{
    gamestates::oh_hell::{OhHell, OhHellGameState},
    istate::IStateKey,
    Action, GameState,
};
use rand::{rngs::StdRng, seq::IndexedRandom, RngExt, SeedableRng};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Instant,
};

fn pick_config() -> TransformerConfig {
    TransformerConfig::from_env(
        "OH_BOOT_CONFIG",
        "paper",
        OhHellTokenizer::VOCAB_SIZE,
        OhHellTokenizer::MAX_CONTEXT,
    )
}

fn main() {
    let n_games: usize = parse("OH_BOOT_GAMES", 2000);
    let n_threads: usize = parse("OH_BOOT_THREADS", 24).max(1);
    let eps: f64 = parse("OH_BOOT_EPS", 0.15);
    let rollouts: usize = parse("OH_BOOT_ROLLOUTS", 50);
    let num_players: usize = parse("OH_PLAYERS", 3);
    let min_tricks: usize = parse("OH_MIN_TRICKS", 1);
    let max_tricks: usize = parse("OH_MAX_TRICKS", 10);
    let n_epochs: usize = parse("OH_BOOT_EPOCHS", 6);
    let batch_size: usize = parse("OH_BOOT_BATCH", 256);
    let lr: f64 = parse("OH_BOOT_LR", 1e-4);
    let base_seed: u64 = parse("OH_BOOT_SEED", 0);
    let collect_only = parse::<usize>("OH_COLLECT_ONLY", 0) == 1;
    let init_weights: Option<PathBuf> =
        std::env::var("OH_BOOT_INIT").ok().map(PathBuf::from);
    let out_path: PathBuf = parse_path(
        "OH_BOOT_OUT",
        "/home/steven/card_platypus/gomcts/oh_hell/bootstrap.safetensors",
    );
    let data_default = out_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join(format!(
            "dataset_pimcts{}_eps_{}p_{}games.rmp",
            rollouts, num_players, n_games
        ));
    assert!(min_tricks >= 1 && min_tricks <= max_tricks);
    assert!(
        max_tricks <= games::gamestates::oh_hell::max_tricks_for(num_players),
        "n_tricks {} exceeds max for {} players",
        max_tricks,
        num_players
    );

    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent).expect("create out dir");
    }

    let cfg = pick_config();
    println!(
        "OhHell bootstrap: pimcts{}+eps={}, players={}, tricks={}..={}, games={}, threads={}, \
         epochs={}, batch={}, lr={}, out={}",
        rollouts,
        eps,
        num_players,
        min_tricks,
        max_tricks,
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

    // --- Phase 1: load cached dataset(s), or collect. ---
    let t0 = Instant::now();
    let multi_paths: Option<Vec<PathBuf>> = std::env::var("OH_BOOT_DATA")
        .ok()
        .filter(|v| v.contains(','))
        .map(|v| v.split(',').map(PathBuf::from).collect());
    let data_path: PathBuf = std::env::var("OH_BOOT_DATA")
        .ok()
        .filter(|v| !v.contains(','))
        .map(PathBuf::from)
        .unwrap_or(data_default);
    let examples: Vec<TrainExample> = if let Some(paths) = multi_paths {
        let mut exs: Vec<TrainExample> = Vec::new();
        for path in &paths {
            assert!(path.exists(), "dataset cache {} not found", path.display());
            let bytes = std::fs::read(path).expect("read dataset cache");
            let part: Vec<TrainExample> =
                rmp_serde::from_slice(&bytes).expect("decode dataset cache");
            println!("loaded {} cached examples from {}", part.len(), path.display());
            exs.extend(part);
        }
        exs
    } else if data_path.exists() {
        let bytes = std::fs::read(&data_path).expect("read dataset cache");
        let exs: Vec<TrainExample> =
            rmp_serde::from_slice(&bytes).expect("decode dataset cache");
        println!(
            "loaded {} cached examples from {} (collection skipped)",
            exs.len(),
            data_path.display(),
        );
        exs
    } else {
        let games_per_thread = (n_games + n_threads - 1) / n_threads;
        let shared: Arc<Mutex<Vec<TrainExample>>> =
            Arc::new(Mutex::new(Vec::with_capacity(n_games * 24)));
        std::thread::scope(|s| {
            for t in 0..n_threads {
                let shared = Arc::clone(&shared);
                s.spawn(move || {
                    let mut pimcts = PIMCTSBot::new(
                        rollouts,
                        OpenHandSolver::new_oh_hell(),
                        StdRng::seed_from_u64(base_seed.wrapping_add(1_000 + t as u64)),
                    );
                    let start = t * games_per_thread;
                    let end = ((t + 1) * games_per_thread).min(n_games);
                    let mut local: Vec<TrainExample> = Vec::with_capacity((end - start) * 24);
                    let mut last_log = Instant::now();
                    for game_idx in start..end {
                        let mut rng: StdRng = SeedableRng::seed_from_u64(
                            base_seed.wrapping_add(100 + game_idx as u64),
                        );
                        let n_tricks =
                            min_tricks + (rng.random::<u64>() as usize) % (max_tricks - min_tricks + 1);
                        let exs = play_one_pimcts_eps_game(
                            &mut pimcts,
                            num_players,
                            n_tricks,
                            eps,
                            &mut rng,
                        );
                        local.extend(exs);
                        if last_log.elapsed().as_secs() > 30 {
                            let done = game_idx + 1 - start;
                            let total = end - start;
                            eprintln!(
                                "thread {}: {}/{} games ({}%), {} examples",
                                t,
                                done,
                                total,
                                done * 100 / total.max(1),
                                local.len()
                            );
                            println!(
                                "kestrel: step={} phase=collect thread={} games_done={} examples={}",
                                done, t, done, local.len()
                            );
                            last_log = Instant::now();
                        }
                    }
                    let mut s = shared.lock().expect("lock");
                    s.extend(local);
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
        "dataset ready: {} examples ({} exploration / {} on-policy) in {:.1}s ({:.2} games/s)",
        examples.len(),
        n_explore,
        examples.len() - n_explore,
        collect_secs,
        n_games as f64 / collect_secs,
    );
    println!(
        "kestrel: step=1 phase=dataset examples={} explore={} collect_secs={:.2}",
        examples.len(),
        n_explore,
        collect_secs,
    );
    if collect_only {
        println!("OH_COLLECT_ONLY=1 — stopping after dataset cache.");
        return;
    }

    // --- Phase 2: train. ---
    if parse::<usize>("TF32", 1) == 1 {
        enable_tf32();
    }
    let device = tch::Device::cuda_if_available();
    println!("training device: {:?}", device);
    // Scalar value head: Oh Hell's evaluate() is mean-centred score
    // (continuous), unlike Euchre's 6 discrete payoffs.
    let mut net = GoMctsTransformerTch::new(cfg, device).expect("build");
    if let Some(init) = init_weights.as_ref() {
        assert!(init.exists(), "OH_BOOT_INIT={} not found", init.display());
        net.load_safetensors(init).expect("load OH_BOOT_INIT");
        println!("fine-tuning from {}", init.display());
    }
    let tokenizer = OhHellTokenizer;
    let mut rng: StdRng = SeedableRng::seed_from_u64(base_seed.wrapping_add(7));
    let t1 = Instant::now();
    let _ = train_tch_with_callback(
        &mut net,
        &tokenizer,
        &examples,
        n_epochs,
        batch_size,
        lr,
        &mut rng,
        |epoch, loss| {
            println!(
                "  epoch {:>3}/{}: loss={:.4}  cum_secs={:.1}",
                epoch,
                n_epochs,
                loss,
                t1.elapsed().as_secs_f64()
            );
            println!(
                "kestrel: step={} phase=train_epoch epoch_loss={:.6} cum_train_secs={:.4}",
                epoch,
                loss,
                t1.elapsed().as_secs_f64(),
            );
        },
    )
    .expect("training");
    net.save_safetensors(&out_path).expect("save");
    println!("saved bootstrap weights to {}", out_path.display());
}

/// One Oh Hell game with PIMCTS at every seat and ε-greedy uniform
/// exploration. Exploration moves are value-only examples
/// (`policy_weight = 0`); PIMCTS moves are hard imitation targets.
fn play_one_pimcts_eps_game(
    pimcts: &mut PIMCTSBot<OhHellGameState, OpenHandSolver<OhHellGameState>>,
    num_players: usize,
    n_tricks: usize,
    eps: f64,
    rng: &mut StdRng,
) -> Vec<TrainExample> {
    let mut gs = OhHell::new_state(num_players, n_tricks);
    let mut buf = Vec::new();

    while gs.is_chance_node() {
        buf.clear();
        gs.legal_actions(&mut buf);
        let a = *buf.choose(rng).expect("non-empty chance");
        gs.apply_action(a);
    }

    let mut per_player: Vec<Vec<(IStateKey, Action, bool)>> = vec![Vec::new(); num_players];
    while !gs.is_terminal() {
        let p = gs.cur_player();
        let history = gs.istate_key(p);
        buf.clear();
        gs.legal_actions(&mut buf);
        let explore = rng.random::<f64>() < eps;
        let action = if explore || buf.len() == 1 {
            *buf.choose(rng).expect("non-empty legal")
        } else {
            pimcts.step(&gs)
        };
        // Forced moves (|legal| == 1) carry no policy information but
        // are still valid value examples; record them as on-policy.
        let as_explore = explore && buf.len() > 1;
        per_player[p].push((history, action, as_explore));
        gs.apply_action(action);
    }

    let mut out = Vec::new();
    for p in 0..num_players {
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
