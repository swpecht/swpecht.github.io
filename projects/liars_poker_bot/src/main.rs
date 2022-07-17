pub mod game_tree;
pub mod liars_poker;

use clap::Parser;
use itertools::Itertools;
use liars_poker::{apply_action, get_winner, Action, GameState, LiarsPoker};
use log::*;
use rand::prelude::SliceRandom;

use crate::{
    game_tree::GameTree,
    liars_poker::{parse_bet, parse_highest_bet, DiceState, Player, DICE_SIDES, NUM_DICE},
};

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
        if count > counts[value] {
            return Action::Call;
        }
    }

    for a in possible_moves {
        if let Action::Bet(i) = a {
            let (count, value) = parse_bet(*i);
            let a = Action::Bet(value);
            if counts[value] >= count && possible_moves.contains(&a) {
                return a;
            }
        }
    }

    return Action::Call;
}

fn minimax_agent(g: &GameState, possible_moves: &Vec<Action>) -> Action {
    debug!("Minimax agent evaluating moves: {:?}", possible_moves);

    let mut cur_max = f32::MIN;
    let mut cur_move = None;
    for a in possible_moves {
        let f = apply_action(g, a);
        debug!("Evaluating: {:?}", f);
        // let value = minimax(g, &mut f32::MIN, &mut f32::MAX, true);
        let t = GameTree::new(&f);
        // print!("{:?}", t);

        let value = t.get(0).score.unwrap();

        debug!("value: {:?}", value);
        if value > cur_max {
            cur_max = value;
            cur_move = Some(a)
        }
    }

    return *cur_move.unwrap();
}

/// Returns the chance of P1 winning from this game state
fn score_game_state(g: &GameState) -> f32 {
    let known_dice = g
        .dice_state
        .iter()
        .filter(|&x| match x {
            DiceState::K(_) => true,
            _ => false,
        })
        .collect_vec();

    let num_unknown = g.dice_state.iter().filter(|&x| *x == DiceState::U).count();
    assert!(num_unknown >= 1);

    let unknown_dice = (0..num_unknown)
        .map(|_| 0..DICE_SIDES)
        .multi_cartesian_product();
    let mut dice_state = [DiceState::K(1); NUM_DICE];

    for i in 0..known_dice.len() {
        dice_state[i] = *known_dice[i];
    }

    let mut wins = 0;
    let mut games = 0;
    for p in unknown_dice {
        let mut guess = p.iter();
        for i in known_dice.len()..NUM_DICE {
            dice_state[i] = DiceState::K(*guess.next().unwrap());
        }

        let mut state = g.clone();
        state.dice_state = dice_state;

        wins += match get_winner(&state) {
            Some(x) if x == Player::P1 => 1,
            _ => 0,
        };
        games += 1;
    }

    return wins as f32 / games as f32;
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
        // let score = game.play(own_dice_agent, own_dice_agent);
        let score = game.play(minimax_agent, own_dice_agent);
        if score == 1 {
            p1_wins += 1;
        } else {
            p2_wins += 1;
        }
    }

    print!("P1 wins: {},  P2 wins: {}\n\n", p1_wins, p2_wins);
}

#[cfg(test)]
mod tests {
    use crate::{
        liars_poker::{DiceState, GameState, Player, DICE_SIDES, NUM_DICE},
        score_game_state, GameTree,
    };

    #[test]
    fn test_score_game_state() {
        let mut g = GameState {
            dice_state: [DiceState::K(1), DiceState::K(1), DiceState::U, DiceState::U],
            bet_state: [None; NUM_DICE * DICE_SIDES],
            call_state: None,
        };
        g.call_state = Some(Player::P2);

        g.bet_state[0] = Some(Player::P1);
        let score = score_game_state(&g);
        assert_eq!(score, 1.0);

        g.bet_state[2 * DICE_SIDES] = Some(Player::P1);
        let score = score_game_state(&g);
        assert_eq!(
            score,
            2.0 / DICE_SIDES as f32 - 1.0 / DICE_SIDES as f32 / DICE_SIDES as f32
        );

        g.bet_state[2 * DICE_SIDES + 1] = Some(Player::P1);
        let score = score_game_state(&g);
        assert_eq!(score, 0.0);
    }
}
