use std::{
    fmt::Debug,
    hash::Hash,
    ops::{Index, IndexMut},
};

use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

use crate::game::Action;

pub mod diskbackedvec;

#[derive(Clone, Copy, Serialize, Deserialize, PartialOrd, Ord)]
pub struct ArrayVec<const N: usize> {
    len: usize,
    #[serde(with = "BigArray")]
    data: [Action; N],
}

impl<const N: usize> Default for ArrayVec<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> ArrayVec<N> {
    pub fn new() -> Self {
        Self {
            len: 0,
            data: [Action(0); N],
        }
    }

    pub fn push(&mut self, a: Action) {
        assert!(self.len < self.data.len());
        self.data[self.len] = a;
        self.len += 1;
    }

    /// Returns a version of the ArrayVec trimmed to a certain number of elements
    pub fn trim(&mut self, n: usize) -> Self {
        assert!(N >= n);

        let mut new = *self;
        if n >= self.len {
            return new;
        }
        new.len = n;
        new
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<const N: usize> Debug for ArrayVec<N> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", &self.data[..self.len])
    }
}

impl<const N: usize, Idx> Index<Idx> for ArrayVec<N>
where
    Idx: std::slice::SliceIndex<[Action]>,
{
    type Output = Idx::Output;

    fn index(&self, index: Idx) -> &Self::Output {
        &self.data[index]
    }
}

impl<const N: usize> IndexMut<usize> for ArrayVec<N> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        debug_assert!(index < self.len);
        &mut self.data[index]
    }
}

impl<const N: usize> PartialEq for ArrayVec<N> {
    fn eq(&self, other: &Self) -> bool {
        self.len == other.len && self.data[0..self.len] == other.data[0..self.len]
    }
}

impl<const N: usize> Eq for ArrayVec<N> {}

impl<const N: usize> Hash for ArrayVec<N> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.data[0..self.len].hash(state);
    }
}

#[cfg(test)]
mod tests {
    use crate::game::Action;

    use super::ArrayVec;

    #[test]
    fn test_array_vec() {
        let mut v = ArrayVec::<5>::new();
        v.push(Action(42));
        v.push(Action(10));

        assert_eq!(v[0], Action(42));
        assert_eq!(v[1], Action(10));
        assert_eq!(v.len(), 2);

        let n = v.trim(3);
        assert_eq!(v, n);
    }
}
