//! R-NaD convergence validation on Kuhn Poker.
//!
//! Kuhn has a known Nash equilibrium and a tractable exact best
//! response, so `algorithms::exploitability` (NashConv) gives a hard
//! correctness signal for the R-NaD implementation: NashConv must trend
//! toward 0. Reference points: uniform random = 11/12 ≈ 0.917,
//! exact Nash = 0.
//!
//! Run:
//!   cargo run -p card_platypus --release --example kuhn_rnad_train
//!
//! Knobs (env vars):
//!   RN_ITERS              learner iterations                  (default 600)
//!   RN_GAMES_PER_ITER     self-play games per iteration       (default 256)
//!   RN_ETA                regularization strength η           (default 0.2)
//!   RN_LR                 learning rate                       (default 5e-4)
//!   RN_REG_EVERY          learner steps per π_reg refresh     (default 50)
//!   RN_VALUE_WEIGHT       value-loss weight                   (default 1.0)
//!   RN_NEURD_CLIP         NeuRD logit threshold β             (default 2.0)
//!   RN_IS_CLIP            importance-weight cap               (default 10.0)
//!   RN_EVAL_EVERY         iterations between NashConv evals   (default 25)
//!   RN_SEED               base RNG seed                       (default 0)
//!   RN_CKPT_DIR           checkpoint dir          (default /tmp/kuhn_rnad)

use card_platypus::algorithms::{
    exploitability::exploitability,
    gomcts_transformer::{
        kuhn::KuhnTokenizer, parse_env as parse, parse_env_path, ActionTokenFn,
        GoMctsTransformerTch, Tokenizer, TransformerConfig,
    },
    rnad::{collect_rnad_games_batched_tch, RnadConfig, RnadNetPolicy, RnadTrainer},
};
use games::gamestates::kuhn_poker::{KPGameState, KuhnPoker};
use rand::{rngs::StdRng, SeedableRng};
use std::time::Instant;

fn main() {
    let iters: usize = parse("RN_ITERS", 600);
    let games_per_iter: usize = parse("RN_GAMES_PER_ITER", 256);
    let eval_every: usize = parse("RN_EVAL_EVERY", 25);
    let base_seed: u64 = parse("RN_SEED", 0);
    let ckpt_dir = parse_env_path("RN_CKPT_DIR", "/tmp/kuhn_rnad");
    let rnad_cfg = RnadConfig {
        eta: parse("RN_ETA", 0.2),
        lr: parse("RN_LR", 5e-4),
        value_weight: parse("RN_VALUE_WEIGHT", 1.0),
        neurd_clip: parse("RN_NEURD_CLIP", 2.0),
        is_clip: parse("RN_IS_CLIP", 10.0),
        reg_update_every: parse("RN_REG_EVERY", 50),
        minibatch_steps: 1024,
        ..Default::default()
    };
    std::fs::create_dir_all(&ckpt_dir).expect("create ckpt dir");

    let device = tch::Device::cuda_if_available();
    let tokenizer = KuhnTokenizer;
    let cfg = TransformerConfig::kuhn_small(KuhnTokenizer::VOCAB_SIZE, KuhnTokenizer::MAX_CONTEXT);
    println!(
        "Kuhn R-NaD: iters={iters}, games/iter={games_per_iter}, eta={}, lr={}, reg_every={}, \
         neurd_clip={}, is_clip={}, device={device:?}",
        rnad_cfg.eta, rnad_cfg.lr, rnad_cfg.reg_update_every, rnad_cfg.neurd_clip, rnad_cfg.is_clip,
    );

    let net = GoMctsTransformerTch::new(cfg, device).expect("build net");
    let mut trainer: RnadTrainer<KPGameState, _> =
        RnadTrainer::new(net, tokenizer, rnad_cfg).expect("trainer");
    let atf: ActionTokenFn = std::sync::Arc::new(move |a| tokenizer.action_token(a));
    let mut rng: StdRng = SeedableRng::seed_from_u64(base_seed);

    {
        let mut policy = RnadNetPolicy::new(&trainer.net, tokenizer);
        let data = exploitability(|| (KuhnPoker::game().new)(), &mut policy);
        println!("iter 0 (random init): nash_conv={:.4}", data.nash_conv);
    }

    let t_start = Instant::now();
    for iter in 1..=iters {
        let t0 = Instant::now();
        let trajs = collect_rnad_games_batched_tch::<_, _, _>(
            &trainer.net,
            &tokenizer,
            KuhnPoker::new_state,
            games_per_iter,
            base_seed.wrapping_add(1 + iter as u64 * games_per_iter as u64),
            atf.clone(),
            false,
            1,
        );
        let stats = trainer.learner_step(&trajs, &mut rng).expect("learner step");
        let secs = t0.elapsed().as_secs_f64();

        if iter % eval_every == 0 || iter == iters {
            let mut policy = RnadNetPolicy::new(&trainer.net, tokenizer);
            let data = exploitability(|| (KuhnPoker::game().new)(), &mut policy);
            println!(
                "iter {iter:>5}  nash_conv={:.4}  ploss={:+.4}  vloss={:.4}  |adv|={:.3}  \
                 H={:.3}  KL(reg)={:.4}  steps={}  {:.2}s/iter",
                data.nash_conv,
                stats.policy_loss,
                stats.value_loss,
                stats.mean_abs_adv,
                stats.mean_entropy,
                stats.mean_kl_reg,
                stats.n_steps,
                secs,
            );
            println!(
                "kestrel: step={iter} nash_conv={:.6} policy_loss={:.6} value_loss={:.6} \
                 mean_abs_adv={:.6} entropy={:.6} kl_reg={:.6} secs={:.4}",
                data.nash_conv,
                stats.policy_loss,
                stats.value_loss,
                stats.mean_abs_adv,
                stats.mean_entropy,
                stats.mean_kl_reg,
                secs,
            );
        }
    }
    let final_path = ckpt_dir.join("final.safetensors");
    trainer.net.save_safetensors(&final_path).expect("save final");
    println!(
        "done in {:.1}s; final checkpoint: {}",
        t_start.elapsed().as_secs_f64(),
        final_path.display()
    );
}
