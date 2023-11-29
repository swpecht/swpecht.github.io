use core::num;
use std::{
    collections::HashMap,
    fmt::Debug,
    ops::{Deref, DerefMut},
};

use itertools::Itertools;

use crate::{math::binom, rankset::suit_config_size};

type SuitIndex = usize;

#[derive(Clone, Copy, PartialEq, PartialOrd, Eq, Ord, Hash)]
struct RoundConfig<const S: usize>([usize; S]);

impl<const S: usize> RoundConfig<S> {
    fn empty() -> Self {
        RoundConfig([0; S])
    }
}

impl<const S: usize> Deref for RoundConfig<S> {
    type Target = [usize; S];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<const S: usize> DerefMut for RoundConfig<S> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<const S: usize> Debug for RoundConfig<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.0.iter()).finish()
    }
}

/// Enumerates all suit configurations for a given size of round
pub fn enumerate_suit_configs<const S: usize>(
    cards_per_round: &[usize],
    cards_per_suit: [usize; S],
) -> Vec<Vec<Vec<usize>>> {
    let suit_counts = unique_suit_counts(cards_per_round, cards_per_suit);

    // Transform the suit counts into standard suit configuration format, i.e. change te top level of the list to be by
    // suit rather than by round
    let mut configs = Vec::new();
    for x in suit_counts {
        let c = suit_counts_to_config(x);
        // we do not want to de-dupe c as it's possible for multiple
        // suits to have the same config
        configs.push(c);
    }

    // Remove all invalid configs where we may have more that the number of cards per suit in a given
    // suit
    configs.retain(|x| {
        x.iter()
            .zip(cards_per_suit.iter())
            .all(|(y, max_cards)| y.iter().sum::<usize>() <= *max_cards)
    });

    configs.sort();
    configs.reverse();
    configs.dedup();
    configs
}

fn enumerate_suit_configs_round<const S: usize>(
    cards_in_round: usize,
    cards_per_suit: [usize; S],
) -> Vec<RoundConfig<S>> {
    let mut configs = Vec::new();

    let deck = get_deck(cards_per_suit);
    // Iterate through all possible deals of cards by looking at their suit only
    for deal in deck.into_iter().combinations(cards_in_round) {
        // transform the deal into a count by suit
        let mut c = RoundConfig::empty();
        for d in deal {
            c[d] += 1;
        }
        // We do not want to sort or de-dupe at this level since we want to keep the different
        // suit configurations for when we combine by round
        configs.push(c);
    }

    configs
}

/// Returns the unique suit counts by roung
fn unique_suit_counts<const S: usize>(
    cards_per_round: &[usize],
    cards_per_suit: [usize; S],
) -> Vec<Vec<RoundConfig<S>>> {
    let round_configs = cards_per_round
        .iter()
        .map(|x| {
            let mut r = enumerate_suit_configs_round::<S>(*x, cards_per_suit);
            // Sort and remove all duplicate configs for a given round since we only care about the unique ones
            r.sort();
            r.reverse();
            r.dedup();
            r
        })
        .collect_vec();

    round_configs
        .into_iter()
        .multi_cartesian_product()
        .collect_vec()
}

/// Returns the total size of the index for all configurations
pub fn configuration_index_size<const S: usize>(
    cards_per_round: &[usize],
    cards_per_suit: [usize; S],
) -> HashMap<Vec<Vec<usize>>, usize> {
    let configs = enumerate_suit_configs(cards_per_round, cards_per_suit);
    let std_suit_counts = configs.into_iter().map(config_to_suit_counts).collect_vec();

    let round_configs = cards_per_round
        .iter()
        .map(|x| {
            // unlike above, we don't remove the the duplicates since we want the full count of deals, not just the number of configs
            enumerate_suit_configs_round::<S>(*x, cards_per_suit)
        })
        .collect_vec();

    // todo filter out things that don't match the valid suit configs

    let deal_configs = round_configs
        .into_iter()
        .multi_cartesian_product()
        .collect_vec();

    let mut sizes = HashMap::new();
    for deal in deal_configs {
        if !std_suit_counts.contains(&deal) {
            continue;
        }
        let count = sizes.entry(deal).or_insert(0);
        *count += 1;
    }

    let mut config_sizes = HashMap::new();
    for (k, v) in sizes {
        let c = suit_counts_to_config(k);
        config_sizes.insert(c, v);
    }

    config_sizes
}

/// Return a vector where each item is the suit index of a given card
fn get_deck<const S: usize>(cards_per_suit: [usize; S]) -> Vec<SuitIndex> {
    let mut cards = Vec::with_capacity(cards_per_suit.iter().sum());
    for (i, num_cards) in cards_per_suit.iter().enumerate() {
        cards.append(&mut vec![i; *num_cards]);
    }
    cards
}

fn suit_counts_to_config<const S: usize>(suit_count: Vec<RoundConfig<S>>) -> Vec<Vec<usize>> {
    let mut c = vec![vec![0; suit_count.len()]; S];
    (0..S).for_each(|s| {
        (0..suit_count.len()).for_each(|r| {
            c[s][r] = suit_count[r][s];
        });
    });

    // Remove the all 0 suit configs
    c.retain(|x| x.iter().sum::<usize>() > 0);

    c.sort();
    c.reverse();
    c
}

fn config_to_suit_counts<const S: usize>(config: Vec<Vec<usize>>) -> Vec<RoundConfig<S>> {
    let num_rounds = config.iter().map(|x| x.len()).max().unwrap_or(0);
    let mut counts = vec![RoundConfig::empty(); num_rounds];

    for (s, round_count) in config.into_iter().enumerate() {
        for (r, c) in round_count.into_iter().enumerate() {
            counts[r][s] = c;
        }
    }

    counts
}

#[cfg(test)]
mod tests {
    use std::vec;

    use super::*;

    #[test]
    fn test_config_index_size() {
        let sizes = configuration_index_size(&[2], [13; 4]);
        assert_eq!(*sizes.get(&vec![vec![2]]).unwrap(), 78);
        assert_eq!(*sizes.get(&vec![vec![1], vec![1]]).unwrap(), 91);
        assert_eq!(sizes.values().sum::<usize>(), 169);
        // When there is only a single suit, it is equivalent to
        // 13 choose N (no replacement)
        // assert_eq!(configuration_index_size(&[vec![vec![1]]], [13; 4]), 13);
        // assert_eq!(configuration_index_size(&vec![vec![2]], [13; 4]), 78);
        // assert_eq!(configuration_index_size(&vec![vec![3]], [13; 4]), 286);

        // // When only choose a single card from each suit, we can model this as
        // // 13 choose 2 with replacement
        // assert_eq!(configuration_index_size(&vec![vec![1]; 2], [13; 4]), 91);
        // assert_eq!(configuration_index_size(&vec![vec![1]; 3], [13; 4]), 455);
        // assert_eq!(configuration_index_size(&vec![vec![1]; 4], [13; 4]), 1820);

        // assert_eq!(
        //     configuration_index_size(&vec![vec![2], vec![2]], [13; 4]),
        //     3081
        // );

        // assert_eq!(
        //     configuration_index_size(&vec![vec![2], vec![1]], [13; 4]),
        //     1014
        // );
        // assert_eq!(
        //     configuration_index_size(&vec![vec![2], vec![2], vec![1]], [13; 4]),
        //     40053
        // );

        // // test that rounds are working
        // assert_eq!(
        //     configuration_index_size(&vec![vec![1, 2], vec![0, 2]], 13),
        //     13 * 2805
        // );

        // // there should only be 11 cards left for the first suit, so the factor
        // // is 11 choose 1
        // assert_eq!(
        //     configuration_index_size(&vec![vec![2, 1], vec![2], vec![1]], 13),
        //     40053 * 11
        // );
        todo!()
    }

    #[test]
    fn test_enumerate_suit_configs() {
        // test to make sure we filter out configs with too many cards in a single suit
        assert_eq!(
            enumerate_suit_configs(&[2, 1], [1; 4]),
            vec![vec![vec![1, 0], vec![1, 0], vec![0, 1]]]
        );

        assert_eq!(
            enumerate_suit_configs(&[2, 1], [13; 4]),
            vec![
                vec![vec![2, 1]],
                vec![vec![2, 0], vec![0, 1]],
                vec![vec![1, 1], vec![1, 0]],
                vec![vec![1, 0], vec![1, 0], vec![0, 1]]
            ]
        );

        assert_eq!(
            enumerate_suit_configs(&[6], [13; 4]),
            vec![
                vec![vec![6]],
                vec![vec![5], vec![1]],
                vec![vec![4], vec![2]],
                vec![vec![4], vec![1], vec![1]],
                vec![vec![3], vec![3]],
                vec![vec![3], vec![2], vec![1]],
                vec![vec![3], vec![1], vec![1], vec![1]],
                vec![vec![2], vec![2], vec![2]],
                vec![vec![2], vec![2], vec![1], vec![1]],
            ]
        );
    }
}
