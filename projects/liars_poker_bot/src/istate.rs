use serde::{Deserialize, Serialize};

use std::{hash::Hash, ops::Index};

use crate::game::{arrayvec::ArrayVec, Action};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash, PartialOrd, Ord)]
pub struct IStateKey {
    actions: ArrayVec<64>,
}

impl IStateKey {
    pub fn new() -> Self {
        Self {
            actions: ArrayVec::new(),
        }
    }

    /// Push a new action to the key
    pub fn push(&mut self, a: Action) {
        self.actions.push(a);
    }

    /// Returns a version of the IStateKey trimmed to a certain number of actions
    ///
    /// Upper bits are set to 0.
    pub fn trim(&self, n: usize) -> IStateKey {
        if n >= self.len() {
            return self.clone();
        }

        return Self {
            actions: self.actions.clone().trim(n),
        };
    }

    pub fn len(&self) -> usize {
        return self.actions.len();
    }

    pub fn append(&mut self, actions: &[Action]) {
        for &a in actions {
            self.push(a);
        }
    }

    pub fn get_actions(&self) -> ArrayVec<64> {
        return self.actions;
    }
}

impl ToString for IStateKey {
    fn to_string(&self) -> String {
        format!("{:?}", self.actions)
    }
}

impl Index<usize> for IStateKey {
    type Output = Action;

    fn index(&self, index: usize) -> &Self::Output {
        assert!(index <= self.actions.len());
        return &self.actions[index];
    }
}

#[cfg(test)]
mod tests {}
