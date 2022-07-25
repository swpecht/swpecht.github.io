use log::info;

use crate::agents::Agent;

pub trait GameState {
    type Action: Clone;
    fn get_actions(&self) -> Vec<Self::Action>;
    fn apply(&mut self, a: &Self::Action);
    fn evaluate(&self) -> i32;
}

pub trait Game {
    type G: GameState;
    type Action: Clone;

    fn new() -> Self;
    fn play(
        &mut self,
        p1: &(impl Agent<Self::G> + ?Sized),
        p2: &(impl Agent<Self::G> + ?Sized),
    ) -> i32;
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
        return vec![RPSAction::Rock, RPSAction::Paper, RPSAction::Scissors];
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

impl Game for RPS {
    type G = RPSState;
    type Action = RPSAction;

    fn new() -> Self {
        return Self {};
    }

    fn play(
        &mut self,
        p1: &(impl Agent<RPSState> + ?Sized),
        p2: &(impl Agent<RPSState> + ?Sized),
    ) -> i32 {
        let mut state = RPSState::new();
        let actions = state.get_actions();
        state.apply(&p1.play(&state, &actions));
        state.apply(&p2.play(&state, &actions));

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
