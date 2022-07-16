pub mod liars_poker;

use clap::Parser;
use liars_poker::{GameState, LiarsPoker};
use log::*;
use rand::Rng;

/// Agent that randomly chooses moves
fn random_agent(possible_moves: &Vec<GameState>) -> usize {
    debug!("Evaluating moves: {:#?}", possible_moves);
    let mut rng = rand::thread_rng();
    return rng.gen_range(0..possible_moves.len());
}

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(short, long, value_parser, default_value_t = 5)]
    num_games: usize,

    #[clap(short, long, action)]
    quiet: bool,
}

fn main() {
    let args = Args::parse();

    print!("{}", args.quiet);

    stderrlog::new()
        .module(module_path!())
        .quiet(args.quiet)
        .verbosity(log::Level::Debug)
        .timestamp(stderrlog::Timestamp::Second)
        .init()
        .unwrap();

    let mut p1_wins = 0;
    let mut p2_wins = 0;

    for _ in 0..args.num_games {
        let mut game = LiarsPoker::new();
        let mut score = game.step(random_agent);
        while score == 0 {
            score = game.step(random_agent);
        }

        if score == 1 {
            p1_wins += 1;
        } else {
            p2_wins += 1;
        }
    }

    print!("P1 wins: {},  P2 wins: {}", p1_wins, p2_wins)
}
