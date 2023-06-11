use itertools::Itertools;
use liars_poker_bot::{
    agents::{Agent, PolicyAgent, RandomAgent},
    algorithms::{
        alphamu::AlphaMuBot,
        ismcts::{RandomRolloutEvaluator, ResampleFromInfoState},
        open_hand_solver::OpenHandSolver,
        pimcts::PIMCTSBot,
    },
    game::{
        euchre::{actions::EAction, EuchreGameState},
        GameState,
    },
};
use log::{debug, info};
use rand::{rngs::StdRng, seq::SliceRandom, thread_rng, SeedableRng};

use crate::Args;

use super::pass_on_bower::PassOnBowerIterator;

pub fn benchmark_pass_on_bower(args: Args) {
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
        PIMCTSBot::new(10, OpenHandSolver::new(), policy_rng.clone()),
        agent_rng.clone(),
    );
    agents.push(("pimcts, 10 worlds, open hand", a));

    let a = &mut PolicyAgent::new(
        PIMCTSBot::new(20, OpenHandSolver::new(), policy_rng.clone()),
        agent_rng.clone(),
    );
    agents.push(("pimcts, 100 worlds, open hand", a));

    // let config = ISMCTBotConfig {
    //     final_policy_type: ISMCTSFinalPolicyType::NormalizedVisitedCount,
    //     ..Default::default()
    // };
    // let ismcts = &mut ISMCTSBot::new(Euchre::game(), 1.5, 100, OpenHandSolver::new(), config);
    // agents.push(("ismcts, 100 simulations", ismcts));

    let alphamu = &mut PolicyAgent::new(
        AlphaMuBot::new(OpenHandSolver::new(), 10, 1, policy_rng.clone()),
        agent_rng.clone(),
    );
    agents.push(("alphamu, open hand, m=1, 10 worlds", alphamu));

    let alphamu = &mut PolicyAgent::new(
        AlphaMuBot::new(OpenHandSolver::new(), 10, 5, policy_rng),
        agent_rng,
    );
    agents.push(("alphamu, open hand", alphamu));

    let generator = PassOnBowerIterator::new();
    let mut worlds = generator.collect_vec();
    worlds.shuffle(&mut rng());
    let worlds = worlds
        .iter()
        .take(args.num_games)
        .map(|w| w.resample_from_istate(w.cur_player(), &mut rng()))
        .collect_vec();

    info!("starting benchmark, defended by: {}", "PIMCTS, n=100");

    for (name, agent) in agents.into_iter() {
        // this is the agent all oponents will play against in the 0 and 2 spot (team 0)
        // We re-initialize to ensure everyone is playing against the same agent
        let agent1 = &mut PolicyAgent::new(
            PIMCTSBot::new(100, OpenHandSolver::new(), SeedableRng::seed_from_u64(100)),
            SeedableRng::seed_from_u64(101),
        );

        let mut returns = vec![0.0; 4];

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
        info!("{:?}\t{}", name, returns[1] / args.num_games as f64);
    }
}

pub fn rng() -> StdRng {
    StdRng::from_rng(thread_rng()).unwrap()
}
