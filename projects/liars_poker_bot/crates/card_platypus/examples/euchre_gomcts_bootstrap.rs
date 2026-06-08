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
//!   cargo run -p card_platypus --release --features gpu_cuda \
//!     --example euchre_gomcts_bootstrap
//!
//! Knobs:
//!   EU_BOOT_GAMES        cfr3-vs-cfr3 games to play           (default 5000)
//!   EU_BOOT_THREADS      data-collection worker threads       (default 8)
//!   EU_BOOT_EPOCHS       training epochs over collected data  (default 20)
//!   EU_BOOT_BATCH        training batch size                  (default 256)
//!   EU_BOOT_LR           learning rate                        (default 5e-4)
//!   EU_BOOT_CONFIG       smoke|medium|paper                   (default paper)
//!   EU_BOOT_OUT          output safetensors path
//!                        (default /tmp/euchre_gomcts_bootstrap/bootstrap.safetensors)
//!   EU_BOOT_SEED         base RNG seed                        (default 0)
//!   EUCHRE_CFR3_WEIGHTS  cfr3 weights dir
//!                        (default /home/steven/card_platypus/infostate.three_card_played_f32)

use card_platypus::{
    agents::Agent,
    algorithms::{
        cfres::EuchreCfres,
        gomcts_transformer::{
            default_device, euchre::EuchreTokenizer, train_with_callback, GoMctsTransformer,
            TrainExample, TransformerConfig, TransformerGenerativeModel,
        },
    },
};
use games::{
    gamestates::euchre::{Euchre, EuchreGameState},
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

fn main() {
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

    assert!(cfr3_path.exists(), "cfr3 weights not found at {}", cfr3_path.display());

    let cfg = pick_config();
    println!(
        "Bootstrap: games={}, threads={}, epochs={}, batch={}, lr={}, cfr3={}, out={}",
        n_games,
        n_threads,
        n_epochs,
        batch_size,
        lr,
        cfr3_path.display(),
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
                // Each worker owns its own cfr3 instance. They share the
                // underlying mmap on disk so memory cost is O(1) per
                // thread, but the agent's per-call state is per-thread
                // (no contention).
                let mut cfr3 = EuchreCfres::new_euchre(
                    StdRng::seed_from_u64(base_seed.wrapping_add(1_000 + t as u64)),
                    3,
                    Some(&cfr3_path),
                );
                let start = t * games_per_thread;
                let end = ((t + 1) * games_per_thread).min(n_games);
                let mut local: Vec<TrainExample> = Vec::with_capacity((end - start) * 80);
                let mut last_log = Instant::now();
                for game_idx in start..end {
                    let mut rng: StdRng =
                        SeedableRng::seed_from_u64(base_seed.wrapping_add(100 + game_idx as u64));
                    let exs = play_one_cfr3_game(&mut cfr3, &mut rng);
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
        "collected {} examples from {} cfr3-vs-cfr3 games in {:.1}s ({:.1} games/s, \
         {:.1} examples/s)",
        examples.len(),
        n_games,
        collect_secs,
        n_games as f64 / collect_secs,
        examples.len() as f64 / collect_secs,
    );

    // --- Phase 2: train transformer on collected examples ---
    let device = default_device();
    println!("training device: {:?}", device);
    let net = GoMctsTransformer::new(cfg, device).expect("build");
    let mut model = TransformerGenerativeModel::new(net, EuchreTokenizer);
    let mut rng: StdRng = SeedableRng::seed_from_u64(base_seed.wrapping_add(7));

    let t1 = Instant::now();
    let loss_before = std::cell::Cell::new(f32::NAN);
    let last_loss = std::cell::Cell::new(f32::NAN);
    // Single train() call with an epoch-end callback. AdamW state
    // (moment buffers) persists across epochs since the optimizer is
    // constructed once inside train_with_callback.
    let _final = train_with_callback(
        &mut model,
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
    model.net.save(&out_path).expect("save");
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
