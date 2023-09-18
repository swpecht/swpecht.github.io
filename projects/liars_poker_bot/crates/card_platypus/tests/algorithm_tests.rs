use approx::assert_relative_eq;
use card_platypus::{
    algorithms::{
        cfres::{self, CFRES},
        exploitability::exploitability,
        ismcts::Evaluator,
        open_hand_solver::OpenHandSolver,
    },
    cfragent::{CFRAgent, CFRAlgorithm},
    database::memory_node_store::MemoryNodeStore,
    game::{bluff::Bluff, euchre::Euchre, get_games, kuhn_poker::KuhnPoker, GameState},
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
fn test_cfr_exploitability() {
    let ns = MemoryNodeStore::default();
    let mut agent = CFRAgent::new(|| (KuhnPoker::game().new)(), 1, ns, CFRAlgorithm::CFRCS);
    agent.train(1_000_000);

    let exploitability = exploitability(|| (KuhnPoker::game().new)(), &mut agent.ns).nash_conv;
    assert_relative_eq!(exploitability, 0.0, epsilon = 0.001);
}

#[test]
fn test_cfr_euchre() {
    cfres::feature::enable(cfres::feature::NormalizeSuit);
    cfres::feature::enable(cfres::feature::LinearCFR);

    let mut alg = CFRES::new(|| (Euchre::game().new)(), SeedableRng::seed_from_u64(43));
    alg.train(1);
}

#[test]
fn test_cfres_nash_kuhn_poker() {
    cfres::feature::enable(cfres::feature::LinearCFR);

    // Verify the nash equilibrium is reached. From https://en.wikipedia.org/wiki/Kuhn_poker
    let mut alg = CFRES::new(|| (KuhnPoker::game().new)(), SeedableRng::seed_from_u64(43));

    alg.train(50_000);
    alg.train(50_000);
    let exploitability = exploitability(|| (KuhnPoker::game().new)(), &mut alg).nash_conv;
    assert_relative_eq!(exploitability, 0.0, epsilon = 0.01);
}

#[test]
fn test_cfres_nash_bluff11() {
    cfres::feature::enable(cfres::feature::LinearCFR);

    let mut alg = CFRES::new(|| (Bluff::game(1, 1).new)(), SeedableRng::seed_from_u64(43));

    alg.train(2_000_000);
    let exploitability = exploitability(|| (Bluff::game(1, 1).new)(), &mut alg).nash_conv;
    assert_relative_eq!(exploitability, 0.0, epsilon = 0.01);
}
