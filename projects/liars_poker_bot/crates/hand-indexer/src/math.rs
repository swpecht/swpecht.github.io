pub fn factorial(n: u8) -> usize {
    let mut result: usize = 1;

    for i in 2..(n + 1) {
        result *= i as usize;
    }

    result
}

/// Compute the binomial coefficient for n choose m.
///
/// If n<m, returns 0
pub fn binom(n: u8, m: u8) -> usize {
    if n < m {
        return 0;
    }

    factorial(n) / (factorial(m) * factorial(n - m))
}

/// Finds the largest value x such that x choose m <= idx
pub fn find_x(idx: usize, m: u8) -> u8 {
    let mut x = 0;

    // TODO: replace with binary search
    loop {
        // when we find an x larger than our target index
        // we return the one before it
        if binom(x, m) > idx {
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
    }
}
