//! Throughput benchmark comparing sequential vs cross-game-batched
//! self-play (E5).
//!
//! Runs N self-play games via both code paths against the same fresh
//! transformer, reports games/sec, and shows a few sample-batch sizes.
//! No training involved — just data collection throughput.
//!
//! Run:
//!   cargo run -p card_platypus --release --features gpu_cuda \
//!     --example euchre_gomcts_batch_bench
//!
//! Knobs:
//!   EU_CONFIG       smoke|medium|paper                   (default paper)
//!   EU_GAMES        games per condition                  (default 32)
//!   EU_MCTS_ITER    MCTS sims per decision               (default 16)
//!   EU_BATCH_SET    comma-sep batch_games values         (default 1,4,16,32)
//!   EU_BATCH_MAX    soft cap on histories per forward    (default 512)
//!   EU_SEED         base seed                            (default 0)

use card_platypus::algorithms::{
    gomcts::GenerativeModel,
    gomcts_transformer::{
        collect_self_play_game_alphazero, collect_self_play_games_batched_alphazero,
        default_device, euchre::EuchreTokenizer, GoMctsTransformer, McfsConfig, TransformerConfig,
        TransformerGenerativeModel,
    },
};
use card_platypus::algorithms::gomcts::{GoMcts, GoMctsConfig};
use games::gamestates::euchre::{Euchre, EuchreGameState};
use rand::{rngs::StdRng, SeedableRng};
use std::time::Instant;

fn parse<T: std::str::FromStr>(name: &str, default: T) -> T {
    std::env::var(name).ok().and_then(|s| s.parse().ok()).unwrap_or(default)
}

fn parse_batch_set() -> Vec<usize> {
    let raw = std::env::var("EU_BATCH_SET").unwrap_or_else(|_| "1,4,16,32".to_string());
    raw.split(',').filter_map(|s| s.trim().parse().ok()).collect()
}

fn pick_config() -> TransformerConfig {
    let v = EuchreTokenizer::VOCAB_SIZE;
    let c = EuchreTokenizer::MAX_CONTEXT;
    match std::env::var("EU_CONFIG").as_deref() {
        Ok("smoke") => TransformerConfig::euchre_smoke(v, c),
        Ok("medium") => TransformerConfig::euchre_medium(v, c),
        _ => TransformerConfig::paper_default(v, c),
    }
}

fn main() {
    let n_games: usize = parse("EU_GAMES", 32);
    let mcts_iter: usize = parse("EU_MCTS_ITER", 16);
    let batch_max: usize = parse("EU_BATCH_MAX", 512);
    let base_seed: u64 = parse("EU_SEED", 0);
    let batch_set = parse_batch_set();

    let device = default_device();
    let cfg = pick_config();
    println!(
        "Batch-self-play bench: device={:?}, games={}, mcts_iter={}, batch_max={}, config: \
         d={}, layers={}, heads={}, ff={}, vocab={}, ctx={}",
        device,
        n_games,
        mcts_iter,
        batch_max,
        cfg.d_model,
        cfg.n_layers,
        cfg.n_heads,
        cfg.d_ff,
        cfg.vocab_size,
        cfg.max_context,
    );

    let net = GoMctsTransformer::new(cfg, device).expect("build");
    let tokenizer = EuchreTokenizer;
    let model = TransformerGenerativeModel::new(net, tokenizer);

    println!(
        "{:>14} {:>10} {:>14} {:>12}",
        "condition", "secs", "games/sec", "speedup"
    );
    let baseline = run_sequential(&model, n_games, mcts_iter, base_seed);
    let baseline_rate = n_games as f64 / baseline;
    println!(
        "{:>14} {:>10.2} {:>14.3} {:>12}",
        "sequential", baseline, baseline_rate, "1.00x"
    );
    println!(
        "kestrel: step=0 condition=sequential games={} secs={:.4} games_per_sec={:.6}",
        n_games, baseline, baseline_rate
    );

    for bg in batch_set.iter().filter(|&&b| b > 1) {
        let secs = run_batched(&model, n_games, *bg, batch_max, mcts_iter, base_seed);
        let rate = n_games as f64 / secs;
        let speedup = rate / baseline_rate;
        println!(
            "{:>14} {:>10.2} {:>14.3} {:>11.2}x",
            format!("batched={}", bg),
            secs,
            rate,
            speedup
        );
        println!(
            "kestrel: step={} condition=batched_{} games={} secs={:.4} games_per_sec={:.6} speedup={:.4}",
            bg, bg, n_games, secs, rate, speedup
        );
    }
}

fn run_sequential(
    model: &TransformerGenerativeModel<EuchreGameState, EuchreTokenizer>,
    n_games: usize,
    mcts_iter: usize,
    base_seed: u64,
) -> f64 {
    let cfg = *model.net.config();
    let device = model.net.device().clone();
    let placeholder_net = GoMctsTransformer::new(cfg, device).expect("placeholder");
    let placeholder = TransformerGenerativeModel::new(placeholder_net, EuchreTokenizer);
    let mut search: GoMcts<EuchreGameState, TransformerGenerativeModel<EuchreGameState, EuchreTokenizer>> =
        GoMcts::new(
            GoMctsConfig { uct_c: 0.4, n_iterations: mcts_iter, mu: 0.01 },
            placeholder,
            SeedableRng::seed_from_u64(base_seed.wrapping_add(7)),
        );
    // Swap the real model in for the duration of the run.
    let live_copy = duplicate_via_disk(&model.net);
    let mut live_owned = TransformerGenerativeModel::new(live_copy, EuchreTokenizer);
    std::mem::swap(&mut live_owned, search.model_mut());

    let mcfs = McfsConfig::default();
    let t0 = Instant::now();
    for game_idx in 0..n_games {
        let mut rng: StdRng = SeedableRng::seed_from_u64(base_seed.wrapping_add(100 + game_idx as u64));
        let _ = collect_self_play_game_alphazero(Euchre::new_state, &mut search, mcfs, &mut rng);
    }
    let secs = t0.elapsed().as_secs_f64();
    // Restore (not needed for timing, but tidy).
    std::mem::swap(&mut live_owned, search.model_mut());
    let _ = live_owned;
    secs
}

fn run_batched(
    model: &TransformerGenerativeModel<EuchreGameState, EuchreTokenizer>,
    n_games: usize,
    batch_games: usize,
    batch_max: usize,
    mcts_iter: usize,
    base_seed: u64,
) -> f64 {
    let mcfs = McfsConfig::default();
    let t0 = Instant::now();
    let chunks = (n_games + batch_games - 1) / batch_games;
    for chunk_idx in 0..chunks {
        let games_this_chunk = batch_games.min(n_games - chunk_idx * batch_games);
        let chunk_seed = base_seed.wrapping_add((chunk_idx as u64) * 1_000);
        let _exs = collect_self_play_games_batched_alphazero::<EuchreGameState, _, _>(
            &model.net,
            &model.tokenizer,
            Euchre::new_state,
            games_this_chunk,
            batch_max,
            mcts_iter,
            mcfs,
            chunk_seed,
        );
    }
    t0.elapsed().as_secs_f64()
}

/// Save→load roundtrip to make a deep copy of a GoMctsTransformer on
/// the same device. Used so the sequential bench has its own model
/// instance and we don't accidentally fight with the batched code path
/// over the same VarMap.
fn duplicate_via_disk(net: &GoMctsTransformer) -> GoMctsTransformer {
    let cfg = *net.config();
    let device = net.device().clone();
    let tmp = tempfile::NamedTempFile::new().expect("tmp");
    net.save(tmp.path()).expect("save");
    let mut clone = GoMctsTransformer::new(cfg, device).expect("build");
    clone.load(tmp.path()).expect("load");
    clone
}
