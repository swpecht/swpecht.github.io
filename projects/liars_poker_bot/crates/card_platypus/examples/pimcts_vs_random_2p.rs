//! Quick PIMCTS-vs-random baseline on existing 2-player games (Bluff and
//! Kuhn poker). Confirms PIMCTS dominates a random opponent in 2-player
//! settings, which is the natural sanity check when the 3-player Oh Hell
//! baseline showed muddy results.
//!
//! Run with:
//!   cargo run --release --example pimcts_vs_random_2p

use std::time::Instant;

use card_platypus::{
    agents::Agent,
    algorithms::{ismcts::RandomRolloutEvaluator, open_hand_solver::OpenHandSolver, pimcts::PIMCTSBot},
};
use games::{
    gamestates::{
        bluff::Bluff,
        kuhn_poker::KuhnPoker,
    },
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
    let n_games = parse_env("OH_GAMES", 400);
    let rollouts = parse_env("OH_ROLLOUTS", 50);

    println!(
        "PIMCTS-vs-random 2-player baseline: {} games, PIMCTS rollouts={}",
        n_games, rollouts
    );
    println!(
        "{:>14} {:>10} {:>10} {:>10} {:>10} {:>10} {:>8}",
        "game",
        "pimcts_avg",
        "rand_avg",
        "win%",
        "tie%",
        "loss%",
        "secs"
    );

    // ---- Kuhn poker (Random rollouts) ----
    let start = Instant::now();
    let (pavg, ravg, w, t, l) = run_kuhn(n_games, rollouts);
    let elapsed = start.elapsed().as_secs_f64();
    print_row("kuhn+random", pavg, ravg, w, t, l, n_games, elapsed);

    // ---- Kuhn poker (OpenHandSolver) ----
    let start = Instant::now();
    let (pavg, ravg, w, t, l) = run_kuhn_oh(n_games, rollouts);
    let elapsed = start.elapsed().as_secs_f64();
    print_row("kuhn+oh_solv", pavg, ravg, w, t, l, n_games, elapsed);

    // ---- Bluff(2,2) (Random rollouts) ----
    let start = Instant::now();
    let (pavg, ravg, w, t, l) = run_bluff(n_games, rollouts);
    let elapsed = start.elapsed().as_secs_f64();
    print_row("bluff22+rand", pavg, ravg, w, t, l, n_games, elapsed);

    // ---- Bluff(2,2) (OpenHandSolver) ----
    let start = Instant::now();
    let (pavg, ravg, w, t, l) = run_bluff_oh(n_games, rollouts);
    let elapsed = start.elapsed().as_secs_f64();
    print_row("bluff22+ohs", pavg, ravg, w, t, l, n_games, elapsed);
}

#[allow(clippy::too_many_arguments)]
fn print_row(
    label: &str,
    pavg: f64,
    ravg: f64,
    w: usize,
    t: usize,
    l: usize,
    n_games: usize,
    elapsed: f64,
) {
    let g = n_games as f64;
    println!(
        "{:>14} {:>10.3} {:>10.3} {:>9.1}% {:>9.1}% {:>9.1}% {:>8.2}",
        label,
        pavg,
        ravg,
        100.0 * w as f64 / g,
        100.0 * t as f64 / g,
        100.0 * l as f64 / g,
        elapsed
    );
}

fn run_kuhn(n_games: usize, rollouts: usize) -> (f64, f64, usize, usize, usize) {
    let mut pavg = 0.0_f64;
    let mut ravg = 0.0_f64;
    let mut w = 0;
    let mut t = 0;
    let mut l = 0;
    let mut acts = Vec::new();

    for i in 0..n_games {
        let seed = i as u64;
        let mut rng: StdRng = SeedableRng::seed_from_u64(seed);
        let mut agent = PIMCTSBot::new(
            rollouts,
            RandomRolloutEvaluator::new(1),
            SeedableRng::seed_from_u64(seed.wrapping_add(1)),
        );
        // Alternate seat across games.
        let pimcts_pos = i % 2;
        let mut gs = KuhnPoker::new_state();
        while gs.is_chance_node() {
            gs.legal_actions(&mut acts);
            gs.apply_action(*acts.choose(&mut rng).unwrap());
        }
        while !gs.is_terminal() {
            let cp = gs.cur_player();
            let a = if cp == pimcts_pos {
                agent.step(&gs)
            } else {
                gs.legal_actions(&mut acts);
                *acts.choose(&mut rng).unwrap()
            };
            gs.apply_action(a);
        }
        let p_score = gs.evaluate(pimcts_pos);
        let r_score = gs.evaluate(1 - pimcts_pos);
        pavg += p_score;
        ravg += r_score;
        if p_score > r_score {
            w += 1;
        } else if p_score < r_score {
            l += 1;
        } else {
            t += 1;
        }
    }
    let g = n_games as f64;
    (pavg / g, ravg / g, w, t, l)
}

fn run_kuhn_oh(n_games: usize, rollouts: usize) -> (f64, f64, usize, usize, usize) {
    let mut pavg = 0.0_f64;
    let mut ravg = 0.0_f64;
    let mut w = 0;
    let mut t = 0;
    let mut l = 0;
    let mut acts = Vec::new();

    for i in 0..n_games {
        let seed = i as u64;
        let mut rng: StdRng = SeedableRng::seed_from_u64(seed);
        let mut agent = PIMCTSBot::new(
            rollouts,
            OpenHandSolver::default(),
            SeedableRng::seed_from_u64(seed.wrapping_add(1)),
        );
        let pimcts_pos = i % 2;
        let mut gs = KuhnPoker::new_state();
        while gs.is_chance_node() {
            gs.legal_actions(&mut acts);
            gs.apply_action(*acts.choose(&mut rng).unwrap());
        }
        while !gs.is_terminal() {
            let cp = gs.cur_player();
            let a = if cp == pimcts_pos {
                agent.step(&gs)
            } else {
                gs.legal_actions(&mut acts);
                *acts.choose(&mut rng).unwrap()
            };
            gs.apply_action(a);
        }
        let p_score = gs.evaluate(pimcts_pos);
        let r_score = gs.evaluate(1 - pimcts_pos);
        pavg += p_score;
        ravg += r_score;
        if p_score > r_score {
            w += 1;
        } else if p_score < r_score {
            l += 1;
        } else {
            t += 1;
        }
    }
    let g = n_games as f64;
    (pavg / g, ravg / g, w, t, l)
}

fn run_bluff(n_games: usize, rollouts: usize) -> (f64, f64, usize, usize, usize) {
    let mut pavg = 0.0_f64;
    let mut ravg = 0.0_f64;
    let mut w = 0;
    let mut t = 0;
    let mut l = 0;
    let mut acts = Vec::new();

    for i in 0..n_games {
        let seed = i as u64;
        let mut rng: StdRng = SeedableRng::seed_from_u64(seed);
        let mut agent = PIMCTSBot::new(
            rollouts,
            RandomRolloutEvaluator::new(1),
            SeedableRng::seed_from_u64(seed.wrapping_add(1)),
        );
        let pimcts_pos = i % 2;
        let mut gs = Bluff::new_state(2, 2);
        while gs.is_chance_node() {
            gs.legal_actions(&mut acts);
            gs.apply_action(*acts.choose(&mut rng).unwrap());
        }
        while !gs.is_terminal() {
            let cp = gs.cur_player();
            let a = if cp == pimcts_pos {
                agent.step(&gs)
            } else {
                gs.legal_actions(&mut acts);
                *acts.choose(&mut rng).unwrap()
            };
            gs.apply_action(a);
        }
        let p_score = gs.evaluate(pimcts_pos);
        let r_score = gs.evaluate(1 - pimcts_pos);
        pavg += p_score;
        ravg += r_score;
        if p_score > r_score {
            w += 1;
        } else if p_score < r_score {
            l += 1;
        } else {
            t += 1;
        }
    }
    let g = n_games as f64;
    (pavg / g, ravg / g, w, t, l)
}

fn run_bluff_oh(n_games: usize, rollouts: usize) -> (f64, f64, usize, usize, usize) {
    let mut pavg = 0.0_f64;
    let mut ravg = 0.0_f64;
    let mut w = 0;
    let mut t = 0;
    let mut l = 0;
    let mut acts = Vec::new();

    for i in 0..n_games {
        let seed = i as u64;
        let mut rng: StdRng = SeedableRng::seed_from_u64(seed);
        let mut agent = PIMCTSBot::new(
            rollouts,
            OpenHandSolver::default(),
            SeedableRng::seed_from_u64(seed.wrapping_add(1)),
        );
        let pimcts_pos = i % 2;
        let mut gs = Bluff::new_state(2, 2);
        while gs.is_chance_node() {
            gs.legal_actions(&mut acts);
            gs.apply_action(*acts.choose(&mut rng).unwrap());
        }
        while !gs.is_terminal() {
            let cp = gs.cur_player();
            let a = if cp == pimcts_pos {
                agent.step(&gs)
            } else {
                gs.legal_actions(&mut acts);
                *acts.choose(&mut rng).unwrap()
            };
            gs.apply_action(a);
        }
        let p_score = gs.evaluate(pimcts_pos);
        let r_score = gs.evaluate(1 - pimcts_pos);
        pavg += p_score;
        ravg += r_score;
        if p_score > r_score {
            w += 1;
        } else if p_score < r_score {
            l += 1;
        } else {
            t += 1;
        }
    }
    let g = n_games as f64;
    (pavg / g, ravg / g, w, t, l)
}
