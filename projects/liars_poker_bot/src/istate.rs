use serde::{Deserialize, Serialize};

use std::{hash::Hash, ops::Index};

use crate::game::{arrayvec::ArrayVec, Action};

/// The number of bits per action for the hash_key
const HASH_WORD_LENGTH: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct IStateKey {
    key: ArrayVec<64>,
    hash_key: u128,
}

impl IStateKey {
    pub fn new() -> Self {
        Self {
            key: ArrayVec::new(),
            hash_key: 0,
        }
    }

    /// Push a new action to the key
    pub fn push(&mut self, a: Action) {
        self.key.push(a);
        let mut new_key = self.hash_key;
        new_key = new_key << HASH_WORD_LENGTH;
        new_key |= a as u128;
        self.hash_key = new_key;
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
            hash_key: self.hash_key >> (self.len() - n) * HASH_WORD_LENGTH,
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
        format!("{:?}", self.hash_key)
    }
}

impl Index<usize> for IStateKey {
    type Output = Action;

    fn index(&self, index: usize) -> &Self::Output {
        assert!(index <= self.key.len());
        return &self.key[index];
    }
}

/// A more performant custom hash function
///
/// The keys are hashed in a tight loop for retrieval from the page caches. Before using a custom hash function
/// the majority of runtime for euchre was dominated by hashing. And hasing the long arrays backing array vec can take considerable time.
///
/// Instead, we push the actions onto a u128 key with a set word length and then just hash that key. While the key may overflow, this is ok because only
/// the older history will be lost. And we only care about hashes in the context of a page.
impl Hash for IStateKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.hash_key.hash(state);
    }
}

#[cfg(test)]
mod tests {}
