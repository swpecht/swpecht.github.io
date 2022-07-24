pub mod agents;
pub mod cfr_agent;
pub mod game;
pub mod game_tree;
pub mod liars_poker;
pub mod minimax_agent;

use agents::{IncorporateBetAgent, RandomAgent};
use clap::Parser;

use game::RPSState;

use game::RPS;
use liars_poker::{LPAction, LPGameState, LiarsPoker};
use minimax_agent::MetaMinimaxAgent;

use crate::agents::AlwaysFirstAgent;
use crate::cfr_agent::CFRAgent;
use crate::game::RPSAction;
use crate::{
    agents::{Agent, OwnDiceAgent},
    game::Game,
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
        let ra = RandomAgent {};

        let mma = MinimaxAgent {};
        let meta = MetaMinimaxAgent {};
        let oda = OwnDiceAgent {
            name: "OwnDiceAgent".to_string(),
        };

        let iba = IncorporateBetAgent {
            name: "IncorporateBetAgent".to_string(),
        };

        let agents: Vec<Box<dyn Agent<LPGameState, LPAction>>> = vec![
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
        let p1 = &AlwaysFirstAgent {} as &dyn Agent<RPSState, RPSAction>;
        let p2 = &CFRAgent::new() as &dyn Agent<RPSState, RPSAction>;

        let mut running_score = 0;
        for _ in 0..args.num_games {
            let mut game = RPS::new();
            running_score += game.play(p1, p2);
        }

        // TODO: figure out why the CFR agent isn't winning more;

        println!(
            "{} vs {}, score over {} games: {}",
            p1.name(),
            p2.name(),
            args.num_games,
            running_score
        );
    }
}
