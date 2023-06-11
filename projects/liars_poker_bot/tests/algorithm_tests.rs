use approx::assert_relative_eq;
use liars_poker_bot::{
    agents::{Agent, PolicyAgent},
    algorithms::{
        alphamu::AlphaMuBot,
        exploitability::exploitability,
        ismcts::{
            Evaluator, ISMCTBotConfig, ISMCTSBot, ISMCTSFinalPolicyType, RandomRolloutEvaluator,
        },
        open_hand_solver::OpenHandSolver,
        pimcts::PIMCTSBot,
    },
    cfragent::{CFRAgent, CFRAlgorithm},
    database::memory_node_store::MemoryNodeStore,
    game::{
        euchre::{actions::EAction, Euchre},
        kuhn_poker::KuhnPoker,
        GameState,
    },
};
use rand::{rngs::StdRng, seq::SliceRandom, SeedableRng};

/// Confirm that the open hand solver with and without the cache gives the same results.
///
/// This is critical not only for ensuring proper results but also for determinism of agents
#[test]
fn test_alg_open_hand_solver_euchre() {
    let mut rng: StdRng = SeedableRng::seed_from_u64(51);
    let mut actions = Vec::new();

    let mut cached = OpenHandSolver::new();
    let mut no_cache = OpenHandSolver::new_without_cache();

    for i in 0..1000 {
        let mut gs = Euchre::new_state();
        // let mut gs = Bluff::new_state(1, 1);
        while gs.is_chance_node() {
            gs.legal_actions(&mut actions);
            let a = actions.choose(&mut rng).unwrap();
            gs.apply_action(*a);
        }

        while !gs.is_terminal() {
            let c = cached.evaluate_player(&gs, gs.cur_player());
            let no_c = no_cache.evaluate_player(&gs, gs.cur_player());
            if c != no_c {
                println!("{}: {}", i, gs);
            }
            assert_eq!(c, no_c);
            gs.legal_actions(&mut actions);
            let a = actions.choose(&mut rng).unwrap();
            gs.apply_action(*a);
        }
    }
}

/// AlphaMu with M=1 should be equivalent to PIMCTS
#[test]
fn alpha_mu_pimcts_equivalent() {
    let policy_rng: StdRng = SeedableRng::seed_from_u64(56);
    let agent_rng: StdRng = SeedableRng::seed_from_u64(57);
    let rollouts = 5;
    let mut alpha = PolicyAgent::new(
        AlphaMuBot::new(OpenHandSolver::new(), rollouts, 1, policy_rng.clone()),
        agent_rng.clone(),
    );
    let mut pimcts = PolicyAgent::new(
        PIMCTSBot::new(rollouts, OpenHandSolver::new(), policy_rng),
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
