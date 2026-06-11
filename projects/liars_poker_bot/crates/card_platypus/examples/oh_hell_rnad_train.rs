//! R-NaD training for 3-player Oh Hell across trick counts 1–10.
//!
//! Incorporates the lessons of the Euchre run (rnad-implementation.md
//! entries 2–5):
//!   * warm start from the PIMCTS-bootstrap (`bootstrap_v2`, t1–10) with
//!     π_reg = the bootstrap, so early dynamics are KL-tethered;
//!   * MANY more games per fixed point (RN_REG_EVERY default 500) —
//!     the Euchre outer loop never settled at 200;
//!   * lr + η annealing over the final third (damps the late-run orbit,
//!     shrinks the η-smoothing gap);
//!   * `spread_penalty` routing: Oh Hell's `evaluate` is mean-centred
//!     3-player zero-sum, so the actor pays −η·logratio and the other
//!     two players receive +η·logratio/2 each;
//!   * trick counts cycled uniformly through every self-play batch —
//!     self-play natively covers the post-deviation contexts the OH-4
//!     distribution-shift diagnosis showed PIMCTS data misses;
//!   * in-training evals at n that means something (≥500/trick-count).
//!
//! Run:
//!   cargo run -p card_platypus --release --example oh_hell_rnad_train
//!
//! Knobs (env vars):
//!   RN_INIT          warm-start weights ('' = from scratch)
//!                    (default /home/steven/card_platypus/gomcts/oh_hell/bootstrap_v2.safetensors)
//!   RN_VHEAD         scalar | outcome — must match RN_INIT   (default scalar)
//!   RN_CONFIG        smoke | medium | paper                  (default paper)
//!   RN_PLAYERS       players                                 (default 3)
//!   RN_TRICKS_MIN/MAX trick-count range cycled in self-play  (default 1/10)
//!   RN_ITERS         learner iterations                      (default 8000)
//!   RN_GAMES_PER_ITER self-play games per iteration          (default 256)
//!   RN_ETA           regularization strength η               (default 0.3)
//!   RN_LR            learning rate                           (default 3e-5)
//!   RN_ETA_FINAL     η at the end of the anneal              (default RN_ETA/4)
//!   RN_LR_FINAL      lr at the end of the anneal             (default RN_LR/10)
//!   RN_ANNEAL_START  fraction of iters before annealing      (default 0.65)
//!   RN_REG_EVERY     learner steps per π_reg refresh         (default 500)
//!   RN_VALUE_WEIGHT  value-loss weight (payoffs are ±~10:
//!                    0.25 keeps the value MSE from dominating
//!                    the shared trunk)                        (default 0.25)
//!   RN_EVAL_EVERY    iters between evals                     (default 100)
//!   RN_EVAL_GAMES    games per eval condition per trick count (default 500)
//!   RN_EVAL_TRICKS   comma-separated eval trick counts       (default 2,5,8)
//!   RN_EVAL_TEMP     greedy-LM eval temperature              (default 0.05)
//!   RN_CKPT_EVERY    iters between checkpoints               (default 500)
//!   RN_CKPT_DIR      checkpoint dir (default /home/steven/card_platypus/gomcts/oh_hell/rnad)
//!   RN_SEED          base RNG seed                           (default 0)

use card_platypus::algorithms::{
    gomcts_transformer::{
        eval_vs_random_batched_tch_infer, head_to_head_eval_batched_tch_infer,
        oh_hell::OhHellTokenizer, parse_env as parse, parse_env_path, ActionTokenFn,
        GoMctsTransformerTch, InferenceMode, SnapshotTch, Tokenizer, TransformerConfig,
    },
    rnad::{collect_rnad_games_batched_tch, RnadConfig, RnadTrainer},
};
use games::gamestates::oh_hell::{OhHell, OhHellGameState};
use rand::{rngs::StdRng, SeedableRng};
use std::{path::PathBuf, time::Instant};

fn main() {
    let init: PathBuf = parse_env_path(
        "RN_INIT",
        "/home/steven/card_platypus/gomcts/oh_hell/bootstrap_v2.safetensors",
    );
    let outcome_head = std::env::var("RN_VHEAD").as_deref().unwrap_or("scalar") == "outcome";
    assert!(!outcome_head, "Oh Hell payoffs are not a small discrete set; scalar head only");
    let players: usize = parse("RN_PLAYERS", 3);
    let tricks_min: u8 = parse("RN_TRICKS_MIN", 1);
    let tricks_max: u8 = parse("RN_TRICKS_MAX", 10);
    let iters: usize = parse("RN_ITERS", 8000);
    let games_per_iter: usize = parse("RN_GAMES_PER_ITER", 256);
    let eval_every: usize = parse("RN_EVAL_EVERY", 100);
    let eval_games: usize = parse("RN_EVAL_GAMES", 500);
    let eval_temp: f64 = parse("RN_EVAL_TEMP", 0.05);
    let eval_tricks: Vec<u8> = std::env::var("RN_EVAL_TRICKS")
        .unwrap_or_else(|_| "2,5,8".to_string())
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();
    let ckpt_every: usize = parse("RN_CKPT_EVERY", 500);
    let ckpt_dir = parse_env_path("RN_CKPT_DIR", "/home/steven/card_platypus/gomcts/oh_hell/rnad");
    let base_seed: u64 = parse("RN_SEED", 0);
    let rnad_cfg = RnadConfig {
        eta: parse("RN_ETA", 0.3),
        lr: parse("RN_LR", 3e-5),
        value_weight: parse("RN_VALUE_WEIGHT", 0.25),
        neurd_clip: parse("RN_NEURD_CLIP", 2.0),
        is_clip: parse("RN_IS_CLIP", 10.0),
        reg_update_every: parse("RN_REG_EVERY", 500),
        minibatch_steps: parse("RN_MINIBATCH", 1024),
        spread_penalty: true,
        ..Default::default()
    };
    std::fs::create_dir_all(&ckpt_dir).expect("create ckpt dir");

    let device = tch::Device::cuda_if_available();
    let tokenizer = OhHellTokenizer;
    let cfg = TransformerConfig::from_env(
        "RN_CONFIG",
        "paper",
        OhHellTokenizer::VOCAB_SIZE,
        OhHellTokenizer::MAX_CONTEXT,
    );
    let n_trick_opts = (tricks_max - tricks_min + 1) as usize;
    println!(
        "OhHell R-NaD: players={players}, tricks={tricks_min}..={tricks_max}, iters={iters}, \
         games/iter={games_per_iter}, eta={}, lr={}, reg_every={}, value_weight={}, \
         spread_penalty=true, device={device:?}",
        rnad_cfg.eta, rnad_cfg.lr, rnad_cfg.reg_update_every, rnad_cfg.value_weight,
    );

    let mut net = GoMctsTransformerTch::new(cfg, device).expect("build net");
    let warm_started = init.as_os_str().len() > 0 && init.exists();
    if warm_started {
        net.load_safetensors(&init).expect("load warm-start weights");
        println!("warm-started from {}", init.display());
    } else {
        println!("training from scratch (no RN_INIT found)");
    }
    let init_ref = SnapshotTch::from_model(&net)
        .expect("snapshot init")
        .hydrate(device)
        .expect("hydrate init ref");

    let mut trainer: RnadTrainer<OhHellGameState, _> =
        RnadTrainer::new(net, tokenizer, rnad_cfg).expect("trainer");
    let atf: ActionTokenFn = std::sync::Arc::new(move |a| tokenizer.action_token(a));
    let mut rng: StdRng = SeedableRng::seed_from_u64(base_seed);

    // Per-trick-count evals: greedy-LM vs 2 random seats, and greedy-LM
    // h2h vs the frozen warm start (coarse: with 3 players the h2h
    // helper mixes 2-vs-1 seatings, but it tracks relative movement).
    let eval_all = |net: &GoMctsTransformerTch, seed: u64, iter: usize| {
        for &t in &eval_tricks {
            let new_state = move || OhHell::new_state(players, t as usize);
            let (vs_rand, vs_rand_se) = eval_vs_random_batched_tch_infer::<OhHellGameState, _, _>(
                net,
                &tokenizer,
                new_state,
                eval_games,
                seed.wrapping_add(t as u64),
                false,
                1,
                InferenceMode::LmSoftmax,
                0.0,
                Some(atf.clone()),
                eval_temp,
            );
            let (vs_init, _) = head_to_head_eval_batched_tch_infer::<OhHellGameState, _, _>(
                net,
                &init_ref,
                &tokenizer,
                new_state,
                eval_games,
                seed.wrapping_add(31 + t as u64),
                false,
                1,
                InferenceMode::LmSoftmax,
                0.0,
                Some(atf.clone()),
                eval_temp,
            );
            println!(
                "iter {iter:>5} t={t:>2}  vs_random={vs_rand:+.4}±{vs_rand_se:.4}  \
                 vs_init={vs_init:+.4}"
            );
            println!(
                "kestrel: step={iter} t{t}_vs_random={vs_rand:.6} t{t}_vs_init={vs_init:.6}"
            );
        }
    };

    eval_all(&trainer.net, base_seed.wrapping_add(999_983), 0);

    let eta0 = rnad_cfg.eta;
    let lr0 = rnad_cfg.lr;
    let eta_final: f64 = parse("RN_ETA_FINAL", eta0 / 4.0);
    let lr_final: f64 = parse("RN_LR_FINAL", lr0 / 10.0);
    let anneal_start: f64 = parse("RN_ANNEAL_START", 0.65);

    let t_start = Instant::now();
    for iter in 1..=iters {
        let t0 = Instant::now();
        let progress = iter as f64 / iters as f64;
        if progress > anneal_start {
            let f = ((progress - anneal_start) / (1.0 - anneal_start)).clamp(0.0, 1.0);
            trainer.set_eta(eta0 + (eta_final - eta0) * f);
            trainer.set_lr(lr0 + (lr_final - lr0) * f);
        }
        // Cycle trick counts across the batch so every iteration covers
        // the full 1–10 range (higher counts contribute proportionally
        // more decision steps, which matches their bigger state space).
        let trajs = collect_rnad_games_batched_tch::<_, _, _>(
            &trainer.net,
            &tokenizer,
            move |game_idx| {
                OhHell::new_state(players, (tricks_min as usize) + game_idx % n_trick_opts)
            },
            games_per_iter,
            base_seed.wrapping_add(1 + iter as u64 * games_per_iter as u64),
            atf.clone(),
            false,
            1,
        );
        let stats = trainer.learner_step(&trajs, &mut rng).expect("learner step");
        let secs = t0.elapsed().as_secs_f64();
        println!(
            "kestrel: step={iter} policy_loss={:.6} value_loss={:.6} mean_abs_adv={:.6} \
             entropy={:.6} kl_reg={:.6} eta={:.6} steps={} secs={:.4}",
            stats.policy_loss,
            stats.value_loss,
            stats.mean_abs_adv,
            stats.mean_entropy,
            stats.mean_kl_reg,
            trainer.cfg.eta,
            stats.n_steps,
            secs,
        );
        if let Some(kl_outer) = stats.kl_outer {
            println!("iter {iter:>5}  π_reg refresh: kl_outer={kl_outer:.5}");
            println!("kestrel: step={iter} kl_outer={kl_outer:.6}");
        }

        if iter % eval_every == 0 || iter == iters {
            eval_all(&trainer.net, base_seed.wrapping_add(7_000_000 + iter as u64), iter);
        }
        if iter % ckpt_every == 0 || iter == iters {
            let path = ckpt_dir.join(format!("rnad_iter_{iter:05}.safetensors"));
            trainer.net.save_safetensors(&path).expect("save checkpoint");
        }
    }
    let final_path = ckpt_dir.join("rnad_final.safetensors");
    trainer.net.save_safetensors(&final_path).expect("save final");
    println!(
        "done in {:.1}s; final checkpoint: {}",
        t_start.elapsed().as_secs_f64(),
        final_path.display()
    );
}
