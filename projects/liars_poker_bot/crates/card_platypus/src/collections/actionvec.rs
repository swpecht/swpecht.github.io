use std::ops::IndexMut;

use std::ops::Index;

use serde::Deserialize;
use serde::Serialize;

use crate::game::Action;

/// A helper struct to make working with sparse action vectors easy
///
/// It uses actions to index into a vector
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionVec<T: Default + Clone> {
    pub(crate) data: Vec<T>,
    // TODO: Can change this to a reference to same memory in the future
    pub(crate) actions: Vec<Action>,
}

impl<T: Default + Clone> ActionVec<T> {
    pub fn new(actions: &Vec<Action>) -> Self {
        let mut data = Vec::with_capacity(actions.len());

        for _ in actions {
            data.push(T::default())
        }

        Self {
            data,
            actions: actions.clone(),
        }
    }

    pub(crate) fn get_index(&self, a: Action) -> usize {
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
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn to_vec(&self) -> Vec<(Action, T)> {
        let mut output = Vec::new();

        for i in 0..self.actions.len() {
            output.push((self.actions[i], self.data[i].clone()))
        }

        output
    }

    pub fn actions(&self) -> &Vec<Action> {
        &self.actions
    }
}

impl<T: Default + Clone> Index<Action> for ActionVec<T> {
    type Output = T;

    fn index(&self, a: Action) -> &Self::Output {
        let idx = self.get_index(a);
        &self.data[idx]
    }
}

impl<T: Default + Clone> IndexMut<Action> for ActionVec<T> {
    fn index_mut(&mut self, a: Action) -> &mut Self::Output {
        let idx = self.get_index(a);
        &mut self.data[idx]
    }
}
