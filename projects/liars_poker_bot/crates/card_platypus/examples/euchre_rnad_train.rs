//! R-NaD (DeepNash-style Regularized Nash Dynamics) training for Euchre.
//!
//! Warm-starts from a behavior-cloned GO-MCTS transformer checkpoint
//! (default: the ε-bootstrap `bootstrap_combined.safetensors`, the
//! strongest checkpoint in the experiment log) and runs regularized
//! self-play policy iteration on top. The initial π_reg IS the
//! bootstrap policy, so early dynamics are KL-tethered to the BC
//! teacher; each π_reg refresh lets the policy drift one fixed-point
//! step further toward the (team-zero-sum) regularized equilibrium.
//!
//! Run:
//!   cargo run -p card_platypus --release --example euchre_rnad_train
//!
//! Knobs (env vars):
//!   RN_INIT          warm-start weights ('' = from scratch)
//!                    (default /home/steven/card_platypus/gomcts/bootstrap_combined.safetensors)
//!   RN_VHEAD         scalar | outcome — must match RN_INIT  (default outcome)
//!   RN_CONFIG        smoke | medium | paper                 (default paper)
//!   RN_ITERS         learner iterations                     (default 2000)
//!   RN_GAMES_PER_ITER self-play games per iteration         (default 256)
//!   RN_ETA           regularization strength η              (default 0.2)
//!   RN_LR            learning rate                          (default 2e-5)
//!   RN_REG_EVERY     learner steps per π_reg refresh        (default 200)
//!   RN_VALUE_WEIGHT  value-loss weight                      (default 1.0)
//!   RN_NEURD_CLIP    NeuRD logit threshold β                (default 2.0)
//!   RN_IS_CLIP       importance-weight cap                  (default 10.0)
//!   RN_MINIBATCH     learner minibatch rows                 (default 1024)
//!   RN_EVAL_EVERY    iters between (vs-random, vs-init h2h) evals (default 50)
//!   RN_EVAL_GAMES    games per eval condition               (default 500)
//!   RN_EVAL_TEMP     greedy-LM eval temperature             (default 0.05)
//!   RN_CKPT_EVERY    iters between checkpoints              (default 100)
//!   RN_CKPT_DIR      checkpoint dir (default /home/steven/card_platypus/gomcts/rnad)
//!   RN_SEED          base RNG seed                          (default 0)

use card_platypus::algorithms::{
    gomcts_transformer::{
        euchre::{EuchreTokenizer, OUTCOME_VALUES as EUCHRE_OUTCOME_VALUES},
        eval_vs_random_batched_tch_infer, head_to_head_eval_batched_tch_infer,
        parse_env as parse, parse_env_path, ActionTokenFn, GoMctsTransformerTch, InferenceMode,
        SnapshotTch, Tokenizer, TransformerConfig,
    },
    rnad::{collect_rnad_games_batched_tch, RnadConfig, RnadTrainer},
};
use games::gamestates::euchre::{Euchre, EuchreGameState};
use rand::{rngs::StdRng, SeedableRng};
use std::{path::PathBuf, time::Instant};

fn build_net(cfg: TransformerConfig, device: tch::Device, outcome: bool) -> GoMctsTransformerTch {
    if outcome {
        GoMctsTransformerTch::new_with_outcomes(cfg, device, EUCHRE_OUTCOME_VALUES.to_vec())
            .expect("build net (outcome head)")
    } else {
        GoMctsTransformerTch::new(cfg, device).expect("build net")
    }
}

fn main() {
    let init: PathBuf = parse_env_path(
        "RN_INIT",
        "/home/steven/card_platypus/gomcts/bootstrap_combined.safetensors",
    );
    let outcome_head = std::env::var("RN_VHEAD").as_deref().unwrap_or("outcome") == "outcome";
    let iters: usize = parse("RN_ITERS", 2000);
    let games_per_iter: usize = parse("RN_GAMES_PER_ITER", 256);
    let eval_every: usize = parse("RN_EVAL_EVERY", 50);
    let eval_games: usize = parse("RN_EVAL_GAMES", 500);
    let eval_temp: f64 = parse("RN_EVAL_TEMP", 0.05);
    let ckpt_every: usize = parse("RN_CKPT_EVERY", 100);
    let ckpt_dir = parse_env_path("RN_CKPT_DIR", "/home/steven/card_platypus/gomcts/rnad");
    let base_seed: u64 = parse("RN_SEED", 0);
    let rnad_cfg = RnadConfig {
        eta: parse("RN_ETA", 0.2),
        lr: parse("RN_LR", 2e-5),
        value_weight: parse("RN_VALUE_WEIGHT", 1.0),
        neurd_clip: parse("RN_NEURD_CLIP", 2.0),
        is_clip: parse("RN_IS_CLIP", 10.0),
        reg_update_every: parse("RN_REG_EVERY", 200),
        minibatch_steps: parse("RN_MINIBATCH", 1024),
        ..Default::default()
    };
    std::fs::create_dir_all(&ckpt_dir).expect("create ckpt dir");

    let device = tch::Device::cuda_if_available();
    let tokenizer = EuchreTokenizer;
    let cfg = TransformerConfig::from_env(
        "RN_CONFIG",
        "paper",
        EuchreTokenizer::VOCAB_SIZE,
        EuchreTokenizer::MAX_CONTEXT,
    );
    println!(
        "Euchre R-NaD: iters={iters}, games/iter={games_per_iter}, eta={}, lr={}, reg_every={}, \
         neurd_clip={}, is_clip={}, minibatch={}, vhead={}, device={device:?}",
        rnad_cfg.eta,
        rnad_cfg.lr,
        rnad_cfg.reg_update_every,
        rnad_cfg.neurd_clip,
        rnad_cfg.is_clip,
        rnad_cfg.minibatch_steps,
        if outcome_head { "outcome" } else { "scalar" },
    );
    println!(
        "transformer: d={}, layers={}, heads={}, d_ff={}; init={}",
        cfg.d_model,
        cfg.n_layers,
        cfg.n_heads,
        cfg.d_ff,
        init.display()
    );

    let mut net = build_net(cfg, device, outcome_head);
    let warm_started = init.as_os_str().len() > 0 && init.exists();
    if warm_started {
        net.load_safetensors(&init).expect("load warm-start weights");
        println!("warm-started from {}", init.display());
    } else {
        println!("training from scratch (no RN_INIT found)");
    }
    // Frozen copy of the starting point — the h2h reference that tells
    // us whether R-NaD ever exceeds the bootstrap ceiling.
    let init_ref = SnapshotTch::from_model(&net)
        .expect("snapshot init")
        .hydrate(device)
        .expect("hydrate init ref");

    let mut trainer: RnadTrainer<EuchreGameState, _> =
        RnadTrainer::new(net, tokenizer, rnad_cfg).expect("trainer");
    let atf: ActionTokenFn = std::sync::Arc::new(move |a| tokenizer.action_token(a));
    let mut rng: StdRng = SeedableRng::seed_from_u64(base_seed);

    let eval_all = |net: &GoMctsTransformerTch, seed: u64| -> (f64, f64, f64, f64) {
        let (vs_rand, vs_rand_se) = eval_vs_random_batched_tch_infer::<EuchreGameState, _, _>(
            net,
            &tokenizer,
            Euchre::new_state,
            eval_games,
            seed,
            false,
            1,
            InferenceMode::LmSoftmax,
            0.0,
            Some(atf.clone()),
            eval_temp,
        );
        let (vs_init, vs_init_wr) = head_to_head_eval_batched_tch_infer::<EuchreGameState, _, _>(
            net,
            &init_ref,
            &tokenizer,
            Euchre::new_state,
            eval_games,
            seed.wrapping_add(31),
            false,
            1,
            InferenceMode::LmSoftmax,
            0.0,
            Some(atf.clone()),
            eval_temp,
        );
        (vs_rand, vs_rand_se, vs_init, vs_init_wr)
    };

    let (r0, r0_se, _, _) = eval_all(&trainer.net, base_seed.wrapping_add(999_983));
    println!("iter 0: vs_random={r0:+.4}±{r0_se:.4} (greedy-LM temp={eval_temp})");

    let t_start = Instant::now();
    for iter in 1..=iters {
        let t0 = Instant::now();
        let trajs = collect_rnad_games_batched_tch::<_, _, _>(
            &trainer.net,
            &tokenizer,
            Euchre::new_state,
            games_per_iter,
            base_seed.wrapping_add(1 + iter as u64 * games_per_iter as u64),
            atf.clone(),
            false,
            1,
        );
        let n_games = trajs.len();
        let stats = trainer.learner_step(&trajs, &mut rng).expect("learner step");
        let secs = t0.elapsed().as_secs_f64();
        println!(
            "kestrel: step={iter} policy_loss={:.6} value_loss={:.6} mean_abs_adv={:.6} \
             entropy={:.6} kl_reg={:.6} steps={} games={} secs={:.4}",
            stats.policy_loss,
            stats.value_loss,
            stats.mean_abs_adv,
            stats.mean_entropy,
            stats.mean_kl_reg,
            stats.n_steps,
            n_games,
            secs,
        );

        if iter % eval_every == 0 || iter == iters {
            let (vs_rand, vs_rand_se, vs_init, vs_init_wr) =
                eval_all(&trainer.net, base_seed.wrapping_add(7_000_000 + iter as u64));
            println!(
                "iter {iter:>5}  vs_random={vs_rand:+.4}±{vs_rand_se:.4}  \
                 vs_init={vs_init:+.4} (wr={vs_init_wr:.3})  KL(reg)={:.4}  H={:.3}",
                stats.mean_kl_reg, stats.mean_entropy,
            );
            println!(
                "kestrel: step={iter} eval_vs_random={vs_rand:.6} eval_vs_random_se={vs_rand_se:.6} \
                 eval_vs_init={vs_init:.6} eval_vs_init_wr={vs_init_wr:.6}"
            );
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
