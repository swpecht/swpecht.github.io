use std::cmp::min;

use log::debug;
use rand::prelude::SliceRandom;

use crate::{
    game::GameState,
    liars_poker::{parse_bet, parse_highest_bet, DiceState, LPAction, LPGameState, NUM_DICE},
};

pub trait Agent<G>
where
    G: GameState,
{
    fn name(&self) -> &str;
    fn play(&self, g: &G, possible_moves: &Vec<G::Action>) -> G::Action;
}

/// Agent that randomly chooses moves
pub struct RandomAgent {}

impl<G: GameState> Agent<G> for RandomAgent {
    fn name(&self) -> &str {
        return &"RandomAgent";
    }

    fn play(&self, _: &G, possible_moves: &Vec<G::Action>) -> G::Action {
        let mut rng = rand::thread_rng();
        return possible_moves.choose(&mut rng).unwrap().clone();
    }
}

/// Agent always plays the first action
pub struct AlwaysFirstAgent {}

impl<G: GameState> Agent<G> for AlwaysFirstAgent {
    fn name(&self) -> &str {
        return &"AlwaysFirstAgent";
    }

    fn play(&self, _: &G, possible_moves: &Vec<G::Action>) -> G::Action {
        return possible_moves[0].clone();
    }
}

pub struct OwnDiceAgent {
    pub name: String,
}

impl Agent<LPGameState> for OwnDiceAgent {
    fn name(&self) -> &str {
        return &self.name;
    }

    fn play(&self, g: &LPGameState, possible_moves: &Vec<LPAction>) -> LPAction {
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
                return LPAction::Call;
            }
        }

        for a in possible_moves {
            if let LPAction::Bet(i) = a {
                let (count, value) = parse_bet(*i);
                if counts[value] >= count {
                    return *a;
                }
            }
        }

        return LPAction::Call;
    }
}

/// Similar to the OwnDiceAgent, but it assumes the competitor never bluffs
/// and uses their bets as information about their dice.
///
/// It it meant to show the weakness of expectation maximization in an imperfect
/// information game.
pub struct IncorporateBetAgent {
    pub name: String,
}

impl Agent<LPGameState> for IncorporateBetAgent {
    fn name(&self) -> &str {
        return &self.name;
    }

    fn play(&self, g: &LPGameState, possible_moves: &Vec<LPAction>) -> LPAction {
        // count own dice
        let mut counts = [0; 6];
        for d in g.dice_state {
            match d {
                DiceState::K(x) => counts[x] += 1,
                _ => {}
            }
        }

        let unknown_dice = NUM_DICE / 2;
        // estimate opponent dice
        if let Some((count, value)) = parse_highest_bet(&g) {
            counts[value] += min(unknown_dice, count);
            debug!("Estimated counts: {:?}", counts);
        }

        for a in possible_moves {
            if let LPAction::Bet(i) = a {
                let (count, value) = parse_bet(*i);
                if counts[value] >= count {
                    return *a;
                }
            }
        }

        return LPAction::Call;
    }
}
