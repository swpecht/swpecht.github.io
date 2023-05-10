use approx::assert_relative_eq;
use liars_poker_bot::{
    algorithms::{
        exploitability::{self, exploitability},
        ismcts::{ISMCTSBot, RandomRolloutEvaluator},
    },
    cfragent::{CFRAgent, CFRAlgorithm},
    database::memory_node_store::MemoryNodeStore,
    game::kuhn_poker::KuhnPoker,
};
use rand::SeedableRng;

#[test]
fn test_ismcts_exploitability() {
    let mut ismcts = ISMCTSBot::new(
        KuhnPoker::game(),
        1.5,
        100,
        RandomRolloutEvaluator::new(100, SeedableRng::seed_from_u64(42)),
    );

    let e = exploitability(KuhnPoker::game(), &mut ismcts).nash_conv;
    assert_relative_eq!(e, 0.0, epsilon = 0.001);
}

#[test]
fn test_cfr_exploitability() {
    let ns = MemoryNodeStore::new();
    let mut agent = CFRAgent::new(KuhnPoker::game(), 1, 100_001, ns, CFRAlgorithm::CFRCS);

    let exploitability = exploitability(KuhnPoker::game(), &mut agent.ns).nash_conv;
    assert_relative_eq!(exploitability, 0.0, epsilon = 0.001);
}
