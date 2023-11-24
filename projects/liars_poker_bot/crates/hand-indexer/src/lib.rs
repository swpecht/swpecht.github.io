use configurations::{configuration_index_size, enumerate_suit_configs};

use math::find_x;
use rankset::{cmp_group_rank, group_config, suit_config_size, RankSet};

use crate::math::binom;

mod configurations;
mod math;
pub mod rankset;

/// Translates a euchre hand to a conanical index
///
/// N: number of cards per suit
/// S: max number of suits
///
/// Based on:
///     https://www.cs.cmu.edu/~waugh/publications/isomorphism13.pdf
#[derive(Default)]
pub struct HandIndexer<const N: usize, const S: usize> {
    /// Contains the sorted list of configurations and the index each of those configurations starts at
    configurations: Vec<(usize, Vec<Vec<usize>>)>,
}

impl<const N: usize, const S: usize> HandIndexer<N, S> {
    /// TODO: add offset for cards in just the earlier rounds
    pub fn new(cards_per_round: &[usize]) -> Self {
        let mut configurations = Vec::new();
        let mut index_start = 0;

        for c in enumerate_suit_configs::<N, S>(cards_per_round) {
            let size = configuration_index_size(&c, N);
            configurations.push((index_start, c));
            index_start += size;
        }

        HandIndexer { configurations }
    }

    /// Returns the maxmimum index for the indexer
    pub fn max_index(&self) -> usize {
        let (idx, c) = self.configurations.last().unwrap();
        idx + configuration_index_size(c, N)
    }

    /// Returns the relative index for a hand within a given suit configuration
    ///
    /// Each element in hand is the group of a given suit
    /// hand.len() == num_suits
    fn index_hand(&self, mut hand: Vec<Vec<RankSet>>) -> usize {
        if hand.is_empty() {
            return 0;
        }

        // TODO: Move this to be called only once at the start rather than on each iteration
        // We sort by the group confguration, but if there are equal group configurations,
        // we sort by the group index
        hand.sort_by(|a, b| {
            use std::cmp::Ordering::*;
            match cmp_group_rank(a, b) {
                Equal => self
                    .index_group(a.clone(), RankSet::default())
                    .cmp(&self.index_group(b.clone(), RankSet::default())),
                x => x,
            }
        });
        hand.reverse();

        // todo: we need to process this in batches of tied suits -- that's actually
        // what's happening in the paper
        //
        // The index is actually:
        //      sum( nCr(group_index + remaining_tied_suits - 1)(remaining_tied_suits) )
        // When the number of tied suits is 1, this collapses to
        //      nCr(group_index)(1) == group_index
        //
        // But I still need to figure out how to estimate the size of these combined indexes
        // for the single case, this is trivial as it's the number of possible cards
        //
        // It looks like the 156 in the paper is actually the size of the group index for the given
        // configuration 12 choose 1 * 11 choose 1
        // so that factor becomes:
        //      nCr(group_size + tied_suits - 1)(tied_suits)

        // todo: group all the suits with the same config together, and then apply the above
        // todo: look at the recursive description in the paper -- this is essentially what we're doing where j is the tied suit ranks

        // Collect all of the groups with the same config to process at once
        let g_1 = hand.remove(0);
        let config_1 = group_config(&g_1);
        let config_1_size = suit_config_size(&config_1, N);
        let mut same_config_group_indexes = vec![self.index_group(g_1, RankSet::default())];
        while hand
            .get(0)
            .map(|x| group_config(x) == config_1)
            .unwrap_or(false)
        {
            let g_i = hand.remove(0);
            same_config_group_indexes.push(self.index_group(g_i, RankSet::default()));
        }

        let next = self.index_hand(hand);

        let mut this = 0;
        let matching_configs = same_config_group_indexes.len();
        for (i, group_index) in same_config_group_indexes.into_iter().enumerate() {
            let remaing_tied_suits = matching_configs - i;
            this += binom(group_index + remaing_tied_suits - 1, remaing_tied_suits);
        }

        this + binom(config_1_size + matching_configs - 1, matching_configs) * next
    }

    fn unindex_hand(
        &self,
        idx: usize,
        mut suit_configutation: Vec<Vec<usize>>,
    ) -> Option<Vec<Vec<RankSet>>> {
        assert!(suit_configutation.len() < S);

        let c_1 = suit_configutation.remove(0);
        let c_1_size = suit_config_size(&c_1, N);
        let c_1_idx = idx % c_1_size;
        let g_1 = self.unindex_group(c_1_idx, c_1, RankSet::default())?;

        let remainder = idx / c_1_size;

        let mut hand = vec![g_1];

        if !suit_configutation.is_empty() {
            let mut remaining_suits = self.unindex_hand(remainder, suit_configutation)?;
            hand.append(&mut remaining_suits)
        }

        Some(hand)
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
        let mut idx = binom(N - used.len() as usize, m_1 as usize) * next;

        for i in 1..(m_1 + 1) {
            let largest = B.largest();
            if largest >= N as u8 {
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
            idx += binom(rank as usize, (m_1 - i + 1) as usize);
            B.remove(largest);
        }

        idx
    }

    fn unindex_group(&self, idx: usize, mut ms: Vec<usize>, used: RankSet) -> Option<Vec<RankSet>> {
        // Calcuate the size of the index group to see if we're at an impossible index
        let mut size = 1;
        for i in 0..ms.len() {
            size *= binom(
                N - ms.iter().take(i).sum::<usize>() as usize,
                ms[i] as usize,
            );
        }
        if idx >= size {
            return None;
        }

        let m_1 = ms.remove(0);
        let this = idx % binom((N as u8 - used.len()) as usize, m_1 as usize);
        let next = idx / binom((N as u8 - used.len()) as usize, m_1 as usize);

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

    fn unindex_set(&self, idx: usize, m: usize) -> Option<RankSet> {
        if m == 1 {
            return Some(RankSet::new(&[idx as u8]));
        }

        let x = find_x(idx, m as u8);
        // Over the max index
        if x >= N as u8 {
            return None;
        }
        let set = RankSet::new(&[x]);
        let children = self.unindex_set(idx - binom(x as usize, m), m - 1)?;
        let set = set.union(&children);
        assert_eq!(set.len() as usize, m);
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
    use std::{collections::HashSet, vec};

    use super::*;

    #[test]
    fn max_index() {
        // The deal in texas holdem
        // (2): 13 choose 2 = 78
        // (1)(1): 13 choose 2 with replacement = 91

        // Look the paper again to see how the config sizes are calculated -- doing something wrong here
        // maybe use the indexer to figure this out? Figure out what the max possible index is? for a given config?
        let indexer = HandIndexer::<13, 4>::new(&[2]);
        assert_eq!(indexer.max_index(), 169);

        // flop
        //
        // [[2, 3]]                             12870
        // [[2, 2], [0, 1]]                     55770
        // [[2, 1], [0, 2]]                     66924
        // [[2, 1], [0, 1], [0, 1]]             145002
        // [[2, 0], [0, 3]]                     22308
        // [[2, 0], [0, 2], [0, 1]]             79092
        // [[2, 0], [0, 1], [0, 1], [0, 1]]     171366
        // [[1, 3], [1, 0]]                     37180
        // [[1, 2], [1, 1]]
        // [[1, 2], [1, 0], [0, 1]]
        // [[1, 1], [1, 1], [0, 1]]
        // [[1, 1], [1, 0], [0, 2]]
        // [[1, 1], [1, 0], [0, 1], [0, 1]]
        // [[1, 0], [1, 0], [0, 3]]
        // [[1, 0], [1, 0], [0, 2], [0, 1]]
        let indexer = HandIndexer::<13, 4>::new(&[2, 3]);
        let configs = enumerate_suit_configs::<13, 4>(&[2, 3]);
        for c in configs {
            println!("{:?}", c);
        }

        assert_eq!(indexer.max_index(), 1_286_792); // from isomorphism paper

        // turn
        let indexer = HandIndexer::<13, 4>::new(&[2, 3, 1]);
        assert_eq!(indexer.max_index(), 55_190_538); // from isomorphism paper

        // river
        let indexer = HandIndexer::<13, 4>::new(&[2, 3, 1, 1]);
        assert_eq!(indexer.max_index(), 2_428_287_420); // from isomorphism paper
    }

    #[test]
    fn test_index_set() {
        let indexer = HandIndexer::<6, 4>::default();

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
        let indexer = HandIndexer::<6, 4>::default();

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
        let indexer = HandIndexer::<13, 4>::default();

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

        // This is corrected from the paper. In the paper, it is incorrectly calculated
        // that \binom{112+1}{2} = 6,216. But it actually is 6,328
        assert_eq!(idx, 141_090);

        // In the paper, this unindexes to: 2♠7♥|8♠6♠4♥
        // But \binom{13}{1} \binom{12}{2} is incorrectly evaluated as
        // 198 in the paper. But in reality it is 858. Making this
        // change results in the below hand
        assert_eq!(
            indexer
                .unindex_hand(6_220, vec![vec![1, 2], vec![1, 1]])
                .unwrap(),
            vec![
                vec![RankSet::new(&[6]), RankSet::new(&[7, 1])],
                vec![RankSet::new(&[7]), RankSet::new(&[0])]
            ]
        );
    }

    #[test]
    fn test_hand_integration() {
        let indexer = HandIndexer::<13, 4>::default();

        // A single handed suit should have \binom{13}{5} combinations
        validate_hand_config(&indexer, vec![vec![5]], 1_287);
        // \binom{13}{3} \binom{13}{2}
        validate_hand_config(&indexer, vec![vec![3], vec![2]], 22_308);
    }

    fn validate_hand_config<const N: usize, const S: usize>(
        indexer: &HandIndexer<N, S>,
        config: Vec<Vec<usize>>,
        amount: usize,
    ) {
        let mut hash_set = HashSet::new();
        for i in 0..amount {
            let hand = indexer.unindex_hand(i, config.clone()).unwrap();
            hash_set.insert(hand.clone());
            let idx = indexer.index_hand(hand);
            assert_eq!(idx, i);
        }

        // this should wrap to the first hand, so it shouldn't change the hashset size
        let hand = indexer.unindex_hand(amount, config.clone()).unwrap();
        hash_set.insert(hand.clone());
        assert_eq!(hash_set.len(), amount);
    }
}
