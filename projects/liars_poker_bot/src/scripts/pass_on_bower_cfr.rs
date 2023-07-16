use liars_poker_bot::{
    algorithms::ismcts::ResampleFromInfoState,
    cfragent::{CFRAgent, CFRAlgorithm},
    database::memory_node_store::MemoryNodeStore,
    game::euchre::EuchreGameState,
};
use rand::thread_rng;

pub fn run_pass_on_bower_cfr() {
    let generator = || {
        // can't just do this as the opponent CFR will learn to exploit player 3's exact cards, TBD how big of a deal this is in practice
        let gs = EuchreGameState::from("JcQcKcAc9s|TsQsKsAsJh|KhAh9dTdJd|9cTc9hThQh|Js|PPP");
        gs.resample_from_istate(3, &mut thread_rng())
    };

    let mut agent = CFRAgent::new(
        generator,
        42,
        MemoryNodeStore::default(),
        CFRAlgorithm::CFRCS,
    );
    for i in 0..3 {
        println!("starting iteration {}...", i);
        agent.train(1);
    }
}
