//! Baseline benchmark: PIMCTS vs random in 3-player Oh Hell across
//! n_tricks 1..=10.
//!
//! Setup: each game has one PIMCTS player and two random players. The
//! PIMCTS position rotates across games to remove seat bias. Cards and
//! random opponents' decisions are sampled from a single per-game RNG so
//! results are reproducible.
//!
//! PIMCTS uses a `RandomRolloutEvaluator` (one random rollout per resampled
//! world) so the sweep scales to large n_tricks. A stronger evaluator like
//! `OpenHandSolver` produces sharper play but is intractable past ~n=4 for
//! Oh Hell because there are no game-specific search optimizations.
//!
//! Run with:
//!   cargo run --release --example oh_hell_baseline
//!
//! Optional env vars:
//!   OH_GAMES         games per n_tricks (default 60)
//!   OH_ROLLOUTS      PIMCTS world-rollout count (default 25)
//!   OH_MAX_TRICKS    upper bound on n_tricks (default 10)

use std::time::Instant;

use card_platypus::{
    agents::Agent,
    algorithms::{ismcts::RandomRolloutEvaluator, pimcts::PIMCTSBot},
};
use games::{
    gamestates::oh_hell::{OhHell, NUM_PLAYERS},
    GameState,
};
use rand::{rngs::StdRng, seq::IndexedRandom, SeedableRng};

fn parse_env(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

fn main() {
    let n_games = parse_env("OH_GAMES", 60);
    let rollouts = parse_env("OH_ROLLOUTS", 25);
    let max_tricks = parse_env("OH_MAX_TRICKS", 10).min(10);

    println!(
        "Oh Hell baseline: {} games/n_tricks, PIMCTS rollouts={}, n_tricks=1..={}",
        n_games, rollouts, max_tricks
    );
    println!(
        "{:>8} {:>10} {:>10} {:>10} {:>10} {:>10} {:>9}",
        "n_tricks", "pimcts_avg", "rand_avg", "win%", "tie%", "loss%", "secs"
    );

    for n_tricks in 1..=max_tricks {
        let start = Instant::now();
        let (pimcts_avg, rand_avg, wins, ties, losses) =
            run_block(n_tricks, n_games, rollouts);
        let elapsed = start.elapsed().as_secs_f64();
        let g = n_games as f64;
        println!(
            "{:>8} {:>10.3} {:>10.3} {:>9.1}% {:>9.1}% {:>9.1}% {:>9.2}",
            n_tricks,
            pimcts_avg,
            rand_avg,
            100.0 * wins as f64 / g,
            100.0 * ties as f64 / g,
            100.0 * losses as f64 / g,
            elapsed,
        );
    }
}

/// Returns (pimcts_avg, random_avg, wins, ties, losses).
fn run_block(
    n_tricks: usize,
    n_games: usize,
    rollouts: usize,
) -> (f64, f64, usize, usize, usize) {
    let mut pimcts_total = 0.0_f64;
    let mut random_total = 0.0_f64;
    let mut wins = 0usize;
    let mut ties = 0usize;
    let mut losses = 0usize;

    for game_idx in 0..n_games {
        let seed = (n_tricks as u64) * 100_000 + game_idx as u64;
        let mut rng: StdRng = SeedableRng::seed_from_u64(seed);

        // Rotate the PIMCTS seat across games.
        let pimcts_pos = game_idx % NUM_PLAYERS;

        let mut agent = PIMCTSBot::new(
            rollouts,
            RandomRolloutEvaluator::new(1),
            SeedableRng::seed_from_u64(seed.wrapping_add(1)),
        );

        let mut gs = OhHell::new_state(n_tricks);
        // Random chance-node resolution (cards, face-up).
        let mut acts = Vec::new();
        while gs.is_chance_node() {
            gs.legal_actions(&mut acts);
            let a = *acts.choose(&mut rng).expect("non-empty chance actions");
            gs.apply_action(a);
        }

        while !gs.is_terminal() {
            let cp = gs.cur_player();
            let a = if cp == pimcts_pos {
                agent.step(&gs)
            } else {
                gs.legal_actions(&mut acts);
                *acts.choose(&mut rng).expect("non-empty actions")
            };
            gs.apply_action(a);
        }

        let scores: Vec<f64> = (0..NUM_PLAYERS).map(|p| gs.evaluate(p)).collect();
        pimcts_total += scores[pimcts_pos];
        for (p, &s) in scores.iter().enumerate() {
            if p != pimcts_pos {
                random_total += s;
            }
        }

        let pimcts_score = scores[pimcts_pos];
        let max_other = (0..NUM_PLAYERS)
            .filter(|p| *p != pimcts_pos)
            .map(|p| scores[p])
            .fold(f64::NEG_INFINITY, f64::max);

        if pimcts_score > max_other {
            wins += 1;
        } else if pimcts_score >= max_other {
            ties += 1;
        } else {
            losses += 1;
        }
    }

    let g = n_games as f64;
    let random_avg = random_total / (g * (NUM_PLAYERS as f64 - 1.0));
    (pimcts_total / g, random_avg, wins, ties, losses)
}
