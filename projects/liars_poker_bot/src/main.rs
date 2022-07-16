pub mod liars_poker;

use clap::Parser;
use liars_poker::{Action, GameState, LiarsPoker};
use log::*;
use rand::prelude::SliceRandom;

use crate::liars_poker::{parse_bet, parse_highest_bet, DiceState};

/// Agent that randomly chooses moves
fn random_agent(_: &GameState, possible_moves: &Vec<Action>) -> Action {
    debug!("Random agent evaluating moves: {:?}", possible_moves);
    let mut rng = rand::thread_rng();
    return possible_moves.choose(&mut rng).unwrap().clone();
}

/// Bets based on own dice info only
fn own_dice_agent(g: &GameState, possible_moves: &Vec<Action>) -> Action {
    debug!("Own dice agent evaluating moves: {:?}", possible_moves);

    // count own dice
    let mut counts = [0; 6];
    for d in g.dice_state {
        match d {
            DiceState::K(x) => counts[x] += 1,
            _ => {}
        }
    }

    if let Some((count, value)) = parse_highest_bet(&g) {
        if count > counts[value - 1] {
            return Action::Call;
        }
    }

    for a in possible_moves {
        if let Action::Bet(i) = a {
            let (count, value) = parse_bet(*i);
            let a = Action::Bet(value);
            if counts[value - 1] >= count && possible_moves.contains(&a) {
                return a;
            }
        }
    }

    return Action::Call;
}

fn minmax_agent(g: &GameState, possible_moves: &Vec<Action>) -> Action {
    debug!("Expecation agent evaluating moves: {:?}", possible_moves);
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
        // let score = game.play(random_agent, random_agent);
        // let score = game.play(own_dice_agent, random_agent);
        // let score = game.play(random_agent, own_dice_agent);
        let score = game.play(own_dice_agent, own_dice_agent);
        if score == 1 {
            p1_wins += 1;
        } else {
            p2_wins += 1;
        }
    }

    print!("P1 wins: {},  P2 wins: {}\n\n", p1_wins, p2_wins)
}
