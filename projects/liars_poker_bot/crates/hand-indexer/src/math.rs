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
