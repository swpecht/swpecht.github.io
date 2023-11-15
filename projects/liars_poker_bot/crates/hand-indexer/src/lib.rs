use std::default;

use math::find_x;
use rankset::RankSet;

use crate::math::binom;

mod math;
pub mod rankset;

/// Translates a euchre hand to a conanical index
///
/// N: number of cards per suit
///
/// Based on:
///     https://www.cs.cmu.edu/~waugh/publications/isomorphism13.pdf
#[derive(Default)]
struct HandIndexer<const N: u8> {}

impl<const N: u8> HandIndexer<N> {
    /// Compute the index for k M-rank sets of the same suit
    ///
    /// These groups must not share cards
    fn index_group(&self, mut group: Vec<RankSet>, used: RankSet) -> usize {
        if group.is_empty() {
            return 0;
        }

        let mut B = group.remove(0);

        let next = self.index_group(group, used.union(&B));
        let m_1 = B.len();
        let mut idx = binom(N - used.len(), m_1) * next;

        for i in 1..(m_1 + 1) {
            let largest = B.largest();

            // count how many lower rank cards have already been used, this is the adapted rank
            // todo move to rank set function?
            let rank = used.donwnshift_rank(largest);

            // check if this is right, should this not be the same as the index_set function?
            // or should it match what is in the paper
            idx += binom(rank, m_1 - i + 1);
            B.remove(largest);
        }

        idx
    }

    fn unindex_group(&self, idx: usize, mut ms: Vec<u8>, used: RankSet) -> Option<Vec<RankSet>> {
        let m_1 = ms.remove(0);
        let this = idx % binom(N - used.len(), m_1);
        let next = idx / binom(N - used.len(), m_1);

        let mut B = self.unindex_set(this, m_1)?;
        let mut A_1 = RankSet::default();

        for _ in 0..B.len() {
            let b = B.largest();
            B.remove(b);
            let a = used.upshift_rank(b);
            A_1.insert(a);
        }

        let mut group = vec![A_1];
        if !ms.is_empty() {
            let used = used.union(&A_1);
            let mut children = self.unindex_group(next, ms, used)?;
            group.append(&mut children);
        }

        Some(group)
    }

    /// Compute the index for M-rank sets
    ///
    /// which are sets of M card (`set.len()`) of the same suit, where
    /// the ranks are [0, N)
    ///
    /// The set it represented by a bit mask, 1 representing that card is present
    /// 0 representing it is not
    fn index_set(&self, set: RankSet) -> usize {
        self.index_group(vec![set], RankSet::default())
    }

    fn unindex_set(&self, idx: usize, m: u8) -> Option<RankSet> {
        if m == 1 {
            return Some(RankSet::new(&[idx as u8]));
        }

        let x = find_x(idx, m);
        // Over the max index
        if x >= N {
            return None;
        }
        let set = RankSet::new(&[x]);
        let children = self.unindex_set(idx - binom(x, m), m - 1)?;
        let set = set.union(&children);
        assert_eq!(set.len(), m);
        Some(set)
    }
}

pub struct Rank(u8);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_set() {
        let indexer = HandIndexer::<6>::default();

        for i in 0..6 {
            let set = RankSet::new(&[i]);
            assert_eq!(indexer.index_set(set), i as usize);
        }

        for i in 0..15 {
            let set = indexer.unindex_set(i, 2).unwrap();
            println!("{}: {:?} {}", i, set, indexer.index_set(set));
            let idx = indexer.index_set(set);
            assert_eq!(idx, i);
        }

        let set = RankSet::new(&[1, 0]);
        assert_eq!(indexer.index_set(set), 0);
        assert_eq!(indexer.unindex_set(0, 2).unwrap(), set);

        let set = RankSet::new(&[2, 0]);
        assert_eq!(indexer.index_set(set), 1);
        assert_eq!(indexer.unindex_set(1, 2).unwrap(), set);

        let set = RankSet::new(&[2, 1]);
        assert_eq!(indexer.index_set(set), 2);

        let set = RankSet::new(&[3, 0]);
        assert_eq!(indexer.index_set(set), 3);

        let set = RankSet::new(&[3, 1]);
        assert_eq!(indexer.index_set(set), 4);

        for i in 0..20 {
            let set = indexer.unindex_set(i, 3).unwrap();
            println!("{}: {:?}", i, set);
            let idx = indexer.index_set(set);
            assert_eq!(idx, i);
        }
    }

    #[test]
    fn test_index_group() {
        let indexer = HandIndexer::<6>::default();

        let set = RankSet::new(&[2, 1]);
        assert_eq!(indexer.index_group(vec![set], RankSet::new(&[0])), 0);

        for i in 0..60 {
            let group = indexer
                .unindex_group(i, vec![1, 2], RankSet::default())
                .unwrap();
            let idx = indexer.index_group(group.clone(), RankSet::default());
            println!("{}: {:?} {}", i, group, idx);
            assert_eq!(idx, i);
        }
    }
}
