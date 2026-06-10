//! Euchre GO-MCTS transformer training run (tch / libtorch).
//!
//! Single-flow trainer: tch primary on `tch::Device::cuda_if_available()`,
//! batched cross-game self-play with CUDA-graph capture for the AlphaZero
//! collection path, eager-mode pop self-play (two concurrent CUDA
//! captures don't compose), tch-backed eval + h2h. AdamW lives on tch
//! and never leaves it.
//!
//! Run:
//!   cargo run -p card_platypus --release --example euchre_gomcts_train
//!
//! Perf tuning (in-process knobs):
//!   TF32                  enable TF32 cuBLAS+cuDNN matmul (default 1)
//!   TCH_NUM_THREADS       libtorch intra-op thread pool cap (default 2)
//!
//! Perf tuning (set BEFORE the process starts — these are libtorch
//! init-time knobs, not Rust-level):
//!   OMP_NUM_THREADS              Recommended `2`. Caps libgomp's
//!     worker pool. Default is num_cores, which adds ~30 idle threads
//!     and context-switch tax for no benefit on a GPU-bound workload.
//!   PYTORCH_CUDA_ALLOC_CONF      Tunes the CUDACachingAllocator. The
//!     value is a comma-separated list. Recommended for long runs:
//!       expandable_segments:True   grow on demand instead of
//!                                  pre-reserving large blocks
//!       max_split_size_mb:512      stop carving big blocks into tiny
//!                                  slivers (anti-fragmentation)
//!       garbage_collection_threshold:0.8  auto-call empty_cache when
//!                                  80%+ of the pool is unused
//!     Combined example:
//!       PYTORCH_CUDA_ALLOC_CONF="expandable_segments:True,max_split_size_mb:512,garbage_collection_threshold:0.8"
//!
//! Knobs (env vars):
//!   EU_ITERS              outer training iterations           (default 6)
//!   EU_GAMES_PER_ITER     self-play games per iteration       (default 200)
//!   EU_MCTS_GAMES_FRAC    fraction of games using MCTS        (default 0.5)
//!   EU_MCTS_ITER          GO-MCTS sims per decision           (default 16)
//!   EU_EPOCHS_PER_ITER    training epochs per iteration       (default 4)
//!   EU_BATCH_SIZE         training batch size                 (default 64)
//!   EU_LR                 learning rate                       (default 1e-4)
//!   EU_EVAL_GAMES         validation games per evaluation     (default 200)
//!   EU_H2H_GAMES          head-to-head games vs frozen snap   (default 100)
//!   EU_DIRICHLET_ALPHA    Dirichlet α for self-play noise     (default 0.3)
//!   EU_DIRICHLET_EPS      noise mixing weight ε (0 = off)     (default 0.0)
//!   EU_BATCH_GAMES        cross-game batching factor          (default 16)
//!   EU_BATCH_MAX          soft cap on histories per forward   (default 512)
//!   EU_TCH_GRAPH_BATCH    captured CUDA-graph batch size      (default 48)
//!   EU_ROLLOUT_STEPS      MCTS rollout phase length per leaf  (default 0)
//!   EU_PARALLEL_SIMS      per-game MCTS virtual-loss width    (default 1)
//!   EU_RESUME_FROM        resume from `iter_NNN.safetensors`  (default 0)
//!   EU_INIT_WEIGHTS       load these weights before iter 1    (default unset)
//!   EU_CKPT_DIR           directory for snapshots / final     (default /tmp/euchre_gomcts)
//!   EU_SEED               base RNG seed                       (default 0)
//!   EU_CONFIG             "smoke" | "medium" | "paper"        (default medium)

use card_platypus::algorithms::gomcts_transformer::{
    collect_pop_examples_batched_tch, collect_self_play_games_batched_alphazero_tch,
    empty_cuda_cache, enable_tf32, euchre::EuchreTokenizer, eval_vs_random_batched_tch,
    head_to_head_eval_batched_tch, train_tch_with_callback, GoMctsTransformerTch, McfsConfig,
    PopulationTch, TransformerConfig,
};
use games::gamestates::euchre::Euchre;
use rand::{rngs::StdRng, SeedableRng};
use std::{path::PathBuf, time::Instant};

fn parse<T: std::str::FromStr>(name: &str, default: T) -> T {
    std::env::var(name).ok().and_then(|s| s.parse().ok()).unwrap_or(default)
}
fn parse_path(name: &str, default: &str) -> PathBuf {
    std::env::var(name).map(PathBuf::from).unwrap_or_else(|_| PathBuf::from(default))
}

fn pick_config() -> TransformerConfig {
    let v = EuchreTokenizer::VOCAB_SIZE;
    let c = EuchreTokenizer::MAX_CONTEXT;
    match std::env::var("EU_CONFIG").as_deref() {
        Ok("smoke") => TransformerConfig::euchre_smoke(v, c),
        Ok("paper") => TransformerConfig::paper_default(v, c),
        _ => TransformerConfig::euchre_medium(v, c),
    }
}

fn main() {
    let iters: usize = parse("EU_ITERS", 6);
    let games_per_iter: usize = parse("EU_GAMES_PER_ITER", 200);
    let mcts_frac: f64 = parse("EU_MCTS_GAMES_FRAC", 0.5);
    let mcts_iter: usize = parse("EU_MCTS_ITER", 16);
    let epochs: usize = parse("EU_EPOCHS_PER_ITER", 4);
    let batch_size: usize = parse("EU_BATCH_SIZE", 64);
    let lr: f64 = parse("EU_LR", 1e-4);
    let eval_games: usize = parse("EU_EVAL_GAMES", 200);
    let h2h_games: usize = parse("EU_H2H_GAMES", 100);
    let dirichlet_alpha: f64 = parse("EU_DIRICHLET_ALPHA", 0.3);
    let dirichlet_eps: f64 = parse("EU_DIRICHLET_EPS", 0.0);
    // Cross-game concurrency. Sweep at paper config, EU_GAMES_PER_ITER=200,
    // showed throughput climbing through bg=128 then plateauing; iter wall
    // ~26 s at bg=128 vs ~41 s at bg=24. Past bg=128 the chunk loop
    // starves at small game counts but doesn't hurt at the production
    // 3750-game scale.
    let batch_games: usize = parse("EU_BATCH_GAMES", 128).max(1);
    let batch_max: usize = parse("EU_BATCH_MAX", 512);
    // Captured CUDA-graph batch size. Smaller wins than expected — real
    // request bursts are tighter than batch_games would suggest because
    // MCTS-iter requests arrive interleaved across game threads, so each
    // burst is closer to ~16 histories than the worst-case
    // batch_games × parallel_sims × |legal|. gb=16 + the eager fallback
    // for over-size batches outperformed gb=24/32/48 across the sweep.
    let tch_graph_batch: i64 = parse("EU_TCH_GRAPH_BATCH", 16);
    let resume_from: usize = parse("EU_RESUME_FROM", 0);
    let rollout_steps: usize = parse("EU_ROLLOUT_STEPS", 0);
    // Paper-faithful rollout: ignore the step cap, sample until the
    // determinised world reaches a terminal state, return the actual
    // game payoff. This is what makes V learn from real reward
    // instead of from V's own estimate at a fixed horizon (the
    // suspected source of the value-head fixed-point plateau).
    let rollout_to_terminal: bool = parse::<usize>("EU_ROLLOUT_TO_TERMINAL", 0) == 1;
    let parallel_sims: usize = parse("EU_PARALLEL_SIMS", 1).max(1);
    let init_weights: Option<PathBuf> = std::env::var("EU_INIT_WEIGHTS").ok().map(PathBuf::from);
    let ckpt_dir: PathBuf = parse_path("EU_CKPT_DIR", "/tmp/euchre_gomcts");
    let base_seed: u64 = parse("EU_SEED", 0);

    std::fs::create_dir_all(&ckpt_dir).expect("create ckpt dir");

    // --- Process-wide perf toggles. Must run before any tensor work. ----------
    // TF32: ~5 bits of mantissa for 1.3-2x matmul on Ampere+ tensor
    // cores. Set TF32=0 to disable for numerical-equivalence comparison.
    if parse::<usize>("TF32", 1) == 1 {
        enable_tf32();
    }
    // Cap libtorch's intra-op CPU thread pool. The profile shows ~30
    // libgomp workers parked at any moment — the pool defaults to
    // num_cores, which means a lot of context-switch tax for no
    // benefit on a GPU-bound workload. 2 is enough for the small
    // amount of CPU-side tensor work our service thread does
    // (host->device copies, gather, readback).
    tch::set_num_threads(parse::<i32>("TCH_NUM_THREADS", 2).max(1));
    // For OMP_NUM_THREADS to cap libgomp itself it has to be set in
    // the parent shell before this binary launches — libgomp reads
    // it at lib-load time, not when we call set_num_threads here.
    // See the header doc-comment.

    let device = tch::Device::cuda_if_available();
    let cfg = pick_config();
    let tokenizer = EuchreTokenizer;
    println!(
        "Euchre GO-MCTS train (tch): iters={}, games/iter={}, mcts_frac={:.2}, mcts_iter={}, \
         epochs={}, batch={}, lr={}, eval={}, dirichlet_alpha={}, dirichlet_eps={}, \
         batch_games={}, batch_max={}, graph_batch={}, device={:?}, ckpt_dir={}",
        iters,
        games_per_iter,
        mcts_frac,
        mcts_iter,
        epochs,
        batch_size,
        lr,
        eval_games,
        dirichlet_alpha,
        dirichlet_eps,
        batch_games,
        batch_max,
        tch_graph_batch,
        device,
        ckpt_dir.display(),
    );
    println!(
        "transformer: d={}, layers={}, heads={}, d_ff={}, vocab={}, max_ctx={}",
        cfg.d_model, cfg.n_layers, cfg.n_heads, cfg.d_ff, cfg.vocab_size, cfg.max_context,
    );

    // --- Build the live tch net + population. ----------------------------------
    let mut net = GoMctsTransformerTch::new(cfg, device).expect("build tch net");
    if resume_from > 0 {
        let resume_path = ckpt_dir.join(format!("iter_{:03}.safetensors", resume_from));
        assert!(
            resume_path.exists(),
            "EU_RESUME_FROM={} requires {}",
            resume_from,
            resume_path.display()
        );
        net.load_safetensors(&resume_path).expect("load resume checkpoint");
        println!("resumed live weights from {}", resume_path.display());
    } else if let Some(path) = init_weights.as_ref() {
        assert!(
            path.exists(),
            "EU_INIT_WEIGHTS={} does not exist",
            path.display()
        );
        net.load_safetensors(path).expect("load init weights");
        println!("loaded initial weights from {}", path.display());
    }
    let mut pop = PopulationTch::new(net);
    // Replay prior snapshots so the population matches the state at the
    // end of iter `resume_from`. Each prior iter wrote its weights to
    // `iter_NNN.safetensors`; we load each into the live model briefly,
    // snapshot, then restore the latest as live.
    for k in 1..=resume_from {
        let snap_path = ckpt_dir.join(format!("iter_{:03}.safetensors", k));
        assert!(snap_path.exists(), "missing snapshot {}", snap_path.display());
        pop.live.load_safetensors(&snap_path).expect("load resumed snap");
        pop.snapshot().expect("snapshot resumed");
    }
    if resume_from > 0 {
        let latest = ckpt_dir.join(format!("iter_{:03}.safetensors", resume_from));
        pop.live.load_safetensors(&latest).expect("restore live tch");
        println!(
            "rebuilt population with {} frozen snapshots; resuming at iter {}",
            pop.num_snapshots(),
            resume_from + 1
        );
    }

    let mut rng: StdRng = SeedableRng::seed_from_u64(base_seed.wrapping_add(resume_from as u64));

    // --- iter 0 baseline. --------------------------------------------------------
    let (raw0, _) = eval_vs_random_batched_tch::<_, _, _>(
        &pop.live,
        &tokenizer,
        Euchre::new_state,
        eval_games,
        base_seed.wrapping_add(99),
        false,
        1,
    );
    println!(
        "iter {} ({}):  mean_reward={:+.4}",
        resume_from,
        if resume_from == 0 { "random init" } else { "resume start" },
        raw0
    );

    println!(
        "{:>4}  {:>9}  {:>8}  {:>8}  {:>10}  {:>12}  {:>10}  {:>10}  {:>8}",
        "iter", "examples", "mcts", "pop", "train_loss", "mean_reward", "h2h_mean", "h2h_win%", "secs"
    );

    for iter in (resume_from + 1)..=iters {
        let t0 = Instant::now();
        let seed = base_seed.wrapping_add(iter as u64);
        let n_mcts = (games_per_iter as f64 * mcts_frac).round() as usize;
        let n_pop = games_per_iter - n_mcts;

        let mcfs_cfg = McfsConfig {
            root_dirichlet_alpha: dirichlet_alpha,
            root_dirichlet_eps: dirichlet_eps,
            n_rollout_steps: rollout_steps,
            rollout_to_terminal,
            n_parallel_sims: parallel_sims,
        };

        // --- MCTS-driven AlphaZero self-play (cross-game batched + CUDA graph). --
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
                    Euchre::new_state,
                    games_this_chunk,
                    batch_max,
                    mcts_iter,
                    mcfs_cfg,
                    chunk_seed,
                    /* use_graph = */ true,
                    tch_graph_batch,
                );
                mcts_examples.extend(exs);
                // Each chunk's scoped self-play just dropped 128 game
                // threads' worth of MCTS trees, rollout tensors, and
                // captured-graph state. Without reclaiming here the
                // CUDACachingAllocator pool grows monotonically across
                // chunks — fine for non-rollout self-play, fatal for
                // rollout-to-terminal where every leaf evaluation
                // produces ~10 extra tensors. Reclaim between chunks.
                empty_cuda_cache();
            }
        }

        // --- Pop self-play. Graph capture used to fail here with
        // "operation not permitted when stream is capturing" when two
        // service threads (live + frozen) entered capture mode
        // simultaneously, but that was while candle was still in the
        // process holding its own CUDA contexts; under the tch-only
        // stack the two captures don't interfere. Default on; flip
        // EU_POP_USE_GRAPH=0 to fall back to eager if WSL2 regresses. -
        let pop_use_graph = parse::<usize>("EU_POP_USE_GRAPH", 1) == 1;
        let pop_examples = collect_pop_examples_batched_tch::<_, _, _>(
            &mut pop,
            &tokenizer,
            Euchre::new_state,
            n_pop,
            batch_games,
            &mut rng,
            seed.wrapping_add(7),
            pop_use_graph,
            if pop_use_graph { tch_graph_batch } else { 1 },
        );
        // Same reclaim as between MCTS chunks: pop self-play just
        // dropped two service threads' worth of tensors.
        empty_cuda_cache();

        let mut examples = mcts_examples;
        examples.extend(pop_examples);

        let ckpt_path = ckpt_dir.join(format!("iter_{:03}.safetensors", iter));

        // --- Train on the collected examples. ------------------------------------
        let loss = train_tch_with_callback(
            &mut pop.live,
            &tokenizer,
            &examples,
            epochs,
            batch_size,
            lr,
            &mut rng,
            |_, _| {},
        )
        .expect("train_tch");
        pop.live.save_safetensors(&ckpt_path).expect("save tch ckpt");
        pop.snapshot().expect("tch snapshot");
        // Training just freed its activations, optimizer scratch, and
        // forward-pass intermediates. The CUDACachingAllocator holds
        // those blocks unless we reclaim them — and the next h2h call
        // is about to hydrate a fresh frozen snapshot's worth of
        // parameters. Reclaim before that hydrate so it allocates into
        // clean space rather than fighting fragmentation.
        empty_cuda_cache();

        // --- Eval vs random. Eager mode (see eval_rationale above). --------------
        let (mean, _) = eval_vs_random_batched_tch::<_, _, _>(
            &pop.live,
            &tokenizer,
            Euchre::new_state,
            eval_games,
            seed.wrapping_add(10_000),
            false,
            1,
        );

        // --- H2H vs previous-iter snapshot. --------------------------------------
        let (h2h_mean, h2h_win) = if pop.num_snapshots() >= 2 {
            let prev = pop
                .sample_specific_frozen(pop.num_snapshots() - 2)
                .expect("hydrate prev snapshot")
                .expect("snapshot index in bounds");
            let result = head_to_head_eval_batched_tch::<_, _, _>(
                &pop.live,
                &prev,
                &tokenizer,
                Euchre::new_state,
                h2h_games,
                seed.wrapping_add(20_000),
                false,
                1,
            );
            // Drop the hydrated `prev` and reclaim its parameters
            // before the next iter starts a new self-play scope.
            drop(prev);
            empty_cuda_cache();
            result
        } else {
            (f64::NAN, f64::NAN)
        };

        let elapsed = t0.elapsed().as_secs_f64();
        println!(
            "{:>4}  {:>9}  {:>8}  {:>8}  {:>10.4}  {:>12.4}  {:>10}  {:>10}  {:>8.2}",
            iter,
            examples.len(),
            n_mcts,
            n_pop,
            loss,
            mean,
            if h2h_mean.is_nan() { "—".to_string() } else { format!("{:+.4}", h2h_mean) },
            if h2h_win.is_nan() { "—".to_string() } else { format!("{:.1}%", 100.0 * h2h_win) },
            elapsed,
        );
        println!(
            "kestrel: step={} train_loss={:.6} mean_reward={:.6} h2h_mean={:.6} \
             h2h_win_rate={:.6} examples={} mcts_games={} pop_games={} snapshots={} secs={:.4}",
            iter,
            loss,
            mean,
            if h2h_mean.is_nan() { 0.0 } else { h2h_mean },
            if h2h_win.is_nan() { 0.5 } else { h2h_win },
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
