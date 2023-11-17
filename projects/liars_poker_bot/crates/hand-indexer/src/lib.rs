use std::default;

use math::find_x;
use rankset::{Group, RankSet};

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
    /// Returns the index for a hand
    ///
    /// Each element in hand is the group of a given suit
    /// hand.len() == num_suits
    /// hand[0] == hand configuration
    fn index_hand(&self, hand: Vec<Vec<RankSet>>) -> usize {
        if hand.is_empty() {
            return 0;
        }

        let mut hand: Vec<Group> = hand.into_iter().map(Group).collect();
        hand.sort();
        hand.reverse();

        // todo: add sorting here to ensure the suits are sorted?

        let suit_groupindex: Vec<usize> = hand
            .iter()
            .map(|h| self.index_group(h.0.clone(), RankSet::default()))
            .collect();

        let mut idx = suit_groupindex[0];
        for i in 1..suit_groupindex.len() {
            let mut prev_idx_size = 1;
            let mut used_cards = 0;
            for x in hand[i - 1].0.iter() {
                prev_idx_size *= binom(N - used_cards, x.len());
                used_cards += x.len();
            }

            idx += prev_idx_size * suit_groupindex[i]
        }

        // todo: need to implement the suit configuration indexes to
        // give the right offsets. Right now, we only index within a
        // given suit configuration

        idx
    }

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
            if largest >= N {
                panic!(
                    "attempted to index a rank >= N. N: {}, rank: {}",
                    N, largest
                );
            }

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

/// Returns the maximum valid index for a given configuration
pub fn find_max() -> usize {
    todo!()
}

pub struct Rank(u8);

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

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

        let mut hash_set = HashSet::new();
        for i in 0..20 {
            let set = indexer.unindex_set(i, 3).unwrap();
            println!("{}: {:?}", i, set);
            let idx = indexer.index_set(set);
            assert_eq!(idx, i);
            hash_set.insert(set);
        }
        assert_eq!(hash_set.len(), 20);
    }

    #[test]
    fn test_index_group() {
        let indexer = HandIndexer::<6>::default();

        let set = RankSet::new(&[2, 1]);
        assert_eq!(indexer.index_group(vec![set], RankSet::new(&[0])), 0);

        let mut hash_set = HashSet::new();
        for i in 0..60 {
            let group = indexer
                .unindex_group(i, vec![1, 2], RankSet::default())
                .unwrap();
            let idx = indexer.index_group(group.clone(), RankSet::default());
            println!("{}: {:?} {}", i, group, idx);
            assert_eq!(idx, i);
            hash_set.insert(group);
        }
        assert_eq!(hash_set.len(), 60);
    }

    /// Test examples from the paper
    #[test]
    fn test_paper_examples() {
        // 13 cards per suit is a regular deck of cards
        let indexer = HandIndexer::<13>::default();

        // Compute the index for 2♣A♣|6♣J♥K♥
        // Spades
        let idx = indexer.index_group(
            vec![RankSet::new(&[12, 0]), RankSet::new(&[4])],
            RankSet::default(),
        );
        // This should be 300, there is an error in the published paper
        // that has this result as 306, but this does not match the actual
        // evaluation of the intermediate terms listed in the paper
        assert_eq!(idx, 300);

        // For hearts
        let idx = indexer.index_group(
            vec![RankSet::default(), RankSet::new(&[11, 9])],
            RankSet::default(),
        );
        assert_eq!(idx, 64);

        let idx = indexer.index_hand(vec![
            vec![RankSet::new(&[12, 0]), RankSet::new(&[4])], // clubs
            vec![RankSet::default(), RankSet::new(&[11, 9])], // hearts
        ]);
        // Propogating the correction to 300 instead of 306
        // gives the below corrected index
        assert_eq!(idx, 55_212);

        // verify the order doesn't matter
        let idx = indexer.index_hand(vec![
            vec![RankSet::default(), RankSet::new(&[11, 9])], // hearts
            vec![RankSet::new(&[12, 0]), RankSet::new(&[4])], // clubs
        ]);
        assert_eq!(idx, 55_212);

        // index for 6♦T ♣|J♣7♦K♥
        // clubs
        let idx = indexer.index_group(
            vec![RankSet::new(&[8]), RankSet::new(&[9])],
            RankSet::default(),
        );
        assert_eq!(idx, 112);

        // diamonds
        let idx = indexer.index_group(
            vec![RankSet::new(&[4]), RankSet::new(&[5])],
            RankSet::default(),
        );
        assert_eq!(idx, 56);

        // hearts
        let idx = indexer.index_group(
            vec![RankSet::new(&[]), RankSet::new(&[11])],
            RankSet::default(),
        );
        assert_eq!(idx, 11);

        let idx = indexer.index_hand(vec![
            vec![RankSet::new(&[8]), RankSet::new(&[9])], // clubs
            vec![RankSet::new(&[4]), RankSet::new(&[5])], // diamonds
            vec![RankSet::new(&[]), RankSet::new(&[11])], // hearts
        ]);
        assert_eq!(idx, 140_078);

        todo!()
    }
}
