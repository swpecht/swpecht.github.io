use std::collections::HashMap;

use itertools::Itertools;
use liars_poker_bot::{
    agents::{Agent, PolicyAgent, RandomAgent},
    algorithms::{
        ismcts::{RandomRolloutEvaluator, ResampleFromInfoState},
        open_hand_solver::OpenHandSolver,
    },
    game::{euchre::Euchre, kuhn_poker::KuhnPoker, run_game, Game, GameState},
};
use rand::{rngs::StdRng, thread_rng, SeedableRng};

use crate::{Args, GameType};

pub fn run_benchmark(args: Args) {
    match args.game {
        GameType::KuhnPoker => run_benchmark_for_game(args, KuhnPoker::game()),
        GameType::Euchre => run_benchmark_for_game(args, Euchre::game()),
        GameType::Bluff11 => todo!(),
        GameType::Bluff21 => todo!(),
        GameType::Bluff22 => todo!(),
    }
}

fn run_benchmark_for_game<G: GameState + ResampleFromInfoState + Send>(args: Args, game: Game<G>) {
    let mut agents: HashMap<String, &mut dyn Agent<G>> = HashMap::new();
    let ra: &mut dyn Agent<G> = &mut RandomAgent::new();
    agents.insert(ra.get_name(), ra);

    let a = &mut PolicyAgent::new(OpenHandSolver::new(20, rng()), rng());
    agents.insert("pimcts, 20 worlds, open hand".to_string(), a);

    let a = &mut PolicyAgent::new(RandomRolloutEvaluator::new(20, rng()), rng());
    agents.insert("pimcts, 20 worlds, random rollout".to_string(), a);

    // let config = ISMCTBotConfig::default();
    // let ismcts = &mut ISMCTSBot::new(
    //     game.clone(),
    //     1.5,
    //     100,
    //     RandomRolloutEvaluator::new(100, SeedableRng::seed_from_u64(1)),
    //     config,
    // );
    // agents.insert(ismcts.get_name(), ismcts);

    // let alphamu = &mut AlphaMuBot::new(
    //     RandomRolloutEvaluator::new(100, SeedableRng::seed_from_u64(1)),
    //     30,
    //     30,
    // );
    // agents.insert(alphamu.get_name(), alphamu);

    // let mut cfr = CFRAgent::new(
    //     game.clone(),
    //     42,
    //     MemoryNodeStore::default(),
    //     CFRAlgorithm::CFRCS,
    // );
    // cfr.train(10000);
    // agents.insert(cfr.get_name(), &mut cfr);

    let agent_names = agents.keys().cloned().collect_vec();

    for a1_name in agent_names.clone() {
        for a2_name in agent_names.clone() {
            let a1 = agents.remove(&a1_name).unwrap();
            let mut a2 = if a1_name != a2_name {
                Some(agents.remove(&a2_name).unwrap())
            } else {
                None
            };

            let mut returns = vec![0.0; 4];
            for _ in 0..args.num_games {
                let r = run_game(&mut (game.new)(), a1, &mut a2, &mut rng());
                for (i, v) in r.iter().enumerate() {
                    returns[i] += v;
                }
            }
            println!(
                "{:?}\t{:?}\t{}\t{}",
                a1_name,
                a2_name,
                returns[0] / args.num_games as f64,
                returns[1] / args.num_games as f64
            );

            agents.insert(a1_name.clone(), a1);
            if a1_name != a2_name {
                agents.insert(a2_name.clone(), a2.unwrap());
            }
        }
    }
}

fn rng() -> StdRng {
    StdRng::from_rng(thread_rng()).unwrap()
}
