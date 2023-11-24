use itertools::Itertools;

use crate::{math::binom, rankset::suit_config_size};

/// Enumerates all suit configurations for a given size of round
pub fn enumerate_suit_configs<const N: usize, const S: usize>(
    cards_per_round: &[usize],
) -> Vec<Vec<Vec<usize>>> {
    let mut round_configs = cards_per_round
        .into_iter()
        .map(|x| enumerate_suit_configs_round::<N, S>(*x))
        .collect_vec();

    let suit_counts = round_configs
        .into_iter()
        .multi_cartesian_product()
        .collect_vec();

    let mut configs = Vec::new();
    for x in suit_counts {
        let mut c = vec![vec![0; cards_per_round.len()]; S];
        for s in 0..S {
            for r in 0..cards_per_round.len() {
                c[s][r] = x[r][s];
            }
        }

        // Remove the all 0 suit configs
        c.retain(|x| x.iter().sum::<usize>() > 0);

        c.sort();
        c.reverse();
        // we do not want to de-dupe c as it's possible for multiple
        // suits to have the same config
        configs.push(c);
    }

    // Remove all invalid configs where we may have more that the number of cards per suit in a given
    // suit
    configs.retain(|x| x.iter().all(|y| y.iter().sum::<usize>() <= N as usize));

    configs.sort();
    configs.reverse();
    configs.dedup();
    configs
}

fn enumerate_suit_configs_round<const N: usize, const S: usize>(
    cards_in_rounds: usize,
) -> Vec<Vec<usize>> {
    let mut configs = Vec::new();

    // Iterate through all possible deals of cards by looking at their suit only
    for deal in (0..S).combinations_with_replacement(cards_in_rounds) {
        // transform the deal into a count by suit
        let mut c = [0; S];
        for d in deal {
            c[d] += 1;
        }
        // We do not want to sort at this level since we want to keep the different
        // suit configurations for when we combine by round
        configs.push(c.to_vec());
    }

    // Sort and remove all duplicate configs
    configs.sort();
    configs.reverse();
    configs.dedup();

    configs
}

/// Returns the total size of the index for a given configuration
///
/// for the (1)(1) config with N=13, should be 91
/// (1)(1)          91 (choose with replacement, or sum(i, 1 to N))
/// (1)(1)(1)       455
/// (1)(1)(1)(1)    1820
/// (2)(1)
pub fn configuration_index_size(config: &Vec<Vec<usize>>, cards_per_suit: usize) -> usize {
    if config.len() == 1 {
        return suit_config_size(&config[0], 13);
    }

    todo!()
}

#[cfg(test)]
mod tests {
    use std::vec;

    use super::*;

    #[test]
    fn test_config_index_size() {
        // When there is only a single suit, it is equivalent to
        // 13 choose N (no replacement)
        assert_eq!(configuration_index_size(&vec![vec![1]], 13), 13);
        assert_eq!(configuration_index_size(&vec![vec![2]], 13), 78);
        assert_eq!(configuration_index_size(&vec![vec![3]], 13), 286);

        // When only choose a single card from each suit, we can model this as
        // 13 choose 2 with replacement
        assert_eq!(configuration_index_size(&vec![vec![1]; 2], 13), 91);
        assert_eq!(configuration_index_size(&vec![vec![1]; 3], 13), 455);
        assert_eq!(configuration_index_size(&vec![vec![1]; 4], 13), 1820);

        // todo: fix this, tbd on what this should calculate to
        // Suit 1   Suit 2  Count
        // 12, 11   0..12 = 13
        // 12, 10   0..12 = 13
        // 12, 9    0..12 = 13
        // ...      ...     ...
        // 12, 0    0..12 = 13
        // 11, 10   0..12
        // is this (N-1) * (1)(1) = 91 * 12 = 1092?
        assert_eq!(configuration_index_size(&vec![vec![2], vec![1]], 13), 1092);
    }

    #[test]
    fn test_unindex_suit_config() {
        // test to make sure we filter out configs with too many cards in a single suit
        assert_eq!(
            enumerate_suit_configs::<1, 4>(&[2, 1]),
            vec![vec![vec![1, 0], vec![1, 0], vec![0, 1]]]
        );

        assert_eq!(
            enumerate_suit_configs::<13, 4>(&[2, 1]),
            vec![
                vec![vec![2, 1]],
                vec![vec![2, 0], vec![0, 1]],
                vec![vec![1, 1], vec![1, 0]],
                vec![vec![1, 0], vec![1, 0], vec![0, 1]]
            ]
        );

        assert_eq!(
            enumerate_suit_configs::<13, 4>(&[6]),
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
