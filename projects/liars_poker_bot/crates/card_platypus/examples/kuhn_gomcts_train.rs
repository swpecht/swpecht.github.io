//! Paper-faithful (v2) Kuhn Poker training run for the GO-MCTS
//! transformer generative model. Tch / libtorch backend.
//!
//! Architecture: cross-game batched AlphaZero self-play, eager-mode pop
//! self-play, tch-backed eval. Matches the Euchre trainer's pipeline at
//! a smaller scale.
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
//!   KP_BATCH_GAMES        cross-game batching factor         (default 16)
//!   KP_BATCH_MAX          soft cap on histories per forward  (default 256)
//!   KP_CKPT_DIR           directory for snapshots / final    (default /tmp/kuhn_gomcts)
//!   KP_SEED               base RNG seed                      (default 0)

use card_platypus::algorithms::gomcts_transformer::{
    collect_pop_examples_batched_tch, collect_self_play_games_batched_alphazero_tch,
    eval_vs_random_batched_tch, kuhn::KuhnTokenizer, train_tch, GoMctsTransformerTch, McfsConfig,
    PopulationTch, TransformerConfig,
};
use games::gamestates::kuhn_poker::KuhnPoker;
use rand::{rngs::StdRng, SeedableRng};
use std::{path::PathBuf, time::Instant};

fn parse<T: std::str::FromStr>(name: &str, default: T) -> T {
    std::env::var(name).ok().and_then(|s| s.parse().ok()).unwrap_or(default)
}
fn parse_path(name: &str, default: &str) -> PathBuf {
    std::env::var(name).map(PathBuf::from).unwrap_or_else(|_| PathBuf::from(default))
}

fn main() {
    let iters: usize = parse("KP_ITERS", 8);
    let games_per_iter: usize = parse("KP_GAMES_PER_ITER", 1000);
    let mcts_frac: f64 = parse("KP_MCTS_GAMES_FRAC", 0.5);
    let mcts_iter: usize = parse("KP_MCTS_ITER", 32);
    let epochs: usize = parse("KP_EPOCHS_PER_ITER", 6);
    let batch_size: usize = parse("KP_BATCH_SIZE", 64);
    let lr: f64 = parse("KP_LR", 5e-3);
    let eval_games: usize = parse("KP_EVAL_GAMES", 1500);
    let batch_games: usize = parse("KP_BATCH_GAMES", 16).max(1);
    let batch_max: usize = parse("KP_BATCH_MAX", 256);
    let ckpt_dir: PathBuf = parse_path("KP_CKPT_DIR", "/tmp/kuhn_gomcts");
    let base_seed: u64 = parse("KP_SEED", 0);

    std::fs::create_dir_all(&ckpt_dir).expect("create ckpt dir");

    let device = tch::Device::cuda_if_available();
    let tokenizer = KuhnTokenizer;
    println!(
        "Kuhn GO-MCTS train (tch): iters={}, games/iter={}, mcts_frac={:.2}, mcts_iter={}, \
         epochs/iter={}, batch={}, lr={}, batch_games={}, batch_max={}, device={:?}, ckpt_dir={}",
        iters,
        games_per_iter,
        mcts_frac,
        mcts_iter,
        epochs,
        batch_size,
        lr,
        batch_games,
        batch_max,
        device,
        ckpt_dir.display(),
    );

    let cfg = TransformerConfig::kuhn_small(KuhnTokenizer::VOCAB_SIZE, KuhnTokenizer::MAX_CONTEXT);
    let net = GoMctsTransformerTch::new(cfg, device).expect("build");
    let mut pop = PopulationTch::new(net);
    let mut rng: StdRng = SeedableRng::seed_from_u64(base_seed);

    let (raw0, _) = eval_vs_random_batched_tch::<_, _, _>(
        &pop.live,
        &tokenizer,
        KuhnPoker::new_state,
        eval_games,
        base_seed.wrapping_add(99),
        false,
        1,
    );
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

        // 1) MCTS-driven AlphaZero self-play (cross-game batched). No
        //    CUDA-graph capture for Kuhn — the model is so small that
        //    eager mode wins anyway and the smaller batch sizes wouldn't
        //    benefit from a captured graph.
        let mcfs = McfsConfig::default();
        let mut mcts_examples = Vec::new();
        if n_mcts > 0 {
            let chunk_size = batch_games;
            let chunks = n_mcts.div_ceil(chunk_size);
            for chunk_idx in 0..chunks {
                let games_this_chunk = chunk_size.min(n_mcts - chunk_idx * chunk_size);
                let chunk_seed = seed.wrapping_add((chunk_idx as u64) * 1_000);
                let exs = collect_self_play_games_batched_alphazero_tch::<_, _, _>(
                    &pop.live,
                    &tokenizer,
                    KuhnPoker::new_state,
                    games_this_chunk,
                    batch_max,
                    mcts_iter,
                    mcfs,
                    chunk_seed,
                    false,
                    1,
                );
                mcts_examples.extend(exs);
            }
        }

        // 2) Population games (hard targets, frozen opponents at non-live seats).
        let pop_examples = collect_pop_examples_batched_tch::<_, _, _>(
            &mut pop,
            &tokenizer,
            KuhnPoker::new_state,
            n_pop,
            batch_games,
            &mut rng,
            seed.wrapping_add(7),
            false,
            1,
        );

        let mut examples = mcts_examples;
        examples.extend(pop_examples);

        // 3) Train.
        let loss = train_tch(
            &mut pop.live,
            &tokenizer,
            &examples,
            epochs,
            batch_size,
            lr,
            &mut rng,
        )
        .expect("train");

        // 4) Snapshot + 5) Checkpoint.
        let ckpt_path = ckpt_dir.join(format!("iter_{:03}.safetensors", iter));
        pop.live.save_safetensors(&ckpt_path).expect("save checkpoint");
        pop.snapshot().expect("snapshot");

        // 6) Eval.
        let (mean, _) = eval_vs_random_batched_tch::<_, _, _>(
            &pop.live,
            &tokenizer,
            KuhnPoker::new_state,
            eval_games,
            seed.wrapping_add(10_000),
            false,
            1,
        );
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
    pop.live.save_safetensors(&final_path).expect("save final");
    println!("final checkpoint: {}", final_path.display());
}
