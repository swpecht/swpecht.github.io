use std::fmt::Debug;

use crate::Rank;

#[derive(Default, PartialEq, Eq, Clone, Copy, Hash)]
pub struct RankSet(u16);

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

    pub fn smallest(&self) -> u8 {
        self.0.trailing_zeros() as u8
    }

    /// Converts a downshifted rank `rank` to it's true rank assuming
    /// all items in `self` had already been used
    pub fn upshift_rank(&self, mut rank: u8) -> u8 {
        let mut x = *self;

        while x.smallest() <= rank && !x.is_empty() {
            rank += 1;
            x.remove(x.smallest());
        }

        rank
    }

    /// Convers a true rank to a downshifted rank by subtracting the already used
    /// items from the rank
    pub fn donwnshift_rank(&self, rank: u8) -> u8 {
        // is there a clearer way to implement this?
        let lower_used = (!(!0 << rank) & self.0).count_ones() as u8;
        rank - lower_used
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

#[derive(PartialEq, Debug, Eq)]
pub struct Group(pub(super) Vec<RankSet>);

impl Group {
    pub fn new(items: Vec<RankSet>) -> Self {
        Group(items)
    }
}

impl PartialOrd for Group {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(&other))
    }
}

impl Ord for Group {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        for i in 0..self.0.len().max(other.0.len()) {
            let self_suit_len = self.0.get(i).map(|x| x.len()).unwrap_or(0);
            let other_suit_len = other.0.get(i).map(|x| x.len()).unwrap_or(0);

            use std::cmp::Ordering::*;
            match self_suit_len.cmp(&other_suit_len) {
                Less => return Less,
                Equal => continue,
                Greater => return Greater,
            };
        }
        std::cmp::Ordering::Equal
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

    #[test]
    fn test_group_ord() {
        // group of (2, 1)
        let g1 = Group::new(vec![RankSet::new(&[0, 1]), RankSet::new(&[0])]);
        // group of (0, 1)
        let g2 = Group::new(vec![RankSet::new(&[]), RankSet::new(&[1])]);

        assert!(g1 > g2);
        assert_eq!(g2, g2);
        assert_eq!(g1, g1);

        // group of (2), implying (2, 0)
        let g1 = Group::new(vec![RankSet::new(&[0, 1])]);
        // group of (2, 1)
        let g2 = Group::new(vec![RankSet::new(&[0, 1]), RankSet::new(&[1])]);
        assert!(g2 > g1);
    }
}
