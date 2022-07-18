use rand::prelude::SliceRandom;

use crate::liars_poker::{parse_bet, parse_highest_bet, Action, DiceState, GameState};

pub trait Agent {
    fn name(&self) -> &str;
    fn play(&self, g: &GameState, possible_moves: &Vec<Action>) -> Action;
}

/// Agent that randomly chooses moves
pub struct RandomAgent {
    pub name: String,
}

impl Agent for RandomAgent {
    fn play(&self, _: &GameState, possible_moves: &Vec<Action>) -> Action {
        let mut rng = rand::thread_rng();
        return possible_moves.choose(&mut rng).unwrap().clone();
    }

    fn name(&self) -> &str {
        return &self.name;
    }
}

pub struct OwnDiceAgent {
    pub name: String,
}

impl Agent for OwnDiceAgent {
    fn name(&self) -> &str {
        return &self.name;
    }

    fn play(&self, g: &GameState, possible_moves: &Vec<Action>) -> Action {
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
}
