pub mod agents;
pub mod game_tree;
pub mod liars_poker;
pub mod minimax_agent;

use agents::{IncorporateBetAgent, RandomAgent};
use clap::Parser;

use liars_poker::LiarsPoker;
use minimax_agent::MetaMinimaxAgent;

use crate::{
    agents::{Agent, OwnDiceAgent},
    minimax_agent::MinimaxAgent,
};

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
        let ra = RandomAgent {
            name: "Random".to_string(),
        };

        let mma = MinimaxAgent {};
        let meta = MetaMinimaxAgent {};
        let oda = OwnDiceAgent {
            name: "OwnDiceAgent".to_string(),
        };

        let iba = IncorporateBetAgent {
            name: "IncorporateBetAgent".to_string(),
        };

        let agents: Vec<Box<dyn Agent>> = vec![
            Box::new(ra),
            Box::new(mma),
            Box::new(meta),
            Box::new(oda),
            Box::new(iba),
        ];

        for i in 0..agents.len() {
            for j in 0..agents.len() {
                let mut p1_wins = 0;
                let mut p2_wins = 0;
                for _ in 0..args.num_games {
                    let mut game = LiarsPoker::new();
                    let score = game.play(agents[i].as_ref(), agents[j].as_ref());
                    if score == 1 {
                        p1_wins += 1;
                    } else {
                        p2_wins += 1;
                    }
                }

                print!(
                    "{} wins: {},  {} wins: {}\n",
                    &agents[i].name(),
                    p1_wins,
                    &agents[j].name(),
                    p2_wins
                );
            }
        }
    } else {
        let meta = MetaMinimaxAgent {};

        let mma = MinimaxAgent {};

        let mut game = LiarsPoker::new();
        game.play(&mma, &meta);
    }
}
