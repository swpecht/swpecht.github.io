use std::ops::{Index, IndexMut};

use crate::game::Action;

/// Adapted from: https://towardsdatascience.com/counterfactual-regret-minimization-ff4204bf4205
#[derive(Debug, Clone)]
pub struct CFRNode {
    pub regret_sum: ActionVec<f32>,
    pub move_prob: ActionVec<f32>,
    pub total_move_prob: ActionVec<f32>,
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
    pub fn get_move_prob(&mut self, realization_weight: f32) -> ActionVec<f32> {
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
                self.move_prob[a] = 1.0 / num_actions as f32;
            }
            self.total_move_prob[a] += realization_weight * self.move_prob[a];
        }

        return self.move_prob.clone();
    }

    pub(super) fn get_average_strategy(&self) -> Vec<f32> {
        let actions = &self.regret_sum.actions;

        let mut avg_strat = vec![0.0; self.move_prob.len()];
        let mut normalizing_sum = 0.0;
        for i in 0..self.move_prob.len() {
            let a = actions[i];
            normalizing_sum += self.total_move_prob[a];
        }

        for i in 0..self.move_prob.len() {
            let a = actions[i];
            if normalizing_sum > 0.0 {
                avg_strat[i] = self.total_move_prob[a] / normalizing_sum;
            } else {
                avg_strat[i] = 1.0 / self.move_prob.len() as f32;
            }
        }

        return avg_strat;
    }
}

#[derive(Clone, Debug)]
pub struct ActionVec<T: Default> {
    data: Vec<T>,
    // TODO: Can change this to a reference to same memory in the future
    actions: Vec<u8>,
}

impl<T: Default> ActionVec<T> {
    pub fn new(actions: &Vec<Action>) -> Self {
        let mut map = Vec::with_capacity(actions.len());
        let mut data = Vec::with_capacity(map.len());
        for &a in actions {
            map.push(a as u8);
            data.push(T::default())
        }

        return Self { data, actions: map };
    }

    fn get_index(&self, a: Action) -> usize {
        for i in 0..self.actions.len() {
            if self.actions[i] == a as u8 {
                return i;
            }
        }
        panic!(
            "invalid index: got action of: {}, valid actions are: {:?}",
            a, self.actions
        )
    }

    pub fn len(&self) -> usize {
        return self.data.len();
    }
}

impl<T: Default> Index<usize> for ActionVec<T> {
    type Output = T;

    fn index(&self, a: usize) -> &Self::Output {
        let idx = self.get_index(a);
        return &self.data[idx];
    }
}

impl<T: Default> IndexMut<usize> for ActionVec<T> {
    fn index_mut(&mut self, a: usize) -> &mut Self::Output {
        let idx = self.get_index(a);
        return &mut self.data[idx];
    }
}

impl<T: Default> Index<u8> for ActionVec<T> {
    type Output = T;

    fn index(&self, a: u8) -> &Self::Output {
        let idx = self.get_index(a as usize);
        return &self.data[idx];
    }
}

impl<T: Default> IndexMut<u8> for ActionVec<T> {
    fn index_mut(&mut self, a: u8) -> &mut Self::Output {
        let idx = self.get_index(a as usize);
        return &mut self.data[idx];
    }
}