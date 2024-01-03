use super::{MAX_GROUP_INDEX, RANKS, SUITS};

pub(super) struct IndexerCache {
    pub nth_unset: Vec<[i32; RANKS]>,
    pub equal: [[bool; SUITS]; 1 << (SUITS - 1)],
    pub ncr_ranks: [[usize; RANKS + 1]; RANKS + 1],
    pub rank_set_to_index: Vec<usize>,
    pub index_to_rank_set: [[i32; 1 << RANKS]; RANKS + 1],
    pub suit_permutations: Vec<[i32; SUITS]>,
    pub ncr_groups: Vec<[u128; SUITS + 1]>,
}

impl Default for IndexerCache {
    fn default() -> Self {
        let mut equal: [[bool; SUITS]; 1 << (SUITS - 1)] = Default::default();
        for i in 0..1 << (SUITS - 1) {
            for j in 1..SUITS {
                equal[i][j] = (i & 1 << (j - 1)) != 0;
            }
        }

        let mut nth_unset = vec![[0i32; RANKS]; 1 << RANKS];
        for i in 0..1 << RANKS {
            // todo: precedence may be wrong
            let mut set: i32 = !i & ((1 << RANKS) - 1);
            for j in 0..RANKS {
                nth_unset[i as usize][j] = if set == 0 {
                    0xff
                } else {
                    set.trailing_zeros() as i32
                };
                set &= set - 1;
            }
        }

        let mut ncr_ranks: [[usize; RANKS + 1]; RANKS + 1] = Default::default();
        ncr_ranks[0][0] = 1;
        for i in 1..RANKS + 1 {
            ncr_ranks[i][0] = 1;
            ncr_ranks[i][i] = 1;
            for j in 1..i {
                ncr_ranks[i][j] = ncr_ranks[i - 1][j - 1] + ncr_ranks[i - 1][j];
            }
        }

        let mut ncr_groups = vec![[0; SUITS + 1]; MAX_GROUP_INDEX];
        ncr_groups[0][0] = 1;
        for i in 1..MAX_GROUP_INDEX {
            ncr_groups[i][0] = 1;
            if i < SUITS + 1 {
                ncr_groups[i][i] = 1;
            }
            let max_j = if i < SUITS + 1 { i } else { SUITS + 1 };
            for j in 1..max_j {
                ncr_groups[i][j] = ncr_groups[i - 1][j - 1] + ncr_groups[i - 1][j];
            }
        }

        let mut rank_set_to_index = vec![0; 1 << RANKS];
        let mut index_to_rank_set = [[0; 1 << RANKS]; RANKS + 1];
        for i in 0..rank_set_to_index.len() {
            let mut set = i;
            let mut j: usize = 0;
            while set != 0 {
                rank_set_to_index[i] += ncr_ranks[set.trailing_zeros() as usize][j];
                set &= set - 1;
            }
            index_to_rank_set[i.count_ones() as usize][rank_set_to_index[i]] = i as i32;
        }

        let mut num_permutations = 1;
        for i in 2..SUITS + 1 {
            num_permutations *= i;
        }
        let mut suit_permutations = vec![[0; SUITS]; num_permutations];
        for i in 0..suit_permutations.len() {
            let mut index = i;
            let mut used = 0;
            for j in 0..SUITS {
                let suit = index % (SUITS - j);
                index /= SUITS - j;
                let shifted_suit = nth_unset[used][suit];
                suit_permutations[i][j] = shifted_suit;
                used |= 1 << shifted_suit;
            }
        }

        Self {
            nth_unset,
            equal,
            ncr_ranks,
            rank_set_to_index,
            index_to_rank_set,
            suit_permutations,
            ncr_groups,
        }
    }
}
