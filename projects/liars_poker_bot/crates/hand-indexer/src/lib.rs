use std::default;

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
    /// Compute the index for M-rank sets
    ///
    /// which are sets of M card (`set.len()`) of the same suit, where
    /// the ranks are [0, N)
    ///
    /// The set it represented by a bit mask, 1 representing that card is present
    /// 0 representing it is not
    pub fn index_set(&self, mut set: RankSet) -> usize {
        assert!(!set.is_empty(), "cannot take rank of empty set");
        assert!(
            set.largest() < N,
            "found rank of: {}, max rank is: {}",
            set.largest(),
            N
        );

        // When the set is length one, we can trivially
        // count the sets less than `a` -- it's just the rank
        // of `a`
        if set.len() == 1 {
            return set.largest() as usize;
        }

        let m = set.len();

        let mut index = 0;
        for i in 1..(m + 1) {
            let a_i = set.largest();
            set.remove(a_i);
            index += binom(a_i, m - i + 1)
        }
        index
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

        let set = RankSet::new(&[1, 0]);
        assert_eq!(indexer.index_set(set), 0);

        let set = RankSet::new(&[2, 0]);
        assert_eq!(indexer.index_set(set), 1);

        let set = RankSet::new(&[2, 1]);
        assert_eq!(indexer.index_set(set), 2);

        let set = RankSet::new(&[3, 0]);
        assert_eq!(indexer.index_set(set), 3);

        let set = RankSet::new(&[3, 1]);
        assert_eq!(indexer.index_set(set), 4);
    }
}
