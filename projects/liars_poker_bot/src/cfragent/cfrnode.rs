use serde::{Deserialize, Serialize};

use crate::game::Action;

const MAX_ACTIONS: usize = 6;

/// Adapted from: https://towardsdatascience.com/counterfactual-regret-minimization-ff4204bf4205
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Copy)]
pub struct CFRNode {
    /// Stores what action each index represents.
    /// There are at most 5 actions (one for each card in hand)
    actions: [usize; MAX_ACTIONS],
    num_actions: usize,
    pub regret_sum: [f32; MAX_ACTIONS],
    pub move_prob: [f32; MAX_ACTIONS],
    pub total_move_prob: [f32; MAX_ACTIONS],
}

impl CFRNode {
    pub fn new(legal_moves: &Vec<Action>) -> Self {
        let num_actions = legal_moves.len();
        let mut actions = [0; MAX_ACTIONS];
        for i in 0..num_actions {
            actions[i] = legal_moves[i]
        }

        Self {
            actions: actions,
            num_actions: num_actions,
            regret_sum: [0.0; MAX_ACTIONS],
            move_prob: [0.0; MAX_ACTIONS],
            total_move_prob: [0.0; MAX_ACTIONS],
        }
    }

    /// Combine the positive regrets into a strategy.
    ///
    /// Defaults to a uniform action strategy if no regrets are present
    pub(super) fn get_move_prob(&mut self, realization_weight: f32) -> [f32; MAX_ACTIONS] {
        let num_actions = self.num_actions;
        let mut normalizing_sum = 0.0;

        for i in 0..num_actions {
            self.move_prob[i] = self.regret_sum[i].max(0.0);
            normalizing_sum += self.move_prob[i];
        }

        for i in 0..num_actions {
            if normalizing_sum > 0.0 {
                self.move_prob[i] = self.move_prob[i] / normalizing_sum;
            } else {
                self.move_prob[i] = 1.0 / num_actions as f32;
            }
            self.total_move_prob[i] += realization_weight * self.move_prob[i];
        }

        return self.move_prob.clone();
    }

    pub(super) fn get_average_strategy(&self) -> Vec<f32> {
        let mut avg_strat = vec![0.0; self.move_prob.len()];
        let mut normalizing_sum = 0.0;
        for i in 0..self.move_prob.len() {
            normalizing_sum += self.total_move_prob[i];
        }

        for i in 0..self.move_prob.len() {
            if normalizing_sum > 0.0 {
                avg_strat[i] = self.total_move_prob[i] / normalizing_sum;
            } else {
                avg_strat[i] = 1.0 / self.move_prob.len() as f32;
            }
        }

        return avg_strat;
    }

    /// Returns the index storing a given action
    pub(super) fn get_index(&self, action: Action) -> usize {
        for i in 0..self.actions.len() {
            if action == self.actions[i] {
                return i;
            }
        }
        panic!("action not found")
    }
}
