use approx::assert_relative_eq;
use liars_poker_bot::{
    agents::{Agent, PolicyAgent},
    algorithms::{
        alphamu::AlphaMuBot, exploitability::exploitability, ismcts::Evaluator,
        open_hand_solver::OpenHandSolver, pimcts::PIMCTSBot,
    },
    cfragent::{cfres::CFRES, CFRAgent, CFRAlgorithm},
    database::memory_node_store::MemoryNodeStore,
    game::{
        bluff::Bluff,
        euchre::{actions::EAction, Euchre},
        get_games,
        kuhn_poker::KuhnPoker,
        GameState,
    },
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

// /// Confirm that alphamu gives the same results with and without optimizations.
// #[test]
// fn test_alpha_mu_optimizations_equivalent() {
//     let mut rng: StdRng = SeedableRng::seed_from_u64(51);
//     let mut actions = Vec::new();

//     let num_worlds = 3;
//     let m = 3;
//     let mut optimized = AlphaMuBot::new(OpenHandSolver::new(), num_worlds, m, rng.clone());
//     // optimized.use_optimizations = false;
//     let mut no_optimized = AlphaMuBot::new(OpenHandSolver::new(), num_worlds, m, rng.clone());
//     no_optimized.use_optimizations = false;

//     for i in 0..100 {
//         let mut gs = Euchre::new_state();
//         // let mut gs = Bluff::new_state(1, 1);
//         while gs.is_chance_node() {
//             gs.legal_actions(&mut actions);
//             let a = actions.choose(&mut rng).unwrap();
//             gs.apply_action(*a);
//         }

//         while !gs.is_terminal() {
//             let o = optimized.evaluate_player(&gs, gs.cur_player());
//             let no_o = no_optimized.evaluate_player(&gs, gs.cur_player());
//             if o != no_o {
//                 println!("{}: {}", i, gs);
//             }
//             assert_eq!(o, no_o);
//             gs.legal_actions(&mut actions);
//             let a = actions.choose(&mut rng).unwrap();
//             gs.apply_action(*a);
//         }
//     }

//     todo!("re-enable optimizations");
// }

/// AlphaMu with M=1 should be equivalent to PIMCTS
#[test]
fn alpha_mu_pimcts_equivalent() {
    let policy_rng: StdRng = SeedableRng::seed_from_u64(56);
    let agent_rng: StdRng = SeedableRng::seed_from_u64(57);
    let rollouts = 5;
    let mut alpha = PolicyAgent::new(
        AlphaMuBot::new(
            OpenHandSolver::new_euchre(),
            rollouts,
            1,
            policy_rng.clone(),
        ),
        agent_rng.clone(),
    );
    let mut pimcts = PolicyAgent::new(
        PIMCTSBot::new(rollouts, OpenHandSolver::new_euchre(), policy_rng),
        agent_rng,
    );
    let mut actions = Vec::new();

    let mut game_rng: StdRng = SeedableRng::seed_from_u64(23);
    for i in 0..100 {
        let mut gs = Euchre::new_state();

        while !gs.is_terminal() {
            if gs.is_chance_node() {
                gs.legal_actions(&mut actions);
                let a = actions.choose(&mut game_rng).unwrap();
                gs.apply_action(*a);
            } else {
                let alpha_action = alpha.step(&gs);
                let pimcts_action = pimcts.step(&gs);
                if alpha_action != pimcts_action {
                    println!("{}: {}", i, gs);
                    println!("alpha action: {}", EAction::from(alpha_action));
                    println!("pimcts action: {}", EAction::from(pimcts_action));
                    gs.apply_action(alpha_action);
                    println!(
                        "alpha action value by pimcts: {}",
                        pimcts.policy.evaluate_player(&gs, gs.cur_player())
                    );
                    gs.undo();
                    gs.apply_action(pimcts_action);
                    println!(
                        "pimcts action value by pimcts: {}",
                        pimcts.policy.evaluate_player(&gs, gs.cur_player())
                    );
                    gs.undo();
                }

                assert_eq!(alpha_action, pimcts_action);
                gs.apply_action(pimcts_action);
            }
        }
        // println!("{}", gs);
    }
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
    let gs = || (Euchre::game().new)();

    let mut agent = CFRAgent::new(gs, 42, MemoryNodeStore::default(), CFRAlgorithm::CFRCS);
    agent.train(1);
}

#[test]
fn test_cfres_nash_kuhn_poker() {
    // Verify the nash equilibrium is reached. From https://en.wikipedia.org/wiki/Kuhn_poker
    let mut alg = CFRES::new(|| (KuhnPoker::game().new)(), SeedableRng::seed_from_u64(43));

    alg.train(1_000_000);
    let exploitability = exploitability(|| (KuhnPoker::game().new)(), &mut alg).nash_conv;
    assert_relative_eq!(exploitability, 0.0, epsilon = 0.001);
}

#[test]
fn test_cfres_nash_bluff11() {
    // Verify the nash equilibrium is reached. From https://en.wikipedia.org/wiki/Kuhn_poker
    let mut alg = CFRES::new(|| (Bluff::game(1, 1).new)(), SeedableRng::seed_from_u64(43));

    alg.train(1_000_000);
    let exploitability = exploitability(|| (Bluff::game(1, 1).new)(), &mut alg).nash_conv;
    assert_relative_eq!(exploitability, 0.0, epsilon = 0.01);
}
