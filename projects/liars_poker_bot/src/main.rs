pub mod agents;
pub mod cfr_agent;
pub mod game;
pub mod game_tree;
pub mod liars_poker;
pub mod minimax_agent;

use agents::AlwaysFirstAgent;
use agents::{IncorporateBetAgent, RandomAgent};
use clap::Parser;

use clap::clap_derive::ArgEnum;
use game::RPSState;

use game::RPS;
use liars_poker::{LPGameState, LiarsPoker};

use crate::{
    agents::{Agent, OwnDiceAgent},
    game::Game,
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
        let p1 = &RandomAgent {} as &dyn Agent<RPSState>;
        let p2 = &MinimaxAgent {} as &dyn Agent<RPSState>;

        let mut running_score = 0;
        for _ in 0..args.num_games {
            let mut game = RPS {};
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

fn run_lp_benchmark(args: Args) {
    let ra = RandomAgent {};
    let mma = MinimaxAgent {};
    let oda = OwnDiceAgent {
        name: "OwnDiceAgent".to_string(),
    };

    let iba = IncorporateBetAgent {
        name: "IncorporateBetAgent".to_string(),
    };

    let agents: Vec<Box<dyn Agent<LPGameState>>> =
        vec![Box::new(ra), Box::new(mma), Box::new(oda), Box::new(iba)];

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
}

fn run_rps_benchmark(args: Args) {
    let ra = RandomAgent {};
    let mma = MinimaxAgent {};
    let af = AlwaysFirstAgent {};

    let agents: Vec<Box<dyn Agent<RPSState>>> = vec![Box::new(ra), Box::new(mma), Box::new(af)];

    for i in 0..agents.len() {
        for j in 0..agents.len() {
            let mut p1_wins = 0;
            let mut p2_wins = 0;
            for _ in 0..args.num_games {
                let mut game = RPS {};
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
}
