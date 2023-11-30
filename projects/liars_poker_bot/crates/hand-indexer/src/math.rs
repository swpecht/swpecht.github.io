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

/// Returns the coefficient of `x^{target}` when the expressions are expanded
///
/// Expressions are a vector of coefficients for: [x^0, x^1, ...]
///
/// For example: $(1+x)^{13}$ is `vec![vec![1, 1]; 13]`
pub fn coefficient(mut expressions: Vec<Vec<usize>>, target: usize) -> usize {
    let mut a = expressions.pop().unwrap_or_default();
    while let Some(b) = expressions.pop() {
        a = coefficient_a_b(&a, &b);
    }

    a[target]
}

fn coefficient_a_b(coeffs_a: &[usize], coeffs_b: &[usize]) -> Vec<usize> {
    let num_terms = (coeffs_a.len() - 1) + (coeffs_b.len() - 1) + 1;
    let mut result = vec![0; num_terms];
    for (a, c_a) in coeffs_a.iter().enumerate() {
        for (b, c_b) in coeffs_b.iter().enumerate() {
            result[a + b] += c_a * c_b;
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_coefficient() {
        assert_eq!(coefficient(vec![vec![1, 1]], 1), 1);
        assert_eq!(coefficient(vec![vec![1, 1]; 2], 1), 2);

        // test equal for binomial coefficients
        assert_eq!(coefficient(vec![vec![1, 1]; 13], 2), binom(13, 2));

        // Confirm an equality from wolfram alpha
        // Coefficient[(1+x+x^2)^13, x^4]
        assert_eq!(coefficient(vec![vec![1, 1, 1]; 13], 4), 1651);
        // Coefficient[(1+x+x^2)^12*(1+x), x^4]
        let mut e = vec![vec![1, 1, 1]; 12];
        e.push(vec![1, 1]);
        assert_eq!(coefficient(e, 4), 1573);
    }

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
