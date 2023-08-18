use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

use std::{fmt::Debug, hash::Hash, ops::Deref, usize};

use crate::game::Action;

#[derive(Clone, Copy, Serialize, Deserialize, PartialOrd, Ord)]
pub struct IStateKey {
    len: usize,
    #[serde(with = "BigArray")]
    actions: [Action; 64],
}

impl Default for IStateKey {
    fn default() -> Self {
        Self {
            actions: [Action::default(); 64],
            len: 0,
        }
    }
}

/// We deref to a slice for full indexing, this is the same approach
/// that ArrayVec uses
impl Deref for IStateKey {
    type Target = [Action];

    fn deref(&self) -> &Self::Target {
        &self.actions[..self.len]
    }
}

impl IStateKey {
    /// Push a new action to the key
    pub fn push(&mut self, a: Action) {
        self.actions[self.len] = a;
        self.len += 1;
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Sorts the keys in the sepcified range
    pub fn sort_range(&mut self, start: usize, n: usize) {
        assert!(start + n <= self.len());
        self.actions[start..start + n].sort()
    }

    pub fn pop(&mut self) -> Action {
        let last = self.actions[self.len() - 1];
        self.len -= 1;
        last
    }
}

impl ToString for IStateKey {
    fn to_string(&self) -> String {
        format!("{:?}", &self.actions[..self.len])
    }
}

impl Debug for IStateKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", &self.actions[..self.len])
    }
}

impl PartialEq for IStateKey {
    fn eq(&self, other: &Self) -> bool {
        self.len == other.len && self.actions[..self.len] == other.actions[..other.len]
    }
}

impl Eq for IStateKey {}

impl Hash for IStateKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.len.hash(state);
        self.actions[..self.len].hash(state);
    }
}

impl IntoIterator for IStateKey {
    type Item = Action;

    type IntoIter = IStateKeyIterator;

    fn into_iter(self) -> Self::IntoIter {
        IStateKeyIterator {
            key: self,
            index: 0,
        }
    }
}

pub struct IStateKeyIterator {
    key: IStateKey,
    index: usize,
}

impl Iterator for IStateKeyIterator {
    type Item = Action;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index < self.key.len() {
            let v = Some(self.key[self.index]);
            self.index += 1;
            return v;
        }
        None
    }
}

/// A key representing the state of the game (with perfect information). Used for transposition table lookups
pub type IsomorphicHash = u64;

/// Helper type to keep track of if a key has been normalized or not
pub struct NormalizedIstate(IStateKey);

impl NormalizedIstate {
    pub fn new(istate: IStateKey) -> Self {
        Self(istate)
    }

    pub fn get(&self) -> IStateKey {
        self.0
    }
}

/// Helper type to keep track of if an action is normalized or not
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Copy)]
pub struct NormalizedAction(Action);

impl NormalizedAction {
    pub fn new(action: Action) -> Self {
        Self(action)
    }

    pub fn get(self) -> Action {
        self.0
    }
}

#[cfg(test)]
mod tests {}
