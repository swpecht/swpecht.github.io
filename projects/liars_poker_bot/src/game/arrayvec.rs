use std::ops::Index;

use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

use super::Action;

/// Array backed card storage that implements Vector-like features and is copyable
/// It also always remains sorted
#[derive(Clone, Copy, Debug)]
pub struct SortedArrayVec<const N: usize> {
    len: usize,
    data: [Action; N],
}

impl<const N: usize> SortedArrayVec<N> {
    pub fn new() -> Self {
        Self {
            len: 0,
            data: [0; N],
        }
    }

    pub fn push(&mut self, c: Action) {
        assert!(self.len < self.data.len());

        if self.len == 0 || self.data[self.len - 1] <= c {
            // put it on the end
            self.data[self.len] = c;
        } else {
            for i in 0..self.len {
                if c < self.data[i] {
                    self.shift_right(i);
                    self.data[i] = c;
                    break;
                }
            }
        }

        self.len += 1;
    }

    /// shifts all elements right starting at the item in idx, so idx will become idx+1
    fn shift_right(&mut self, idx: usize) {
        for i in (idx..self.len).rev() {
            self.data[i + 1] = self.data[i];
        }
    }

    /// shifts all elements left starting at the item in idx, so idx will become idx-1
    fn shift_left(&mut self, idx: usize) {
        for i in idx..self.len {
            self.data[i - 1] = self.data[i];
        }
    }

    pub fn remove(&mut self, c: Action) {
        for i in 0..self.len {
            if self.data[i] == c {
                self.shift_left(i + 1);
                self.len -= 1;
                return;
            }
        }

        panic!("attempted to remove item not in list")
    }

    pub fn len(&self) -> usize {
        return self.len;
    }

    pub fn to_vec(&self) -> Vec<Action> {
        let mut v = Vec::with_capacity(self.len);
        for i in 0..self.len {
            v.push(self.data[i]);
        }

        return v;
    }

    pub fn contains(&self, c: &Action) -> bool {
        let mut contains = false;

        for i in 0..self.len {
            if self.data[i] == *c {
                contains = true;
            }
        }

        return contains;
    }
}

impl<const N: usize> Index<usize> for SortedArrayVec<N> {
    type Output = Action;

    fn index(&self, index: usize) -> &Self::Output {
        assert!(index <= self.len);
        return &self.data[index];
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct ArrayVec<const N: usize> {
    len: usize,
    #[serde(with = "BigArray")]
    data: [Action; N],
}

impl<const N: usize> ArrayVec<N> {
    pub fn new() -> Self {
        Self {
            len: 0,
            data: [0; N],
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

        let mut new = self.clone();
        if n >= self.len {
            return new;
        }
        new.len = n;
        return new;
    }

    pub fn len(&self) -> usize {
        return self.len;
    }
}

impl<const N: usize, Idx> Index<Idx> for ArrayVec<N>
where
    Idx: std::slice::SliceIndex<[Action]>,
{
    type Output = Idx::Output;

    fn index(&self, index: Idx) -> &Self::Output {
        return &self.data[index];
    }
}

impl<const N: usize> PartialEq for ArrayVec<N> {
    fn eq(&self, other: &Self) -> bool {
        self.len == other.len && self.data[0..self.len] == other.data[0..self.len]
    }
}

impl<const N: usize> Eq for ArrayVec<N> {}

#[cfg(test)]
mod tests {
    use super::{ArrayVec, SortedArrayVec};

    #[test]
    fn test_sorted_array_vec() {
        let mut h: SortedArrayVec<5> = SortedArrayVec::new();

        // test basic add and index
        h.push(0);
        h.push(1);
        assert_eq!(h[0], 0);
        assert_eq!(h[1], 1);
        assert!(h.contains(&1));
        assert!(!h.contains(&10));
        assert_eq!(h.len(), 2);

        // test sorting
        h.push(10);
        h.push(2);
        assert_eq!(h[2], 2);
        assert_eq!(h[3], 10);
        assert_eq!(h.len(), 4);

        h.remove(1);
        assert_eq!(h[0], 0);
        assert_eq!(h[1], 2);
        assert_eq!(h[2], 10);
        assert_eq!(h.len(), 3);
    }

    #[test]
    fn test_array_vec() {
        let mut v = ArrayVec::<5>::new();
        v.push(42);
        v.push(10);

        assert_eq!(v[0], 42);
        assert_eq!(v[1], 10);
        assert_eq!(v.len(), 2);

        let n = v.trim(3);
        assert_eq!(v, n);
    }
}
