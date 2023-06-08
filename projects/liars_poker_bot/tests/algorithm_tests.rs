use approx::assert_relative_eq;
use liars_poker_bot::{
    agents::{Agent, PolicyAgent},
    algorithms::{
        alphamu::AlphaMuBot,
        exploitability::exploitability,
        ismcts::{ISMCTBotConfig, ISMCTSBot, ISMCTSFinalPolicyType, RandomRolloutEvaluator},
        open_hand_solver::OpenHandSolver,
        pimcts::PIMCTSBot,
    },
    cfragent::{CFRAgent, CFRAlgorithm},
    database::memory_node_store::MemoryNodeStore,
    game::{
        euchre::{Euchre, EuchreGameState},
        kuhn_poker::KuhnPoker,
        GameState,
    },
};
use rand::{rngs::StdRng, seq::SliceRandom, SeedableRng};

/// AlphaMu with M=1 should be equivalent to PIMCTS
#[test]
fn alpha_mu_pimcts_equivalent() {
    let policy_rng: StdRng = SeedableRng::seed_from_u64(56);
    let agent_rng: StdRng = SeedableRng::seed_from_u64(57);
    let rollouts = 10;
    let mut alpha = PolicyAgent::new(
        AlphaMuBot::new(OpenHandSolver::new(), rollouts, 1, policy_rng.clone()),
        agent_rng.clone(),
    );
    let mut pimcts = PolicyAgent::new(
        PIMCTSBot::new(rollouts, OpenHandSolver::new(), policy_rng),
        agent_rng,
    );
    let mut actions = Vec::new();

    let mut game_rng: StdRng = SeedableRng::seed_from_u64(54);
    for _ in 0..100 {
        let mut gs = Euchre::new_state();

        while !gs.is_terminal() {
            if gs.is_chance_node() {
                gs.legal_actions(&mut actions);
                let a = actions.choose(&mut game_rng).unwrap();
                gs.apply_action(*a);
            } else {
                let alpha_action = alpha.step(&gs);
                let pimcts_action = pimcts.step(&gs);
                assert_eq!(alpha_action, pimcts_action);
                gs.apply_action(pimcts_action);
            }
        }
    }
}

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
        RandomRolloutEvaluator::new(100),
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
