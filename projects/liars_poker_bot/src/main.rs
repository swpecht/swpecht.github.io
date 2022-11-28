pub mod agents;
pub mod game;
pub mod game_tree;
pub mod liars_poker;
pub mod minimax_agent;

use agents::AlwaysFirstAgent;
use agents::RandomAgent;
use clap::Parser;

use clap::clap_derive::ArgEnum;
use game::RPSState;

use liars_poker::LPGameState;

use crate::agents::full_rollout;
use crate::agents::minimax_propogation;
use crate::agents::random_scorer;
use crate::agents::TreeAgent;
use crate::game::play;
use crate::game::GameState;
use crate::{
    agents::{Agent, OwnDiceAgent},
    minimax_agent::MinimaxAgent,
};

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ArgEnum, Debug)]
enum GameType {
    RPS,
    LP,
}

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(short, long, value_parser, default_value_t = 5)]
    num_games: usize,

    #[clap(short, long, action)]
    quiet: bool,

    #[clap(short, long, action)]
    benchmark: bool,

    #[clap(arg_enum, value_parser, default_value_t = GameType::RPS)]
    game: GameType,
}

fn main() {
    let args = Args::parse();

    stderrlog::new()
        .module(module_path!())
        .quiet(args.quiet)
        .verbosity(log::Level::Debug)
        .timestamp(stderrlog::Timestamp::Second)
        .init()
        .unwrap();

    if args.benchmark {
        match args.game {
            GameType::LP => run_lp_benchmark(args),
            GameType::RPS => run_rps_benchmark(args),
        }
    } else {
        let mut running_score = 0;
        for _ in 0..args.num_games {
            let mut g = RPSState::new();

            let mut p1 = TreeAgent::new(
                "random_tree",
                &g,
                full_rollout,
                random_scorer,
                minimax_propogation,
            );
            let mut p2 = RandomAgent::new(&g);

            running_score += play(&mut g, &mut p1, &mut p2);
        }

        println!(
            "{} vs {}, score over {} games: {}",
            "p1", "p2", args.num_games, running_score
        );
    }
}

fn run_lp_benchmark(args: Args) {
    let agents = agents!(
        LPGameState,
        RandomAgent<LPGameState>,
        MinimaxAgent<LPGameState>,
        OwnDiceAgent
    );

    for i in 0..agents.len() {
        for j in 0..agents.len() {
            let mut p1_wins = 0;
            let mut p2_wins = 0;
            for _ in 0..args.num_games {
                let g = LPGameState::new();
                let score = play(
                    &mut g.clone(),
                    agents[i](&g).as_mut(),
                    agents[j](&g).as_mut(),
                );
                if score == 1 {
                    p1_wins += 1;
                } else {
                    p2_wins += 1;
                }
            }

            let g = LPGameState::new();
            print!(
                "{} wins: {},  {} wins: {}\n",
                &agents[i](&g).name(),
                p1_wins,
                &agents[j](&g).name(),
                p2_wins
            );
        }
    }
}

fn run_rps_benchmark(args: Args) {
    let agents = agents!(
        RPSState,
        RandomAgent<RPSState>,
        MinimaxAgent<RPSState>,
        AlwaysFirstAgent<RPSState>
    );

    for i in 0..agents.len() {
        for j in 0..agents.len() {
            let mut total_score = 0;
            for _ in 0..args.num_games {
                let g = RPSState::new();
                let score = play(
                    &mut g.clone(),
                    agents[i](&g).as_mut(),
                    agents[j](&g).as_mut(),
                );
                total_score += score;
            }

            let g = RPSState::new();
            print!(
                "{} vs {}: {}\n",
                &agents[i](&g).name(),
                &agents[j](&g).name(),
                total_score
            );
        }
    }
}

#[macro_export]
macro_rules! agents {
    ( $game:ty, $( $x:ty ),* ) => {
        {
            let mut temp_vec: Vec<fn(&$game) -> Box<dyn Agent<$game>>> = Vec::new();
            $(
                temp_vec.push(|g: &$game| -> Box<dyn Agent<$game>> {
                    Box::new(<$x>::new(g))
                });
            )*
            temp_vec
        }
    };
}
