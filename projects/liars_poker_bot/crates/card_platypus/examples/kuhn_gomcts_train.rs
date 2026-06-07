//! Paper-faithful (v2) Kuhn Poker training run for the GO-MCTS
//! transformer generative model.
//!
//! What changed vs the v1 example:
//!   * Self-play uses **GO-MCTS** at every decision (AlphaZero-style)
//!     with the root visit distribution as the soft policy target.
//!   * **Population** of historical snapshots: opponents in self-play
//!     sample from prior iterations' frozen weights, preventing
//!     fixed-point collapse against the current self.
//!   * Trained weights are **checkpointed** to `KP_CKPT_DIR` so the
//!     final model is reusable.
//!   * Mixed schedule: a slice of pure-sampled hard-target games every
//!     iteration keeps coverage broad while soft targets sharpen
//!     decisions.
//!
//! Run:
//!   cargo run -p card_platypus --release --example kuhn_gomcts_train
//!
//! Knobs (env vars):
//!   KP_ITERS              outer training iterations          (default 8)
//!   KP_GAMES_PER_ITER     self-play games per iteration      (default 1000)
//!   KP_MCTS_GAMES_FRAC    fraction of games using MCTS       (default 0.5)
//!   KP_MCTS_ITER          GO-MCTS sims per decision          (default 32)
//!   KP_EPOCHS_PER_ITER    training epochs per iteration      (default 6)
//!   KP_BATCH_SIZE         training batch size                (default 64)
//!   KP_LR                 learning rate                      (default 5e-3)
//!   KP_EVAL_GAMES         validation games per evaluation    (default 1500)
//!   KP_CKPT_DIR           directory for snapshots / final    (default /tmp/kuhn_gomcts)
//!   KP_SEED               base RNG seed                      (default 0)

use card_platypus::algorithms::{
    gomcts::{GenerativeModel, GoMcts, GoMctsConfig},
    gomcts_transformer::{
        collect_population_game, collect_self_play_game_mcts, default_device,
        kuhn::KuhnTokenizer, train, GoMctsTransformer, Population, TrainExample,
        TransformerConfig, TransformerGenerativeModel,
    },
};
use games::{
    gamestates::kuhn_poker::{KPGameState, KuhnPoker},
    GameState,
};
use rand::{rngs::StdRng, seq::IndexedRandom, SeedableRng};
use std::{path::PathBuf, time::Instant};

fn parse<T: std::str::FromStr>(name: &str, default: T) -> T {
    std::env::var(name).ok().and_then(|s| s.parse().ok()).unwrap_or(default)
}
fn parse_path(name: &str, default: &str) -> PathBuf {
    std::env::var(name).map(PathBuf::from).unwrap_or_else(|_| PathBuf::from(default))
}

type KModel = TransformerGenerativeModel<KPGameState, KuhnTokenizer>;

fn main() {
    let iters: usize = parse("KP_ITERS", 8);
    let games_per_iter: usize = parse("KP_GAMES_PER_ITER", 1000);
    let mcts_frac: f64 = parse("KP_MCTS_GAMES_FRAC", 0.5);
    let mcts_iter: usize = parse("KP_MCTS_ITER", 32);
    let epochs: usize = parse("KP_EPOCHS_PER_ITER", 6);
    let batch_size: usize = parse("KP_BATCH_SIZE", 64);
    let lr: f64 = parse("KP_LR", 5e-3);
    let eval_games: usize = parse("KP_EVAL_GAMES", 1500);
    let ckpt_dir: PathBuf = parse_path("KP_CKPT_DIR", "/tmp/kuhn_gomcts");
    let base_seed: u64 = parse("KP_SEED", 0);

    std::fs::create_dir_all(&ckpt_dir).expect("create ckpt dir");

    let device = default_device();
    println!(
        "Kuhn GO-MCTS train (v2): iters={}, games/iter={}, mcts_frac={:.2}, mcts_iter={}, \
         epochs/iter={}, batch={}, lr={}, device={:?}, ckpt_dir={}",
        iters,
        games_per_iter,
        mcts_frac,
        mcts_iter,
        epochs,
        batch_size,
        lr,
        device,
        ckpt_dir.display(),
    );

    let cfg = TransformerConfig::kuhn_small(KuhnTokenizer::VOCAB_SIZE, KuhnTokenizer::MAX_CONTEXT);
    let net = GoMctsTransformer::new(cfg, device).expect("build");
    let live = KModel::new(net, KuhnTokenizer);
    let mut pop = Population::new(live);
    let mut rng: StdRng = SeedableRng::seed_from_u64(base_seed);

    let raw0 = eval_vs_uniform(&mut pop.live, eval_games, base_seed.wrapping_add(99));
    println!("iter 0 (random init):  mean_reward={:+.4}", raw0);

    println!(
        "{:>4}  {:>9}  {:>8}  {:>8}  {:>10}  {:>12}  {:>8}",
        "iter", "examples", "mcts", "pop", "train_loss", "mean_reward", "secs"
    );

    for iter in 1..=iters {
        let t0 = Instant::now();
        let seed = base_seed.wrapping_add(iter as u64);
        let n_mcts = (games_per_iter as f64 * mcts_frac).round() as usize;
        let n_pop = games_per_iter - n_mcts;

        // 1) MCTS-driven games (soft targets).
        let mcts_examples = collect_mcts_examples(&mut pop.live, n_mcts, mcts_iter, seed);
        // 2) Population games (hard targets, frozen opponents).
        let pop_examples = collect_pop_examples(&mut pop, n_pop, seed.wrapping_add(7));

        let mut examples = mcts_examples;
        examples.extend(pop_examples);

        // 3) Train.
        let loss = train(
            &mut pop.live,
            &examples,
            epochs,
            batch_size,
            lr,
            &mut rng,
        )
        .expect("train");

        // 4) Snapshot into the population so subsequent iters can use
        //    this iter as a frozen opponent.
        pop.snapshot().expect("snapshot");
        // 5) Checkpoint to disk.
        let ckpt_path = ckpt_dir.join(format!("iter_{:03}.safetensors", iter));
        pop.live.net.save(&ckpt_path).expect("save checkpoint");

        // 6) Eval.
        let mean = eval_vs_uniform(&mut pop.live, eval_games, seed.wrapping_add(10_000));
        let elapsed = t0.elapsed().as_secs_f64();
        println!(
            "{:>4}  {:>9}  {:>8}  {:>8}  {:>10.4}  {:>12.4}  {:>8.2}",
            iter,
            examples.len(),
            n_mcts,
            n_pop,
            loss,
            mean,
            elapsed,
        );
        println!(
            "kestrel: step={} train_loss={:.6} mean_reward={:.6} examples={} \
             mcts_games={} pop_games={} snapshots={} secs={:.4}",
            iter,
            loss,
            mean,
            examples.len(),
            n_mcts,
            n_pop,
            pop.num_snapshots(),
            elapsed,
        );
    }

    let final_path = ckpt_dir.join("final.safetensors");
    pop.live.net.save(&final_path).expect("save final");
    println!("final checkpoint: {}", final_path.display());
}

/// AlphaZero-style examples: run GO-MCTS at every decision, soft targets.
fn collect_mcts_examples(
    model: &mut KModel,
    n_games: usize,
    mcts_iter: usize,
    seed: u64,
) -> Vec<TrainExample> {
    if n_games == 0 {
        return Vec::new();
    }
    // Move the model into a GoMcts for self-play, then we'll take it
    // back. We can do this by std::mem::replace through an Option, or
    // simpler: build the GoMcts with a *fresh dummy* and then swap. The
    // cleanest way given our trait is to construct the search around a
    // temporary fresh model, then swap. But that costs a build per call.
    //
    // Even simpler: take the model out via std::mem::take requires
    // Default, which TransformerGenerativeModel isn't. So we use the
    // borrow-via-pointer approach: take ownership inline through a
    // temporary swap.
    let mut search_rng: StdRng = SeedableRng::seed_from_u64(seed.wrapping_add(1));
    let mut out = Vec::with_capacity(n_games * 3);
    // Use the borrow-checker-friendly path: build a GoMcts around the
    // model by temporarily wrapping it in an Option-swap.
    let mut search: GoMcts<KPGameState, KModel> = {
        // Construct a placeholder, then swap. Placeholder is built with
        // a *new* tiny transformer so it has the right type.
        let placeholder_cfg = TransformerConfig::kuhn_small(
            KuhnTokenizer::VOCAB_SIZE,
            KuhnTokenizer::MAX_CONTEXT,
        );
        let placeholder_net =
            GoMctsTransformer::new(placeholder_cfg, model.net.device().clone()).expect("build");
        let placeholder = KModel::new(placeholder_net, KuhnTokenizer);
        GoMcts::new(
            GoMctsConfig { uct_c: 0.4, n_iterations: mcts_iter, mu: 0.01 },
            placeholder,
            SeedableRng::seed_from_u64(seed.wrapping_add(2)),
        )
    };
    // Swap our real model into the search.
    std::mem::swap(model, search.model_mut());
    for game_idx in 0..n_games {
        let mut game_rng: StdRng = SeedableRng::seed_from_u64(seed.wrapping_add(100 + game_idx as u64));
        let exs = collect_self_play_game_mcts(KuhnPoker::new_state, &mut search, &mut game_rng);
        out.extend(exs);
        let _ = search_rng.next_u64();
    }
    // Swap back so the caller can continue using the (updated-not-by-us)
    // model. Then return.
    std::mem::swap(model, search.model_mut());
    out
}

use rand::Rng as _;

/// Population-style games (hard targets, frozen opponents at non-live
/// seats).
fn collect_pop_examples(
    pop: &mut Population<KPGameState, KuhnTokenizer>,
    n_games: usize,
    seed: u64,
) -> Vec<TrainExample> {
    let mut out = Vec::new();
    for game_idx in 0..n_games {
        let mut rng: StdRng = SeedableRng::seed_from_u64(seed.wrapping_add(1 + game_idx as u64));
        let live_seat = (game_idx % 2) as usize;
        let exs = collect_population_game(pop, KuhnPoker::new_state, Some(live_seat), &mut rng)
            .expect("population game");
        out.extend(exs);
    }
    out
}

/// `n_games` head-to-head Kuhn hands: trained transformer vs uniform-
/// random opponent. Seats rotate so seat bias washes out.
fn eval_vs_uniform(model: &mut KModel, n_games: usize, seed: u64) -> f64 {
    let mut total = 0.0;
    for game_idx in 0..n_games {
        let mut rng: StdRng = SeedableRng::seed_from_u64(seed.wrapping_add(game_idx as u64));
        let model_seat = game_idx % 2;
        let mut gs = KuhnPoker::new_state();
        let mut buf = Vec::new();
        while gs.is_chance_node() {
            buf.clear();
            gs.legal_actions(&mut buf);
            let a = *buf.choose(&mut rng).unwrap();
            gs.apply_action(a);
        }
        while !gs.is_terminal() {
            let p = gs.cur_player();
            buf.clear();
            gs.legal_actions(&mut buf);
            let a = if p == model_seat {
                let h = gs.istate_key(p);
                <KModel as GenerativeModel<KPGameState>>::sample(model, &h, &buf, &mut rng)
            } else {
                *buf.choose(&mut rng).unwrap()
            };
            gs.apply_action(a);
        }
        total += gs.evaluate(model_seat);
    }
    total / n_games as f64
}
