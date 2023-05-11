use crate::{
    actions,
    cfragent::cfrnode::ActionVec,
    game::{Action, GameState},
};

/// Wrapper for game policies, usually backed by a node store for CFR
pub trait Policy<G> {
    /// Returns an ActionVec of legal moves and their associated probability for the current player
    fn action_probabilities(&mut self, gs: &G) -> ActionVec<f64>;
}

#[derive(Clone, Copy)]
pub struct UniformRandomPolicy {}

impl Default for UniformRandomPolicy {
    fn default() -> Self {
        Self::new()
    }
}

impl UniformRandomPolicy {
    pub fn new() -> Self {
        Self {}
    }
}

impl<G: GameState> Policy<G> for UniformRandomPolicy {
    fn action_probabilities(&mut self, gs: &G) -> ActionVec<f64> {
        let mut actions = Vec::new();
        gs.legal_actions(&mut actions);
        let prob = 1.0 / actions.len() as f64; // uniform random

        let mut probs = ActionVec::new(&actions);
        for a in actions {
            probs[a] = prob;
        }

        probs
    }
}

/// Policy always takes a given action. If the action isn't available, it panics.
#[derive(Clone, Copy)]
pub struct AlwaysPolicy {
    action: Action,
}

impl AlwaysPolicy {
    pub fn new(a: Action) -> Self {
        Self { action: a }
    }
}

impl<G: GameState> Policy<G> for AlwaysPolicy {
    fn action_probabilities(&mut self, gs: &G) -> ActionVec<f64> {
        let actions = actions!(gs);
        if !actions.contains(&self.action) {
            panic!("attempted to call always policy when action wasn't possible");
        }

        let mut probs = ActionVec::new(&actions);
        probs[self.action] = 1.0;

        probs
    }
}
