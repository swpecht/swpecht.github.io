//! Continue training a Euchre CFR agent (default: cfr3 / 3-cards-played)
//! and stream Kestrel convergence metrics. Modelled on
//! `oh_hell_cfr_train.rs`: per-checkpoint we report
//!
//!   * `pimcts_avg` — mean per-hand score for the CFR-controlled team
//!     when matched up against PIMCTS, with seats rotated across eval
//!     games so seat / dealer bias washes out.
//!   * `win_rate` / `tie_rate` / `loss_rate` — same eval, but match-
//!     level. A "win" is a hand the CFR team scored more points on.
//!   * `policy_delta_l1` — mean L1 distance between the current and
//!     previous normalized avg strategy over a fixed sample of PHF
//!     slots (seeded for reproducibility). Trends to zero as the
//!     average policy stops moving, which is the convergence signal
//!     the user wanted to see.
//!   * `rss_mb` / `peak_rss_mb` / `vsize_mb` / `bytes_per_istate` —
//!     resource snapshots from `/proc/self/status`.
//!
//! Two independent Kestrel x-axes per checkpoint:
//!   * `step=<iteration>` carries the training-quality + convergence
//!     metrics.
//!   * `t=<elapsed_secs>` carries `progress_pct` for wall-clock vs
//!     training-progress charting.
//!
//! Picks up from existing weights at `CFR_WEIGHT_DIR` (the same dir
//! the deployment uses) and saves after every checkpoint, so a long
//! run can be interrupted and resumed.
//!
//! Reporting is *time-based*, not iteration-based — for cfr3 the 91 GB
//! mmap is heavily disk-bound (training rate observed at ~3h per 5M
//! iters), so a fixed 5%-of-total-iters cadence gives single-digit
//! checkpoints over an entire weekend. Instead we train in small inner
//! chunks (`CFR_INNER_CHUNK`, default 25 000 iters) and emit a report
//! whenever `CFR_REPORT_SECS` of wall-clock have passed since the last
//! one. The total budget is still capped by `CFR_ITERS` (default very
//! large) and `CFR_MAX_SECS` (default 24 h) — whichever fires first.
//!
//! Knobs (env vars):
//!   CFR_WEIGHT_DIR     where to load/save weights
//!                      (default /home/steven/card_platypus/infostate.three_card_played_f32)
//!   CFR_MAX_CARDS      EuchreDepthChecker.max_cards_played (default 3)
//!   CFR_ITERS          hard cap on CFR iterations (default 1_000_000_000)
//!   CFR_MAX_SECS       hard wall-clock cap in seconds (default 86_400 = 24 h)
//!   CFR_REPORT_SECS    emit a checkpoint roughly every this many wall-clock
//!                      seconds of training (default 1_800 = 30 min)
//!   CFR_INNER_CHUNK    iters per inner train() call (default 25_000); time
//!                      is checked between inner chunks, so smaller =
//!                      finer report-cadence resolution but more
//!                      overhead from extra eval-side calls.
//!   CFR_EVAL_HANDS     eval hands per checkpoint, CFR-vs-PIMCTS (default 100)
//!   CFR_POLICY_SAMPLE_BYTES  byte budget for L1 sampler (default 100 MB)
//!   CFR_POLICY_SAMPLE_SEED   sampler seed (default 0xC0DEC0DE)
//!
//! Run:
//!   cargo run -p card_platypus --release --example euchre_cfr_train \
//!     | kestrel-tail euchre_cfr3_resume --tag euchre --tag cfr3 --tag resume

use std::{io::Write as _, path::PathBuf, time::Instant};

use card_platypus::{
    agents::Agent,
    algorithms::{
        cfres::{self, EuchreCfres, EUCHRE_MAX_ACTIONS},
        open_hand_solver::OpenHandSolver,
        pimcts::PIMCTSBot,
    },
    diag::process_memory,
};
use games::{
    gamestates::euchre::Euchre,
    GameState,
};
use rand::{rngs::StdRng, seq::IndexedRandom, SeedableRng};

/// Sampled L1 policy-delta convergence metric — same shape as the Oh Hell
/// trainer's `PolicySampler`, adapted to Euchre's slot width (each sample
/// holds at most `EUCHRE_MAX_ACTIONS` f32 strategy entries).
///
/// Pick a fixed random subset of PHF slots at startup (seeded for
/// reproducibility across resumes), and at each checkpoint compute the
/// mean L1 distance between the current normalized avg strategy and the
/// previous snapshot, over slots that were populated at *both*
/// checkpoints. Drops to 0 as the average policy stops changing.
///
/// Default 100 MB budget covers ~2.5M slots at 40 B per sample, which
/// is well under cfr3's ~1.3 B slot space — so we're picking a fixed
/// sparse fingerprint of the policy, which is enough to spot
/// convergence trends without scanning the whole 91 GB mmap on every
/// checkpoint.
struct PolicySampler {
    indices: Vec<usize>,
    prev: Vec<Option<Vec<f32>>>,
}

impl PolicySampler {
    fn new(indexer_size: usize, budget_bytes: usize, max_actions: usize, seed: u64) -> Self {
        let per_sample =
            std::mem::size_of::<usize>() + max_actions * std::mem::size_of::<f32>();
        let max_samples = budget_bytes / per_sample.max(1);
        let n_samples = max_samples.min(indexer_size);
        let indices: Vec<usize> = if n_samples >= indexer_size {
            (0..indexer_size).collect()
        } else {
            let mut rng: StdRng = SeedableRng::seed_from_u64(seed);
            rand::seq::index::sample(&mut rng, indexer_size, n_samples).into_vec()
        };
        let prev = vec![None; indices.len()];
        Self { indices, prev }
    }

    /// Walk samples, compute mean L1 vs previous snapshot, update snapshot.
    /// Returns `(mean_l1, paired_count, populated_count)`. `mean_l1` is
    /// NaN on the first call (no previous snapshot to compare to).
    fn measure(&mut self, cfr: &EuchreCfres) -> (f64, usize, usize) {
        let mut total_l1 = 0.0_f64;
        let mut paired = 0usize;
        let mut populated = 0usize;
        for (i, &idx) in self.indices.iter().enumerate() {
            let current = cfr.avg_strategy_at_index(idx);
            if current.is_some() {
                populated += 1;
            }
            if let (Some(prev), Some(cur)) = (self.prev[i].as_ref(), current.as_ref()) {
                let n = prev.len().min(cur.len());
                let l1: f32 = (0..n).map(|j| (prev[j] - cur[j]).abs()).sum();
                total_l1 += l1 as f64;
                paired += 1;
            }
            self.prev[i] = current;
        }
        let mean_l1 = if paired > 0 {
            total_l1 / paired as f64
        } else {
            f64::NAN
        };
        (mean_l1, paired, populated)
    }

    fn sample_size(&self) -> usize {
        self.indices.len()
    }
}

fn parse_env<T: std::str::FromStr>(name: &str, default: T) -> T {
    std::env::var(name).ok().and_then(|s| s.parse().ok()).unwrap_or(default)
}

const DEFAULT_WEIGHT_DIR: &str = "/home/steven/card_platypus/infostate.three_card_played_f32";

fn main() {
    // Workspace defaults: linear CFR + parallel training.
    cfres::feature::enable(cfres::feature::LinearCFR);
    cfres::feature::disable(cfres::feature::SingleThread);

    let weight_dir: PathBuf = std::env::var("CFR_WEIGHT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_WEIGHT_DIR));
    let max_cards: usize = parse_env("CFR_MAX_CARDS", 3);
    let total_iters: usize = parse_env("CFR_ITERS", 1_000_000_000);
    let max_secs: f64 = parse_env("CFR_MAX_SECS", 86_400.0);
    let report_secs: f64 = parse_env("CFR_REPORT_SECS", 1_800.0);
    let inner_chunk: usize = parse_env("CFR_INNER_CHUNK", 25_000);
    let eval_hands: usize = parse_env("CFR_EVAL_HANDS", 100);
    let sample_budget_bytes: usize =
        parse_env("CFR_POLICY_SAMPLE_BYTES", 100 * 1024 * 1024);
    let sample_seed: u64 = parse_env("CFR_POLICY_SAMPLE_SEED", 0xC0DEC0DE);

    println!(
        "CFR Euchre: weight_dir={} max_cards={} cap_iters={} cap_secs={:.0} \
         report_every≈{:.0}s (inner_chunk={}) eval_hands={}",
        weight_dir.display(),
        max_cards,
        total_iters,
        max_secs,
        report_secs,
        inner_chunk,
        eval_hands,
    );
    println!(
        "{:>12} {:>8} {:>12} {:>8} {:>8} {:>8} {:>14} {:>9} {:>9} {:>10} {:>10}",
        "iter", "time_s", "score_v_pim", "win%", "tie%", "loss%", "info_states",
        "rss_mb", "B/istate", "delta_l1", "paired"
    );

    assert!(
        weight_dir.exists(),
        "CFR_WEIGHT_DIR {} does not exist — point this at an existing trained-weights directory \
         (we resume training, never start from scratch in this entry point)",
        weight_dir.display()
    );

    let mut cfr =
        EuchreCfres::new_euchre(StdRng::seed_from_u64(0), max_cards, Some(weight_dir.as_path()));
    let resume_istates = cfr.num_info_states();
    let resume_iters = cfr.iterations();

    let mut sampler = PolicySampler::new(
        cfr.indexer_size(),
        sample_budget_bytes,
        EUCHRE_MAX_ACTIONS,
        sample_seed,
    );
    println!(
        "Resuming from {} stored info states ({} CFR iters already done). \
         Policy-delta sampler: {} slots sampled out of {} (budget={} MB).",
        resume_istates,
        resume_iters,
        sampler.sample_size(),
        cfr.indexer_size(),
        sample_budget_bytes / (1024 * 1024),
    );

    let start = Instant::now();

    // Baseline checkpoint at t=0 so the chart has a visible "before"
    // point. This also seeds the policy-delta snapshot from the loaded
    // weights so the first post-train report's delta is measured
    // against the resume point rather than against zero.
    let mut done = 0usize;
    let mut last_report = Instant::now();
    report(&mut cfr, &mut sampler, eval_hands, done, &start);

    // Time-based outer loop. Inner train() calls are kept short (
    // `inner_chunk` iters) so we can check the wall clock between
    // chunks and trigger a report at the next chunk boundary after
    // `report_secs` have passed. With cfr3 running at ~10 iters/sec
    // observed (3h per 5M) the default 25 000-iter chunk is ~40
    // minutes, so report cadence resolution is roughly one chunk —
    // shrink `inner_chunk` for finer-grained reports at the cost of
    // more eval-side overhead.
    loop {
        if done >= total_iters {
            break;
        }
        if start.elapsed().as_secs_f64() >= max_secs {
            println!(
                "Wall-clock cap (CFR_MAX_SECS={:.0}) hit; stopping.",
                max_secs
            );
            break;
        }
        let chunk = inner_chunk.min(total_iters - done);
        cfr.train(chunk);
        done += chunk;

        if last_report.elapsed().as_secs_f64() >= report_secs {
            report(&mut cfr, &mut sampler, eval_hands, done, &start);
            last_report = Instant::now();
            // Persist progress after every checkpoint so an interrupted
            // run doesn't lose the past report_secs of work.
            if let Err(e) = cfr.save() {
                eprintln!("checkpoint save failed: {:#}", e);
            }
        }
    }

    // Final report if the last chunk straddled the report boundary —
    // otherwise we'd leave the dashboard missing the very last point.
    if last_report.elapsed().as_secs_f64() > 1.0 {
        report(&mut cfr, &mut sampler, eval_hands, done, &start);
    }

    let elapsed = start.elapsed().as_secs_f64();
    println!(
        "Training finished in {:.2}s. Final info states touched: {}",
        elapsed,
        cfr.num_info_states()
    );
    if let Err(e) = cfr.save() {
        eprintln!("final save failed: {:#}", e);
    } else {
        println!("Saved final weights to {}", weight_dir.display());
    }
}

fn report(
    cfr: &mut EuchreCfres,
    sampler: &mut PolicySampler,
    eval_hands: usize,
    done: usize,
    start: &Instant,
) {
    let elapsed = start.elapsed().as_secs_f64();
    let info_states = cfr.num_info_states();

    let eval = evaluate_vs_pimcts(cfr, eval_hands, done as u64);
    let (delta_l1, paired, populated_sampled) = sampler.measure(cfr);

    let mem = process_memory();
    let (rss_mb, peak_rss_mb, vsize_mb) = mem
        .map(|m| (m.rss_mb(), m.peak_rss_mb(), m.vsize_mb()))
        .unwrap_or((-1.0, -1.0, -1.0));
    let bytes_per_istate = if info_states > 0 && rss_mb >= 0.0 {
        rss_mb * 1024.0 * 1024.0 / info_states as f64
    } else {
        -1.0
    };

    println!(
        "{:>12} {:>8.2} {:>12.3} {:>7.1}% {:>7.1}% {:>7.1}% {:>14} {:>9.1} {:>9.1} {:>10.4} {:>10}",
        done,
        elapsed,
        eval.score_avg,
        100.0 * eval.win_rate,
        100.0 * eval.tie_rate,
        100.0 * eval.loss_rate,
        info_states,
        rss_mb,
        bytes_per_istate,
        delta_l1,
        paired,
    );

    // Iteration-axis: training quality + convergence + resource. Mirrors
    // the Oh Hell trainer's emission. `policy_delta_l1` is the headline
    // convergence number — on the very first checkpoint there's no
    // prior snapshot to compare to, so it sits at NaN. kestrel-tail
    // rejects NaN as un-serialisable JSON, so we omit the line entirely
    // in that case (the next checkpoint posts the first real value).
    // `rss_mb=-1` is a sentinel for "no /proc/self/status available"
    // and is filtered out on the dashboard.
    if delta_l1.is_finite() {
        println!(
            "kestrel: step={} pimcts_avg={:.6} win_rate={:.6} tie_rate={:.6} loss_rate={:.6} \
             info_states={} rss_mb={:.4} peak_rss_mb={:.4} vsize_mb={:.4} bytes_per_istate={:.2} \
             policy_delta_l1={:.6} policy_delta_paired={} policy_sample_populated={} \
             policy_sample_size={} eval_hands={}",
            done,
            eval.score_avg,
            eval.win_rate,
            eval.tie_rate,
            eval.loss_rate,
            info_states,
            rss_mb,
            peak_rss_mb,
            vsize_mb,
            bytes_per_istate,
            delta_l1,
            paired,
            populated_sampled,
            sampler.sample_size(),
            eval_hands,
        );
    } else {
        // First-checkpoint shape: still post the training-quality + resource
        // numbers (kestrel-tail accepts these fine), just drop the delta_l1.
        println!(
            "kestrel: step={} pimcts_avg={:.6} win_rate={:.6} tie_rate={:.6} loss_rate={:.6} \
             info_states={} rss_mb={:.4} peak_rss_mb={:.4} vsize_mb={:.4} bytes_per_istate={:.2} \
             policy_delta_paired={} policy_sample_populated={} policy_sample_size={} eval_hands={}",
            done,
            eval.score_avg,
            eval.win_rate,
            eval.tie_rate,
            eval.loss_rate,
            info_states,
            rss_mb,
            peak_rss_mb,
            vsize_mb,
            bytes_per_istate,
            paired,
            populated_sampled,
            sampler.sample_size(),
            eval_hands,
        );
    }
    // Time-axis: wall-clock vs iterations-completed, so the dashboard
    // can chart the actual training rate (which on cfr3 changes a lot
    // as the mmap warms up).
    println!("kestrel: t={:.4} iters_done={}", elapsed, done);
    // Block-buffered stdout when piped to kestrel-tail would otherwise
    // hold every line until the buffer fills — at this checkpoint
    // cadence that's many minutes of "no data" between updates.
    let _ = std::io::stdout().flush();
}

struct EvalSummary {
    /// Mean per-hand score from CFR-controlled team's perspective.
    /// Positive = CFR team out-points PIMCTS team on average.
    score_avg: f64,
    win_rate: f64,
    tie_rate: f64,
    loss_rate: f64,
}

/// Play `n_hands` single hands of Euchre, CFR-controlled team vs PIMCTS,
/// rotating which seats CFR holds so seat (and therefore dealer) bias
/// washes out. Returns mean score for the CFR team plus hand-level
/// win/tie/loss rates.
///
/// Single-hand eval (rather than to-10 matches) keeps each checkpoint
/// cheap — at ~1s per hand the default 100 hands costs ~100 s, vs
/// minutes per match. The metric still moves with training because
/// PIMCTS is a fixed strong opponent (~45% match win-rate vs cfr3 in
/// the tournament).
fn evaluate_vs_pimcts(
    cfr: &mut EuchreCfres,
    n_hands: usize,
    seed_offset: u64,
) -> EvalSummary {
    let mut total_score = 0.0_f64;
    let mut wins = 0usize;
    let mut ties = 0usize;
    let mut losses = 0usize;
    let mut acts = Vec::new();

    for i in 0..n_hands {
        let seed = seed_offset.wrapping_add((i as u64).wrapping_mul(0x9E3779B97F4A7C15));
        let mut rng: StdRng = SeedableRng::seed_from_u64(seed);
        let mut pimcts = PIMCTSBot::new(
            50,
            OpenHandSolver::new_euchre(),
            StdRng::seed_from_u64(seed.wrapping_add(1)),
        );

        // Alternate which team CFR controls so we don't bake in the
        // P3-is-always-dealer asymmetry: half the hands CFR holds the
        // dealer seat, half it doesn't.
        let cfr_on_team0 = i % 2 == 0;

        let mut gs = Euchre::new_state();
        while gs.is_chance_node() {
            gs.legal_actions(&mut acts);
            let a = *acts.choose(&mut rng).unwrap();
            gs.apply_action(a);
        }
        while !gs.is_terminal() {
            let seat = gs.cur_player();
            let team0 = seat == 0 || seat == 2;
            let cfr_acts = team0 == cfr_on_team0;
            let action = if cfr_acts {
                cfr.step(&gs)
            } else {
                pimcts.step(&gs)
            };
            gs.apply_action(action);
        }

        // evaluate(0) > 0 iff team (0, 2) won the hand; magnitude = points.
        let score0 = gs.evaluate(0);
        let cfr_score = if cfr_on_team0 { score0 } else { -score0 };
        total_score += cfr_score;
        if cfr_score > 0.0 {
            wins += 1;
        } else if cfr_score < 0.0 {
            losses += 1;
        } else {
            ties += 1;
        }
    }

    let g = n_hands as f64;
    EvalSummary {
        score_avg: total_score / g,
        win_rate: wins as f64 / g,
        tie_rate: ties as f64 / g,
        loss_rate: losses as f64 / g,
    }
}
