use itertools::Itertools;

pub fn factorial(n: usize) -> usize {
    let mut result: usize = 1;

    for i in 2..(n + 1) {
        result *= i;
    }

    result
}

/// Compute the binomial coefficient for n choose m.
///
/// If n<m, returns 0
///
/// Adapted from:
///     https://math.stackexchange.com/questions/202554/how-do-i-compute-binomial-coefficients-efficiently
pub fn binom(n: usize, k: usize) -> usize {
    if n < k {
        return 0;
    }

    if k == 0 {
        return 1;
    }

    if k > n / 2 {
        return binom(n, n - k);
    }

    n * binom(n - 1, k - 1) / k
}

/// Finds the largest value x such that x choose m <= idx
pub fn find_x(idx: usize, m: u8) -> u8 {
    let mut x: u8 = 0;

    // TODO: replace with binary search
    loop {
        // when we find an x larger than our target index
        // we return the one before it
        if binom(x as usize, m as usize) > idx {
            return x - 1;
        }
        x += 1;
    }
}

/// Return the count of the number of variants when selecting `n`
/// cards from each suit with a different number of cards in each
///
/// For cases where all suits have the same number of cards in them, this should
/// be the same as the [multi-set coefficient](https://en.wikipedia.org/wiki/Multiset#Counting_multisets).
pub fn variants(n: usize, num_cards: &[usize]) -> usize {
    let mut v = num_cards
        .iter()
        .map(|x| (0..*x).combinations(2).sorted())
        .multi_cartesian_product()
        .map(|mut x| {
            x.sort();
            x
        })
        .collect_vec();

    v.sort();
    v.dedup();

    // println!("{:?}", v);
    v.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_factorial() {
        assert_eq!(factorial(1), 1);
        assert_eq!(factorial(5), 120);
        assert_eq!(factorial(6), 720);
    }

    #[test]
    fn test_binom() {
        assert_eq!(binom(1, 6), 0);
        assert_eq!(binom(6, 1), 6);
        assert_eq!(binom(4, 2), 6);
        assert_eq!(binom(142, 1), 142);
    }
}
