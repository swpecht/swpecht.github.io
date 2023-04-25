use serde::{Deserialize, Serialize};

const MAX_ACTIONS: usize = 32;

/// Adapted from: https://towardsdatascience.com/counterfactual-regret-minimization-ff4204bf4205
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CFRNode {
    pub regret_sum: Vec<f32>,
    pub move_prob: Vec<f32>,
    pub total_move_prob: Vec<f32>,
}

impl CFRNode {
    pub fn new() -> Self {
        Self {
            regret_sum: new_vec(MAX_ACTIONS),
            move_prob: new_vec(MAX_ACTIONS),
            total_move_prob: new_vec(MAX_ACTIONS),
        }
    }

    /// Combine the positive regrets into a strategy.
    ///
    /// Defaults to a uniform action strategy if no regrets are present
    pub(super) fn get_move_prob(&mut self, realization_weight: f32) -> Vec<f32> {
        let num_actions = self.regret_sum.len();
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
}

fn new_vec(n: usize) -> Vec<f32> {
    let mut v = Vec::with_capacity(n);
    for _ in 0..n {
        v.push(0.0);
    }

    return v;
}
