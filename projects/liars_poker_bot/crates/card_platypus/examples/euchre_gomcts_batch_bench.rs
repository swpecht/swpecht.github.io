//! Throughput benchmark for batched (tch) self-play. Compares the tch
//! `collect_self_play_games_batched_alphazero_tch` at multiple
//! batch_games settings against a sequential 1-game baseline.
//!
//! Run:
//!   cargo run -p card_platypus --release --example euchre_gomcts_batch_bench
//!
//! Knobs:
//!   EU_CONFIG       smoke|medium|paper                   (default paper)
//!   EU_GAMES        games per condition                  (default 32)
//!   EU_MCTS_ITER    MCTS sims per decision               (default 16)
//!   EU_BATCH_SET    comma-sep batch_games values         (default 1,4,16,32)
//!   EU_BATCH_MAX    soft cap on histories per forward    (default 512)
//!   EU_USE_GRAPH    1 = capture a CUDA graph             (default 0)
//!   EU_SEED         base seed                            (default 0)

use card_platypus::algorithms::gomcts_transformer::{
    collect_self_play_games_batched_alphazero_tch, euchre::EuchreTokenizer, GoMctsTransformerTch,
    McfsConfig, TransformerConfig,
};
use games::gamestates::euchre::Euchre;
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
    let use_graph: bool = parse::<usize>("EU_USE_GRAPH", 0) == 1;
    let batch_set = parse_batch_set();

    let device = tch::Device::cuda_if_available();
    let cfg = pick_config();
    let tokenizer = EuchreTokenizer;
    println!(
        "Batch-self-play bench (tch): device={:?}, games={}, mcts_iter={}, batch_max={}, \
         use_graph={}, config: d={}, layers={}, heads={}, ff={}, vocab={}, ctx={}",
        device,
        n_games,
        mcts_iter,
        batch_max,
        use_graph,
        cfg.d_model,
        cfg.n_layers,
        cfg.n_heads,
        cfg.d_ff,
        cfg.vocab_size,
        cfg.max_context,
    );

    let net = GoMctsTransformerTch::new(cfg, device).expect("build");

    println!(
        "{:>14} {:>10} {:>14} {:>12}",
        "condition", "secs", "games/sec", "speedup"
    );

    let mut baseline_rate: Option<f64> = None;
    for bg in batch_set.iter() {
        let t0 = Instant::now();
        let mcfs = McfsConfig::default();
        let chunk_size = *bg;
        let chunks = n_games.div_ceil(chunk_size);
        for chunk_idx in 0..chunks {
            let games_this_chunk = chunk_size.min(n_games - chunk_idx * chunk_size);
            let chunk_seed = base_seed.wrapping_add((chunk_idx as u64) * 1_000);
            let _exs = collect_self_play_games_batched_alphazero_tch::<_, _, _>(
                &net,
                &tokenizer,
                Euchre::new_state,
                games_this_chunk,
                batch_max,
                mcts_iter,
                mcfs,
                chunk_seed,
                use_graph && *bg > 1,
                (*bg as i64).saturating_mul(8).max(8),
            );
        }
        let secs = t0.elapsed().as_secs_f64();
        let rate = n_games as f64 / secs;
        let speedup = match baseline_rate {
            Some(b) => rate / b,
            None => {
                baseline_rate = Some(rate);
                1.0
            }
        };
        println!(
            "{:>14} {:>10.2} {:>14.3} {:>11.2}x",
            format!("batch={}", bg),
            secs,
            rate,
            speedup
        );
        println!(
            "kestrel: step={} condition=batch_{} games={} secs={:.4} games_per_sec={:.6} speedup={:.4}",
            bg, bg, n_games, secs, rate, speedup
        );
    }
}
