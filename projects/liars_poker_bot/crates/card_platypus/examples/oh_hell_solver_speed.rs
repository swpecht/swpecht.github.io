//! Micro-benchmark for the Oh Hell `OpenHandSolver`. Compares
//! `OpenHandSolver::default()` (no game-specific optimizations) against
//! `OpenHandSolver::new_oh_hell()` (with early termination, equivalent-card
//! pruning, move ordering, cheap TT hash).
//!
//! Each "trial" picks a random play-phase state at the given n_tricks
//! depth, evaluates it for player 0 with both solvers, and accumulates the
//! wall-clock time. Reports a speedup ratio plus a sanity check that the
//! two solvers agree on the value (this is the safety net while iterating).
//!
//! Run with:
//!   cargo run --release --example oh_hell_solver_speed

use std::time::Instant;

use card_platypus::algorithms::{
    ismcts::Evaluator,
    open_hand_solver::{OpenHandSolver, Optimizations},
};
use games::{
    actions,
    gamestates::oh_hell::{OHPhase, OhHell, OhHellGameState},
    GameState,
};
use rand::{rngs::StdRng, seq::IndexedRandom, SeedableRng};

fn parse_env(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

fn drive_to_play_phase(gs: &mut OhHellGameState, rng: &mut StdRng) {
    while !gs.is_terminal() && gs.phase() != OHPhase::Play {
        let acts = actions!(gs);
        let a = *acts.choose(rng).unwrap();
        gs.apply_action(a);
    }
}

fn main() {
    let trials = parse_env("OH_TRIALS", 30);
    let max_tricks = parse_env("OH_MAX_TRICKS", 5).min(10);

    println!(
        "OpenHandSolver speed: {} random states/n_tricks, n_tricks=1..={}",
        trials, max_tricks
    );
    println!(
        "{:>8} {:>10} {:>10} {:>10} {:>8} {:>8} {:>8}",
        "n_tricks",
        "default_s",
        "minimal_s",
        "full_s",
        "min_spd",
        "full_spd",
        "match%"
    );

    for n_tricks in 1..=max_tricks {
        let mut states = Vec::with_capacity(trials);
        let mut rng: StdRng = SeedableRng::seed_from_u64(0x4042 + n_tricks as u64);
        let mut attempts = 0;
        while states.len() < trials && attempts < trials * 5 {
            attempts += 1;
            let mut gs = OhHell::new_state(3, n_tricks);
            drive_to_play_phase(&mut gs, &mut rng);
            if !gs.is_terminal() {
                states.push(gs);
            }
        }

        // Default solver
        let mut default_total = 0.0;
        let mut default_values = Vec::with_capacity(states.len());
        for gs in &states {
            let mut solver = OpenHandSolver::default();
            let t0 = Instant::now();
            let v = solver.evaluate_player(gs, 0);
            default_total += t0.elapsed().as_secs_f64();
            default_values.push(v);
        }

        // Minimal-tuned solver: early termination + cheap TT hash only
        let mut minimal_total = 0.0;
        let mut minimal_matches = 0;
        for (i, gs) in states.iter().enumerate() {
            let mut solver = OpenHandSolver::new(Optimizations::new_oh_hell_minimal());
            let t0 = Instant::now();
            let v = solver.evaluate_player(gs, 0);
            minimal_total += t0.elapsed().as_secs_f64();
            if v == default_values[i] {
                minimal_matches += 1;
            }
        }

        // Full-tuned solver
        let mut tuned_total = 0.0;
        let mut tuned_matches = 0;
        for (i, gs) in states.iter().enumerate() {
            let mut solver = OpenHandSolver::new_oh_hell();
            let t0 = Instant::now();
            let v = solver.evaluate_player(gs, 0);
            tuned_total += t0.elapsed().as_secs_f64();
            if v == default_values[i] {
                tuned_matches += 1;
            }
        }

        let min_speedup = if minimal_total > 0.0 { default_total / minimal_total } else { f64::INFINITY };
        let full_speedup = if tuned_total > 0.0 { default_total / tuned_total } else { f64::INFINITY };
        let n = states.len() as f64;
        println!(
            "{:>8} {:>10.4} {:>10.4} {:>10.4} {:>7.2}x {:>7.2}x {:>7.1}%",
            n_tricks,
            default_total,
            minimal_total,
            tuned_total,
            min_speedup,
            full_speedup,
            100.0 * (minimal_matches + tuned_matches) as f64 / (2.0 * n)
        );
    }
}
