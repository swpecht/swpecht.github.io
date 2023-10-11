use card_platypus::{
    agents::{Agent, PolicyAgent, RandomAgent},
    algorithms::{
        ismcts::RandomRolloutEvaluator, open_hand_solver::OpenHandSolver, pimcts::PIMCTSBot,
    },
};
use games::{
    gamestates::euchre::{actions::EAction, EuchreGameState},
    resample::ResampleFromInfoState,
    GameState,
};
use itertools::Itertools;
use log::{debug, info};
use rand::{rngs::StdRng, seq::SliceRandom, SeedableRng};

use crate::scripts::benchmark::get_rng;

use super::pass_on_bower::PassOnBowerIterator;

pub fn benchmark_pass_on_bower(num_games: usize) {
    let mut agents: Vec<(&str, &mut dyn Agent<EuchreGameState>)> = Vec::new();

    let policy_rng: StdRng = SeedableRng::seed_from_u64(56);
    let agent_rng: StdRng = SeedableRng::seed_from_u64(57);

    let ra: &mut dyn Agent<EuchreGameState> = &mut RandomAgent::new();
    agents.push(("Random agent", ra));

    let a = &mut PolicyAgent::new(
        PIMCTSBot::new(10, RandomRolloutEvaluator::new(10), policy_rng.clone()),
        agent_rng.clone(),
    );
    agents.push(("pimcts, 10 worlds, random", a));

    let a = &mut PolicyAgent::new(
        PIMCTSBot::new(10, OpenHandSolver::new_euchre(), policy_rng.clone()),
        agent_rng.clone(),
    );
    agents.push(("pimcts, 10 worlds, open hand", a));

    let a = &mut PolicyAgent::new(
        PIMCTSBot::new(20, OpenHandSolver::new_euchre(), policy_rng.clone()),
        agent_rng.clone(),
    );
    agents.push(("pimcts, 100 worlds, open hand", a));

    // let config = ISMCTBotConfig {
    //     final_policy_type: ISMCTSFinalPolicyType::NormalizedVisitedCount,
    //     ..Default::default()
    // };
    // let ismcts = &mut ISMCTSBot::new(Euchre::game(), 1.5, 100, OpenHandSolver::new(), config);
    // agents.push(("ismcts, 100 simulations", ismcts));

    let worlds = get_pass_on_bower_deals(num_games, &mut get_rng());

    info!("starting benchmark, defended by: {}", "PIMCTS, n=100");

    for (name, agent) in agents.into_iter() {
        // this is the agent all oponents will play against in the 0 and 2 spot (team 0)
        // We re-initialize to ensure everyone is playing against the same agent
        let agent1 = &mut PolicyAgent::new(
            PIMCTSBot::new(
                100,
                OpenHandSolver::new_euchre(),
                SeedableRng::seed_from_u64(100),
            ),
            SeedableRng::seed_from_u64(101),
        );

        let mut returns = [0.0; 4];

        // all agents play the same games
        for gs in worlds.clone().iter_mut() {
            while !gs.is_terminal() {
                let a = if gs.cur_player() % 2 == 0 {
                    agent1.step(gs)
                } else {
                    agent.step(gs)
                };

                debug!("{}: {}: {}", name, gs, EAction::from(a));
                gs.apply_action(a);
            }
            for (p, r) in returns.iter_mut().enumerate() {
                *r += gs.evaluate(p);
            }
        }
        info!("{:?}\t{}", name, returns[1] / num_games as f64);
    }
}

pub fn get_pass_on_bower_deals(n: usize, rng: &mut StdRng) -> Vec<EuchreGameState> {
    let generator = PassOnBowerIterator::new();
    let mut worlds = generator
        .take(n)
        .map(|w: EuchreGameState| w.resample_from_istate(w.cur_player(), rng))
        .collect_vec();
    worlds.shuffle(rng);
    worlds
}
