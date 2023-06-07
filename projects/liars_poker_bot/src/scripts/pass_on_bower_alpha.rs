use std::collections::HashMap;

use itertools::Itertools;
use liars_poker_bot::{
    agents::{Agent, PolicyAgent, RandomAgent},
    algorithms::{
        alphamu::AlphaMuBot, ismcts::RandomRolloutEvaluator, open_hand_solver::OpenHandSolver,
        pimcts::PIMCTSBot,
    },
    game::{euchre::EuchreGameState, run_game},
};
use log::info;
use rand::{rngs::StdRng, seq::SliceRandom, thread_rng, SeedableRng};

use crate::Args;

use super::pass_on_bower::PassOnBowerIterator;

pub fn benchmark_pass_on_bower(args: Args) {
    let mut agents: HashMap<String, &mut dyn Agent<EuchreGameState>> = HashMap::new();

    let ra: &mut dyn Agent<EuchreGameState> = &mut RandomAgent::new();
    agents.insert(ra.get_name(), ra);

    let a = &mut PolicyAgent::new(PIMCTSBot::new(20, OpenHandSolver::new(), rng()), rng());
    agents.insert("pimcts, 20 worlds, open hand".to_string(), a);

    let a = &mut PolicyAgent::new(PIMCTSBot::new(10, OpenHandSolver::new(), rng()), rng());
    agents.insert("pimcts, 10 worlds, open hand".to_string(), a);

    let a = &mut PolicyAgent::new(
        PIMCTSBot::new(10, RandomRolloutEvaluator::new(10), rng()),
        rng(),
    );
    agents.insert("pimcts, 10 worlds, random".to_string(), a);

    let alphamu = &mut AlphaMuBot::new(OpenHandSolver::new(), 10, 2);
    agents.insert("alphamu, open hand".to_string(), alphamu);

    let agent_names = agents.keys().cloned().collect_vec();

    let generator = PassOnBowerIterator::new();
    let mut worlds = generator.collect_vec();
    worlds.shuffle(&mut rng());

    info!("starting benchmark, defended by: {}", "PIMCTS, n=20");

    for a2_name in agent_names {
        // this is the agent all oponents will play against in the 0 and 2 spot (team 0)
        // We re-initialize to ensure everyone is playing against the same agent
        let agent1 = &mut PolicyAgent::new(
            PIMCTSBot::new(20, OpenHandSolver::new(), SeedableRng::seed_from_u64(100)),
            SeedableRng::seed_from_u64(101),
        );

        let a2 = agents.remove(&a2_name).unwrap();

        let mut returns = vec![0.0; 4];

        // all agents play the same games
        let mut game_rng: StdRng = SeedableRng::seed_from_u64(42);
        for gs in worlds.clone().iter_mut().take(args.num_games) {
            let r = run_game(gs, agent1, &mut Some(a2), &mut game_rng);
            for (i, v) in r.iter().enumerate() {
                returns[i] += v;
            }
        }
        info!("{:?}\t{}", a2_name, returns[1] / args.num_games as f64);

        agents.insert(a2_name.clone(), a2);
    }
}

pub fn rng() -> StdRng {
    StdRng::from_rng(thread_rng()).unwrap()
}
