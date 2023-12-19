use cards::{cardset::CardSet, iterators::IsomorphicDealIterator, Deck};
use configurations::{configuration_index_size, enumerate_suit_configs};

use itertools::Itertools;
use math::find_x;
use rankset::{cmp_group_rank, group_config, suit_config_size, RankSet};

use crate::math::binom;

pub mod cards;
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
    /// index of configurations for the start of each round
    round_offsets: Vec<usize>,
}

impl HandIndexer<13, 4> {
    /// Create a new hand indexer for texas hold-em. This is an expensive operation
    pub fn poker() -> Self {
        let cards_per_round = [2]; //, 3]; // todo: add the river on later
        let deck = Deck::standard();
        let mut configurations = Vec::new();
        let mut round_offsets = Vec::with_capacity(cards_per_round.len());
        let mut index = 0;

        for r in 0..cards_per_round.len() {
            round_offsets.push(configurations.len());
            for c in enumerate_suit_configs(&cards_per_round[0..r + 1], [13; 4]) {
                configurations.push((index, c.clone()));
                let dealer =
                    IsomorphicDealIterator::for_config(deck, &cards_per_round[0..r + 1], c);
                let config_size = dealer.count();
                index += config_size;
            }
        }

        // push a final one for the last index possible index
        round_offsets.push(configurations.len());
        configurations.push((index, Vec::new()));

        Self {
            configurations,
            round_offsets,
        }
    }
}

impl<const N: usize, const S: usize> HandIndexer<N, S> {
    /// Returns the maxmimum index for the indexer
    pub fn index_size(&self, round: usize) -> usize {
        assert!(round + 1 < self.round_offsets.len());
        self.configurations[self.round_offsets[round + 1]].0
            - self.configurations[self.round_offsets[round]].0
    }

    pub fn unindex(&self, idx: usize) -> Option<Vec<CardSet>> {
        let (config_offset, config) = self.configurations.iter().filter(|x| x.0 <= idx).last()?;
        let hand = self.unindex_hand(idx - config_offset, config.clone())?;
        let rounds = hand.iter().map(|x| x.len()).max().unwrap_or(0);

        let mut result = Vec::with_capacity(hand.len());
        for r in 0..rounds {
            let mut cards = [RankSet::default(); 4];
            for s in 0..hand.len() {
                cards[s] = *hand[s].get(r).unwrap_or(&RankSet::default());
            }
            result.push(cards.into());
        }

        Some(result)
    }

    /// Index a hand where each CardSet is a round
    pub fn index(&self, hand: &[CardSet]) -> Option<usize> {
        let rounds = hand.len();
        let mut rank_hand = vec![vec![RankSet::default(); rounds]; 4];

        for r in 0..rounds {
            let suits: [RankSet; 4] = hand[r].into();
            for s in 0..4 {
                rank_hand[s][r] = suits[s];
            }
        }

        let config = rank_hand
            .iter()
            .map(|x| group_config(x)) // Remove any 0 size suits
            .filter(|x| !x.iter().all(|&c| c == 0))
            .collect_vec();

        let config_offset = self.configurations.iter().find(|x| x.1 == config)?.0;

        Some(config_offset + self.index_hand(rank_hand))
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

        let matching_configs = same_config_group_indexes.len();
        let this = self.multiset_colex(same_config_group_indexes);
        let next = self.index_hand(hand);

        this + binom(config_1_size + matching_configs - 1, matching_configs) * next
    }

    fn multiset_colex(&self, group_indexes: Vec<usize>) -> usize {
        let mut this = 0;
        let matching_configs = group_indexes.len();
        for (i, group_index) in group_indexes.into_iter().enumerate() {
            let remaing_tied_suits = matching_configs - i;
            this += binom(group_index + remaing_tied_suits - 1, remaing_tied_suits);
        }

        this
    }

    /// The first level is the suit
    fn unindex_hand(
        &self,
        mut index: usize,
        suit_configutation: Vec<Vec<usize>>,
    ) -> Option<Vec<Vec<RankSet>>> {
        assert!(suit_configutation.len() < S);

        let mut suit_index = [0; S];
        let mut i = 0;
        while i < suit_configutation.len() {
            let mut j = i + 1;
            while j < suit_configutation.len() && suit_configutation[i] == suit_configutation[j] {
                j += 1;
            }

            let c_i = &suit_configutation[i];
            let suit_size = suit_config_size(c_i, N);
            // use the multiset coefficieint to calculate the size of tied groups, when there are no tied groups,
            // this becomes just the suit size
            let group_size = binom(suit_size + j - i - 1, j - i);
            let mut group_index = index % group_size;
            index /= group_size;

            while i < j - 1 {
                // low = floor(exp(log(group_index)/(j-i) - 1 + log(j-i))-j-i);
                // high = ceil(exp(log(group_index)/(j-i) + log(j-i))-j+i+1);
                let mut low = f64::floor(
                    f64::exp(
                        f64::ln(group_index as f64) / ((j - i) as f64) - 1f64
                            + f64::ln((j - i) as f64),
                    ) - j as f64
                        - i as f64,
                ) as usize;

                let mut high = f64::ceil(
                    f64::exp(
                        f64::ln(group_index as f64) / ((j - i) as f64) + f64::ln((j - i) as f64),
                    ) - j as f64
                        - i as f64
                        + 1f64,
                ) as usize;

                suit_index[i] = low;
                if high > suit_size {
                    high = suit_size;
                }
                if high <= low {
                    low = 0;
                }

                while low < high {
                    let mid = (low + high) / 2;
                    if binom(mid + j - i - 1, j - i) <= group_index {
                        suit_index[i] = mid;
                        low = mid + 1;
                    } else {
                        high = mid;
                    }
                }

                group_index -= binom(suit_index[i] + j - i - 1, j - i);

                i += 1;
            }

            suit_index[i] = group_index;
            i += 1;
        }

        let mut hand = Vec::with_capacity(suit_configutation.len());
        for i in 0..suit_configutation.len() {
            hand.push(self.unindex_group(
                suit_index[i],
                suit_configutation[i].clone(),
                RankSet::default(),
            )?);
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
            size *= binom(N - ms.iter().take(i).sum::<usize>(), ms[i]);
        }
        if idx >= size {
            return None;
        }

        let m_1 = ms.remove(0);
        let this = idx % binom((N as u8 - used.len()) as usize, m_1);
        let next = idx / binom((N as u8 - used.len()) as usize, m_1);

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

pub struct Rank(u8);

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, vec};

    use super::*;

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
        for i in 0..100 {
            let hand = indexer.unindex_hand(i % 13, vec![vec![1]]).unwrap();
            assert_eq!(hand[0][0].largest(), (i % 13) as u8);
        }

        // A single handed suit should have \binom{13}{5} combinations
        validate_hand_config(&indexer, vec![vec![5]], 1_287);
        // \binom{13}{3} \binom{13}{2}
        validate_hand_config(&indexer, vec![vec![3], vec![2]], 22_308);

        // isomorphic hands for pocket deal in poker
        validate_hand_config(&indexer, vec![vec![2]], 78);
        validate_hand_config(&indexer, vec![vec![1], vec![1]], 91);
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
            let idx = indexer.index_hand(hand.clone());
            let unindexed_hand = indexer.unindex_hand(idx, config.clone());
            assert_eq!(idx, i, "{:?} {:?}", unindexed_hand, hand);
        }

        // this should wrap to the first hand, so it shouldn't change the hashset size
        let hand = indexer.unindex_hand(amount, config.clone()).unwrap();
        hash_set.insert(hand.clone());
        assert_eq!(hash_set.len(), amount);
    }
}
