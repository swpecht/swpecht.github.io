use rand::prelude::SliceRandom;

use crate::{
    game::GameState,
    liars_poker::{parse_highest_bet, DiceState, LPAction, LPGameState},
};

pub trait Agent<G>
where
    G: GameState,
{
    fn name(&self) -> &str;
    fn play(&self, g: &G, possible_children: &Vec<G>) -> G;
}

/// Agent that randomly chooses moves
pub struct RandomAgent {}

impl<G: GameState + Clone> Agent<G> for RandomAgent {
    fn name(&self) -> &str {
        return &"RandomAgent";
    }

    fn play(&self, _: &G, possible_moves: &Vec<G>) -> G {
        let mut rng = rand::thread_rng();
        return possible_moves.choose(&mut rng).unwrap().clone();
    }
}

/// Agent always plays the first action
pub struct AlwaysFirstAgent {}

impl<G: GameState + Clone> Agent<G> for AlwaysFirstAgent {
    fn name(&self) -> &str {
        return &"AlwaysFirstAgent";
    }

    fn play(&self, _: &G, possible_moves: &Vec<G>) -> G {
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

    fn play(&self, g: &LPGameState, possible_moves: &Vec<LPGameState>) -> LPGameState {
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
                let p = g.get_acting_player();
                let mut r = g.clone();
                r.apply(p, &LPAction::Call);
                return r;
            }
        }

        for a in possible_moves {
            if let Some((count, value)) = parse_highest_bet(a) {
                if counts[value] >= count {
                    return a.clone();
                }
            }
        }

        let p = g.get_acting_player();
        let mut r = g.clone();
        r.apply(p, &LPAction::Call);
        return r;
    }
}
