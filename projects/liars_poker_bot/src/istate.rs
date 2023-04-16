use serde::{Deserialize, Serialize};

use std::{hash::Hash, ops::Index};

use crate::game::{arrayvec::ArrayVec, Action};

/// The number of bits per action for the hash_key
const HASH_WORD_LENGTH: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub struct IStateKey {
    key: ArrayVec<64>,
}

impl IStateKey {
    pub fn new() -> Self {
        Self {
            key: ArrayVec::new(),
        }
    }

    /// Push a new action to the key
    pub fn push(&mut self, a: Action) {
        self.key.push(a);
    }

    /// Returns a version of the IStateKey trimmed to a certain number of actions
    ///
    /// Upper bits are set to 0.
    pub fn trim(&self, n: usize) -> IStateKey {
        if n >= self.len() {
            return self.clone();
        }

        return Self {
            key: self.key.clone().trim(n),
        };
    }

    pub fn len(&self) -> usize {
        return self.key.len();
    }

    pub fn append(&mut self, actions: &[Action]) {
        for &a in actions {
            self.push(a);
        }
    }
}

impl ToString for IStateKey {
    fn to_string(&self) -> String {
        format!("{:?}", self.key)
    }
}

impl Index<usize> for IStateKey {
    type Output = Action;

    fn index(&self, index: usize) -> &Self::Output {
        assert!(index <= self.key.len());
        return &self.key[index];
    }
}

#[cfg(test)]
mod tests {}
