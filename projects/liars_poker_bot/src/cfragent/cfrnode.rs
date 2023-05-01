use std::ops::{Index, IndexMut};

use crate::game::Action;

/// Adapted from: https://towardsdatascience.com/counterfactual-regret-minimization-ff4204bf4205
#[derive(Debug, Clone)]
pub struct CFRNode {
    pub regret_sum: ActionVec<f64>,
    pub move_prob: ActionVec<f64>,
    pub total_move_prob: ActionVec<f64>,
}

impl CFRNode {
    pub fn new(actions: Vec<Action>) -> Self {
        Self {
            regret_sum: ActionVec::new(&actions),
            move_prob: ActionVec::new(&actions),
            total_move_prob: ActionVec::new(&actions),
        }
    }

    /// Combine the positive regrets into a strategy.
    ///
    /// Defaults to a uniform action strategy if no regrets are present
    // Fix how this handles no data -- can't initialize all to 0
    pub fn get_move_prob(&mut self, realization_weight: f64) -> ActionVec<f64> {
        let actions = &self.regret_sum.actions;
        let num_actions = actions.len();
        let mut normalizing_sum = 0.0;

        for &a in actions {
            self.move_prob[a] = self.regret_sum[a].max(0.0);
            normalizing_sum += self.move_prob[a];
        }

        for &a in actions {
            if normalizing_sum > 0.0 {
                self.move_prob[a] = self.move_prob[a] / normalizing_sum;
            } else {
                self.move_prob[a] = 1.0 / num_actions as f64;
            }
            self.total_move_prob[a] += realization_weight * self.move_prob[a];
        }

        return self.move_prob.clone();
    }

    pub fn get_average_strategy(&self) -> ActionVec<f64> {
        let actions = &self.regret_sum.actions;

        let mut avg_strat = ActionVec::new(&actions);
        let mut normalizing_sum = 0.0;
        for &a in actions {
            normalizing_sum += self.total_move_prob[a];
        }

        for &a in actions {
            if normalizing_sum > 0.0 {
                avg_strat[a] = self.total_move_prob[a] / normalizing_sum;
            } else {
                avg_strat[a] = 1.0 / self.move_prob.len() as f64;
            }
        }

        return avg_strat;
    }
}

/// A helper struct to make working with sparse action vectors easy
///
/// It uses actions to index into a vector
#[derive(Clone, Debug)]
pub struct ActionVec<T: Default + Clone> {
    data: Vec<T>,
    // TODO: Can change this to a reference to same memory in the future
    actions: Vec<Action>,
}

impl<T: Default + Clone> ActionVec<T> {
    pub fn new(actions: &Vec<Action>) -> Self {
        let mut map = Vec::with_capacity(actions.len());
        let mut data = Vec::with_capacity(actions.len());

        for &a in actions {
            map.push(a);
            data.push(T::default())
        }

        return Self {
            data,
            actions: actions.clone(),
        };
    }

    fn get_index(&self, a: Action) -> usize {
        for i in 0..self.actions.len() {
            if self.actions[i] == a {
                return i;
            }
        }
        panic!(
            "invalid index: got action of: {:?}, valid actions are: {:?}",
            a, self.actions
        )
    }

    pub fn len(&self) -> usize {
        return self.data.len();
    }

    pub fn to_vec(&self) -> Vec<(Action, T)> {
        let mut output = Vec::new();

        for i in 0..self.actions.len() {
            output.push((self.actions[i], self.data[i].clone()))
        }

        return output;
    }
}

impl<T: Default + Clone> Index<Action> for ActionVec<T> {
    type Output = T;

    fn index(&self, a: Action) -> &Self::Output {
        let idx = self.get_index(a);
        return &self.data[idx];
    }
}

impl<T: Default + Clone> IndexMut<Action> for ActionVec<T> {
    fn index_mut(&mut self, a: Action) -> &mut Self::Output {
        let idx = self.get_index(a);
        return &mut self.data[idx];
    }
}
