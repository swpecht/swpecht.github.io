//! Sweep the training mini-batch size to find the GPU sweet spot (tch).
//!
//! Builds a fresh paper-config model + a synthetic dataset, then runs
//! `train_tch()` for a fixed number of steps at each batch_size; reports
//! steps/sec, examples/sec, and per-step latency.
//!
//! Run:
//!   cargo run -p card_platypus --release --example euchre_gomcts_train_batch_bench
//!
//! Knobs:
//!   EU_CONFIG          smoke|medium|paper   (default paper)
//!   EU_BATCH_SET       comma-sep list       (default 64,128,256,512,1024)
//!   EU_STEPS           gradient steps per condition (default 200)
//!   EU_LR              learning rate        (default 5e-4)
//!   EU_DATASET_SIZE    synthetic examples   (default 8192)

use card_platypus::algorithms::gomcts_transformer::{
    euchre::EuchreTokenizer, train_tch, GoMctsTransformerTch, TrainExample, TransformerConfig,
};
use games::{gamestates::euchre::Euchre, GameState};
use rand::{rngs::StdRng, seq::IndexedRandom, RngExt, SeedableRng};
use std::time::Instant;

fn parse<T: std::str::FromStr>(name: &str, default: T) -> T {
    std::env::var(name).ok().and_then(|s| s.parse().ok()).unwrap_or(default)
}

fn parse_batch_set() -> Vec<usize> {
    let raw = std::env::var("EU_BATCH_SET").unwrap_or_else(|_| "64,128,256,512,1024".to_string());
    raw.split(',').filter_map(|s| s.trim().parse().ok()).collect()
}

fn pick_config() -> TransformerConfig {
    let v = EuchreTokenizer::VOCAB_SIZE;
    let c = EuchreTokenizer::MAX_CONTEXT;
    match std::env::var("EU_CONFIG").as_deref() {
        Ok("smoke") => TransformerConfig::euchre_smoke(v, c),
        Ok("medium") => TransformerConfig::euchre_medium(v, c),
        _ => TransformerConfig::paper_default(v, c),
    }
}

fn main() {
    let n_steps: usize = parse("EU_STEPS", 200);
    let lr: f64 = parse("EU_LR", 5e-4);
    let dataset_size: usize = parse("EU_DATASET_SIZE", 8192);
    let batch_set = parse_batch_set();

    let device = tch::Device::cuda_if_available();
    let cfg = pick_config();
    let tokenizer = EuchreTokenizer;
    println!(
        "Train batch-size bench: device={:?}, steps_per_condition={}, dataset_size={}, lr={}, \
         config: d={}, layers={}, heads={}, ff={}, vocab={}, ctx={}",
        device,
        n_steps,
        dataset_size,
        lr,
        cfg.d_model,
        cfg.n_layers,
        cfg.n_heads,
        cfg.d_ff,
        cfg.vocab_size,
        cfg.max_context,
    );

    let dataset = generate_synthetic_dataset(dataset_size);
    println!("dataset: {} examples", dataset.len());

    println!(
        "{:>12} {:>10} {:>13} {:>16} {:>14} {:>10}",
        "batch_size", "secs", "steps/sec", "examples/sec", "ms/step", "speedup"
    );

    let mut baseline_examples_per_sec: Option<f64> = None;
    for batch_size in batch_set.iter() {
        let mut net = GoMctsTransformerTch::new(cfg, device).expect("build");
        let steps_per_epoch = (dataset.len() + batch_size - 1) / batch_size;
        let epochs = ((n_steps + steps_per_epoch - 1) / steps_per_epoch).max(1);
        let actual_steps = steps_per_epoch * epochs;
        let mut rng: StdRng = SeedableRng::seed_from_u64(7);
        let t0 = Instant::now();
        let _loss = train_tch(&mut net, &tokenizer, &dataset, epochs, *batch_size, lr, &mut rng)
            .expect("train");
        let secs = t0.elapsed().as_secs_f64();
        let steps_per_sec = actual_steps as f64 / secs;
        let examples_per_sec = (actual_steps * batch_size) as f64 / secs;
        let ms_per_step = 1000.0 * secs / actual_steps as f64;
        let speedup = match baseline_examples_per_sec {
            Some(b) => examples_per_sec / b,
            None => {
                baseline_examples_per_sec = Some(examples_per_sec);
                1.0
            }
        };
        println!(
            "{:>12} {:>10.2} {:>13.2} {:>16.0} {:>14.2} {:>9.2}x",
            batch_size, secs, steps_per_sec, examples_per_sec, ms_per_step, speedup
        );
        println!(
            "kestrel: step={} batch_size={} secs={:.4} steps_per_sec={:.4} \
             examples_per_sec={:.4} ms_per_step={:.4} speedup={:.4}",
            batch_size, batch_size, secs, steps_per_sec, examples_per_sec, ms_per_step, speedup
        );
    }
}

fn generate_synthetic_dataset(n: usize) -> Vec<TrainExample> {
    let mut rng: StdRng = SeedableRng::seed_from_u64(123);
    let mut out = Vec::with_capacity(n);
    let mut acts_buf = Vec::new();
    while out.len() < n {
        let mut gs = Euchre::new_state();
        while gs.is_chance_node() {
            acts_buf.clear();
            gs.legal_actions(&mut acts_buf);
            let a = *acts_buf.choose(&mut rng).unwrap();
            gs.apply_action(a);
        }
        while !gs.is_terminal() && out.len() < n {
            let p = gs.cur_player();
            let h = gs.istate_key(p);
            acts_buf.clear();
            gs.legal_actions(&mut acts_buf);
            let a = *acts_buf.choose(&mut rng).unwrap();
            let mut soft: Vec<(games::Action, f32)> = acts_buf
                .iter()
                .map(|x| (*x, if *x == a { 0.8 } else { 0.2 / (acts_buf.len() - 1).max(1) as f32 }))
                .collect();
            let s: f32 = soft.iter().map(|(_, p)| *p).sum();
            for (_, p) in soft.iter_mut() {
                *p /= s;
            }
            let value: f32 = rng.random::<f32>() * 4.0 - 2.0;
            out.push(TrainExample::soft(h, a, value, soft));
            gs.apply_action(a);
        }
    }
    out.truncate(n);
    out
}
