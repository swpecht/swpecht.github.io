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
//!
//! Example invocation (with kestrel-tail):
//!
//!   cargo run --release --example oh_hell_cfr_train \
//!     | ./kestrel-tail oh_hell_cfr_2p_2t_50k \
//!         --tag oh_hell --tag cfr --tag 2p --tag 2tricks

use std::time::Instant;

use card_platypus::{
    agents::Agent,
    algorithms::{
        cfres::{self, CFRES, OH_MAX_ACTIONS},
    },
};
use games::{
    gamestates::oh_hell::{OhHell, OhHellGameState},
    GameState,
};
use rand::{rngs::StdRng, seq::IndexedRandom, SeedableRng};

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

    let report_every = (((total_iters as f64) * (report_pct / 100.0)) as usize).max(1);

    println!(
        "CFR Oh Hell: {} players, {} tricks, total_iters={}, report every {} iters \
         ({:.1}%), eval_games/report={}, max_cards_played={}",
        n_players, n_tricks, total_iters, report_every, report_pct, eval_games, max_cards
    );
    println!(
        "{:>10} {:>8} {:>8} {:>10} {:>9} {:>9} {:>9} {:>10}",
        "iter", "time_s", "pct", "score_v_rand", "win%", "tie%", "loss%", "info_states"
    );

    let mut cfr: OhCfres = CFRES::new_oh_hell(n_players, n_tricks, max_cards);

    let start = Instant::now();

    // Pre-training (random-policy) baseline at iter=0 so the chart has a
    // visible "before" point.
    let mut done = 0usize;
    report(&mut cfr, n_players, n_tricks, eval_games, done, total_iters, &start);

    while done < total_iters {
        let chunk = report_every.min(total_iters - done);
        cfr.train(chunk);
        done += chunk;
        report(&mut cfr, n_players, n_tricks, eval_games, done, total_iters, &start);
    }

    let elapsed = start.elapsed().as_secs_f64();
    println!(
        "Training finished in {:.2}s. Final info states touched: {}",
        elapsed,
        cfr.num_info_states()
    );
}

fn report(
    cfr: &mut OhCfres,
    n_players: usize,
    n_tricks: usize,
    eval_games: usize,
    done: usize,
    total_iters: usize,
    start: &Instant,
) {
    let elapsed = start.elapsed().as_secs_f64();
    let pct = 100.0 * (done as f64) / (total_iters as f64);

    let eval = evaluate_vs_random(cfr, n_players, n_tricks, eval_games, done as u64);

    println!(
        "{:>10} {:>8.2} {:>7.1}% {:>10.3} {:>8.1}% {:>8.1}% {:>8.1}% {:>10}",
        done,
        elapsed,
        pct,
        eval.pimcts_avg,
        100.0 * eval.win_rate,
        100.0 * eval.tie_rate,
        100.0 * eval.loss_rate,
        cfr.num_info_states(),
    );

    // Iteration-axis metrics.
    println!(
        "kestrel: step={} pimcts_avg={:.6} win_rate={:.6} tie_rate={:.6} loss_rate={:.6} \
         info_states={} num_players={} n_tricks={} eval_games={}",
        done,
        eval.pimcts_avg,
        eval.win_rate,
        eval.tie_rate,
        eval.loss_rate,
        cfr.num_info_states(),
        n_players,
        n_tricks,
        eval_games,
    );

    // Time-axis metric: progress fraction.
    println!("kestrel: t={:.4} progress_pct={:.4}", elapsed, pct);
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
