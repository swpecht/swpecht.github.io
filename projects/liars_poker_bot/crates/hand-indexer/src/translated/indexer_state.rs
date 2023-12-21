use lazy_static::lazy_static;

use super::{indexer_cache::IndexerCache, SUITS};

lazy_static! {
    static ref CACHE: IndexerCache = IndexerCache::default();
}

pub(super) struct HandIndexerState {
    pub(super) suit_index: [u32; SUITS],
    pub(super) suit_multiplier: [u32; SUITS],
    pub(super) round: usize,
    pub(super) permutation_index: usize,
    pub(super) permutation_multipluer: usize,
    pub(super) used_ranks: [u32; SUITS],
}

impl Default for HandIndexerState {
    fn default() -> Self {
        Self {
            suit_index: Default::default(),
            suit_multiplier: [1; SUITS],
            round: Default::default(),
            permutation_index: Default::default(),
            permutation_multipluer: 1,
            used_ranks: Default::default(),
        }
    }
}
