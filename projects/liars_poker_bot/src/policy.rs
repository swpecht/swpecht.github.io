use crate::{cfragent::cfrnode::ActionVec, game::GameState};

/// Wrapper for game policies, usually backed by a node store for CFR
pub trait Policy<G: GameState> {
    /// Returns an ActionVec of legal moves and their associated probability for the current player
    fn action_probabilities(&mut self, gs: &G) -> ActionVec<f64>;
}

pub struct UniformRandomPolicy {}

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

        return probs;
    }
}
