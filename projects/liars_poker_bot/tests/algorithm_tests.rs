use approx::assert_relative_eq;
use liars_poker_bot::{
    algorithms::{
        exploitability::exploitability,
        ismcts::{
            Evaluator, ISMCTBotConfig, ISMCTSBot, ISMCTSFinalPolicyType, RandomRolloutEvaluator,
        },
        open_hand_solver::OpenHandSolver,
    },
    cfragent::{CFRAgent, CFRAlgorithm},
    database::memory_node_store::MemoryNodeStore,
    game::{
        bluff::{Bluff, BluffActions},
        euchre::Euchre,
        kuhn_poker::KuhnPoker,
        GameState,
    },
};
use rand::{rngs::StdRng, seq::SliceRandom, SeedableRng};

#[test]
fn test_ismcts_exploitability() {
    let config = ISMCTBotConfig {
        final_policy_type: ISMCTSFinalPolicyType::NormalizedVisitedCount,
        ..Default::default()
    };

    let mut ismcts = ISMCTSBot::new(
        KuhnPoker::game(),
        1.5,
        10000,
        RandomRolloutEvaluator::new(100, SeedableRng::seed_from_u64(42)),
        config,
    );

    let e = exploitability(KuhnPoker::game(), &mut ismcts).nash_conv;
    assert_relative_eq!(e, 0.0, epsilon = 0.001);
}

#[test]
fn test_cfr_exploitability() {
    let ns = MemoryNodeStore::default();
    let mut agent = CFRAgent::new(KuhnPoker::game(), 1, ns, CFRAlgorithm::CFRCS);
    agent.train(1_000_000);

    let exploitability = exploitability(KuhnPoker::game(), &mut agent.ns).nash_conv;
    assert_relative_eq!(exploitability, 0.0, epsilon = 0.001);
}

#[test]
fn test_open_hand_solver_bluff_cache() {
    // verify cached and uncached versions give the same results
    let mut rng: StdRng = SeedableRng::seed_from_u64(100);
    let mut actions = Vec::new();

    let mut cached = OpenHandSolver::new(100, rng.clone());
    let mut no_cache = OpenHandSolver::new_without_cache(100, rng.clone());

    for _ in 0..100 {
        let mut gs = Bluff::new_state(2, 2);
        while gs.is_chance_node() {
            gs.legal_actions(&mut actions);
            let a = actions.choose(&mut rng).unwrap();
            gs.apply_action(*a);
        }

        while !gs.is_terminal() {
            println!("{}", gs);
            let c = cached.evaluate(&gs);
            let no_c = no_cache.evaluate(&gs);

            assert_eq!(c, no_c);

            gs.legal_actions(&mut actions);
            let a = actions.choose(&mut rng).unwrap();
            gs.apply_action(*a);
        }
    }
}

#[test]
fn test_open_hand_solver_euchre() {
    let mut rng: StdRng = SeedableRng::seed_from_u64(51);
    let mut actions = Vec::new();

    let mut cached = OpenHandSolver::new(10, rng.clone());
    let mut no_cache = OpenHandSolver::new_without_cache(10, rng.clone());

    for _ in 0..10 {
        let mut gs = Euchre::new_state();
        while gs.is_chance_node() {
            gs.legal_actions(&mut actions);
            let a = actions.choose(&mut rng).unwrap();
            gs.apply_action(*a);
        }

        // todo -- once support resampling for later phases, have this iterate
        // through all to verify state. For now we just apply a single pick / pass actions
        // while !gs.is_terminal() {
        for _ in 0..1 {
            println!("{}", gs);
            let c = cached.evaluate(&gs);
            let no_c = no_cache.evaluate(&gs);
            assert_eq!(c, no_c);

            gs.legal_actions(&mut actions);
            let a = actions.choose(&mut rng).unwrap();
            gs.apply_action(*a);
        }
    }
}
