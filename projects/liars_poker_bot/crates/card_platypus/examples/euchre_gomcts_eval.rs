//! Tight-CI eval of a trained Euchre GO-MCTS transformer checkpoint.
//!
//! Loads `EU_WEIGHTS` (default `/tmp/euchre_gomcts/final.safetensors`),
//! plays `EU_GAMES` hands against three uniform-random opponents (the
//! transformer's seat rotates), and reports:
//!   - raw transformer (`eval_vs_random_batched_tch`)
//!   - GO-MCTS-wrapped transformer (search at every decision)
//!
//! Knobs:
//!   EU_WEIGHTS         safetensors path     (default /tmp/euchre_gomcts/final.safetensors)
//!   EU_CONFIG          model architecture   (default smoke; must match training!)
//!   EU_GAMES           hands per condition  (default 2000)
//!   EU_MCTS_ITER       per-decision MCTS budget for the wrapped eval (default 32)
//!   EU_SEED            base RNG seed        (default 0)
//!   EU_SKIP_MCTS=1     skip the MCTS eval (raw only)
//!
//! Run:
//!   EU_GAMES=2000 cargo run -p card_platypus --release --example euchre_gomcts_eval

use card_platypus::algorithms::{
    gomcts::{GoMcts, GoMctsConfig},
    gomcts_transformer::{
        eval_vs_random_batched_tch, euchre::EuchreTokenizer, GoMctsTransformerTch, RemoteModel,
        ServiceRequest, TransformerConfig,
    },
};
use games::{
    gamestates::euchre::{Euchre, EuchreGameState},
    GameState,
};
use rand::{rngs::StdRng, seq::IndexedRandom, SeedableRng};
use std::{path::PathBuf, sync::mpsc, time::Instant};

fn parse<T: std::str::FromStr>(name: &str, default: T) -> T {
    std::env::var(name).ok().and_then(|s| s.parse().ok()).unwrap_or(default)
}

fn pick_config() -> TransformerConfig {
    let v = EuchreTokenizer::VOCAB_SIZE;
    let c = EuchreTokenizer::MAX_CONTEXT;
    match std::env::var("EU_CONFIG").as_deref() {
        Ok("medium") => TransformerConfig::euchre_medium(v, c),
        Ok("paper") => TransformerConfig::paper_default(v, c),
        _ => TransformerConfig::euchre_smoke(v, c),
    }
}

fn main() {
    let weights: PathBuf =
        PathBuf::from(std::env::var("EU_WEIGHTS").unwrap_or_else(|_| {
            "/tmp/euchre_gomcts/final.safetensors".to_string()
        }));
    let n_games: usize = parse("EU_GAMES", 2000);
    let mcts_iter: usize = parse("EU_MCTS_ITER", 32);
    let base_seed: u64 = parse("EU_SEED", 0);
    let skip_mcts = std::env::var("EU_SKIP_MCTS").ok().as_deref() == Some("1");

    assert!(
        weights.exists(),
        "weights not found at {}; train first or override with EU_WEIGHTS",
        weights.display()
    );

    let cfg = pick_config();
    println!(
        "Euchre GO-MCTS eval: weights={}, games={}, mcts_iter={}, skip_mcts={}",
        weights.display(),
        n_games,
        mcts_iter,
        skip_mcts,
    );
    println!(
        "transformer: d={}, layers={}, heads={}, d_ff={}, vocab={}, max_ctx={}",
        cfg.d_model, cfg.n_layers, cfg.n_heads, cfg.d_ff, cfg.vocab_size, cfg.max_context,
    );

    println!("\n[1/3] random baseline (vs 3 random)…");
    let t0 = Instant::now();
    let (rb_mean, rb_se) = eval_random_baseline(n_games, base_seed.wrapping_add(7));
    let rb_secs = t0.elapsed().as_secs_f64();
    println!(
        "random  → mean={:+.4}  SE={:.4}  95% CI=[{:+.4}, {:+.4}]  ({:.1}s)",
        rb_mean,
        rb_se,
        rb_mean - 1.96 * rb_se,
        rb_mean + 1.96 * rb_se,
        rb_secs
    );
    println!(
        "kestrel: step=1 condition=random mean={:.6} se={:.6} secs={:.4}",
        rb_mean, rb_se, rb_secs
    );

    // --- Build the tch net + share across both raw & MCTS conditions.
    let device = tch::Device::cuda_if_available();
    let mut net = GoMctsTransformerTch::new(cfg, device).expect("build");
    net.load_safetensors(&weights).expect("load weights");
    let tokenizer = EuchreTokenizer;

    // --- Raw transformer via the batched eval helper (it already runs
    // the service-thread architecture under the hood).
    println!("\n[2/3] raw transformer (no MCTS wrapping)…");
    let t0 = Instant::now();
    let (raw_mean, raw_se) = eval_vs_random_batched_tch::<EuchreGameState, _, _>(
        &net,
        &tokenizer,
        Euchre::new_state,
        n_games,
        base_seed.wrapping_add(13),
        /* use_graph = */ false,
        /* graph_batch_size = */ 1,
    );
    let raw_secs = t0.elapsed().as_secs_f64();
    println!(
        "raw     → mean={:+.4}  SE={:.4}  95% CI=[{:+.4}, {:+.4}]  ({:.1}s)",
        raw_mean,
        raw_se,
        raw_mean - 1.96 * raw_se,
        raw_mean + 1.96 * raw_se,
        raw_secs
    );
    println!(
        "kestrel: step=2 condition=raw_transformer mean={:.6} se={:.6} secs={:.4}",
        raw_mean, raw_se, raw_secs
    );

    if skip_mcts {
        println!("\n[3/3] (skipped per EU_SKIP_MCTS=1)");
        return;
    }

    // --- GO-MCTS over the transformer via the service-thread architecture.
    // We spin up one service thread that owns the tch net, and run all
    // games sequentially (one MCTS at a time) against it via a
    // `RemoteModel`. Cheaper than the batched-self-play path because the
    // hand outer loop is sequential, but still gets the batched-leaf
    // benefit inside each MCTS via `batch_value`.
    println!("\n[3/3] GO-MCTS over the transformer ({} sims/decision)…", mcts_iter);
    let t0 = Instant::now();
    let (mcts_mean, mcts_se) = eval_search_via_service(
        &net,
        &tokenizer,
        n_games,
        mcts_iter,
        base_seed.wrapping_add(23),
    );
    let mcts_secs = t0.elapsed().as_secs_f64();
    println!(
        "gomcts  → mean={:+.4}  SE={:.4}  95% CI=[{:+.4}, {:+.4}]  ({:.1}s)",
        mcts_mean,
        mcts_se,
        mcts_mean - 1.96 * mcts_se,
        mcts_mean + 1.96 * mcts_se,
        mcts_secs
    );
    println!(
        "kestrel: step=3 condition=gomcts mean={:.6} se={:.6} mcts_iter={} secs={:.4}",
        mcts_mean, mcts_se, mcts_iter, mcts_secs
    );
}

/// Run `n_games` games where the search-wrapped transformer plays at
/// rotating seat `game_idx % 4` and uniform-random fills the rest. Uses
/// a single service thread for inference so the MCTS can batch
/// per-decision leaf evaluations through `RemoteModel::batch_value`.
fn eval_search_via_service(
    net: &GoMctsTransformerTch,
    tokenizer: &EuchreTokenizer,
    n_games: usize,
    mcts_iter: usize,
    base_seed: u64,
) -> (f64, f64) {
    use card_platypus::algorithms::gomcts_transformer::serve_batched_tch;
    let (request_tx, request_rx) = mpsc::channel::<ServiceRequest>();
    let max_batch = 256;
    let scores: Vec<f64> = std::thread::scope(|s| {
        let svc = s.spawn(move || {
            serve_batched_tch::<EuchreGameState, _>(net, tokenizer, request_rx, max_batch, false, 1)
        });
        let scores: Vec<f64> = (0..n_games)
            .map(|game_idx| {
                let rng_seed = base_seed.wrapping_add(game_idx as u64);
                let req_tx = request_tx.clone();
                let mut remote = RemoteModel { request_tx: req_tx };
                let mut search = GoMcts::<EuchreGameState, RemoteModel>::new(
                    GoMctsConfig {
                        uct_c: 0.4,
                        n_iterations: mcts_iter,
                        mu: 0.01,
                        n_rollout_steps: 0,
                        rollout_to_terminal: false,
                        n_parallel_sims: 1,
                    },
                    remote.clone(),
                    SeedableRng::seed_from_u64(rng_seed.wrapping_add(2)),
                );
                let _ = &mut remote;
                let mut rng: StdRng = SeedableRng::seed_from_u64(rng_seed);
                play_one_hand_search(game_idx % 4, &mut search, &mut rng)
            })
            .collect();
        drop(request_tx);
        svc.join().expect("service");
        scores
    });
    let n = scores.len() as f64;
    let mean: f64 = scores.iter().sum::<f64>() / n;
    let var: f64 = scores.iter().map(|x| (x - mean) * (x - mean)).sum::<f64>() / n;
    let se = (var / n).max(0.0).sqrt();
    (mean, se)
}

fn play_one_hand_search(
    subject_seat: usize,
    search: &mut GoMcts<EuchreGameState, RemoteModel>,
    rng: &mut StdRng,
) -> f64 {
    let mut gs = Euchre::new_state();
    let mut buf = Vec::new();
    while gs.is_chance_node() {
        buf.clear();
        gs.legal_actions(&mut buf);
        let a = *buf.choose(rng).expect("non-empty chance");
        gs.apply_action(a);
    }
    while !gs.is_terminal() {
        let p = gs.cur_player();
        let a = if p == subject_seat {
            use card_platypus::agents::Agent;
            search.step(&gs)
        } else {
            buf.clear();
            gs.legal_actions(&mut buf);
            *buf.choose(rng).expect("non-empty legal")
        };
        gs.apply_action(a);
    }
    gs.evaluate(subject_seat)
}

fn eval_random_baseline(n_games: usize, seed: u64) -> (f64, f64) {
    let mut total = 0.0;
    let mut total_sq = 0.0;
    for game_idx in 0..n_games {
        let mut rng: StdRng = SeedableRng::seed_from_u64(seed.wrapping_add(game_idx as u64));
        let subject_seat = game_idx % 4;
        let mut gs = Euchre::new_state();
        let mut buf = Vec::new();
        while gs.is_chance_node() {
            buf.clear();
            gs.legal_actions(&mut buf);
            let a = *buf.choose(&mut rng).expect("non-empty chance");
            gs.apply_action(a);
        }
        while !gs.is_terminal() {
            let _p = gs.cur_player();
            buf.clear();
            gs.legal_actions(&mut buf);
            let a = *buf.choose(&mut rng).expect("non-empty legal");
            gs.apply_action(a);
        }
        let s = gs.evaluate(subject_seat);
        total += s;
        total_sq += s * s;
    }
    let mean = total / n_games as f64;
    let var = (total_sq / n_games as f64) - mean * mean;
    let se = (var / n_games as f64).max(0.0).sqrt();
    (mean, se)
}
