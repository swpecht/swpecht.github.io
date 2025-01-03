use dyn_clone::DynClone;
use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;
use tinyvec::ArrayVec;

use std::{
    fmt::Debug,
    hash::Hash,
    ops::{Deref, DerefMut},
    usize,
};

use crate::Action;

#[derive(Clone, Copy, Serialize, Deserialize, PartialOrd, Ord)]
pub struct IStateKey {
    len: usize,
    #[serde(with = "BigArray")]
    actions: [Action; 64],
}

unsafe impl bytemuck::Pod for IStateKey {}
unsafe impl bytemuck::Zeroable for IStateKey {}

impl Default for IStateKey {
    fn default() -> Self {
        Self {
            actions: [Action::default(); 64],
            len: 0,
        }
    }
}

impl From<&[u8]> for IStateKey {
    fn from(value: &[u8]) -> Self {
        let mut key = IStateKey::default();
        for x in value {
            key.push(Action(*x));
        }
        key
    }
}

impl<T: Copy> From<&[T]> for IStateKey
where
    Action: std::convert::From<T>,
{
    fn from(value: &[T]) -> Self {
        let mut key = IStateKey::default();
        for x in value {
            key.push(Action::from(*x));
        }
        key
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

impl DerefMut for IStateKey {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.actions[..self.len]
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

    pub fn as_slice(&self) -> &[Action] {
        &self.actions[..self.len]
    }

    pub fn to_vec(&self) -> Vec<Action> {
        self.actions[..self.len].to_vec()
    }

    pub fn as_bytes(&self) -> &[u8] {
        let action_slice: &[Action] = self;
        unsafe { std::slice::from_raw_parts(action_slice.as_ptr() as *const u8, self.len()) }
    }

    pub fn as_bytes_mut(&mut self) -> &mut [u8] {
        let action_slice: &mut [Action] = self;
        unsafe { std::slice::from_raw_parts_mut(action_slice.as_mut_ptr() as *mut u8, self.len()) }
    }

    pub fn copy_from_slice(actions: &[Action]) -> Self {
        let mut action_buf = [Action::default(); 64];
        action_buf[..actions.len()].copy_from_slice(actions);
        Self {
            len: actions.len(),
            actions: action_buf,
        }
    }

    /// Swaps all instances of action a and b
    pub fn swap(&mut self, a: Action, b: Action) {
        self.actions.iter_mut().take(self.len).for_each(|x| {
            *x = if *x == a {
                b
            } else if *x == b {
                a
            } else {
                *x
            };
        })
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
#[derive(Debug)]
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
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Copy, Debug, PartialOrd, Ord)]
pub struct NormalizedAction(Action);

impl NormalizedAction {
    pub fn new(action: Action) -> Self {
        Self(action)
    }

    pub fn new_from_id(id: u8) -> Self {
        Self(Action(id))
    }

    pub fn get(self) -> Action {
        self.0
    }
}

pub trait IStateNormalizer<G>: Sync + Send + DynClone {
    fn normalize_action(&self, action: Action, gs: &G) -> NormalizedAction;
    fn denormalize_action(&self, action: NormalizedAction, gs: &G) -> Action;
    fn normalize_istate(&self, istate: &IStateKey, gs: &G) -> NormalizedIstate;
}

dyn_clone::clone_trait_object!(<G>IStateNormalizer<G>);

#[derive(Default, Clone)]
pub struct NoOpNormalizer {}

impl<G> IStateNormalizer<G> for NoOpNormalizer {
    fn normalize_action(&self, action: Action, _: &G) -> NormalizedAction {
        NormalizedAction(action)
    }

    fn denormalize_action(&self, action: NormalizedAction, _: &G) -> Action {
        action.get()
    }

    fn normalize_istate(&self, istate: &IStateKey, _: &G) -> NormalizedIstate {
        NormalizedIstate(*istate)
    }
}

/// Translate an istate to a vector of game specific actions
#[macro_export]
macro_rules! translate_istate {
    ( $x:expr, $t:ty ) => {{
        use itertools::Itertools;
        $x.iter().map(|&y| <$t>::from(y)).collect_vec()
    }};
}

#[cfg(test)]
mod tests {}
