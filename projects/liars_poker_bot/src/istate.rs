use serde::{Deserialize, Serialize};

use std::ops::Index;

use crate::game::{arrayvec::ArrayVec, Action};

/// For euchre, need the following bits:
/// 25 for deal: 5 cards * 5 bits
/// 5 for face up: 1 card * 5 bits
/// 4 for pickup: 4 players * bool
/// 12 for choose trump: 4 players * 3 bits for 5 choices
/// 100 for play: 4 players * 5 cards * 5 bits
pub type KeyFragment = u128;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
        format!("{:X}{:X}", self.key[1], self.key[0])
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
