//! Sweep GO-MCTS inference budget on a trained checkpoint (tch).
//!
//! Eval the same checkpoint against uniform-random opponents at multiple
//! MCTS sim budgets.
//!
//! Run:
//!   cargo run -p card_platypus --release --example euchre_gomcts_mcts_sweep
//!
//! Knobs:
//!   EU_WEIGHTS         safetensors path  (default /tmp/euchre_gomcts/final.safetensors)
//!   EU_CONFIG          smoke|medium|paper  (must match training)
//!   EU_GAMES           hands per condition  (default 300)
//!   EU_BUDGETS         comma-sep list of MCTS budgets  (default 0,16,64,128)
//!                      0 = raw transformer (no search)
//!   EU_SEED            base seed  (default 0)

use card_platypus::algorithms::{
    gomcts::{GoMcts, GoMctsConfig},
    gomcts_transformer::{
        euchre::EuchreTokenizer, eval_vs_random_batched_tch, serve_batched_tch,
        GoMctsTransformerTch, RemoteModel, ServiceRequest, TransformerConfig,
    },
};
use card_platypus::agents::Agent;
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

fn parse_budgets() -> Vec<usize> {
    let raw = std::env::var("EU_BUDGETS").unwrap_or_else(|_| "0,16,64,128".to_string());
    raw.split(',').filter_map(|s| s.trim().parse().ok()).collect()
}

fn main() {
    let weights: PathBuf = PathBuf::from(
        std::env::var("EU_WEIGHTS").unwrap_or_else(|_| {
            "/tmp/euchre_gomcts/final.safetensors".to_string()
        }),
    );
    let n_games: usize = parse("EU_GAMES", 300);
    let base_seed: u64 = parse("EU_SEED", 0);
    let budgets = parse_budgets();

    assert!(weights.exists(), "weights not found at {}", weights.display());

    let cfg = pick_config();
    println!(
        "GO-MCTS budget sweep (tch): weights={}, games={}, budgets={:?}, config={:?}",
        weights.display(),
        n_games,
        budgets,
        cfg
    );

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
        "kestrel: step=0 budget=random mean={:.6} se={:.6} secs={:.4}",
        rb_mean, rb_se, rb_secs
    );

    let device = tch::Device::cuda_if_available();
    let mut net = GoMctsTransformerTch::new(cfg, device).expect("build");
    net.load_safetensors(&weights).expect("load weights");
    let tokenizer = EuchreTokenizer;

    for (i, budget) in budgets.iter().enumerate() {
        let t0 = Instant::now();
        let (mean, se) = if *budget == 0 {
            eval_vs_random_batched_tch::<EuchreGameState, _, _>(
                &net,
                &tokenizer,
                Euchre::new_state,
                n_games,
                base_seed.wrapping_add(100 + i as u64),
                false,
                1,
            )
        } else {
            eval_search_via_service(
                &net,
                &tokenizer,
                n_games,
                *budget,
                base_seed.wrapping_add(300 + i as u64),
            )
        };
        let secs = t0.elapsed().as_secs_f64();
        let label = if *budget == 0 { "raw".to_string() } else { format!("mcts={}", budget) };
        println!(
            "{:<7} → mean={:+.4}  SE={:.4}  95% CI=[{:+.4}, {:+.4}]  ({:.1}s)",
            label,
            mean,
            se,
            mean - 1.96 * se,
            mean + 1.96 * se,
            secs
        );
        println!(
            "kestrel: step={} budget={} mean={:.6} se={:.6} secs={:.4}",
            i + 1,
            budget,
            mean,
            se,
            secs
        );
    }
}

fn eval_search_via_service(
    net: &GoMctsTransformerTch,
    tokenizer: &EuchreTokenizer,
    n_games: usize,
    mcts_iter: usize,
    base_seed: u64,
) -> (f64, f64) {
    let (request_tx, request_rx) = mpsc::channel::<ServiceRequest>();
    let scores: Vec<f64> = std::thread::scope(|s| {
        let svc = s.spawn(move || {
            serve_batched_tch::<EuchreGameState, _>(net, tokenizer, request_rx, 256, false, 1)
        });
        let scores: Vec<f64> = (0..n_games)
            .map(|game_idx| {
                let rng_seed = base_seed.wrapping_add(game_idx as u64);
                let req_tx = request_tx.clone();
                let remote = RemoteModel::new(req_tx);
                let mut search = GoMcts::<EuchreGameState, RemoteModel>::new(
                    GoMctsConfig {
                        uct_c: 0.4,
                        n_iterations: mcts_iter,
                        mu: 0.01,
                        n_rollout_steps: 0,
                        rollout_to_terminal: false,
                        n_parallel_sims: 1,
                    },
                    remote,
                    SeedableRng::seed_from_u64(rng_seed.wrapping_add(2)),
                );
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
