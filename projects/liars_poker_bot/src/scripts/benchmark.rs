use std::collections::HashMap;

use itertools::Itertools;
use liars_poker_bot::{
    agents::{Agent, PolicyAgent},
    algorithms::{
        alphamu::AlphaMuBot,
        ismcts::{
            ChildSelectionPolicy, ISMCTBotConfig, ISMCTSBot, ISMCTSFinalPolicyType,
            ResampleFromInfoState,
        },
        open_hand_solver::OpenHandSolver,
        pimcts::PIMCTSBot,
    },
    game::{bluff::Bluff, euchre::Euchre, kuhn_poker::KuhnPoker, Game, GameState},
};
use log::{debug, info};
use rand::{rngs::StdRng, seq::SliceRandom, thread_rng, SeedableRng};

use crate::{Args, GameType};

pub struct BenchmarkArgs {}

pub fn run_benchmark(args: Args) {
    match args.game {
        GameType::KuhnPoker => run_benchmark_for_game(args, KuhnPoker::game()),
        GameType::Euchre => run_benchmark_for_game(args, Euchre::game()),
        GameType::Bluff11 => run_benchmark_for_game(args, Bluff::game(1, 1)),
        GameType::Bluff21 => run_benchmark_for_game(args, Bluff::game(2, 1)),
        GameType::Bluff22 => run_benchmark_for_game(args, Bluff::game(2, 2)),
    }
}

/// Calculate the win-rate of first to 10 for each agent
fn run_benchmark_for_game<G: GameState + ResampleFromInfoState + Send>(args: Args, game: Game<G>) {
    // all agents play the same games
    let mut game_rng = rng();
    // may need up to 20x the number fo full games to 10
    let games = get_games(game, args.num_games * 20, &mut game_rng);

    let mut agents: HashMap<String, &mut dyn Agent<G>> = HashMap::new();
    // let ra: &mut dyn Agent<G> = &mut RandomAgent::new();
    // agents.insert(ra.get_name(), ra);

    let a = &mut PolicyAgent::new(PIMCTSBot::new(50, OpenHandSolver::new(), rng()), rng());
    agents.insert("pimcts, 50 worlds, open hand".to_string(), a);

    // let a = &mut PolicyAgent::new(
    //     PIMCTSBot::new(10, RandomRolloutEvaluator::new(10), rng()),
    //     rng(),
    // );
    // agents.insert("pimcts, 10 worlds, random".to_string(), a);

    // Based on tuning run for 100 games
    // https://docs.google.com/spreadsheets/d/1AGjEaqjCkuuWveUBqbOBOMH0SPHPQ_YhH1jRHij7ErY/edit#gid=1418816031
    let config = ISMCTBotConfig {
        child_selection_policy: ChildSelectionPolicy::Uct,
        final_policy_type: ISMCTSFinalPolicyType::MaxVisitCount,
        max_world_samples: -1, // unlimited samples
    };
    let ismcts = &mut PolicyAgent::new(
        ISMCTSBot::new(3.0, 100, OpenHandSolver::new(), config),
        rng(),
    );
    agents.insert("ismcts".to_string(), ismcts);

    // let alphamu =
    //     &mut PolicyAgent::new(AlphaMuBot::new(OpenHandSolver::new(), 20, 3, rng()), rng());
    // agents.insert("alphamu, open hand".to_string(), alphamu);

    let agent_names = agents.keys().cloned().collect_vec();

    for a1_name in agent_names.clone() {
        for a2_name in agent_names.clone() {
            // disable self play for now
            if a1_name == a2_name {
                continue;
            }

            let a1 = agents.remove(&a1_name).unwrap();
            let mut a2 = if a1_name != a2_name {
                Some(agents.remove(&a2_name).unwrap())
            } else {
                None
            };

            debug!("starting play for {} vs {}", a1_name, a2_name);
            let mut games_won = vec![0; 2];
            let mut game_source = games.clone().into_iter();

            for i in 0..args.num_games {
                let mut game_score = [0, 0];
                // track the current game in the game of 10 for dealer tracking
                let mut cur_game = 0;
                while game_score[0] < 10 && game_score[1] < 10 {
                    let mut gs = game_source.next().unwrap();
                    while !gs.is_terminal() {
                        // We alternate who starts as the dealer each game
                        // todo: in future should have different player start deal for each game
                        let agent_1_turn = gs.cur_player() % 2 == cur_game % 2;
                        // info!(
                        //     "cur_game: {}, cur_player: {}, is agent 1 turn?: {}: {}",
                        //     cur_game,
                        //     gs.cur_player(),
                        //     agent_1_turn,
                        //     gs
                        // );
                        let a = match (agent_1_turn, a2.is_some()) {
                            (true, true) => a1.step(&gs),
                            (false, true) => a2.as_mut().unwrap().step(&gs),
                            (_, false) => todo!(), // a1.step(&gs), // only agent a1
                        };
                        gs.apply_action(a);
                    }

                    let r = [gs.evaluate(0), gs.evaluate(1)];

                    game_score[0] += r[0].max(0.0) as u8;
                    game_score[1] += r[1].max(0.0) as u8;
                    cur_game += 1;
                }

                info!(
                    "\t{}\t{}\t{}\t{}",
                    a1_name, a2_name, game_score[0], game_score[1]
                );

                let team_0_win = game_score[0] >= 10;
                if team_0_win {
                    games_won[0] += 1;
                } else {
                    games_won[1] += 1;
                }
            }

            println!(
                "{:?}\t{:?}\t{}",
                a1_name,
                a2_name,
                games_won[0] as f64 / args.num_games as f64
            );

            agents.insert(a1_name.clone(), a1);
            if a1_name != a2_name {
                agents.insert(a2_name.clone(), a2.unwrap());
            }
        }
    }
}

fn run_backmark_card_play_only() {}

pub fn get_games<T: GameState>(game: Game<T>, n: usize, rng: &mut StdRng) -> Vec<T> {
    let mut games = Vec::new();
    let mut actions = Vec::new();

    for _ in 0..n {
        let mut gs = (game.new)();
        while gs.is_chance_node() {
            gs.legal_actions(&mut actions);
            let a = actions.choose(rng).unwrap();
            gs.apply_action(*a);
            actions.clear();
        }

        games.push(gs);
    }
    games
}

pub fn rng() -> StdRng {
    StdRng::from_rng(thread_rng()).unwrap()
}
