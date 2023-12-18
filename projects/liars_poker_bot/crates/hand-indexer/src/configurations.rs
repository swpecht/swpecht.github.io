use core::num;
use std::{
    collections::HashMap,
    fmt::Debug,
    ops::{Deref, DerefMut},
};

use itertools::Itertools;

use crate::{
    cards::{cardset::CardSet, Deck},
    math::{binom, coefficient},
};

type SuitIndex = usize;

/// The number of cards for each suit in a given round
#[derive(Clone, Copy, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub(super) struct RoundConfig<const S: usize>([usize; S]);

impl<const S: usize> RoundConfig<S> {
    pub fn empty() -> Self {
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
    // todo!(
    //     "change back to doing combinations with repetition for the suit index to speed things up"
    // );
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

    let deck = (0..S).collect_vec();
    // Iterate through all possible deals of cards by looking at their suit only
    for deal in deck
        .into_iter()
        .combinations_with_replacement(cards_in_round)
    {
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

fn coefficient_config_size<const S: usize>(
    config: Vec<Vec<usize>>,
    mut cards_per_suit: [usize; S],
) -> usize {
    let rounds = config.iter().map(|x| x.len()).max().unwrap_or(0);

    let mut size = 1;
    let mut s = 0;
    while s < config.len() {
        // get all suits with the same config up to the current round, these are dependant on each other
        // j-1 is the index of the last matching suit
        // todo: actually want to do this over all rounds
        let mut j = s + 1;
        for i in j..config.len() {
            if config[s] == config[i] {
                j += 1;
            }
        }
        let matching_suits = j - s;
        for r in 0..rounds {
            let cards_to_deal_per_suit = *config.get(s).and_then(|x| x.get(r)).unwrap_or(&0);
            let remaining_cards = cards_per_suit[s];
            let variant_size = coefficient(
                vec![vec![1; matching_suits + 1]; binom(remaining_cards, cards_to_deal_per_suit)],
                matching_suits,
            );

            size *= variant_size;

            for i in s..j {
                cards_per_suit[i] -= cards_to_deal_per_suit;
            }
        }
        s = j;
    }

    size
}

// Calculates the size of the variant using generating functions
fn variant_size(cards_to_choose: usize, remaining_cards: &[usize]) -> usize {
    let mut valid_suits = 0;
    let mut unique_counts = Vec::new();
    let mut suits_gte_count = Vec::new();

    for &r in remaining_cards {
        if r == 0 {
            continue;
        }

        valid_suits += 1;
        if !unique_counts.contains(&r) {
            unique_counts.push(r);
        }
    }

    unique_counts.sort();
    for c in &unique_counts {
        suits_gte_count.push(remaining_cards.iter().filter(|x| **x >= *c).count());
    }
    assert_eq!(suits_gte_count[0], valid_suits);

    let mut generating_function = Vec::new();

    // start with the smallest of the remaining cards, we don't need to subtract any variants from this
    generating_function.append(&mut vec![
        vec![1; suits_gte_count[0] + 1];
        binom(unique_counts[0], cards_to_choose)
    ]);

    for i in 1..unique_counts.len() {
        generating_function.append(&mut vec![
            vec![1; suits_gte_count[i] + 1];
            binom(unique_counts[i], cards_to_choose)
                - binom(unique_counts[i - 1], cards_to_choose)
        ]);
    }

    coefficient(generating_function, valid_suits)
}

fn deal_to_suit_counts<const R: usize>(deal: [CardSet; R], deck: Deck) -> [RoundConfig<4>; R] {
    // Implement this function, then can use it to filter enumeration by suit config, and validate which ones are wrong
    todo!()
}

#[cfg(test)]
mod tests {
    use std::vec;

    use crate::cards::iterators::IsomorphicDealIterator;

    use super::*;

    #[test]
    fn test_coefficient_config_size() {
        assert_eq!(coefficient_config_size(vec![vec![2]], [13; 4]), 78);
        assert_eq!(coefficient_config_size(vec![vec![1], vec![1]], [13; 4]), 91);

        assert_eq!(
            coefficient_config_size(vec![vec![2], vec![2]], [13; 4]),
            3081
        );

        assert_eq!(
            coefficient_config_size(vec![vec![1, 1], vec![1, 1]], [13; 4]),
            brute_force(vec![vec![1, 1], vec![1, 1]])
        );

        assert_eq!(
            coefficient_config_size(vec![vec![2, 1], vec![0, 1], vec![0, 1]], [13; 4]),
            78_078
        );

        assert_eq!(get_round_size([2]), 169);
        assert_eq!(get_round_size([2, 3]), 1_286_792);
        assert_eq!(get_round_size([2, 3, 1]), 55_190_538);
        assert_eq!(get_round_size([2, 3, 1, 1]), 2_428_287_420);
    }

    fn get_round_size<const R: usize>(cards_per_round: [usize; R]) -> usize {
        let deck = Deck::standard();
        let configs = enumerate_suit_configs(&cards_per_round, [13; 4]);
        let mut size = 0;
        for c in configs {
            let dealer = IsomorphicDealIterator::for_config(deck, &cards_per_round, c.clone());
            let s = coefficient_config_size(c.clone(), [13; 4]);
            let deal_size = dealer.count();
            assert_eq!(s, deal_size, "{:?}", c);
            println!("validated: {:?}: {}", c, s);
            size += s;
        }
        size
    }

    fn brute_force(config: Vec<Vec<usize>>) -> usize {
        let deck = Deck::standard();
        let rounds = config.iter().map(|x| x.len()).max().unwrap_or(0);
        let mut cards_per_round = Vec::new();
        for r in 0..rounds {
            cards_per_round.push(config.iter().map(|x| x.get(r).unwrap_or(&0)).sum())
        }

        IsomorphicDealIterator::for_config(deck, &cards_per_round, config).count()
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

        assert_eq!(
            enumerate_suit_configs(&[2, 3], [13; 4]),
            vec![
                vec![vec![2, 3],],
                vec![vec![2, 2], vec![0, 1],],
                vec![vec![2, 1], vec![0, 2],],
                vec![vec![2, 1], vec![0, 1], vec![0, 1],],
                vec![vec![2, 0], vec![0, 3],],
                vec![vec![2, 0], vec![0, 2], vec![0, 1],],
                vec![vec![2, 0], vec![0, 1], vec![0, 1], vec![0, 1],],
                vec![vec![1, 3], vec![1, 0],],
                vec![vec![1, 2], vec![1, 1],],
                vec![vec![1, 2], vec![1, 0], vec![0, 1],],
                vec![vec![1, 1], vec![1, 1], vec![0, 1],],
                vec![vec![1, 1], vec![1, 0], vec![0, 2],],
                vec![vec![1, 1], vec![1, 0], vec![0, 1], vec![0, 1],],
                vec![vec![1, 0], vec![1, 0], vec![0, 3],],
                vec![vec![1, 0], vec![1, 0], vec![0, 2], vec![0, 1],],
            ]
        );

        // euchre
        assert_eq!(
            enumerate_suit_configs(&[1, 5], [6; 4]),
            vec![
                vec![vec![1, 5],],
                vec![vec![1, 4], vec![0, 1],],
                vec![vec![1, 3], vec![0, 2],],
                vec![vec![1, 3], vec![0, 1], vec![0, 1],],
                vec![vec![1, 2], vec![0, 3],],
                vec![vec![1, 2], vec![0, 2], vec![0, 1],],
                vec![vec![1, 2], vec![0, 1], vec![0, 1], vec![0, 1],],
                vec![vec![1, 1], vec![0, 4],],
                vec![vec![1, 1], vec![0, 3], vec![0, 1],],
                vec![vec![1, 1], vec![0, 2], vec![0, 2],],
                vec![vec![1, 1], vec![0, 2], vec![0, 1], vec![0, 1],],
                vec![vec![1, 0], vec![0, 5],],
                vec![vec![1, 0], vec![0, 4], vec![0, 1],],
                vec![vec![1, 0], vec![0, 3], vec![0, 2],],
                vec![vec![1, 0], vec![0, 3], vec![0, 1], vec![0, 1],],
                vec![vec![1, 0], vec![0, 2], vec![0, 2], vec![0, 1],],
            ]
        );
    }
}
