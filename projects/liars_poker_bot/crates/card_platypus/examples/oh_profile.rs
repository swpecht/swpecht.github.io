//! Focused profiling harness for the Oh Hell OpenHandSolver at high trick
//! counts. Runs only the tuned solver so `perf record` samples land on the
//! configuration we care about.
//!
//!   cargo run --release --example oh_profile
//!
//! Env: OH_TRIALS (default 5), OH_TRICKS (default 10), OH_REPS (default 3)

use std::time::Instant;

use card_platypus::algorithms::{ismcts::Evaluator, open_hand_solver::OpenHandSolver};
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
    let trials = parse_env("OH_TRIALS", 5);
    let n_tricks = parse_env("OH_TRICKS", 10);
    let reps = parse_env("OH_REPS", 3);

    let mut rng: StdRng = SeedableRng::seed_from_u64(0x4042 + n_tricks as u64);
    let mut states = Vec::with_capacity(trials);
    while states.len() < trials {
        let mut gs = OhHell::new_state(3, n_tricks);
        drive_to_play_phase(&mut gs, &mut rng);
        if !gs.is_terminal() {
            states.push(gs);
        }
    }

    let t0 = Instant::now();
    let mut sum = 0.0;
    for _ in 0..reps {
        for gs in &states {
            // Fresh solver per eval: mirrors PIMCTS worlds where the TT is
            // reset between determinizations.
            let mut solver = OpenHandSolver::new_oh_hell();
            sum += solver.evaluate_player(gs, 0);
        }
    }
    println!(
        "tricks={} trials={} reps={} total={:.3}s per_eval={:.1}ms (checksum {})",
        n_tricks,
        trials,
        reps,
        t0.elapsed().as_secs_f64(),
        1000.0 * t0.elapsed().as_secs_f64() / (trials * reps) as f64,
        sum
    );
}
