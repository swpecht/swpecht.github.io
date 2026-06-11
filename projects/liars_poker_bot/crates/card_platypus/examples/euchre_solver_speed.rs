//! Micro-benchmark for the Euchre `OpenHandSolver`: measures the marginal
//! effect of the a-priori value-bounds pruning on top of the existing
//! euchre optimizations, and checks the values are unchanged.
//!
//!   cargo run --release --example euchre_solver_speed
//!
//! Env: E_TRIALS (default 50), E_REPS (default 3)

use std::time::Instant;

use card_platypus::algorithms::{
    ismcts::Evaluator,
    open_hand_solver::{OpenHandSolver, Optimizations},
};
use games::{
    actions,
    gamestates::euchre::{Euchre, EuchreGameState, EPhase},
    GameState,
};
use rand::{rngs::StdRng, seq::IndexedRandom, SeedableRng};

fn parse_env(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

fn drive_to_play_phase(gs: &mut EuchreGameState, rng: &mut StdRng) {
    while !gs.is_terminal() && gs.phase() != EPhase::Play {
        let acts = actions!(gs);
        let a = *acts.choose(rng).unwrap();
        gs.apply_action(a);
    }
}

fn main() {
    let trials = parse_env("E_TRIALS", 50);
    let reps = parse_env("E_REPS", 3);

    let mut rng: StdRng = SeedableRng::seed_from_u64(0xE0C8);
    let mut states = Vec::with_capacity(trials);
    let mut attempts = 0;
    while states.len() < trials && attempts < trials * 20 {
        attempts += 1;
        let mut gs = Euchre::new_state();
        drive_to_play_phase(&mut gs, &mut rng);
        if !gs.is_terminal() {
            states.push(gs);
        }
    }

    let no_bounds = {
        let mut o = Optimizations::new_euchre();
        o.value_bounds = |_, _| (f64::NEG_INFINITY, f64::INFINITY);
        o
    };

    let mut base_total = 0.0;
    let mut base_values = Vec::new();
    for _ in 0..reps {
        for gs in &states {
            let mut solver = OpenHandSolver::new(no_bounds.clone());
            let t0 = Instant::now();
            let v = solver.evaluate_player(gs, 0);
            base_total += t0.elapsed().as_secs_f64();
            base_values.push(v);
        }
    }

    let mut tuned_total = 0.0;
    let mut matches = 0usize;
    let mut idx = 0usize;
    for _ in 0..reps {
        for gs in &states {
            let mut solver = OpenHandSolver::new_euchre();
            let t0 = Instant::now();
            let v = solver.evaluate_player(gs, 0);
            tuned_total += t0.elapsed().as_secs_f64();
            if v == base_values[idx] {
                matches += 1;
            }
            idx += 1;
        }
    }

    println!(
        "euchre solver: trials={} reps={} no_bounds={:.4}s with_bounds={:.4}s speedup={:.2}x match={}/{}",
        states.len(),
        reps,
        base_total,
        tuned_total,
        base_total / tuned_total,
        matches,
        idx
    );
}
