use std::fmt::Debug;

use crate::Rank;

#[derive(Default, PartialEq, Eq, Clone, Copy)]
pub struct RankSet(pub(super) u16);

impl RankSet {
    pub fn new(ranks: &[u8]) -> Self {
        let mut set = RankSet::default();
        for r in ranks {
            set.insert(*r);
        }

        set
    }

    pub fn insert(&mut self, rank: u8) {
        self.0 |= 1 << rank;
    }

    pub fn remove(&mut self, rank: u8) {
        self.0 &= !(1 << rank);
    }

    pub fn len(&self) -> u8 {
        self.0.count_ones() as u8
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn union(&self, other: &RankSet) -> RankSet {
        RankSet(self.0 | other.0)
    }

    /// Return the rank of the largest item in the set
    pub fn largest(&self) -> u8 {
        // 16 bits for the u16
        // off by 1 on the leading zeros
        16 - 1 - self.0.leading_zeros() as u8
    }
}

impl Debug for RankSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut x = *self;

        f.debug_list()
            .entries((0..x.len()).map(|_| {
                let l = x.largest();
                x.remove(l);
                l
            }))
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_largest() {
        let mut set = RankSet::default();

        set.insert(0);
        assert_eq!(set.len(), 1);

        set.insert(3);
        assert_eq!(set.largest(), 3);
        set.insert(1);
        assert_eq!(set.largest(), 3);
        set.remove(3);
        assert_eq!(set.largest(), 1);
        set.insert(5);
        assert_eq!(set.largest(), 5);
    }
}
