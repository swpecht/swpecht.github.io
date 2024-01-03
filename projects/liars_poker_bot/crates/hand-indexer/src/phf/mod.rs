use boomphf::Mphf;
use itertools::Itertools;
use std::{fmt::Debug, hash::Hash};

use crate::cards::{cardset::CardSet, iterators::IsomorphicDealIterator, Deck};

const GAMMA: f64 = 1.7;
const DEFAULT_CHUNK_SIZE: usize = 1_000_000;

/// Perfect hasher
pub struct PerfectIndexer<T> {
    phf: Mphf<T>,
}

impl<T: Send + Hash + Debug> PerfectIndexer<T> {
    pub fn new<I>(iterator: I) -> Self
    where
        I: Iterator<Item = T>,
    {
        let phf = Mphf::new(GAMMA, &iterator.collect_vec());
        Self { phf }
    }

    pub fn index(&self, item: &T) -> usize {
        self.phf.hash(item) as usize
    }
}

impl PerfectIndexer<[CardSet; 5]> {
    /// Creates a perfect indexer for the pocker cards in texas holdem
    pub fn pocket() -> Self {
        let deals = IsomorphicDealIterator::new(&[2], crate::cards::iterators::DeckType::Standard);
        PerfectIndexer::new(deals)
    }
}

#[cfg(test)]
mod tests {

    use std::collections::HashSet;

    use super::*;

    #[test]
    fn test_pocket_phf() {
        let indexer = PerfectIndexer::pocket();
        let deals = IsomorphicDealIterator::new(&[2], crate::cards::iterators::DeckType::Standard);

        let mut indexes = HashSet::new();
        let count = deals.clone().count();
        for deal in deals {
            indexes.insert(indexer.index(&deal));
        }

        assert_eq!(indexes.len(), count);
        let mut indexes = indexes.into_iter().collect_vec();
        indexes.sort();
        assert_eq!(indexes, (0..count).collect_vec());
    }
}
