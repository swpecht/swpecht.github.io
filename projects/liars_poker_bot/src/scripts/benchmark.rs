use std::collections::HashMap;

use clap::{Args, ValueEnum};
use indicatif::ProgressBar;
use itertools::Itertools;
use liars_poker_bot::{
    agents::{Agent, PolicyAgent, RandomAgent},
    algorithms::{
        alphamu::AlphaMuBot,
        ismcts::{
            ChildSelectionPolicy, ISMCTBotConfig, ISMCTSBot, ISMCTSFinalPolicyType,
            ResampleFromInfoState,
        },
        open_hand_solver::OpenHandSolver,
        pimcts::PIMCTSBot,
    },
    game::{
        bluff::Bluff,
        euchre::{EPhase, Euchre, EuchreGameState},
        kuhn_poker::KuhnPoker,
        Game, GameState,
    },
};
use log::{debug, info};
use rand::{rngs::StdRng, seq::SliceRandom, thread_rng, SeedableRng};

use crate::GameType;

#[derive(ValueEnum, Clone, Copy, Debug)]
pub enum BenchmarkMode {
    FullGame,
    CardPlay,
}

#[derive(Args, Debug, Clone, Copy)]
pub struct BenchmarkArgs {
    #[clap(short, long, default_value_t = 20)]
    num_games: usize,
    #[clap(long, value_enum, default_value_t=GameType::Euchre)]
    game: GameType,
    mode: BenchmarkMode,
}

pub fn run_benchmark(args: BenchmarkArgs) {
    match args.mode {
        BenchmarkMode::FullGame => match args.game {
            GameType::KuhnPoker => run_full_game_benchmark(args, KuhnPoker::game()),
            GameType::Euchre => run_full_game_benchmark(args, Euchre::game()),
            GameType::Bluff11 => run_full_game_benchmark(args, Bluff::game(1, 1)),
            GameType::Bluff21 => run_full_game_benchmark(args, Bluff::game(2, 1)),
            GameType::Bluff22 => run_full_game_benchmark(args, Bluff::game(2, 2)),
        },
        BenchmarkMode::CardPlay => run_card_play_benchmark(args),
    }
}

/// Calculate the win-rate of first to 10 for each agent
fn run_full_game_benchmark<G: GameState + ResampleFromInfoState + Send>(
    args: BenchmarkArgs,
    game: Game<G>,
) {
    // all agents play the same games
    let mut game_rng = get_rng();
    // may need up to 19x the number fo full games to 10
    let games = get_games(game, args.num_games * 19, &mut game_rng);

    let mut agents: HashMap<String, &mut dyn Agent<G>> = HashMap::new();
    // let ra: &mut dyn Agent<G> = &mut RandomAgent::new();
    // agents.insert(ra.get_name(), ra);

    let a = &mut PolicyAgent::new(
        PIMCTSBot::new(50, OpenHandSolver::new(), get_rng()),
        get_rng(),
    );
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
        ISMCTSBot::new(3.0, 50, OpenHandSolver::new(), config),
        get_rng(),
    );
    agents.insert("ismcts".to_string(), ismcts);

    // let alphamu = &mut PolicyAgent::new(
    //     AlphaMuBot::new(OpenHandSolver::new(), 32, 10, get_rng()),
    //     get_rng(),
    // );
    // agents.insert("alphamu, open hand".to_string(), alphamu);

    score_games(args, agents, games);
}

/// Calculate the win-rate of first to 10 for each agent
fn score_games<G: GameState + ResampleFromInfoState + Send>(
    args: BenchmarkArgs,
    mut agents: HashMap<String, &mut dyn Agent<G>>,
    games: Vec<G>,
) {
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

            // Make sure that each "game" to 10 is identical, we may need up to 20 games for this to happen
            let mut chunked_games = Vec::new();
            for g in &games.clone().into_iter().chunks(19) {
                chunked_games.push(g.collect_vec());
            }

            let pb = ProgressBar::new(args.num_games as u64);
            for i in 0..args.num_games {
                let mut overall_games = chunked_games.pop().unwrap().into_iter();
                let mut game_score = [0, 0];
                // track the current game in the game of 10 for dealer tracking
                let mut cur_game = 0;
                while game_score[0] < 10 && game_score[1] < 10 {
                    let mut gs = overall_games.next().unwrap();
                    let agent_1_team = (cur_game % 2 + i % 2) % 2;
                    while !gs.is_terminal() {
                        // We alternate who starts as the dealer each game
                        // todo: in future should have different player start deal for each game
                        let agent_1_turn = gs.cur_player() % 2 == agent_1_team;
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

                    // Need to make sure the teams are consistent throughout
                    let r = [
                        gs.evaluate(agent_1_team),
                        gs.evaluate((agent_1_team + 1) % 2),
                    ];

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
                pb.inc(1);
            }

            pb.finish_and_clear();

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

/// Runs the benchmark for euchre, but only for the card play portion.
///
/// Uses PIMCTS to do the bidding for all players
fn run_card_play_benchmark(args: BenchmarkArgs) {
    assert!(matches!(args.game, GameType::Euchre));

    // all agents play the same games
    info!("generating games...");
    let mut game_rng = get_rng();
    let games = get_card_play_games(args.num_games * 19, &mut game_rng);
    info!("finished generated {} games", games.len());

    let mut agents: HashMap<String, &mut dyn Agent<EuchreGameState>> = HashMap::new();
    // let ra: &mut dyn Agent<G> = &mut RandomAgent::new();
    // agents.insert(ra.get_name(), ra);

    let a = &mut PolicyAgent::new(
        PIMCTSBot::new(32, OpenHandSolver::new(), get_rng()),
        get_rng(),
    );
    agents.insert("pimcts, 32 worlds hand".to_string(), a);

    let alphamu = &mut PolicyAgent::new(
        AlphaMuBot::new(OpenHandSolver::new(), 32, 20, get_rng()),
        get_rng(),
    );
    agents.insert("alphamu, 32 worlds, m=20".to_string(), alphamu);

    score_games(args, agents, games);
}

pub fn get_card_play_games(n: usize, rng: &mut StdRng) -> Vec<EuchreGameState> {
    let mut games = get_games(Euchre::game(), n, rng);

    let mut agent = PolicyAgent::new(
        PIMCTSBot::new(50, OpenHandSolver::new(), get_rng()),
        get_rng(),
    );

    fn bid(
        mut gs: EuchreGameState,
        agent: &mut PolicyAgent<PIMCTSBot<EuchreGameState, OpenHandSolver>>,
    ) -> EuchreGameState {
        while gs.phase() != EPhase::Play {
            let a = agent.step(&gs);
            gs.apply_action(a);
        }

        gs
    }

    let pb = ProgressBar::new(n as u64);
    games = games
        .into_iter()
        .map(|gs| {
            pb.inc(1);
            bid(gs, &mut agent)
        })
        .collect_vec();
    pb.finish_and_clear();

    games
}

pub fn get_games<T: GameState>(game: Game<T>, n: usize, rng: &mut StdRng) -> Vec<T> {
    let mut games = Vec::with_capacity(n);
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

pub fn get_rng() -> StdRng {
    StdRng::from_rng(thread_rng()).unwrap()
}
