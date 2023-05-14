use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

use std::{
    fmt::Debug,
    hash::Hash,
    ops::{Index, IndexMut},
};

use crate::game::Action;

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash, PartialOrd, Ord)]
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

impl Index<usize> for IStateKey {
    type Output = Action;

    fn index(&self, index: usize) -> &Self::Output {
        assert!(index < self.len());
        &self.actions[index]
    }
}

impl IndexMut<usize> for IStateKey {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        assert!(index < self.len());
        &mut self.actions[index]
    }
}

#[cfg(test)]
mod tests {}
