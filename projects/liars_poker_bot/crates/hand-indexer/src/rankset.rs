use std::fmt::Debug;

use crate::{math::binom, Rank};

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

pub fn cmp_group_rank(a: &Vec<RankSet>, b: &Vec<RankSet>) -> std::cmp::Ordering {
    for i in 0..a.len().max(b.len()) {
        let self_suit_len = a.get(i).map(|x| x.len()).unwrap_or(0);
        let other_suit_len = b.get(i).map(|x| x.len()).unwrap_or(0);

        use std::cmp::Ordering::*;
        match self_suit_len.cmp(&other_suit_len) {
            Less => return Less,
            Equal => continue,
            Greater => return Greater,
        };
    }

    std::cmp::Ordering::Equal
}

/// Return the size of a given config
///
/// see the paper for how 156 is arrived at, implement that here
pub fn config_size(lengths: &[u8], max_items: usize) -> usize {
    let mut size = 1;
    let mut used_items = 0;

    for x in lengths {
        size *= binom(max_items - used_items, *x as usize);
        used_items += *x as usize;
    }

    size
}

pub fn group_config(a: &[RankSet]) -> Vec<u8> {
    a.iter().map(|x| x.len()).collect()
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
        let g1 = vec![RankSet::new(&[0, 1]), RankSet::new(&[0])];
        // group of (0, 1)
        let g2 = vec![RankSet::new(&[]), RankSet::new(&[1])];

        use std::cmp::Ordering::*;
        assert_eq!(cmp_group_rank(&g1, &g2), Greater);
        assert_eq!(cmp_group_rank(&g2, &g2), Equal);
        assert_eq!(cmp_group_rank(&g1, &g1), Equal);

        // group of (2), implying (2, 0)
        let g1 = vec![RankSet::new(&[0, 1])];
        // group of (2, 1)
        let g2 = vec![RankSet::new(&[0, 1]), RankSet::new(&[1])];
        assert_eq!(cmp_group_rank(&g1, &g2), Less);
    }

    #[test]
    fn test_config_size() {
        assert_eq!(config_size(&[1], 13), 13);
        assert_eq!(config_size(&[1, 1], 13), 156);
        assert_eq!(config_size(&[2, 1], 13), 858);
    }
}
