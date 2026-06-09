//! Spike comparing the candle `GoMctsTransformer` against a freshly
//! ported `GoMctsTransformerTch`. Loads the same safetensors weights
//! into both, runs identical input batches, reports numerical match
//! and per-batch latency / throughput at a sweep of batch sizes.
//!
//! Build:
//!   cargo run -p card_platypus --release \
//!       --features "tch_spike gpu_cuda" \
//!       --example tch_vs_candle_spike
//!
//! Defaults to the paper-config size (d=256, 8 layers). Override via
//! env vars: EU_CANDLE_DEVICE, TCH_DEVICE (cuda|cpu), EU_WEIGHTS,
//! BATCH_SIZES (comma-separated).

use anyhow::{Context, Result};
use card_platypus::algorithms::gomcts_transformer::{
    GoMctsTransformer, TransformerConfig,
};
use card_platypus::algorithms::gomcts_transformer_tch::GoMctsTransformerTch;
#[allow(unused_imports)]
use card_platypus::algorithms::gomcts_transformer_tch::ForwardGraph;
use candle_core::{Device as CDevice, Tensor as CTensor};
use rand::SeedableRng;
use rand::rngs::StdRng;
use rand::RngExt;
use std::time::Instant;
use tch::{Device as TDevice, Kind, Tensor as TTensor};

fn env_str(k: &str, default: &str) -> String {
    std::env::var(k).unwrap_or_else(|_| default.to_string())
}
fn env_usize(k: &str, default: usize) -> usize {
    std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn candle_device(name: &str) -> Result<CDevice> {
    match name {
        "cpu" => Ok(CDevice::Cpu),
        "cuda" => {
            #[cfg(feature = "gpu_cuda")]
            { return Ok(CDevice::new_cuda(0)?); }
            #[cfg(not(feature = "gpu_cuda"))]
            { anyhow::bail!("built without gpu_cuda feature"); }
        }
        s => anyhow::bail!("unknown candle device: {s}"),
    }
}

fn tch_device(name: &str) -> TDevice {
    match name {
        "cpu" => TDevice::Cpu,
        "cuda" => TDevice::Cuda(0),
        _ => panic!("unknown tch device {name}"),
    }
}

fn max_abs_diff(a: &[f32], b: &[f32]) -> (f32, f32, f32) {
    assert_eq!(a.len(), b.len());
    let mut max = 0.0f32;
    let mut sum = 0.0f64;
    let mut sumsq = 0.0f64;
    for (&x, &y) in a.iter().zip(b.iter()) {
        let d = (x - y).abs();
        if d > max { max = d; }
        sum += d as f64;
        sumsq += (d * d) as f64;
    }
    let mean = (sum / a.len() as f64) as f32;
    let rms = (sumsq / a.len() as f64).sqrt() as f32;
    (max, mean, rms)
}

fn main() -> Result<()> {
    let cfg = TransformerConfig::paper_default(33, 48);
    let weights_path = env_str(
        "EU_WEIGHTS",
        "/tmp/euchre_gomcts_bootstrap/random_bootstrap.safetensors",
    );
    let candle_name = env_str("EU_CANDLE_DEVICE", "cpu");
    let tch_name = env_str("TCH_DEVICE", "cpu");
    let batch_sizes: Vec<usize> = env_str("BATCH_SIZES", "1,32,256,1024")
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();
    let warmup = env_usize("WARMUP", 5);
    let iters = env_usize("ITERS", 30);

    println!("tch-vs-candle spike");
    println!("  weights:   {weights_path}");
    println!("  cfg:       vocab={} max_ctx={} d={} layers={} heads={} d_ff={}",
        cfg.vocab_size, cfg.max_context, cfg.d_model, cfg.n_layers, cfg.n_heads, cfg.d_ff);
    println!("  candle dev:{candle_name}");
    println!("  tch dev:   {tch_name}");
    println!("  batches:   {batch_sizes:?}");
    println!("  warmup:    {warmup}");
    println!("  iters:     {iters}");

    // ---------- Build & load ----------
    let cd = candle_device(&candle_name)?;
    let mut candle_net = GoMctsTransformer::new(cfg, cd.clone())
        .context("build candle net")?;
    candle_net.load(&weights_path).context("load candle weights")?;

    let td = tch_device(&tch_name);
    let mut tch_net = GoMctsTransformerTch::new(cfg, td)
        .context("build tch net")?;
    tch_net.load_safetensors(std::path::Path::new(&weights_path))
        .context("load tch weights")?;

    println!("\n--- Numerical parity check (B=8, T={max_ctx}) ---", max_ctx = cfg.max_context);

    let mut rng = StdRng::seed_from_u64(0xDEAD_BEEF);
    let b: usize = 8;
    let t: usize = cfg.max_context;
    let tokens: Vec<u32> = (0..(b * t))
        .map(|_| rng.random_range(0..cfg.vocab_size as u32))
        .collect();

    // Candle forward
    let ct = CTensor::from_vec(tokens.clone(), (b, t), &cd)?;
    let (cl, cv) = candle_net.forward(&ct)?;
    let cl_vec: Vec<f32> = cl.flatten_all()?.to_vec1()?;
    let cv_vec: Vec<f32> = cv.flatten_all()?.to_vec1()?;

    // Tch forward
    let tokens_i64: Vec<i64> = tokens.iter().map(|&u| u as i64).collect();
    let tt = TTensor::from_slice(&tokens_i64)
        .reshape([b as i64, t as i64])
        .to_device(td);
    let (tl, tv) = tch::no_grad(|| tch_net.forward(&tt));
    let tl_vec: Vec<f32> = Vec::<f32>::try_from(tl.flatten(0, -1).to_kind(Kind::Float))?;
    let tv_vec: Vec<f32> = Vec::<f32>::try_from(tv.flatten(0, -1).to_kind(Kind::Float))?;

    let (lm_max, lm_mean, lm_rms) = max_abs_diff(&cl_vec, &tl_vec);
    let (v_max, v_mean, v_rms) = max_abs_diff(&cv_vec, &tv_vec);
    println!("  lm_logits diff:  max={lm_max:.4e}  mean={lm_mean:.4e}  rms={lm_rms:.4e}");
    println!("  value     diff:  max={v_max:.4e}  mean={v_mean:.4e}  rms={v_rms:.4e}");
    let max_abs = lm_max.max(v_max);
    let tol = 5e-3f32; // fp32 transformer should match well under this
    if max_abs <= tol {
        println!("  PARITY OK  (max abs diff {max_abs:.2e} <= {tol:.0e})");
    } else {
        println!("  PARITY FAIL (max abs diff {max_abs:.2e} > {tol:.0e})");
    }

    // ---------- Latency sweep ----------
    println!("\n--- Latency sweep ---");
    println!("                       candle (ms)            tch (ms)         speedup");
    println!("  B    T   ────  warm   p50   p90   mean   warm   p50   p90   mean   tch/candle");
    for &bs in &batch_sizes {
        let toks: Vec<u32> = (0..(bs * t))
            .map(|_| rng.random_range(0..cfg.vocab_size as u32))
            .collect();
        let ct = CTensor::from_vec(toks.clone(), (bs, t), &cd)?;
        let toks_i64: Vec<i64> = toks.iter().map(|&u| u as i64).collect();
        let tt = TTensor::from_slice(&toks_i64)
            .reshape([bs as i64, t as i64])
            .to_device(td);

        // warmup
        for _ in 0..warmup {
            let (l, v) = candle_net.forward(&ct)?;
            let _ = l.flatten_all()?.to_vec1::<f32>()?;
            let _ = v.flatten_all()?.to_vec1::<f32>()?;
        }
        for _ in 0..warmup {
            let _ = tch::no_grad(|| tch_net.forward(&tt));
            if matches!(td, TDevice::Cuda(_)) { tch::Cuda::synchronize(0); }
        }

        // candle
        let mut c_us = Vec::with_capacity(iters);
        for _ in 0..iters {
            let s = Instant::now();
            let (l, v) = candle_net.forward(&ct)?;
            // Synchronise by copying back to host (candle is lazy on CUDA);
            // this is the same pattern self-play uses on every inference.
            let _ = l.flatten_all()?.to_vec1::<f32>()?;
            let _ = v.flatten_all()?.to_vec1::<f32>()?;
            c_us.push(s.elapsed().as_micros() as u64);
        }
        // tch
        let mut t_us = Vec::with_capacity(iters);
        for _ in 0..iters {
            let s = Instant::now();
            let (l, v) = tch::no_grad(|| tch_net.forward(&tt));
            // Realise to host so we include the host-sync cost candle pays.
            let _: Vec<f32> = Vec::<f32>::try_from(l.flatten(0, -1).to_kind(Kind::Float))?;
            let _: Vec<f32> = Vec::<f32>::try_from(v.flatten(0, -1).to_kind(Kind::Float))?;
            t_us.push(s.elapsed().as_micros() as u64);
        }
        c_us.sort_unstable();
        t_us.sort_unstable();
        let pct = |xs: &[u64], p: f64| xs[((xs.len() as f64 - 1.0) * p) as usize] as f64 / 1000.0;
        let mean = |xs: &[u64]| xs.iter().sum::<u64>() as f64 / xs.len() as f64 / 1000.0;
        let c_p50 = pct(&c_us, 0.50);
        let c_p90 = pct(&c_us, 0.90);
        let c_mean = mean(&c_us);
        let t_p50 = pct(&t_us, 0.50);
        let t_p90 = pct(&t_us, 0.90);
        let t_mean = mean(&t_us);
        let speedup = c_mean / t_mean;
        println!(
            "  {bs:4} {t:3}        {:6.2} {:6.2} {:6.2}  {:6.2} {:6.2} {:6.2}     {:5.2}x",
            c_p50, c_p90, c_mean, t_p50, t_p90, t_mean, speedup
        );
    }
    println!("\n(speedup > 1.0 means tch is faster)");

    // ---------- CUDA graph capture / replay bench ----------
    if matches!(td, TDevice::Cuda(_)) {
        println!("\n--- CUDA graph capture / replay bench ---");
        let graph_batches: Vec<i64> = std::env::var("GRAPH_BATCHES")
            .unwrap_or_else(|_| "32,128,512".to_string())
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();
        for &gb in &graph_batches {
            // Capture (this also switches the thread onto a pooled
            // stream — subsequent eager ops in this main thread will
            // run on that stream, which is fine for a bench).
            let cap_t0 = Instant::now();
            let graph = tch_net.capture_forward(gb);
            let cap_ms = cap_t0.elapsed().as_secs_f64() * 1000.0;

            // Build fresh tokens for this size and copy into the
            // static input buffer. Then time pure replay + readback.
            let toks: Vec<i64> = (0..(gb * t as i64))
                .map(|_| rng.random_range(0..cfg.vocab_size as u32) as i64)
                .collect();
            let cpu = TTensor::from_slice(&toks).reshape([gb, t as i64]);
            graph
                .static_in
                .shallow_clone()
                .copy_(&cpu.to_device(td));

            // Verify graph output matches eager forward for the same
            // input — flags any capture-time silent allocation issue.
            let eager_input = graph.static_in.shallow_clone();
            let (eager_lm, eager_val) = tch::no_grad(|| tch_net.forward(&eager_input));
            let pretty_replay_us: Vec<u64> = {
                let mut out = Vec::with_capacity(iters);
                for _ in 0..warmup {
                    graph.replay();
                    tch::Cuda::synchronize(0);
                }
                for _ in 0..iters {
                    let s = Instant::now();
                    graph.replay();
                    tch::Cuda::synchronize(0);
                    out.push(s.elapsed().as_micros() as u64);
                }
                out
            };
            let mut replay_us = pretty_replay_us;
            replay_us.sort_unstable();
            let p50 = replay_us[replay_us.len() / 2] as f64 / 1000.0;
            let p90 = replay_us[(replay_us.len() as f64 * 0.9) as usize] as f64 / 1000.0;
            let mean = replay_us.iter().sum::<u64>() as f64 / replay_us.len() as f64 / 1000.0;
            let parity_lm = (&graph.static_lm - &eager_lm).abs().max();
            let parity_val = (&graph.static_val - &eager_val).abs().max();
            let parity_max = parity_lm
                .double_value(&[])
                .max(parity_val.double_value(&[]));
            println!(
                "  B={gb:4} T={t:3}  capture={cap_ms:6.1}ms  replay p50={p50:6.2}ms p90={p90:6.2}ms mean={mean:6.2}ms  parity_max_abs={parity_max:.2e}"
            );
        }
    }

    Ok(())
}

