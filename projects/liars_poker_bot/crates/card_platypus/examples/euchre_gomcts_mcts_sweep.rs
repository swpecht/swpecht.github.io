//! Sweep GO-MCTS inference budget on a trained checkpoint.
//!
//! Eval the same checkpoint against uniform-random opponents at multiple
//! MCTS sim budgets. Used to test E1: "does more search at inference
//! compound the trained value head?"
//!
//! Run:
//!   cargo run -p card_platypus --release --features gpu_cuda \
//!     --example euchre_gomcts_mcts_sweep
//!
//! Knobs:
//!   EU_WEIGHTS         safetensors path  (default /tmp/euchre_gomcts/final.safetensors)
//!   EU_CONFIG          smoke|medium|paper  (must match training)
//!   EU_GAMES           hands per condition  (default 300)
//!   EU_BUDGETS         comma-sep list of MCTS budgets  (default 0,16,64,128)
//!                      0 = raw transformer (no search)
//!   EU_SEED            base seed  (default 0)

use card_platypus::algorithms::{
    gomcts::{GenerativeModel, GoMcts, GoMctsConfig},
    gomcts_transformer::{
        default_device, euchre::EuchreTokenizer, GoMctsTransformer, TransformerConfig,
        TransformerGenerativeModel,
    },
};
use card_platypus::agents::Agent;
use games::{
    gamestates::euchre::{Euchre, EuchreGameState},
    Action, GameState,
};
use rand::{rngs::StdRng, seq::IndexedRandom, SeedableRng};
use std::{path::PathBuf, time::Instant};

fn parse<T: std::str::FromStr>(name: &str, default: T) -> T {
    std::env::var(name).ok().and_then(|s| s.parse().ok()).unwrap_or(default)
}

type EModel = TransformerGenerativeModel<EuchreGameState, EuchreTokenizer>;

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
        "GO-MCTS budget sweep: weights={}, games={}, budgets={:?}, config={:?}",
        weights.display(),
        n_games,
        budgets,
        cfg
    );

    // Baseline: random vs random calibrates the seat-bias and rotates
    // the same way as gomcts-vs-random below, so any positive number
    // above this is real skill.
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

    for (i, budget) in budgets.iter().enumerate() {
        let t0 = Instant::now();
        let mut net = GoMctsTransformer::new(cfg, default_device()).expect("build");
        net.load(&weights).expect("load");
        let mut model = EModel::new(net, EuchreTokenizer);
        let (mean, se) = if *budget == 0 {
            eval_raw(&mut model, n_games, base_seed.wrapping_add(100 + i as u64))
        } else {
            let search = GoMcts::new(
                GoMctsConfig { uct_c: 0.4, n_iterations: *budget, mu: 0.01, n_rollout_steps: 0, n_parallel_sims: 1 },
                model,
                SeedableRng::seed_from_u64(base_seed.wrapping_add(200 + i as u64)),
            );
            eval_search(search, n_games, base_seed.wrapping_add(300 + i as u64))
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

fn eval_raw(model: &mut EModel, n_games: usize, seed: u64) -> (f64, f64) {
    let mut total = 0.0;
    let mut total_sq = 0.0;
    for game_idx in 0..n_games {
        let mut rng: StdRng = SeedableRng::seed_from_u64(seed.wrapping_add(game_idx as u64));
        let s = play_one_hand(
            game_idx % 4,
            |gs, rng| {
                let p = gs.cur_player();
                let h = gs.istate_key(p);
                let mut buf = Vec::new();
                gs.legal_actions(&mut buf);
                <EModel as GenerativeModel<EuchreGameState>>::sample(model, &h, &buf, rng)
            },
            &mut rng,
        );
        total += s;
        total_sq += s * s;
    }
    finish(total, total_sq, n_games)
}

fn eval_search(
    mut search: GoMcts<EuchreGameState, EModel>,
    n_games: usize,
    seed: u64,
) -> (f64, f64) {
    let mut total = 0.0;
    let mut total_sq = 0.0;
    for game_idx in 0..n_games {
        let mut rng: StdRng = SeedableRng::seed_from_u64(seed.wrapping_add(game_idx as u64));
        let s = play_one_hand(game_idx % 4, |gs, _rng| search.step(gs), &mut rng);
        total += s;
        total_sq += s * s;
    }
    finish(total, total_sq, n_games)
}

fn eval_random_baseline(n_games: usize, seed: u64) -> (f64, f64) {
    let mut total = 0.0;
    let mut total_sq = 0.0;
    for game_idx in 0..n_games {
        let mut rng: StdRng = SeedableRng::seed_from_u64(seed.wrapping_add(game_idx as u64));
        let s = play_one_hand(
            game_idx % 4,
            |gs, rng| {
                let mut buf = Vec::new();
                gs.legal_actions(&mut buf);
                *buf.choose(rng).unwrap()
            },
            &mut rng,
        );
        total += s;
        total_sq += s * s;
    }
    finish(total, total_sq, n_games)
}

fn finish(total: f64, total_sq: f64, n: usize) -> (f64, f64) {
    let mean = total / n as f64;
    let var = (total_sq / n as f64) - mean * mean;
    let se = (var / n as f64).max(0.0).sqrt();
    (mean, se)
}

fn play_one_hand(
    subject_seat: usize,
    mut subject_action: impl FnMut(&EuchreGameState, &mut StdRng) -> Action,
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
            subject_action(&gs, rng)
        } else {
            buf.clear();
            gs.legal_actions(&mut buf);
            *buf.choose(rng).expect("non-empty legal")
        };
        gs.apply_action(a);
    }
    gs.evaluate(subject_seat)
}
