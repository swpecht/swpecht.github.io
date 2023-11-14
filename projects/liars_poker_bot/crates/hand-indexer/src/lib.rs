use rankset::RankSet;

mod math;
pub mod rankset;

/// Translates a euchre hand to a conanical index
///
/// Based on:
///     https://www.cs.cmu.edu/~waugh/publications/isomorphism13.pdf
struct HandIndexer {}

/// The number of cards per suit
const N: u8 = 6;

pub struct Rank(u8);

/// Compute the index for M-rank sets
///
/// which are sets of M card (`set.len()`) of the same suit, where
/// the ranks are [0, N)
///
/// The set it represented by a bit mask, 1 representing that card is present
/// 0 representing it is not
pub fn index_set(set: &RankSet) -> usize {
    assert!(!set.is_empty(), "cannot take rank of empty set");

    // When the set is length one, we can trivially
    // count the sets less than `a` -- it's just the rank
    // of `a`
    if set.len() == 1 {
        return set.largest() as usize;
    }

    let m = set.len();

    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_set() {
        for i in 0..N {
            let set = RankSet::new(&[i]);
            assert_eq!(index_set(&set), i as usize);
        }
    }
}
