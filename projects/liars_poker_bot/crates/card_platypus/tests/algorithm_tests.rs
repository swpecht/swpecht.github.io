use approx::assert_relative_eq;
use card_platypus::algorithms::{
    cfres::{self, CFRES},
    exploitability::exploitability,
    ismcts::Evaluator,
    open_hand_solver::OpenHandSolver,
};
use games::{
    gamestates::{bluff::Bluff, euchre::Euchre, kuhn_poker::KuhnPoker},
    get_games, GameState,
};
use rand::{rngs::StdRng, seq::SliceRandom, thread_rng, SeedableRng};
use rayon::prelude::*;

/// Confirm that the open hand solver with and without the cache gives the same results.
///
/// This is critical not only for ensuring proper results but also for determinism of agents
#[test]
fn test_alg_open_hand_solver_euchre() {
    let mut rng: StdRng = SeedableRng::seed_from_u64(51);
    let games = get_games(Euchre::game(), 1000, &mut rng);

    // Also use the euchre specific optimizations for the cached one
    let cached = OpenHandSolver::new_euchre();
    let no_cache = OpenHandSolver::new_without_cache();

    // Change to a non parallel iterator to see the error message
    games.into_par_iter().enumerate().for_each(|(i, mut gs)| {
        let mut actions = Vec::new();
        while !gs.is_terminal() {
            let c = cached.clone().evaluate_player(&gs, gs.cur_player());
            let no_c = no_cache.clone().evaluate_player(&gs, gs.cur_player());
            assert_eq!(c, no_c, "Different evaluations: {}: {}", i, gs);
            gs.legal_actions(&mut actions);
            let a = actions.choose(&mut thread_rng()).unwrap();
            gs.apply_action(*a);
        }
    });
}

#[test]
fn test_cfr_euchre() {
    cfres::feature::enable(cfres::feature::LinearCFR);

    let mut alg = CFRES::new_euchre(SeedableRng::seed_from_u64(43), 0);
    alg.train(1);
}

#[test]
fn test_cfres_nash_kuhn_poker() {
    cfres::feature::enable(cfres::feature::LinearCFR);
    // avoid lock contention
    cfres::feature::enable(cfres::feature::SingleThread);

    // Verify the nash equilibrium is reached. From https://en.wikipedia.org/wiki/Kuhn_poker
    let mut alg = CFRES::new_kp();
    alg.train(1_000_000);
    let exploitability = exploitability(|| (KuhnPoker::game().new)(), &mut alg).nash_conv;
    assert_relative_eq!(exploitability, 0.0, epsilon = 0.01);
}

#[test]
fn test_cfres_nash_bluff11() {
    cfres::feature::enable(cfres::feature::LinearCFR);
    // avoid lock contention
    cfres::feature::enable(cfres::feature::SingleThread);

    let mut alg = CFRES::new_bluff_11();

    alg.train(1_000_000);
    let exploitability = exploitability(|| (Bluff::game(1, 1).new)(), &mut alg).nash_conv;
    assert_relative_eq!(exploitability, 0.0, epsilon = 0.01);
}
