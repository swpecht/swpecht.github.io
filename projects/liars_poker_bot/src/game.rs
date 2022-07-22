use log::info;

use crate::agents::Agent;

pub trait Game {
    type GameState;
    type Action: Clone;

    fn new() -> Self;
    fn play(
        &mut self,
        p1: &(impl Agent<Self::GameState, Self::Action> + ?Sized),
        p2: &(impl Agent<Self::GameState, Self::Action> + ?Sized),
    ) -> i32;
    fn get_possible_actions(&self, g: &Self::GameState) -> Vec<Self::Action>;
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum RPSAction {
    Rock,
    Paper,
    Scissors,
}

pub struct RPSState {
    actions: [Option<RPSAction>; 2],
}

/// Implementation of weighted RPS. Any game involving scissors means the payoff is doubled
///
/// https://arxiv.org/pdf/2007.13544.pdf
pub struct RPS {}

impl Game for RPS {
    type GameState = RPSState;
    type Action = RPSAction;

    fn new() -> Self {
        return Self {};
    }

    fn play(
        &mut self,
        p1: &(impl Agent<RPSState, RPSAction> + ?Sized),
        p2: &(impl Agent<RPSState, RPSAction> + ?Sized),
    ) -> i32 {
        let mut state = RPSState { actions: [None; 2] };
        let actions = self.get_possible_actions(&state);
        state.actions[0] = Some(p1.play(&state, &actions));
        state.actions[1] = Some(p2.play(&state, &actions));

        info!(
            "{} played {:?}, {} played {:?}",
            p1.name(),
            state.actions[0],
            p2.name(),
            state.actions[1]
        );

        let score = match (state.actions[0], state.actions[1]) {
            (Some(x), Some(y)) if x == y => 0,
            (Some(RPSAction::Paper), Some(RPSAction::Rock)) => 1,
            (Some(RPSAction::Paper), Some(RPSAction::Scissors)) => -2,
            (Some(RPSAction::Rock), Some(RPSAction::Scissors)) => 2,
            (Some(RPSAction::Rock), Some(RPSAction::Paper)) => -1,
            (Some(RPSAction::Scissors), Some(RPSAction::Paper)) => 2,
            (Some(RPSAction::Scissors), Some(RPSAction::Rock)) => -2,
            _ => panic!("invalid state: both players must play"),
        };
        info!("Score: {:?}", score);
        return score;
    }

    fn get_possible_actions(&self, _: &Self::GameState) -> Vec<Self::Action> {
        return vec![RPSAction::Rock, RPSAction::Paper, RPSAction::Scissors];
    }
}
