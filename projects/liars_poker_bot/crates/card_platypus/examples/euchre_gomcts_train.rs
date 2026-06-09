//! Euchre GO-MCTS transformer training run.
//!
//! Same shape as `kuhn_gomcts_train.rs`: MCTS-driven games + population
//! games, soft + hard targets, snapshotting, checkpointing. Sized for
//! Euchre via `TransformerConfig::euchre_medium` (d=128 / 4L / 4H /
//! FF=256). Still CPU-only by default — the `gpu_cuda` cargo feature
//! swaps in candle's CUDA backend if the box has it.
//!
//! Run:
//!   cargo run -p card_platypus --release --example euchre_gomcts_train
//!
//! Knobs (env vars):
//!   EU_ITERS              outer training iterations           (default 6)
//!   EU_GAMES_PER_ITER     self-play games per iteration       (default 200)
//!   EU_MCTS_GAMES_FRAC    fraction of games using MCTS        (default 0.5)
//!   EU_MCTS_ITER          GO-MCTS sims per decision           (default 16)
//!   EU_EPOCHS_PER_ITER    training epochs per iteration       (default 4)
//!   EU_BATCH_SIZE         training batch size                 (default 64)
//!   EU_LR                 learning rate                       (default 1e-3)
//!   EU_EVAL_GAMES         validation games per evaluation     (default 200)
//!   EU_H2H_GAMES          head-to-head games vs frozen snap   (default 100)
//!   EU_DIRICHLET_ALPHA    Dirichlet α for self-play noise     (default 0.3)
//!   EU_DIRICHLET_EPS      noise mixing weight ε (0 = off)     (default 0.0)
//!   EU_PIMCTS_BOOTSTRAP   1 = use OpenHandSolver as value     (default 0)
//!                         oracle during MCTS-driven self-play
//!                         (E2). Much lower-variance value
//!                         targets at ~2-3× wall cost.
//!   EU_OHS_K              determinizations averaged per       (default 4)
//!                         OHS query (E2). Higher K → less
//!                         strategy-fusion overestimate, more
//!                         OHS calls.
//!   EU_ALPHAZERO          1 = use MCTS root value as the      (default 0)
//!                         value-head target (pure self-
//!                         bootstrap). Mutually exclusive with
//!                         EU_PIMCTS_BOOTSTRAP.
//!   EU_BATCH_GAMES        cross-game batching factor (E5).    (default 1)
//!                         When > 1 (and EU_ALPHAZERO=1), run
//!                         this many games in parallel sharing
//!                         a single batched forward service.
//!                         Higher → more GPU saturation, more
//!                         host threads. Try 16 to start.
//!   EU_BATCH_MAX          soft cap on histories per batched   (default 512)
//!                         forward call (only used with
//!                         EU_BATCH_GAMES > 1).
//!   EU_RESUME_FROM        resume from `iter_NNN.safetensors`   (default 0)
//!                         in EU_CKPT_DIR. 0 = fresh random
//!                         init. When >0, also rebuilds the
//!                         Population from iter_001..iter_NNN
//!                         so frozen-snapshot self-play sees
//!                         the same history as a fresh run
//!                         would have. AdamW state is NOT
//!                         saved/restored — momentum recovers
//!                         within a few hundred batches.
//!   EU_ROLLOUT_STEPS      MCTS rollout phase length per         (default 0)
//!                         leaf expansion (paper Algorithm 1
//!                         uses ~4-10). 0 = AlphaZero-style (no
//!                         rollout, value head at leaf). Higher
//!                         = more search depth per decision,
//!                         essentially for free.
//!   EU_INIT_WEIGHTS       load weights from this safetensors   (default unset)
//!                         path BEFORE iter 1, but treat them
//!                         as initial state (NO Population
//!                         rebuild). Use for CFR bootstrap or
//!                         similar pretraining. Mutually
//!                         exclusive with EU_RESUME_FROM (resume
//!                         takes precedence if both set).
//!   EU_CKPT_DIR           directory for snapshots / final     (default /tmp/euchre_gomcts)
//!   EU_SEED               base RNG seed                       (default 0)
//!   EU_CONFIG             "smoke" | "medium" | "paper"        (default medium)

use card_platypus::algorithms::{
    gomcts::{GenerativeModel, GoMcts, GoMctsConfig},
    gomcts_transformer::{
        collect_population_game, collect_population_games_batched,
        collect_self_play_game_alphazero, collect_self_play_game_mcts_cfg,
        collect_self_play_game_mcts_with_value_oracle, collect_self_play_games_batched_alphazero,
        collect_self_play_games_batched_alphazero_multi_device, default_device, default_devices,
        euchre::EuchreTokenizer, eval_vs_random_batched, head_to_head_eval,
        head_to_head_eval_batched, sync_replicas_from_primary, train, GoMctsTransformer,
        McfsConfig, Population, TrainExample, TransformerConfig, TransformerGenerativeModel,
    },
    ismcts::Evaluator,
    open_hand_solver::OpenHandSolver,
};
#[cfg(feature = "tch_spike")]
use card_platypus::algorithms::gomcts_transformer_tch::{
    collect_self_play_games_batched_alphazero_tch, GoMctsTransformerTch,
};
use games::{
    gamestates::euchre::{Euchre, EuchreGameState},
    GameState,
};
use rand::{rngs::StdRng, seq::IndexedRandom, RngExt, SeedableRng};
use std::{path::PathBuf, time::Instant};

fn parse<T: std::str::FromStr>(name: &str, default: T) -> T {
    std::env::var(name).ok().and_then(|s| s.parse().ok()).unwrap_or(default)
}
fn parse_path(name: &str, default: &str) -> PathBuf {
    std::env::var(name).map(PathBuf::from).unwrap_or_else(|_| PathBuf::from(default))
}

type EModel = TransformerGenerativeModel<EuchreGameState, EuchreTokenizer>;

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
    let pimcts_bootstrap: usize = parse("EU_PIMCTS_BOOTSTRAP", 0);
    let ohs_k: usize = parse("EU_OHS_K", 4).max(1);
    let alphazero: usize = parse("EU_ALPHAZERO", 0);
    let batch_games: usize = parse("EU_BATCH_GAMES", 1).max(1);
    let batch_max: usize = parse("EU_BATCH_MAX", 512);
    let resume_from: usize = parse("EU_RESUME_FROM", 0);
    let rollout_steps: usize = parse("EU_ROLLOUT_STEPS", 0);
    let parallel_sims: usize = parse("EU_PARALLEL_SIMS", 1).max(1);
    let num_devices: usize = parse("EU_NUM_DEVICES", 1).max(1);
    let use_tch: bool = parse::<usize>("EU_USE_TCH", 0) == 1;
    let use_tch_graph: bool = parse::<usize>("EU_USE_TCH_GRAPH", 0) == 1;
    let tch_graph_batch: usize = parse("EU_TCH_GRAPH_BATCH", 0); // 0 = auto
    let init_weights: Option<PathBuf> = std::env::var("EU_INIT_WEIGHTS").ok().map(PathBuf::from);
    assert!(
        pimcts_bootstrap == 0 || alphazero == 0,
        "EU_PIMCTS_BOOTSTRAP and EU_ALPHAZERO are mutually exclusive"
    );
    if batch_games > 1 {
        assert!(
            alphazero == 1,
            "EU_BATCH_GAMES > 1 currently requires EU_ALPHAZERO=1"
        );
    }
    let ckpt_dir: PathBuf = parse_path("EU_CKPT_DIR", "/tmp/euchre_gomcts");
    let base_seed: u64 = parse("EU_SEED", 0);

    std::fs::create_dir_all(&ckpt_dir).expect("create ckpt dir");

    let device = default_device();
    let cfg = pick_config();
    println!(
        "Euchre GO-MCTS train: iters={}, games/iter={}, mcts_frac={:.2}, mcts_iter={}, \
         epochs={}, batch={}, lr={}, eval={}, dirichlet_alpha={}, dirichlet_eps={}, \
         pimcts_bootstrap={}, alphazero={}, batch_games={}, batch_max={}, \
         device={:?}, ckpt_dir={}",
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
        pimcts_bootstrap,
        alphazero,
        batch_games,
        batch_max,
        device,
        ckpt_dir.display(),
    );
    println!(
        "transformer: d={}, layers={}, heads={}, d_ff={}, vocab={}, max_ctx={}",
        cfg.d_model, cfg.n_layers, cfg.n_heads, cfg.d_ff, cfg.vocab_size, cfg.max_context,
    );

    let mut net = GoMctsTransformer::new(cfg, device).expect("build");
    if resume_from > 0 {
        let resume_path = ckpt_dir.join(format!("iter_{:03}.safetensors", resume_from));
        assert!(
            resume_path.exists(),
            "EU_RESUME_FROM={} requires {}",
            resume_from,
            resume_path.display()
        );
        net.load(&resume_path).expect("load resume checkpoint");
        println!("resumed live weights from {}", resume_path.display());
    } else if let Some(path) = init_weights.as_ref() {
        assert!(
            path.exists(),
            "EU_INIT_WEIGHTS={} does not exist",
            path.display()
        );
        net.load(path).expect("load init weights");
        println!("loaded initial weights from {}", path.display());
    }
    let live = EModel::new(net, EuchreTokenizer);
    let mut pop = Population::new(live);
    // Multi-device replicas (E5 stream overlap). When EU_NUM_DEVICES > 1
    // we build N-1 additional GoMctsTransformer replicas on independent
    // CUDA devices and sync them from the primary after each training
    // step. Self-play then spreads games across (1 + replicas.len())
    // pipelines, with each pipeline owning its own service thread +
    // CUDA stream → host work on one device overlaps GPU work on the
    // others.
    let mut replicas: Vec<GoMctsTransformer> = Vec::new();
    if num_devices > 1 {
        let all_devices = default_devices(num_devices);
        // all_devices[0] is the same kind as the primary's; the primary
        // already exists on a device from `default_device()`. We use
        // [1..N] for the replicas.
        for d in all_devices.into_iter().skip(1) {
            let mut r = GoMctsTransformer::new(cfg, d).expect("build replica");
            // Sync initial weights from primary so iter-1 self-play
            // uses identical models on all devices.
            let tmp = tempfile::NamedTempFile::new().expect("tmp");
            pop.live.net.save(tmp.path()).expect("save primary");
            r.load(tmp.path()).expect("load replica");
            replicas.push(r);
        }
        println!(
            "device replicas: {} ({} total devices for self-play)",
            replicas.len(),
            1 + replicas.len()
        );
    }
    // Replay prior snapshots so the Population matches the state at
    // end-of-iter `resume_from`. Each prior iter wrote its weights to
    // `iter_NNN.safetensors`; we load each into a fresh transformer and
    // record it as a frozen snapshot.
    for k in 1..=resume_from {
        let snap_path = ckpt_dir.join(format!("iter_{:03}.safetensors", k));
        assert!(snap_path.exists(), "missing snapshot {}", snap_path.display());
        let cfg_snap = *pop.live.net.config();
        let mut snap_net = GoMctsTransformer::new(cfg_snap, pop.live.net.device().clone())
            .expect("build snap");
        snap_net.load(&snap_path).expect("load snap");
        // Reuse the live model briefly: swap snap into it so we can call
        // `pop.snapshot()` which records the current live weights. After
        // recording, swap back the original live weights.
        std::mem::swap(&mut pop.live.net, &mut snap_net);
        pop.snapshot().expect("record snapshot");
        std::mem::swap(&mut pop.live.net, &mut snap_net);
    }
    if resume_from > 0 {
        println!(
            "rebuilt Population with {} frozen snapshots; resuming at iter {}",
            pop.num_snapshots(),
            resume_from + 1
        );
    }
    // EU_USE_TCH=1: build a tch replica synced from the candle primary.
    // Self-play inference will go through this replica (much lower
    // per-launch overhead on WSL2). Candle still owns training.
    #[cfg(feature = "tch_spike")]
    let mut tch_net: Option<GoMctsTransformerTch> = if use_tch {
        let dev = match pop.live.net.device() {
            candle_core::Device::Cuda(_) => tch::Device::Cuda(0),
            _ => tch::Device::Cpu,
        };
        let mut t = GoMctsTransformerTch::new(cfg, dev).expect("build tch net");
        let tmp = tempfile::NamedTempFile::new().expect("tmp");
        pop.live.net.save(tmp.path()).expect("save primary for tch sync");
        t.load_safetensors(tmp.path()).expect("load tch weights");
        println!("tch self-play enabled (EU_USE_TCH=1), device={:?}", dev);
        Some(t)
    } else {
        None
    };
    #[cfg(not(feature = "tch_spike"))]
    {
        if use_tch {
            panic!("EU_USE_TCH=1 requires building with --features tch_spike");
        }
    }
    let mut rng: StdRng = SeedableRng::seed_from_u64(base_seed.wrapping_add(resume_from as u64));

    let raw0 = eval_vs_random(&mut pop.live, eval_games, base_seed.wrapping_add(99));
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
            n_parallel_sims: parallel_sims,
        };
        let mcts_examples = if pimcts_bootstrap == 1 {
            collect_mcts_examples_with_oracle(
                &mut pop.live, n_mcts, mcts_iter, mcfs_cfg, ohs_k, seed,
            )
        } else if alphazero == 1 && batch_games > 1 {
            #[cfg(feature = "tch_spike")]
            let tch_ref = tch_net.as_ref();
            #[cfg(not(feature = "tch_spike"))]
            let tch_ref: Option<&()> = None;
            // Default graph batch to a value sized for typical request
            // bursts: batch_games × parallel_sims × 8 (legal actions
            // upper bound) is the rough cap; if user gives 0, fall back
            // to that estimate but clamp at batch_max so the captured
            // graph never exceeds the request cap.
            let auto_gb = (batch_games as i64)
                .saturating_mul(parallel_sims.max(1) as i64)
                .saturating_mul(8);
            let gb = if tch_graph_batch == 0 {
                auto_gb.min(batch_max as i64).max(64)
            } else {
                tch_graph_batch as i64
            };
            collect_mcts_examples_alphazero_batched(
                &mut pop.live,
                &replicas,
                tch_ref,
                use_tch_graph,
                gb,
                n_mcts,
                mcts_iter,
                mcfs_cfg,
                batch_games,
                batch_max,
                seed,
            )
        } else if alphazero == 1 {
            collect_mcts_examples_alphazero(&mut pop.live, n_mcts, mcts_iter, mcfs_cfg, seed)
        } else {
            collect_mcts_examples(&mut pop.live, n_mcts, mcts_iter, mcfs_cfg, seed)
        };
        let pop_examples = if batch_games > 1 {
            collect_pop_examples_batched(&mut pop, n_pop, batch_games, &mut rng, seed.wrapping_add(7))
        } else {
            collect_pop_examples(&mut pop, n_pop, seed.wrapping_add(7))
        };

        let mut examples = mcts_examples;
        examples.extend(pop_examples);

        let loss = train(
            &mut pop.live,
            &examples,
            epochs,
            batch_size,
            lr,
            &mut rng,
        )
        .expect("train");

        pop.snapshot().expect("snapshot");
        let ckpt_path = ckpt_dir.join(format!("iter_{:03}.safetensors", iter));
        pop.live.net.save(&ckpt_path).expect("save");
        // Sync replicas from the freshly-trained primary so next iter's
        // self-play uses the updated weights on every device.
        if !replicas.is_empty() {
            sync_replicas_from_primary(&pop.live.net, &mut replicas)
                .expect("sync replicas");
        }
        // Sync tch replica from the freshly-trained candle primary so
        // next iter's tch-backed self-play uses the updated weights.
        #[cfg(feature = "tch_spike")]
        if let Some(t) = tch_net.as_mut() {
            t.load_safetensors(&ckpt_path).expect("sync tch from candle ckpt");
        }

        let mean = if batch_games > 1 {
            let (m, _) = eval_vs_random_batched::<EuchreGameState, _, _>(
                &pop.live.net,
                &pop.live.tokenizer,
                Euchre::new_state,
                eval_games,
                seed.wrapping_add(10_000),
            );
            m
        } else {
            eval_vs_random(&mut pop.live, eval_games, seed.wrapping_add(10_000))
        };
        // Head-to-head: live vs the previous iter's frozen snapshot. This
        // is the convergence signal — when this drops to ~0 (and win%
        // settles at ~50%), the model has stopped self-improving. For
        // iter 1 there's only one snapshot (just taken: the live's own
        // weights), so we record NaN.
        let (h2h_mean, h2h_win) = if pop.num_snapshots() >= 2 {
            let mut prev = pop
                .sample_specific_frozen(pop.num_snapshots() - 2)
                .expect("hydrate prior snapshot")
                .expect("snapshot index in bounds");
            if batch_games > 1 {
                head_to_head_eval_batched::<EuchreGameState, _, _>(
                    &pop.live.net,
                    &prev.net,
                    &pop.live.tokenizer,
                    Euchre::new_state,
                    h2h_games,
                    seed.wrapping_add(20_000),
                )
            } else {
                head_to_head_eval(
                    &mut pop.live,
                    &mut prev,
                    Euchre::new_state,
                    h2h_games,
                    seed.wrapping_add(20_000),
                )
            }
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
    pop.live.net.save(&final_path).expect("save final");
    println!("final checkpoint: {}", final_path.display());
}

fn collect_mcts_examples(
    model: &mut EModel,
    n_games: usize,
    mcts_iter: usize,
    mcfs_cfg: McfsConfig,
    seed: u64,
) -> Vec<TrainExample> {
    if n_games == 0 {
        return Vec::new();
    }
    let cfg = *model.net.config();
    let device = model.net.device().clone();
    let placeholder_net = GoMctsTransformer::new(cfg, device).expect("build placeholder");
    let placeholder = EModel::new(placeholder_net, EuchreTokenizer);
    let mut search: GoMcts<EuchreGameState, EModel> = GoMcts::new(
        GoMctsConfig { uct_c: 0.4, n_iterations: mcts_iter, mu: 0.01, n_rollout_steps: mcfs_cfg.n_rollout_steps, n_parallel_sims: mcfs_cfg.n_parallel_sims },
        placeholder,
        SeedableRng::seed_from_u64(seed.wrapping_add(2)),
    );
    std::mem::swap(model, search.model_mut());
    let mut out = Vec::new();
    for game_idx in 0..n_games {
        let mut game_rng: StdRng = SeedableRng::seed_from_u64(seed.wrapping_add(100 + game_idx as u64));
        let exs = collect_self_play_game_mcts_cfg(
            Euchre::new_state,
            &mut search,
            mcfs_cfg,
            &mut game_rng,
        );
        out.extend(exs);
    }
    std::mem::swap(model, search.model_mut());
    out
}

/// Pure AlphaZero-style self-play: value-head target is the MCTS root
/// value at each decision, not the terminal payoff. Strictly cheaper
/// than the OHS variant (no extra perfect-info solve per position) —
/// the root value is a byproduct of the MCTS we already run for the
/// policy target.
fn collect_mcts_examples_alphazero(
    model: &mut EModel,
    n_games: usize,
    mcts_iter: usize,
    mcfs_cfg: McfsConfig,
    seed: u64,
) -> Vec<TrainExample> {
    if n_games == 0 {
        return Vec::new();
    }
    let cfg = *model.net.config();
    let device = model.net.device().clone();
    let placeholder_net = GoMctsTransformer::new(cfg, device).expect("build placeholder");
    let placeholder = EModel::new(placeholder_net, EuchreTokenizer);
    let mut search: GoMcts<EuchreGameState, EModel> = GoMcts::new(
        GoMctsConfig { uct_c: 0.4, n_iterations: mcts_iter, mu: 0.01, n_rollout_steps: mcfs_cfg.n_rollout_steps, n_parallel_sims: mcfs_cfg.n_parallel_sims },
        placeholder,
        SeedableRng::seed_from_u64(seed.wrapping_add(2)),
    );
    std::mem::swap(model, search.model_mut());
    let mut out = Vec::new();
    for game_idx in 0..n_games {
        let mut game_rng: StdRng = SeedableRng::seed_from_u64(seed.wrapping_add(100 + game_idx as u64));
        let exs = collect_self_play_game_alphazero(
            Euchre::new_state,
            &mut search,
            mcfs_cfg,
            &mut game_rng,
        );
        out.extend(exs);
    }
    std::mem::swap(model, search.model_mut());
    out
}

/// E5: cross-game batched AlphaZero self-play. Spins up `batch_games`
/// game threads that share one batched forward service so the GPU sees
/// one large forward call instead of `batch_games` small ones.
///
/// When `replicas` is non-empty (multi-device mode), uses
/// `collect_self_play_games_batched_alphazero_multi_device` so the
/// games-per-chunk are spread across the primary + replica nets. Each
/// device runs its own service pipeline → CUDA contexts overlap.
#[cfg(feature = "tch_spike")]
type TchOpt<'a> = Option<&'a GoMctsTransformerTch>;
#[cfg(not(feature = "tch_spike"))]
type TchOpt<'a> = Option<&'a ()>;

fn collect_mcts_examples_alphazero_batched(
    primary: &mut EModel,
    replicas: &[GoMctsTransformer],
    tch_net: TchOpt,
    use_tch_graph: bool,
    tch_graph_batch: i64,
    n_games: usize,
    mcts_iter: usize,
    mcfs_cfg: McfsConfig,
    batch_games: usize,
    batch_max: usize,
    seed: u64,
) -> Vec<TrainExample> {
    if n_games == 0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    let n_devices = 1 + replicas.len();
    // Each "chunk" runs `batch_games * n_devices` games concurrently
    // (batch_games per device replica). That keeps per-device service
    // load identical to single-device mode.
    let chunk_size = batch_games * n_devices;
    let chunks = n_games.div_ceil(chunk_size);
    // Build the slice of net references once. Borrow lifetimes are
    // tied to the closures inside the loop below; reconstructed each
    // iter so the &mut on `primary` can still be reused.
    for chunk_idx in 0..chunks {
        let games_this_chunk = chunk_size.min(n_games - chunk_idx * chunk_size);
        let chunk_seed = seed.wrapping_add((chunk_idx as u64) * 1_000);
        #[cfg(feature = "tch_spike")]
        if let Some(t) = tch_net {
            // Tch backend: ignore candle replicas; tch service handles
            // batching directly on its CUDA stream.
            let exs = collect_self_play_games_batched_alphazero_tch::<EuchreGameState, _, _>(
                t,
                &primary.tokenizer,
                Euchre::new_state,
                games_this_chunk,
                batch_max,
                mcts_iter,
                mcfs_cfg,
                chunk_seed,
                use_tch_graph,
                tch_graph_batch,
            );
            out.extend(exs);
            continue;
        }
        if n_devices == 1 {
            let exs = collect_self_play_games_batched_alphazero::<EuchreGameState, _, _>(
                &primary.net,
                &primary.tokenizer,
                Euchre::new_state,
                games_this_chunk,
                batch_max,
                mcts_iter,
                mcfs_cfg,
                chunk_seed,
            );
            out.extend(exs);
        } else {
            let mut nets: Vec<&GoMctsTransformer> = Vec::with_capacity(n_devices);
            nets.push(&primary.net);
            for r in replicas {
                nets.push(r);
            }
            let exs = collect_self_play_games_batched_alphazero_multi_device::<
                EuchreGameState,
                _,
                _,
            >(
                &nets,
                &primary.tokenizer,
                Euchre::new_state,
                games_this_chunk,
                batch_max,
                mcts_iter,
                mcfs_cfg,
                chunk_seed,
            );
            out.extend(exs);
        }
    }
    out
}

/// E2: PIMCTS-bootstrap value targets. The value-head target at each
/// position is the average over `ohs_k` determinizations of
/// `OpenHandSolver(world, player)`. This is exactly the leaf value
/// PIMCTS uses (avg over K perfect-info solves of resampled worlds) —
/// the strongest target we can build with OHS as the only oracle.
///
/// Cost scales linearly with K: K=1 ≈ single OHS per position
/// (cheaper but biased high by strategy fusion); K=4 is roughly the
/// PIMCTS-leaf approximation; K=16+ approaches the imperfect-info
/// expectation but with diminishing returns.
fn collect_mcts_examples_with_oracle(
    model: &mut EModel,
    n_games: usize,
    mcts_iter: usize,
    mcfs_cfg: McfsConfig,
    ohs_k: usize,
    seed: u64,
) -> Vec<TrainExample> {
    use games::resample::ResampleFromInfoState;
    if n_games == 0 {
        return Vec::new();
    }
    let cfg = *model.net.config();
    let device = model.net.device().clone();
    let placeholder_net = GoMctsTransformer::new(cfg, device).expect("build placeholder");
    let placeholder = EModel::new(placeholder_net, EuchreTokenizer);
    let mut search: GoMcts<EuchreGameState, EModel> = GoMcts::new(
        GoMctsConfig { uct_c: 0.4, n_iterations: mcts_iter, mu: 0.01, n_rollout_steps: mcfs_cfg.n_rollout_steps, n_parallel_sims: mcfs_cfg.n_parallel_sims },
        placeholder,
        SeedableRng::seed_from_u64(seed.wrapping_add(2)),
    );
    std::mem::swap(model, search.model_mut());
    let mut solver = OpenHandSolver::new_euchre();
    // Closure-internal RNG so the K resamples are deterministic per
    // (game, position) tuple given the outer seed.
    let mut oracle_rng: StdRng = SeedableRng::seed_from_u64(seed.wrapping_add(5_000_000));
    let mut out = Vec::new();
    for game_idx in 0..n_games {
        let mut game_rng: StdRng = SeedableRng::seed_from_u64(seed.wrapping_add(100 + game_idx as u64));
        let exs = collect_self_play_game_mcts_with_value_oracle(
            Euchre::new_state,
            &mut search,
            mcfs_cfg,
            |gs, p| {
                // K-determinization average. The first sample is the
                // actual self-play world (free — already on hand); the
                // remaining (K-1) draws come from resample_from_istate
                // and reset to the same trick/phase as the live state
                // before evaluating.
                let mut total = solver.evaluate_player(gs, p);
                for _ in 1..ohs_k {
                    let w = gs.resample_from_istate(p, &mut oracle_rng);
                    total += solver.evaluate_player(&w, p);
                }
                total / ohs_k as f64
            },
            &mut game_rng,
        );
        out.extend(exs);
    }
    std::mem::swap(model, search.model_mut());
    out
}

fn collect_pop_examples(
    pop: &mut Population<EuchreGameState, EuchreTokenizer>,
    n_games: usize,
    seed: u64,
) -> Vec<TrainExample> {
    let mut out = Vec::new();
    for game_idx in 0..n_games {
        let mut rng: StdRng = SeedableRng::seed_from_u64(seed.wrapping_add(1 + game_idx as u64));
        // Rotate live seat 0..4 across games for balance.
        let live_seat = game_idx % 4;
        let exs = collect_population_game(pop, Euchre::new_state, Some(live_seat), &mut rng)
            .expect("population game");
        out.extend(exs);
    }
    out
}

/// Batched population games. Per `batch_games`-sized chunk, picks ONE
/// random frozen snapshot to play at non-live seats. Less per-game
/// frozen-opponent diversity than the sequential variant but the same
/// "opponent comes from population, not current self" property.
fn collect_pop_examples_batched(
    pop: &mut Population<EuchreGameState, EuchreTokenizer>,
    n_games: usize,
    batch_games: usize,
    rng: &mut StdRng,
    seed: u64,
) -> Vec<TrainExample> {
    use rand::RngExt;
    if n_games == 0 || pop.num_snapshots() == 0 {
        // Without any frozen, fall back to sequential (it falls back to
        // live-vs-live which is what we want at iter 1 anyway).
        return collect_pop_examples(pop, n_games, seed);
    }
    let mut out = Vec::new();
    let chunks = (n_games + batch_games - 1) / batch_games;
    for chunk_idx in 0..chunks {
        let games_this_chunk = batch_games.min(n_games - chunk_idx * batch_games);
        let frozen = pop
            .sample_frozen(rng)
            .expect("hydrate frozen")
            .expect("snapshots non-empty");
        let chunk_seed = seed.wrapping_add((chunk_idx as u64) * 1_000_000 + rng.random::<u64>());
        let exs = collect_population_games_batched::<EuchreGameState, _, _>(
            &pop.live.net,
            &frozen.net,
            &pop.live.tokenizer,
            Euchre::new_state,
            games_this_chunk,
            chunk_seed,
        );
        out.extend(exs);
    }
    out
}

/// `n_games` Euchre hands: trained transformer at one seat (rotating)
/// vs uniform-random at the other three. Returns the transformer's
/// per-hand team payoff averaged across all evaluated games.
fn eval_vs_random(model: &mut EModel, n_games: usize, seed: u64) -> f64 {
    let mut total = 0.0;
    for game_idx in 0..n_games {
        let mut rng: StdRng = SeedableRng::seed_from_u64(seed.wrapping_add(game_idx as u64));
        let model_seat = game_idx % 4;
        let mut gs = Euchre::new_state();
        let mut buf = Vec::new();
        while gs.is_chance_node() {
            buf.clear();
            gs.legal_actions(&mut buf);
            let a = *buf.choose(&mut rng).expect("non-empty chance");
            gs.apply_action(a);
        }
        while !gs.is_terminal() {
            let p = gs.cur_player();
            buf.clear();
            gs.legal_actions(&mut buf);
            let a = if p == model_seat {
                let h = gs.istate_key(p);
                <EModel as GenerativeModel<EuchreGameState>>::sample(model, &h, &buf, &mut rng)
            } else {
                *buf.choose(&mut rng).expect("non-empty legal")
            };
            gs.apply_action(a);
        }
        total += gs.evaluate(model_seat);
    }
    total / n_games as f64
}

