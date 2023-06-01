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
        bluff::Bluff,
        euchre::{Euchre, EuchreGameState},
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
fn test_alg_open_hand_solver_bluff_cache() {
    // verify cached and uncached versions give the same results
    let mut rng: StdRng = SeedableRng::seed_from_u64(100);
    let mut actions = Vec::new();

    let mut cached = OpenHandSolver::new();
    let mut no_cache = OpenHandSolver::new_without_cache();

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

// Disabling this test for now as it is highly sensative to world selection. And many
// isomorphic changes will impact world selection. But still good to verify ismorphic implementations
#[test]
fn test_alg_open_hand_solver_euchre() {
    let mut rng: StdRng = SeedableRng::seed_from_u64(51);
    let mut actions = Vec::new();

    let mut cached = OpenHandSolver::new();
    let mut no_cache = OpenHandSolver::new_without_cache();

    for _ in 0..10 {
        let mut gs = Euchre::new_state();
        while gs.is_chance_node() {
            gs.legal_actions(&mut actions);
            let a = actions.choose(&mut rng).unwrap();
            gs.apply_action(*a);
        }

        println!("{}", gs);
        let c = cached.evaluate(&gs);
        let no_c = no_cache.evaluate(&gs);
        assert_eq!(c[0], no_c[0]);
        assert_eq!(c[1], no_c[1]);
    }
}

#[test]
fn test_open_hand_solver_euchre_samples() {
    let mut e1 = OpenHandSolver::new_without_cache();
    let mut game = "TCQCQHAHTD|9HKHJDKDAD|AC9SQSTHJH|9CJCKCJSQD|AS|PPPP|H".to_string();
    let gs1 = EuchreGameState::from(game.as_str());
    let mut e2 = OpenHandSolver::new_without_cache();
    // manually downshit spade cards since some of them weren't dealt
    // game = game.replace("KS", "QS");
    game = game.replace("AS", "KS");
    let gs2 = EuchreGameState::from(game.as_str());

    let v1 = e1.evaluate_player(&gs1, 3);
    let v2 = e2.evaluate_player(&gs2, 3);

    assert_eq!(v1, v2);
}
