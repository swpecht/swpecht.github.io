//! EPIMC vs PIMCTS baseline on Euchre.
//!
//! Pits one EPIMC bot (varying `depth`) against three PIMCTS bots (the
//! depth=1 baseline) at all 4 seats over N games per depth, rotating which
//! seat holds the EPIMC bot. Both use the same leaf evaluator and rollout
//! count so the only difference being measured is the postponing depth.
//!
//! Run with:
//!   cargo run -p card_platypus --release --example euchre_epimc_baseline
//!
//! Optional env vars:
//!   EU_GAMES        games per depth (default 40)
//!   EU_ROLLOUTS     world-rollout count for every bot (default 25)
//!   EU_DEPTHS       comma-sep list of EPIMC depths to scan (default 1,2,3)
//!
//! Output mirrors `oh_hell_baseline.rs`: a human-readable table plus one
//! `kestrel: …` line per depth for plotting via `kestrel-tail`.
//!
//!   cargo run -p card_platypus --release --example euchre_epimc_baseline \
//!     | ./kestrel-tail euchre_epimc_vs_pimc_r25_g40

use std::time::Instant;

use card_platypus::{
    agents::Agent,
    algorithms::{epimc::EPIMCBot, open_hand_solver::OpenHandSolver, pimcts::PIMCTSBot},
};
use games::{
    gamestates::euchre::{Euchre, EuchreGameState},
    GameState,
};
use rand::{rngs::StdRng, seq::IndexedRandom, SeedableRng};

fn parse_env(name: &str, default: usize) -> usize {
    std::env::var(name).ok().and_then(|s| s.parse().ok()).unwrap_or(default)
}

fn parse_depths() -> Vec<usize> {
    let raw = std::env::var("EU_DEPTHS").unwrap_or_else(|_| "1,2,3".to_string());
    raw.split(',')
        .filter_map(|s| s.trim().parse::<usize>().ok())
        .filter(|d| *d >= 1)
        .collect()
}

fn main() {
    let n_games = parse_env("EU_GAMES", 40);
    let rollouts = parse_env("EU_ROLLOUTS", 25);
    let depths = parse_depths();

    println!(
        "Euchre EPIMC baseline: {} games/depth, rollouts={}, depths={:?}",
        n_games, rollouts, depths,
    );
    println!(
        "EPIMC bot at one seat (rotating), PIMCTS at the other three. Both use OpenHandSolver."
    );
    println!(
        "{:>5} {:>10} {:>10} {:>10} {:>10} {:>10} {:>10} {:>9}",
        "depth", "epimc_avg", "pimc_avg", "win%", "tie%", "loss%", "s/move", "secs"
    );

    for depth in depths {
        let start = Instant::now();
        let (epimc_avg, pimc_avg, wins, ties, losses, moves) =
            run_block(depth, n_games, rollouts);
        let elapsed = start.elapsed().as_secs_f64();
        let g = n_games as f64;
        let win_rate = wins as f64 / g;
        let tie_rate = ties as f64 / g;
        let loss_rate = losses as f64 / g;
        let s_per_move = if moves == 0 { 0.0 } else { elapsed / moves as f64 };

        println!(
            "{:>5} {:>10.3} {:>10.3} {:>9.1}% {:>9.1}% {:>9.1}% {:>10.4} {:>9.2}",
            depth,
            epimc_avg,
            pimc_avg,
            100.0 * win_rate,
            100.0 * tie_rate,
            100.0 * loss_rate,
            s_per_move,
            elapsed,
        );
        println!(
            "kestrel: step={} win_rate={:.6} tie_rate={:.6} loss_rate={:.6} \
             epimc_avg={:.6} pimc_avg={:.6} secs_per_move={:.6} secs={:.4} \
             rollouts={} games={}",
            depth, win_rate, tie_rate, loss_rate, epimc_avg, pimc_avg, s_per_move, elapsed,
            rollouts, n_games,
        );
    }
}

/// (epimc_avg, pimc_avg, wins, ties, losses, total_epimc_moves)
fn run_block(
    depth: usize,
    n_games: usize,
    rollouts: usize,
) -> (f64, f64, usize, usize, usize, usize) {
    let mut epimc_total = 0.0_f64;
    let mut pimc_total = 0.0_f64;
    let mut wins = 0usize;
    let mut ties = 0usize;
    let mut losses = 0usize;
    let mut epimc_moves = 0usize;

    for game_idx in 0..n_games {
        let seed = (depth as u64) * 1_000_000 + game_idx as u64;
        let mut rng: StdRng = SeedableRng::seed_from_u64(seed);

        let epimc_seat = game_idx % 4;
        let mut epimc = EPIMCBot::new(
            rollouts,
            depth,
            OpenHandSolver::new_euchre(),
            SeedableRng::seed_from_u64(seed.wrapping_add(1)),
        );
        let mut pimc = PIMCTSBot::new(
            rollouts,
            OpenHandSolver::new_euchre(),
            SeedableRng::seed_from_u64(seed.wrapping_add(2)),
        );

        let mut gs: EuchreGameState = Euchre::new_state();
        let mut acts = Vec::new();
        // Resolve all chance nodes (deal + face-up).
        while gs.is_chance_node() {
            gs.legal_actions(&mut acts);
            let a = *acts.choose(&mut rng).expect("non-empty chance actions");
            gs.apply_action(a);
            acts.clear();
        }

        while !gs.is_terminal() {
            let cp = gs.cur_player();
            // Sit-out: the dealer's partner when going alone has no legal
            // actions and is forced into a Pass sentinel. The agents handle
            // that themselves via legal_actions.
            let a = if cp == epimc_seat {
                epimc_moves += 1;
                epimc.step(&gs)
            } else {
                pimc.step(&gs)
            };
            gs.apply_action(a);
        }

        // Euchre is team-based: seats {0, 2} vs {1, 3}. evaluate(p) returns
        // the score for player p's team.
        let epimc_score = gs.evaluate(epimc_seat);
        // Average opponent (PIMC) score over the 3 non-EPIMC seats.
        let pimc_score: f64 = (0..4)
            .filter(|p| *p != epimc_seat)
            .map(|p| gs.evaluate(p))
            .sum::<f64>()
            / 3.0;

        epimc_total += epimc_score;
        pimc_total += pimc_score;

        let opponent_team = if epimc_seat % 2 == 0 { 1 } else { 0 };
        let opp_seat = opponent_team;
        let opp_score = gs.evaluate(opp_seat);
        if epimc_score > opp_score {
            wins += 1;
        } else if epimc_score < opp_score {
            losses += 1;
        } else {
            ties += 1;
        }
    }

    let g = n_games as f64;
    (
        epimc_total / g,
        pimc_total / g,
        wins,
        ties,
        losses,
        epimc_moves,
    )
}
