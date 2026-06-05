//! Train a CFR agent for Oh Hell and stream Kestrel metric lines.
//!
//! Two independent x-axes are emitted on separate metric lines, per the
//! Kestrel format (one reserved x-axis key per line):
//!
//!   * `step=<iteration>` — average score against random opponents
//!     (`pimcts_avg`) plus `win_rate` / `tie_rate` / `loss_rate`, measured
//!     by playing `eval_games` evaluation games against random opponents
//!     after every `report_pct` of total training iterations.
//!   * `t=<elapsed_secs>` — `progress_pct` so the dashboard can show how
//!     wall-clock time maps to training progress (useful for spotting
//!     slowdowns / progress stalls).
//!
//! Defaults are sized for a quick 2-player, 2-trick smoke run that
//! finishes in well under a minute. Override via env vars:
//!
//!   CFR_PLAYERS        num_players (2)
//!   CFR_TRICKS         n_tricks (2)
//!   CFR_ITERS          total CFR iterations (50_000)
//!   CFR_REPORT_PCT     report every this % of iters (5.0)
//!   CFR_EVAL_GAMES     evaluation games per report (200)
//!   CFR_MAX_CARDS      OhHellDepthChecker max_cards_played (100 → full)
//!   CFR_MMAP_DIR       directory for disk-backed mmap + PHF storage.
//!                      Builds the PHF on first startup and writes it
//!                      to `<dir>/indexer`; the mmap goes to
//!                      `<dir>/mmap` and the populated count to
//!                      `<dir>/meta`. If `<dir>` already contains the
//!                      three files, training resumes from that
//!                      checkpoint. When unset, runs purely in-memory
//!                      (anonymous mmap, no persistence).
//!
//! Example invocation (with kestrel-tail):
//!
//!   cargo run --release --example oh_hell_cfr_train \
//!     | ./kestrel-tail oh_hell_cfr_2p_2t_50k \
//!         --tag oh_hell --tag cfr --tag 2p --tag 2tricks

use std::{path::PathBuf, time::Instant};

use card_platypus::{
    agents::Agent,
    algorithms::{
        cfres::{self, CFRES, OH_MAX_ACTIONS},
    },
    diag::process_memory,
};
use games::{
    gamestates::oh_hell::{OhHell, OhHellGameState},
    GameState,
};
use rand::{rngs::StdRng, seq::IndexedRandom, SeedableRng};

/// Sampled L1 policy-delta convergence metric.
///
/// Picks a fixed random subset of PHF slots once at startup (seeded for
/// reproducibility across resumes), and at each checkpoint computes the
/// mean L1 distance between the current normalized avg strategy and the
/// previous snapshot, over slots populated at *both* checkpoints. Drops
/// to 0 as the average policy stops changing.
///
/// Sample size is bounded by a byte budget (default 100 MB). For Oh Hell
/// each sample stores 1 × usize (index) + OH_MAX_ACTIONS × f32 (strategy
/// snapshot) ≈ 40 B, so 100 MB covers up to 2.5M slots. Smaller PHFs are
/// sampled in full.
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
    /// Returns (mean_l1, paired_count, populated_count). `mean_l1` is NaN
    /// when no slot was populated at both checkpoints.
    fn measure(&mut self, cfr: &OhCfres) -> (f64, usize, usize) {
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
    std::env::var(name)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

type OhCfres = CFRES<OhHellGameState, OH_MAX_ACTIONS>;

fn main() {
    // Match the workspace default: LinearCFR + parallel training.
    cfres::feature::enable(cfres::feature::LinearCFR);
    cfres::feature::disable(cfres::feature::SingleThread);

    let n_players: usize = parse_env("CFR_PLAYERS", 2);
    let n_tricks: usize = parse_env("CFR_TRICKS", 2);
    let total_iters: usize = parse_env("CFR_ITERS", 50_000);
    let report_pct: f64 = parse_env("CFR_REPORT_PCT", 5.0);
    let eval_games: usize = parse_env("CFR_EVAL_GAMES", 200);
    let max_cards: usize = parse_env("CFR_MAX_CARDS", 100);
    let mmap_dir: Option<PathBuf> = std::env::var("CFR_MMAP_DIR").ok().map(PathBuf::from);

    let report_every = (((total_iters as f64) * (report_pct / 100.0)) as usize).max(1);
    let sample_budget_bytes: usize = parse_env("CFR_POLICY_SAMPLE_BYTES", 100 * 1024 * 1024);
    let sample_seed: u64 = parse_env("CFR_POLICY_SAMPLE_SEED", 0xC0DEC0DE);

    // Early-termination on policy-delta-L1 plateau. Disabled when
    // `CFR_TARGET_L1` is unset → loop runs until `CFR_ITERS`.
    //
    //   CFR_TARGET_L1   stop once delta_l1 < this for `patience`
    //                   consecutive checkpoints (only checked once
    //                   iters >= CFR_MIN_ITERS). Unset = no early stop.
    //   CFR_PATIENCE    consecutive sub-threshold checkpoints required
    //                   before stopping. Defaults to 3, which damps
    //                   the single-checkpoint noise in the sampled L1.
    //   CFR_MIN_ITERS   floor on training before the patience counter
    //                   can fire. Defaults to 0 (any checkpoint
    //                   counts). Useful when the sampler has only a
    //                   tiny populated fraction early on and produces
    //                   misleadingly small L1s.
    //
    // The L1 metric is a sampled "average strategy stopped changing"
    // signal, NOT a "reached Nash" signal. Multi-player CFR has no
    // Nash guarantee anyway — this just stops once more iterations
    // aren't moving the average policy.
    let target_l1: Option<f64> = std::env::var("CFR_TARGET_L1")
        .ok()
        .and_then(|s| s.parse().ok());
    let patience: usize = parse_env("CFR_PATIENCE", 3);
    let min_iters: usize = parse_env("CFR_MIN_ITERS", 0);

    println!(
        "CFR Oh Hell: {} players, {} tricks, total_iters={}, report every {} iters \
         ({:.1}%), eval_games/report={}, max_cards_played={}, mmap_dir={:?}, \
         target_l1={:?}, patience={}, min_iters={}",
        n_players, n_tricks, total_iters, report_every, report_pct, eval_games, max_cards,
        mmap_dir, target_l1, patience, min_iters,
    );
    println!(
        "{:>10} {:>8} {:>8} {:>10} {:>9} {:>9} {:>9} {:>10} {:>9} {:>9} {:>10} {:>10}",
        "iter", "time_s", "pct", "score_v_rand", "win%", "tie%", "loss%", "info_states",
        "rss_mb", "B/istate", "delta_l1", "paired"
    );

    let mut cfr: OhCfres =
        CFRES::new_oh_hell(n_players, n_tricks, max_cards, mmap_dir.as_deref());

    let mut sampler = PolicySampler::new(
        cfr.indexer_size(),
        sample_budget_bytes,
        OH_MAX_ACTIONS,
        sample_seed,
    );
    println!(
        "Policy-delta sampler: {} slots sampled out of {} (budget={} MB)",
        sampler.sample_size(),
        cfr.indexer_size(),
        sample_budget_bytes / (1024 * 1024),
    );

    let start = Instant::now();

    // Pre-training (random-policy) baseline at iter=0 so the chart has a
    // visible "before" point. Also seeds the policy-delta snapshot from
    // any pre-loaded weights so the first post-train report's delta is
    // measured against the resume point rather than zero.
    let mut done = 0usize;
    let _ = report(
        &mut cfr, &mut sampler, n_players, n_tricks, eval_games, done, total_iters, &start,
    );

    // Counts consecutive checkpoints satisfying delta_l1 < target_l1.
    // NaN checkpoints (no prior snapshot to compare against) neither
    // increment nor reset — they're skipped. A real measurement at or
    // above the threshold resets the counter so a single noisy dip can't
    // stop training early.
    let mut consecutive_below: usize = 0;
    let mut early_stopped = false;

    while done < total_iters {
        let chunk = report_every.min(total_iters - done);
        cfr.train(chunk);
        done += chunk;
        let delta_l1 = report(
            &mut cfr, &mut sampler, n_players, n_tricks, eval_games, done, total_iters, &start,
        );
        if mmap_dir.is_some() {
            if let Err(e) = cfr.save() {
                eprintln!("checkpoint save failed: {:#}", e);
            }
        }

        if let Some(target) = target_l1 {
            if delta_l1.is_finite() {
                if done >= min_iters && delta_l1 < target {
                    consecutive_below += 1;
                } else {
                    consecutive_below = 0;
                }
            }
            if consecutive_below >= patience {
                early_stopped = true;
                println!(
                    "Early termination at iter {}: delta_l1 < {:.6} for {} consecutive \
                     checkpoints (patience={}, min_iters={})",
                    done, target, consecutive_below, patience, min_iters,
                );
                // Emit a kestrel signal on the step axis so the dashboard
                // shows where the loop bailed and why.
                println!(
                    "kestrel: step={} early_stop=1 early_stop_iter={} target_l1={:.6} \
                     final_delta_l1={:.6} consecutive_below={}",
                    done, done, target, delta_l1, consecutive_below,
                );
                break;
            }
        }
    }

    let elapsed = start.elapsed().as_secs_f64();
    println!(
        "Training finished in {:.2}s ({}). Final info states touched: {}",
        elapsed,
        if early_stopped {
            "early-stop"
        } else {
            "iter cap reached"
        },
        cfr.num_info_states()
    );

    if let Some(p) = mmap_dir.as_ref() {
        if let Err(e) = cfr.save() {
            eprintln!("final save failed: {:#}", e);
        } else {
            println!("Saved final weights to {}", p.display());
        }
    }
}

/// Emit a progress + kestrel checkpoint and return the measured
/// `delta_l1` so the outer training loop can run patience-based
/// early termination without calling `sampler.measure` twice (it
/// has side effects on the snapshot ring).
fn report(
    cfr: &mut OhCfres,
    sampler: &mut PolicySampler,
    n_players: usize,
    n_tricks: usize,
    eval_games: usize,
    done: usize,
    total_iters: usize,
    start: &Instant,
) -> f64 {
    let elapsed = start.elapsed().as_secs_f64();
    let pct = 100.0 * (done as f64) / (total_iters as f64);
    let info_states = cfr.num_info_states();

    let eval = evaluate_vs_random(cfr, n_players, n_tricks, eval_games, done as u64);
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
        "{:>10} {:>8.2} {:>7.1}% {:>10.3} {:>8.1}% {:>8.1}% {:>8.1}% {:>10} {:>9.1} {:>9.1} {:>10.4} {:>10}",
        done,
        elapsed,
        pct,
        eval.pimcts_avg,
        100.0 * eval.win_rate,
        100.0 * eval.tie_rate,
        100.0 * eval.loss_rate,
        info_states,
        rss_mb,
        bytes_per_istate,
        delta_l1,
        paired,
    );

    // Iteration-axis metrics: training quality + resource snapshots +
    // policy-delta convergence signal. Every metric here is owned by
    // the `step` x-axis — emitting the same metric on a second line
    // with a different x-axis key (`t=...`) makes Kestrel chart it as
    // a mixed series, which is what produced the zigzag in earlier
    // logs. `rss_mb=-1` marks an unsupported platform (no
    // /proc/self/status); the dashboard filters that sentinel out.
    // `policy_delta_l1` is the mean L1 distance between the current
    // and previous normalized avg strategy over the fixed sample (NaN
    // on the first checkpoint when there's nothing to compare against).
    println!(
        "kestrel: step={} pimcts_avg={:.6} win_rate={:.6} tie_rate={:.6} loss_rate={:.6} \
         info_states={} rss_mb={:.4} peak_rss_mb={:.4} vsize_mb={:.4} bytes_per_istate={:.2} \
         policy_delta_l1={:.6} policy_delta_paired={} policy_sample_populated={} \
         policy_sample_size={} num_players={} n_tricks={} eval_games={}",
        done,
        eval.pimcts_avg,
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
        n_players,
        n_tricks,
        eval_games,
    );

    // Time-axis metric: just the progress fraction, so the dashboard
    // can show how wall-clock maps to training progress (useful for
    // spotting slowdowns / stalls). Resource and convergence metrics
    // already live on the `step` axis above.
    println!(
        "kestrel: t={:.4} progress_pct={:.4}",
        elapsed, pct,
    );
    delta_l1
}

struct EvalSummary {
    pimcts_avg: f64,
    win_rate: f64,
    tie_rate: f64,
    loss_rate: f64,
}

/// Play `n_games` games of CFR-vs-random and return win / tie / loss
/// rates plus the average score for the CFR-controlled seat (which
/// rotates across games to remove seat bias).
fn evaluate_vs_random(
    cfr: &mut OhCfres,
    n_players: usize,
    n_tricks: usize,
    n_games: usize,
    seed_offset: u64,
) -> EvalSummary {
    let mut total_score = 0.0_f64;
    let mut wins = 0usize;
    let mut ties = 0usize;
    let mut losses = 0usize;
    let mut acts = Vec::new();

    for i in 0..n_games {
        let seed = seed_offset.wrapping_add((i as u64).wrapping_mul(0x9E3779B97F4A7C15));
        let mut rng: StdRng = SeedableRng::seed_from_u64(seed);
        let cfr_pos = i % n_players;

        let mut gs = OhHell::new_state(n_players, n_tricks);

        while !gs.is_terminal() {
            if gs.is_chance_node() {
                gs.legal_actions(&mut acts);
                gs.apply_action(*acts.choose(&mut rng).unwrap());
                continue;
            }
            let cp = gs.cur_player();
            let a = if cp == cfr_pos {
                cfr.step(&gs)
            } else {
                gs.legal_actions(&mut acts);
                *acts.choose(&mut rng).unwrap()
            };
            gs.apply_action(a);
        }

        let cfr_score = gs.evaluate(cfr_pos);
        total_score += cfr_score;

        let max_other = (0..n_players)
            .filter(|p| *p != cfr_pos)
            .map(|p| gs.evaluate(p))
            .fold(f64::NEG_INFINITY, f64::max);

        if cfr_score > max_other {
            wins += 1;
        } else if cfr_score < max_other {
            losses += 1;
        } else {
            ties += 1;
        }
    }

    let g = n_games as f64;
    EvalSummary {
        pimcts_avg: total_score / g,
        win_rate: wins as f64 / g,
        tie_rate: ties as f64 / g,
        loss_rate: losses as f64 / g,
    }
}
