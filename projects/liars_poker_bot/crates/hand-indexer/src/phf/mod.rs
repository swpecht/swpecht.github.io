use boomphf::Mphf;
use itertools::Itertools;
use std::{fmt::Debug, hash::Hash, iter};

use crate::cards::{cardset::CardSet, iterators::IsomorphicDealIterator, Deck};

const GAMMA: f64 = 1.7;
const DEFAULT_CHUNK_SIZE: usize = 1_000_000;

/// Perfect hasher
pub struct RoundIndexer<T> {
    phf: Mphf<T>,
    size: usize,
}

impl<T: Send + Hash + Debug> RoundIndexer<T> {
    pub fn new<I>(iterator: I) -> Self
    where
        I: Iterator<Item = T>,
    {
        let items = iterator.collect_vec();
        let size = items.len();
        let phf = Mphf::new(GAMMA, &items);
        Self { phf, size }
    }

    /// Returns the round index for a given item. This may return None if the item was not in the set
    /// used to generate the phf, but this is not guaranteed.
    ///
    /// This method does not do any normalizing of the input.
    pub fn index(&self, item: &T) -> Option<usize> {
        self.phf.try_hash(item).map(|x| x as usize)
    }
}

impl<T> RoundIndexer<T> {
    pub fn size(&self) -> usize {
        self.size
    }
}

impl RoundIndexer<[CardSet; 5]> {
    /// Creates a perfect indexer for the pocker cards in texas holdem
    pub fn _pocket() -> Self {
        let deals = IsomorphicDealIterator::new(&[2], crate::cards::iterators::DeckType::Standard);
        RoundIndexer::new(deals)
    }
}

#[cfg(test)]
mod tests {

    use std::collections::HashSet;

    use crate::cards::iterators::DeckType;

    use super::*;

    #[test]
    fn test_pocket_phf() {
        let indexer = RoundIndexer::_pocket();
        assert_eq!(indexer.size, 169);
        let deals = IsomorphicDealIterator::new(&[2], DeckType::Standard);

        let mut indexes = HashSet::new();
        let count = deals.clone().count();
        for deal in deals {
            indexes.insert(indexer.index(&deal).unwrap());
        }

        assert_eq!(indexes.len(), count);
        let mut indexes = indexes.into_iter().collect_vec();
        indexes.sort();
        assert_eq!(indexes, (0..count).collect_vec());
    }
}
