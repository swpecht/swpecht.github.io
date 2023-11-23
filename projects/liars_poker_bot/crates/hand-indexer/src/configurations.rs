use itertools::Itertools;

/// Enumerates all suit configurations for a given size of round
fn enumerate_suit_configs<const N: usize, const S: usize>(
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

#[cfg(test)]
mod tests {
    use std::vec;

    use super::*;

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
