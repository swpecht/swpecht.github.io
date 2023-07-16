use liars_poker_bot::{
    cfragent::{CFRAgent, CFRAlgorithm},
    database::memory_node_store::MemoryNodeStore,
    game::euchre::Euchre,
};

pub fn run_pass_on_bower_cfr() {
    let gs = || (Euchre::game().new)();

    let mut agent = CFRAgent::new(gs, 42, MemoryNodeStore::default(), CFRAlgorithm::CFRCS);
    agent.train(1);
}
