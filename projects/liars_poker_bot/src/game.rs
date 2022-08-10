use itertools::Itertools;
use log::info;

use crate::{agents::Agent, liars_poker::Player};

pub trait GameState: Sized {
    type Action: Clone;
    fn get_actions(&self) -> Vec<Self::Action>;
    fn apply(&mut self, a: &Self::Action);
    fn evaluate(&self) -> i32;
    fn get_acting_player(&self) -> Player;
    /// Return all poassible game states given hidden information
    fn get_possible_states(&self) -> Vec<Self>;
}

pub trait Game<State: GameState> {
    fn play(&mut self, p1: &(impl Agent<State> + ?Sized), p2: &(impl Agent<State> + ?Sized))
        -> i32;
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum RPSAction {
    Rock,
    Paper,
    Scissors,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct RPSState {
    actions: [Option<RPSAction>; 2],
}

impl GameState for RPSState {
    type Action = RPSAction;

    fn get_actions(&self) -> Vec<Self::Action> {
        return match self.actions {
            [Some(_), Some(_)] => Vec::new(),
            _ => vec![RPSAction::Rock, RPSAction::Paper, RPSAction::Scissors],
        };
    }

    fn apply(&mut self, a: &Self::Action) {
        match self.actions {
            [None, None] => self.actions[0] = Some(*a),
            [Some(_), None] => self.actions[1] = Some(*a),
            _ => panic!("applied invalid action"),
        }
    }

    fn evaluate(&self) -> i32 {
        return match (self.actions[0], self.actions[1]) {
            (Some(x), Some(y)) if x == y => 0,
            (Some(RPSAction::Paper), Some(RPSAction::Rock)) => 1,
            (Some(RPSAction::Paper), Some(RPSAction::Scissors)) => -2,
            (Some(RPSAction::Rock), Some(RPSAction::Scissors)) => 2,
            (Some(RPSAction::Rock), Some(RPSAction::Paper)) => -1,
            (Some(RPSAction::Scissors), Some(RPSAction::Paper)) => 2,
            (Some(RPSAction::Scissors), Some(RPSAction::Rock)) => -2,
            _ => panic!("invalid state: both players must play"),
        };
    }

    fn get_acting_player(&self) -> Player {
        match self.actions {
            [None, _] => Player::P1,
            _ => Player::P2,
        }
    }

    fn get_possible_states(&self) -> Vec<Self> {
        let mut possibilities = Vec::new();

        for i in 0..self.actions.len() {
            possibilities.push(match self.actions[i] {
                None => vec![
                    Some(RPSAction::Rock),
                    Some(RPSAction::Paper),
                    Some(RPSAction::Scissors),
                ],
                _ => vec![self.actions[i]],
            });
        }

        let actions = possibilities.iter().multi_cartesian_product();
        let mut states = Vec::new();
        for a in actions {
            let mut s = self.clone();
            for i in 0..s.actions.len() {
                s.actions[i] = *a[i];
            }
            states.push(s);
        }

        return states;
    }
}

impl RPSState {
    pub fn new() -> Self {
        return Self { actions: [None; 2] };
    }
}

/// Implementation of weighted RPS. Any game involving scissors means the payoff is doubled
///
/// https://arxiv.org/pdf/2007.13544.pdf
pub struct RPS {}

impl Game<RPSState> for RPS {
    fn play(
        &mut self,
        p1: &(impl Agent<RPSState> + ?Sized),
        p2: &(impl Agent<RPSState> + ?Sized),
    ) -> i32 {
        info!("{} playing {}", p1.name(), p2.name());
        let mut state = RPSState::new();
        let actions = state.get_actions();
        state.apply(&p1.play(&state, &actions));
        let mut filtered_state = state.clone();
        filtered_state.actions[0] = None;
        state.apply(&p2.play(&filtered_state, &actions));

        info!(
            "{} played {:?}, {} played {:?}",
            p1.name(),
            state.actions[0],
            p2.name(),
            state.actions[1]
        );

        let score = state.evaluate();
        info!("Score: {:?}", score);
        return score;
    }
}
