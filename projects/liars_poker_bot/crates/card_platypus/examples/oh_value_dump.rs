//! Dump solver values for a deterministic set of Oh Hell states, for
//! cross-build correctness diffing. Prints one line per (state, player):
//! the default-config value and the tuned-config value.
//!
//!   cargo run --release --example oh_value_dump > /tmp/values.txt
//!
//! Env: OH_TRIALS (default 20), OH_MAX_TRICKS (default 9)

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
    let trials = parse_env("OH_TRIALS", 20);
    let max_tricks = parse_env("OH_MAX_TRICKS", 9);

    for n_tricks in 1..=max_tricks {
        let mut rng: StdRng = SeedableRng::seed_from_u64(0x4042 + n_tricks as u64);
        let mut count = 0;
        let mut attempts = 0;
        while count < trials && attempts < trials * 5 {
            attempts += 1;
            let mut gs = OhHell::new_state(3, n_tricks);
            drive_to_play_phase(&mut gs, &mut rng);
            if gs.is_terminal() {
                continue;
            }
            count += 1;
            let use_no_tt_ref = std::env::var("OH_REF").as_deref() == Ok("no_tt");
            for p in 0..3 {
                let vd = if use_no_tt_ref {
                    // Ground truth: plain alpha-beta, no transposition table
                    // (so no reliance on the iso-hash being collision-free).
                    let mut reference = OpenHandSolver::<OhHellGameState>::new_without_cache();
                    reference.evaluate_player(&gs, p)
                } else {
                    let mut baseline = OpenHandSolver::default();
                    baseline.evaluate_player(&gs, p)
                };
                let mut tuned = OpenHandSolver::new_oh_hell();
                let vt = tuned.evaluate_player(&gs, p);
                let flag = if vd != vt { " MISMATCH" } else { "" };
                println!("tricks={} state={} p={} default={:?} tuned={:?}{}", n_tricks, gs, p, vd, vt, flag);
            }
        }
    }
}
