//! Tight-CI eval of a trained Euchre GO-MCTS transformer checkpoint.
//!
//! Loads `EU_WEIGHTS` (default `/tmp/euchre_gomcts/final.safetensors`),
//! plays `EU_GAMES` hands against three uniform-random opponents (the
//! transformer's seat rotates), and reports:
//!   - raw transformer (sample directly from `model.policy`)
//!   - GO-MCTS-wrapped transformer (search at every decision)
//!
//! Compared to the per-iter eval in `euchre_gomcts_train.rs` (300 games)
//! this can be run with `EU_GAMES=2000+` to drive 95% CI down to ~±0.03
//! per condition.
//!
//! Knobs:
//!   EU_WEIGHTS         safetensors path     (default /tmp/euchre_gomcts/final.safetensors)
//!   EU_CONFIG          model architecture   (default smoke; must match training!)
//!   EU_GAMES           hands per condition  (default 2000)
//!   EU_MCTS_ITER       per-decision MCTS budget for the wrapped eval (default 32)
//!   EU_SEED            base RNG seed        (default 0)
//!   EU_SKIP_MCTS=1     skip the MCTS eval (raw only)
//!   EU_INFER           argmaxval | lm   (default argmaxval). lm = use the
//!                      LM head softmax directly. Use for supervised
//!                      bootstraps where the value head hasn't seen
//!                      counterfactual actions.
//!
//! Run:
//!   EU_GAMES=2000 cargo run -p card_platypus --release --example euchre_gomcts_eval

use card_platypus::algorithms::{
    gomcts::{GenerativeModel, GoMcts, GoMctsConfig},
    gomcts_transformer::{
        default_device, euchre::EuchreTokenizer, GoMctsTransformer, InferenceMode,
        TransformerConfig, TransformerGenerativeModel,
    },
};
use card_platypus::agents::Agent;
use games::{
    gamestates::euchre::{Euchre, EuchreGameState},
    GameState,
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
    let infer_mode = match std::env::var("EU_INFER").as_deref() {
        Ok("lm") | Ok("LmSoftmax") => InferenceMode::LmSoftmax,
        _ => InferenceMode::ArgmaxVal,
    };
    println!(
        "Euchre GO-MCTS eval: weights={}, games={}, mcts_iter={}, skip_mcts={}, infer={:?}",
        weights.display(),
        n_games,
        mcts_iter,
        skip_mcts,
        infer_mode,
    );
    println!(
        "transformer: d={}, layers={}, heads={}, d_ff={}, vocab={}, max_ctx={}",
        cfg.d_model, cfg.n_layers, cfg.n_heads, cfg.d_ff, cfg.vocab_size, cfg.max_context,
    );

    // --- Baseline: random vs random (3 random opponents from rotating
    // seat 0..4). Gives us a calibration of "what does no-skill look
    // like" for this eval setup. Should be close to 0 within ±0.03 at
    // n=2000.
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

    // --- Raw transformer (sample from policy directly).
    println!("\n[2/3] raw transformer (no MCTS wrapping)…");
    let mut net = GoMctsTransformer::new(cfg, default_device()).expect("build");
    net.load(&weights).expect("load weights");
    let mut model = EModel::new(net, EuchreTokenizer).with_inference_mode(infer_mode);
    let t0 = Instant::now();
    let (raw_mean, raw_se) = eval_raw(&mut model, n_games, base_seed.wrapping_add(13));
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

    // --- GO-MCTS over the transformer.
    println!("\n[3/3] GO-MCTS over the transformer ({} sims/decision)…", mcts_iter);
    let search = GoMcts::new(
        GoMctsConfig { uct_c: 0.4, n_iterations: mcts_iter, mu: 0.01, n_rollout_steps: 0, n_parallel_sims: 1 },
        model,
        SeedableRng::seed_from_u64(base_seed.wrapping_add(17)),
    );
    let t0 = Instant::now();
    let (mcts_mean, mcts_se) = eval_search(search, n_games, base_seed.wrapping_add(23));
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

/// Returns (mean, standard_error_of_mean) of search-seat payoff over n_games.
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
        let s = play_one_hand(
            game_idx % 4,
            |gs, _rng| search.step(gs),
            &mut rng,
        );
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

/// Play one full Euchre hand. The `subject_seat` is the seat whose
/// move is provided by `subject_action`; the other three seats play
/// uniformly at random. Returns the subject seat's team score.
fn play_one_hand(
    subject_seat: usize,
    mut subject_action: impl FnMut(&EuchreGameState, &mut StdRng) -> games::Action,
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
