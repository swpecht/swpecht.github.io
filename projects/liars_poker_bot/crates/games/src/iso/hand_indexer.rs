//! Rust port of Kevin Waugh's hand isomorphism algorithm.
//!
//! Source: <https://www.cs.cmu.edu/~waugh/publications/isomorphism13.pdf>
//! Reference C implementation: <https://github.com/kdub0/hand-isomorphism>
//!
//! ## Algorithm overview
//!
//! Cards are dealt across one or more *rounds*. Within a round, the
//! order of cards does not matter (a hand of {Ah, Kh} is the same as
//! {Kh, Ah}), but between rounds the order is preserved (preflop cards
//! aren't interchangeable with flop cards).
//!
//! Two card sequences are *isomorphic* if one can be transformed into
//! the other by a permutation of suits that preserves the per-round
//! membership. The algorithm enumerates one canonical representative
//! per isomorphism class and assigns it a dense integer index.
//!
//! The two key primitives this module exposes:
//!
//! * [`HandIndexer::size`] — the total number of iso classes through
//!   round `r`. This is the cardinality the eventual perfect hash
//!   function will be built over.
//! * [`HandIndexer::unindex`] — given a dense index, recover the
//!   canonical card sequence. This is the "stream into the PHF" hook
//!   that lets us produce each iso class exactly once with no
//!   HashSet.
//!
//! Plus the forward direction:
//!
//! * [`HandIndexer::index_all`] — given a raw card sequence, compute
//!   the canonical indices for each round (used for the round-trip
//!   correctness check `index(unindex(idx)) == idx`).
//!
//! ## Configuration vs. permutation
//!
//! Each iso class is determined by a *configuration* — for every suit,
//! the count of cards contributed by every round. Two suits with
//! identical configurations are interchangeable. Within a
//! configuration, each suit's specific cards are encoded by a
//! **colex** rank-set index (Bollobas), and the per-suit indices are
//! combined via a multiset combination of the interchangeable suits.
//!
//! ## Departures from the C reference
//!
//! * `MAX_ROUNDS = 16` (up from 8) to cover Oh Hell games up to ~7
//!   tricks per player. Per-suit per-round counts are still 4 bits
//!   each (max 15 cards per round), packed into a `u64` so up to 16
//!   rounds fit per suit.
//! * `nCr_groups` is computed lazily via the closed-form
//!   `n_choose_k_group()` function instead of a 40 MB static table —
//!   the reference's `MAX_GROUP_INDEX = 2^20` cap isn't large enough
//!   for OH play-phase configurations anyway (one suit can hold up to
//!   ~170M raw cards × rounds combinations).
//! * Intermediate combinatorics use `u128` to avoid overflow; the
//!   final per-round size must still fit in `u64`.
//! * Configurations are collected during enumeration and sorted once
//!   afterwards (avoids the C reference's per-insertion linear-time
//!   insertion sort).
//!
//! ## Status
//!
//! Phase 1 of the OH disk-backed indexer work. Test coverage:
//!
//! * static-table sanity: nCr triangle, colex round-trip, suit perms
//!   are a bijection.
//! * known holdem numbers reproduced: 169 preflop iso classes, 1,286
//!   colex-indexed flop combinations, etc.
//! * OH config counts: `size(round=1)` for 2p × 3-trick rounds
//!   `[3, 1]` matches the hand-rolled enumerator's 63,193 (canonical
//!   hand × face-up cardinality).
//! * `unindex` round-trips: for small configurations, every index
//!   `i ∈ 0..size` round-trips through `unindex → index → i`.

use std::sync::LazyLock;

/// 4 suits — the C reference's `SUITS` constant.
pub const SUITS: usize = 4;
/// 13 ranks per suit — the C reference's `RANKS`.
pub const RANKS: usize = 13;
/// 52-card deck.
pub const CARDS: usize = SUITS * RANKS;

/// Hard cap on round count. The reference uses 8; we use 16 so OH
/// games up to 7 tricks per player fit (rounds = 2 + np × n_tricks).
pub const MAX_ROUNDS: usize = 16;
/// Hard cap on cards per round (room for 4-bit per-round count).
pub const MAX_CARDS_PER_ROUND: usize = 15;

/// Bits per round in the packed per-suit configuration.
const ROUND_SHIFT: u32 = 4;
const ROUND_MASK: u64 = 0xf;

/// Cards are packed as `(rank << 2) | suit`, matching the C reference.
pub type Card = u8;
pub type HandIndex = u64;

#[inline]
pub fn card_make(suit: u8, rank: u8) -> Card {
    debug_assert!((suit as usize) < SUITS);
    debug_assert!((rank as usize) < RANKS);
    (rank << 2) | suit
}

#[inline]
pub fn card_suit(card: Card) -> u8 {
    card & 3
}

#[inline]
pub fn card_rank(card: Card) -> u8 {
    card >> 2
}

// =====================================================================
// Static lazy tables
// =====================================================================

/// `NCR_RANKS[i][j] = C(i, j)` for `0 ≤ i, j ≤ RANKS`. Stored as `u64`
/// (matches the reference's `uint_fast32_t` upgraded to `hand_index_t`
/// in some sites; values here fit comfortably in 32 bits).
pub static NCR_RANKS: LazyLock<[[u64; RANKS + 1]; RANKS + 1]> = LazyLock::new(|| {
    let mut table = [[0u64; RANKS + 1]; RANKS + 1];
    table[0][0] = 1;
    for i in 1..=RANKS {
        table[i][0] = 1;
        table[i][i] = 1;
        for j in 1..i {
            table[i][j] = table[i - 1][j - 1] + table[i - 1][j];
        }
    }
    table
});

/// `EQUAL[mask][j] = (mask >> (j-1)) & 1` for `j ∈ 1..SUITS`, used to
/// detect when neighbouring suits in canonical order are still equal
/// (their configurations match through the current round).
pub static EQUAL: LazyLock<[[bool; SUITS]; 1 << (SUITS - 1)]> = LazyLock::new(|| {
    let mut table = [[false; SUITS]; 1 << (SUITS - 1)];
    for i in 0..(1 << (SUITS - 1)) {
        for j in 1..SUITS {
            table[i][j] = i & (1 << (j - 1)) != 0;
        }
    }
    table
});

/// `NTH_UNSET[mask][n]` = position of the n-th unset bit in `mask`
/// (0xff if there isn't one). Used to convert between rank positions
/// in a shrunken space (after some ranks are already used) and full
/// 0..RANKS space.
pub static NTH_UNSET: LazyLock<Vec<[u8; RANKS]>> = LazyLock::new(|| {
    let size = 1usize << RANKS;
    let mut table = vec![[0u8; RANKS]; size];
    for i in 0..size {
        let mut set = !i & ((1 << RANKS) - 1);
        for j in 0..RANKS {
            if set == 0 {
                table[i][j] = 0xff;
            } else {
                table[i][j] = set.trailing_zeros() as u8;
                set &= set - 1;
            }
        }
    }
    table
});

/// `(RANK_SET_TO_INDEX, INDEX_TO_RANK_SET)`.
///
/// `RANK_SET_TO_INDEX[mask]` = colex index of the rank set (Bollobas
/// colex ranking). Closed form:
///     sum_{i: bit i set in mask, k = position-from-LSB starting at 1}
///         C(i, k)
///
/// `INDEX_TO_RANK_SET[popcount][colex_idx]` = the unique 13-bit rank
/// set with that popcount and colex index. Stored as `u16`.
pub static COLEX_TABLES: LazyLock<(Vec<u64>, Vec<Vec<u16>>)> = LazyLock::new(|| {
    let size = 1usize << RANKS;
    let mut rank_set_to_index = vec![0u64; size];
    let mut index_to_rank_set: Vec<Vec<u16>> = (0..=RANKS)
        .map(|k| vec![0u16; NCR_RANKS[RANKS][k] as usize])
        .collect();
    for i in 0..size {
        let mut set = i as u64;
        let mut j: u64 = 1;
        let mut idx: u64 = 0;
        while set != 0 {
            let pos = set.trailing_zeros() as usize;
            idx += NCR_RANKS[pos][j as usize];
            j += 1;
            set &= set - 1;
        }
        rank_set_to_index[i] = idx;
        let popcount = (i as u32).count_ones() as usize;
        index_to_rank_set[popcount][idx as usize] = i as u16;
    }
    (rank_set_to_index, index_to_rank_set)
});

/// All `SUITS!` permutations of suit indices `[0, 1, …, SUITS-1]`,
/// enumerated in the same order as the C reference (factorial number
/// system).
///
/// `SUIT_PERMUTATIONS[i]` is a permutation array `pi` where `pi[j]`
/// is the suit chosen at slot `j` for permutation `i`.
pub static SUIT_PERMUTATIONS: LazyLock<Vec<[u8; SUITS]>> = LazyLock::new(|| {
    let mut num_permutations: usize = 1;
    for i in 2..=SUITS {
        num_permutations *= i;
    }
    let mut perms = vec![[0u8; SUITS]; num_permutations];
    let nth_unset = &*NTH_UNSET;
    for i in 0..num_permutations {
        let mut index = i;
        let mut used: u32 = 0;
        for j in 0..SUITS {
            let suit_choice = index % (SUITS - j);
            index /= SUITS - j;
            let shifted_suit = nth_unset[used as usize][suit_choice];
            perms[i][j] = shifted_suit;
            used |= 1 << shifted_suit;
        }
    }
    perms
});

// =====================================================================
// Combinatorial helper for "multiset of group_size identical-suit
// indices, drawn from suit_size options" — `C(suit_size + g - 1, g)`.
// Replaces the C reference's `nCr_groups` static table.
// =====================================================================

/// `n_choose_k_group(n, k)` = `C(n, k)`.
///
/// Returns 0 if `n < k`. Computed iteratively in `u128` so very
/// large `n` and small `k` (up to SUITS = 4) don't overflow. Panics
/// if the final value doesn't fit in `u64`.
#[inline]
pub fn n_choose_k_group(n: u64, k: usize) -> u64 {
    if k == 0 {
        return 1;
    }
    if (n as usize) < k {
        return 0;
    }
    let mut result: u128 = 1;
    for i in 0..k {
        result = result * (n as u128 - i as u128) / (i as u128 + 1);
    }
    u64::try_from(result).expect("n_choose_k_group overflowed u64")
}

// =====================================================================
// Configuration & permutation enumeration
// =====================================================================

/// One *configuration* through round R: for every suit, the per-round
/// card count, packed into a `u64`. Suits are kept in canonical
/// (descending lex) order so isomorphic raw configurations collapse
/// to a single representative.
///
/// Encoding: for suit s, configuration[s] = sum_{r=0..=R} (count_in_r
/// << ROUND_SHIFT * (rounds - r - 1)). The high bits hold round 0;
/// the low bits hold round R. (Identical to the C reference.)
pub type Configuration = [u64; SUITS];

/// Enumerate every configuration through every round, calling
/// `observe(round, config)` once per (round, distinct canonical
/// configuration). Configurations are emitted in the order the
/// recursive walker visits them — not lex sorted; callers that
/// require sorted order must sort themselves.
pub fn enumerate_configurations(
    rounds: usize,
    cards_per_round: &[u8],
    mut observe: impl FnMut(usize, &Configuration),
) {
    let mut used = [0u32; SUITS];
    let mut configuration = [0u64; SUITS];
    enumerate_configurations_r(
        rounds,
        cards_per_round,
        0,
        cards_per_round[0] as u32,
        0,
        (1u32 << SUITS) - 2,
        &mut used,
        &mut configuration,
        &mut observe,
    );
}

#[allow(clippy::too_many_arguments)]
fn enumerate_configurations_r(
    rounds: usize,
    cards_per_round: &[u8],
    round: usize,
    remaining: u32,
    suit: usize,
    equal: u32,
    used: &mut [u32; SUITS],
    configuration: &mut [u64; SUITS],
    observe: &mut impl FnMut(usize, &Configuration),
) {
    if suit == SUITS {
        observe(round, configuration);
        if round + 1 < rounds {
            enumerate_configurations_r(
                rounds,
                cards_per_round,
                round + 1,
                cards_per_round[round + 1] as u32,
                0,
                equal,
                used,
                configuration,
                observe,
            );
        }
        return;
    }

    // Last suit gets all remaining cards.
    let min = if suit == SUITS - 1 { remaining } else { 0 };
    let mut max = (RANKS as u32) - used[suit];
    if remaining < max {
        max = remaining;
    }

    // If this suit's predecessor was equal in *all* previous rounds,
    // we'd over-count by emitting non-canonical orderings — cap our
    // round-`round` choice at the predecessor's round-`round` count.
    let mut previous: u32 = (RANKS as u32) + 1;
    let was_equal = equal & (1u32 << suit) != 0;
    if was_equal {
        // configuration[suit-1] holds the predecessor's packed counts;
        // extract round `round`'s value (lives in bits
        // ROUND_SHIFT*(rounds-round-1)..+ROUND_SHIFT).
        previous = ((configuration[suit - 1] >> (ROUND_SHIFT * (rounds as u32 - round as u32 - 1)))
            & ROUND_MASK) as u32;
        if previous < max {
            max = previous;
        }
    }

    let old_configuration = configuration[suit];
    let old_used = used[suit];
    for i in min..=max {
        let new_configuration =
            old_configuration | ((i as u64) << (ROUND_SHIFT * (rounds as u32 - round as u32 - 1)));
        let new_equal =
            (equal & !(1u32 << suit)) | ((if was_equal && i == previous { 1 } else { 0 }) << suit);
        used[suit] = old_used + i;
        configuration[suit] = new_configuration;
        enumerate_configurations_r(
            rounds,
            cards_per_round,
            round,
            remaining - i,
            suit + 1,
            new_equal,
            used,
            configuration,
            observe,
        );
        configuration[suit] = old_configuration;
        used[suit] = old_used;
    }
}

/// Enumerate every per-suit count vector (the raw "permutation"
/// before any suit reordering — what `hand_index_next_round` sees).
pub fn enumerate_permutations(
    rounds: usize,
    cards_per_round: &[u8],
    mut observe: impl FnMut(usize, &[u64; SUITS]),
) {
    let mut used = [0u32; SUITS];
    let mut count = [0u64; SUITS];
    enumerate_permutations_r(
        rounds,
        cards_per_round,
        0,
        cards_per_round[0] as u32,
        0,
        &mut used,
        &mut count,
        &mut observe,
    );
}

#[allow(clippy::too_many_arguments)]
fn enumerate_permutations_r(
    rounds: usize,
    cards_per_round: &[u8],
    round: usize,
    remaining: u32,
    suit: usize,
    used: &mut [u32; SUITS],
    count: &mut [u64; SUITS],
    observe: &mut impl FnMut(usize, &[u64; SUITS]),
) {
    if suit == SUITS {
        observe(round, count);
        if round + 1 < rounds {
            enumerate_permutations_r(
                rounds,
                cards_per_round,
                round + 1,
                cards_per_round[round + 1] as u32,
                0,
                used,
                count,
                observe,
            );
        }
        return;
    }
    let min = if suit == SUITS - 1 { remaining } else { 0 };
    let mut max = (RANKS as u32) - used[suit];
    if remaining < max {
        max = remaining;
    }
    let old_count = count[suit];
    let old_used = used[suit];
    for i in min..=max {
        let new_count =
            old_count | ((i as u64) << (ROUND_SHIFT * (rounds as u32 - round as u32 - 1)));
        used[suit] = old_used + i;
        count[suit] = new_count;
        enumerate_permutations_r(
            rounds,
            cards_per_round,
            round,
            remaining - i,
            suit + 1,
            used,
            count,
            observe,
        );
        count[suit] = old_count;
        used[suit] = old_used;
    }
}

// =====================================================================
// HandIndexer
// =====================================================================

/// Per-round per-configuration data, after init. Mirrors the C
/// reference's `hand_indexer_t` struct but with `Vec`-backed dynamic
/// arrays instead of malloc'd pointers.
#[derive(Debug, Clone)]
pub struct HandIndexer {
    pub rounds: usize,
    pub cards_per_round: Vec<u8>,
    pub round_start: Vec<u8>,

    /// `round_size[r]` = `hand_indexer_size(r)`.
    pub round_size: Vec<u64>,

    /// For each round: list of canonical configurations (sorted
    /// descending by per-suit lex).
    configurations: Vec<Vec<Configuration>>,
    /// For each round, for each config: cumulative offset in the
    /// global index space.
    configuration_to_offset: Vec<Vec<u64>>,
    /// For each round, for each config: bitmask of "this suit is
    /// equal to the predecessor" (bits 1..SUITS).
    configuration_to_equal: Vec<Vec<u32>>,
    /// For each round, for each config: per-suit colex size (number
    /// of distinct rank-set combinations across all rounds for that
    /// suit, given the per-round count).
    configuration_to_suit_size: Vec<Vec<[u64; SUITS]>>,

    /// Per-round mapping from "raw permutation index" (count tuple
    /// indexed by `(remaining+1)` mixed radix) to the canonical
    /// configuration index in `configurations[round]`.
    permutation_to_configuration: Vec<Vec<u32>>,
    /// Per-round mapping from "raw permutation index" to the
    /// `SUIT_PERMUTATIONS` index that canonicalises the raw count
    /// tuple.
    permutation_to_pi: Vec<Vec<u32>>,
}

impl HandIndexer {
    /// Build a `HandIndexer` for the given per-round card counts.
    /// Returns `None` if the configuration is invalid (no rounds,
    /// too many rounds, or total cards exceed `CARDS`).
    pub fn init(cards_per_round: &[u8]) -> Option<Self> {
        let rounds = cards_per_round.len();
        if rounds == 0 || rounds > MAX_ROUNDS {
            return None;
        }
        let mut total: usize = 0;
        for &n in cards_per_round {
            if (n as usize) > MAX_CARDS_PER_ROUND {
                return None;
            }
            total += n as usize;
            if total > CARDS {
                return None;
            }
        }

        let mut indexer = Self {
            rounds,
            cards_per_round: cards_per_round.to_vec(),
            round_start: {
                let mut v = Vec::with_capacity(rounds);
                let mut j = 0u8;
                for &n in cards_per_round {
                    v.push(j);
                    j += n;
                }
                v
            },
            round_size: vec![0u64; rounds],
            configurations: vec![Vec::new(); rounds],
            configuration_to_offset: vec![Vec::new(); rounds],
            configuration_to_equal: vec![Vec::new(); rounds],
            configuration_to_suit_size: vec![Vec::new(); rounds],
            permutation_to_configuration: vec![Vec::new(); rounds],
            permutation_to_pi: vec![Vec::new(); rounds],
        };

        // Step 1: collect every (round, configuration) into the
        // per-round configurations list.
        let mut raw_configs: Vec<Vec<Configuration>> = vec![Vec::new(); rounds];
        enumerate_configurations(rounds, cards_per_round, |round, config| {
            raw_configs[round].push(*config);
        });

        // Step 2: sort + tabulate per round.
        for r in 0..rounds {
            // Sort ASCENDING by per-suit lex. The C reference's
            // insertion-sort in `tabulate_configurations` maintains
            // ascending order (it inserts a new config *after* any
            // prev where new > prev), and the binary searches in
            // both `tabulate_permutations` and `unindex` depend on
            // this ordering.
            raw_configs[r].sort();
            let count = raw_configs[r].len();
            indexer.configurations[r] = raw_configs[r].clone();
            indexer.configuration_to_offset[r] = vec![0u64; count];
            indexer.configuration_to_equal[r] = vec![0u32; count];
            indexer.configuration_to_suit_size[r] = vec![[0u64; SUITS]; count];

            for (id, configuration) in raw_configs[r].iter().enumerate() {
                // Compute the per-suit colex size = product over
                // rounds of C(remaining, count_in_round).
                let mut equal_mask: u32 = 0;
                let mut size_for_config: u128 = 1;
                let mut i = 0;
                while i < SUITS {
                    let mut size: u128 = 1;
                    let mut remaining: u32 = RANKS as u32;
                    for j in 0..=r {
                        let ranks = ((configuration[i]
                            >> (ROUND_SHIFT * (rounds as u32 - j as u32 - 1)))
                            & ROUND_MASK) as usize;
                        size *= NCR_RANKS[remaining as usize][ranks] as u128;
                        remaining -= ranks as u32;
                    }
                    // Find run of identical configs at this slot.
                    let mut j = i + 1;
                    while j < SUITS && configuration[j] == configuration[i] {
                        j += 1;
                    }
                    let group_size = j - i;
                    for k in i..j {
                        indexer.configuration_to_suit_size[r][id][k] = size as u64;
                    }
                    // Multiset combination of `group_size` suits each
                    // chosen from `size` options.
                    let multiset_size =
                        n_choose_k_group((size as u64).saturating_add(group_size as u64 - 1), group_size);
                    size_for_config = size_for_config
                        .checked_mul(multiset_size as u128)
                        .expect("config offset overflowed u128");
                    for k in (i + 1)..j {
                        equal_mask |= 1u32 << k;
                    }
                    i = j;
                }

                indexer.configuration_to_offset[r][id] = size_for_config as u64;
                indexer.configuration_to_equal[r][id] = equal_mask >> 1;
            }

            // Prefix-sum into actual offsets.
            let mut accum: u128 = 0;
            for id in 0..count {
                let next = accum + indexer.configuration_to_offset[r][id] as u128;
                indexer.configuration_to_offset[r][id] = u64::try_from(accum)
                    .expect("configuration_to_offset overflowed u64");
                accum = next;
            }
            indexer.round_size[r] =
                u64::try_from(accum).expect("round_size overflowed u64");
        }

        // Step 3: build permutation_to_configuration + _to_pi tables.
        let mut max_perm_indices = vec![0u32; rounds];
        enumerate_permutations(rounds, cards_per_round, |round, count| {
            let idx = perm_index(round, count, cards_per_round, rounds);
            if max_perm_indices[round] < idx + 1 {
                max_perm_indices[round] = idx + 1;
            }
        });
        for r in 0..rounds {
            indexer.permutation_to_configuration[r] = vec![0u32; max_perm_indices[r] as usize];
            indexer.permutation_to_pi[r] = vec![0u32; max_perm_indices[r] as usize];
        }
        enumerate_permutations(rounds, cards_per_round, |round, count| {
            let idx = perm_index(round, count, cards_per_round, rounds);
            indexer.tabulate_permutation(round, count, idx);
        });

        Some(indexer)
    }

    /// Number of canonical iso classes for card sequences of length
    /// `cards_per_round[0..=round]`.
    pub fn size(&self, round: usize) -> u64 {
        assert!(round < self.rounds);
        self.round_size[round]
    }

    /// Total cards covered through round `r`.
    pub fn cards_through(&self, round: usize) -> usize {
        self.cards_per_round[..=round]
            .iter()
            .map(|&n| n as usize)
            .sum()
    }

    fn tabulate_permutation(&mut self, round: usize, count: &[u64; SUITS], idx: u32) {
        // Build pi: a permutation of {0..SUITS} that sorts `count`
        // descending (count[pi[0]] >= count[pi[1]] >= …).
        let mut pi = [0u8; SUITS];
        for (i, slot) in pi.iter_mut().enumerate() {
            *slot = i as u8;
        }
        // Insertion sort descending on count.
        for i in 1..SUITS {
            let pi_i = pi[i];
            let mut j = i;
            while j > 0 {
                if count[pi_i as usize] > count[pi[j - 1] as usize] {
                    pi[j] = pi[j - 1];
                    j -= 1;
                } else {
                    break;
                }
            }
            pi[j] = pi_i;
        }

        // pi_idx = factorial-base index of `pi` in SUIT_PERMUTATIONS.
        let mut pi_idx: u32 = 0;
        let mut pi_mult: u32 = 1;
        let mut pi_used: u32 = 0;
        for i in 0..SUITS {
            let this_bit = 1u32 << pi[i];
            let smaller = ((this_bit - 1) & pi_used).count_ones();
            pi_idx += (pi[i] as u32 - smaller) * pi_mult;
            pi_mult *= (SUITS - i) as u32;
            pi_used |= this_bit;
        }
        self.permutation_to_pi[round][idx as usize] = pi_idx;

        // Find configuration matching the sorted count via binary search.
        let configs = &self.configurations[round];
        let mut low = 0usize;
        let mut high = configs.len();
        while low < high {
            let mid = (low + high) / 2;
            let mut compare = 0i32;
            for i in 0..SUITS {
                let this_val = count[pi[i] as usize];
                let other_val = configs[mid][i];
                if other_val > this_val {
                    compare = -1;
                    break;
                } else if other_val < this_val {
                    compare = 1;
                    break;
                }
            }
            match compare.cmp(&0) {
                std::cmp::Ordering::Less => high = mid,
                std::cmp::Ordering::Equal => {
                    low = mid;
                    high = mid;
                }
                std::cmp::Ordering::Greater => low = mid + 1,
            }
        }
        self.permutation_to_configuration[round][idx as usize] = low as u32;
    }
}

/// Convert a per-round per-suit count tuple into a mixed-radix index
/// `permutation_index` used to address `permutation_to_configuration`
/// and `permutation_to_pi`. Mirrors the formula in
/// `count_permutations`.
fn perm_index(round: usize, count: &[u64; SUITS], cards_per_round: &[u8], rounds: usize) -> u32 {
    let mut idx: u32 = 0;
    let mut mult: u32 = 1;
    for i in 0..=round {
        let mut remaining = cards_per_round[i] as u32;
        for j in 0..(SUITS - 1) {
            let size =
                ((count[j] >> (ROUND_SHIFT * (rounds as u32 - i as u32 - 1))) & ROUND_MASK) as u32;
            idx += mult * size;
            mult *= remaining + 1;
            remaining -= size;
        }
    }
    idx
}

// =====================================================================
// Incremental forward indexing (hand_index_next_round)
// =====================================================================

/// Mutable state carried between rounds during forward indexing.
#[derive(Debug, Clone, Default)]
pub struct IndexerState {
    pub suit_index: [u64; SUITS],
    pub suit_multiplier: [u64; SUITS],
    pub round: usize,
    pub permutation_index: u32,
    pub permutation_multiplier: u32,
    pub used_ranks: [u32; SUITS],
}

impl IndexerState {
    pub fn new() -> Self {
        Self {
            suit_index: [0; SUITS],
            suit_multiplier: [1; SUITS],
            round: 0,
            permutation_index: 0,
            permutation_multiplier: 1,
            used_ranks: [0; SUITS],
        }
    }
}

impl HandIndexer {
    /// Index a single round's worth of cards on top of `state`.
    /// Returns the cumulative index through this round. Mirrors
    /// `hand_index_next_round` in the C reference.
    pub fn next_round(&self, cards: &[Card], state: &mut IndexerState) -> u64 {
        let round = state.round;
        state.round += 1;
        assert!(round < self.rounds);
        let n_cards = self.cards_per_round[round] as usize;
        assert_eq!(cards.len(), n_cards);

        let (rank_set_to_index, _) = &*COLEX_TABLES;

        let mut ranks = [0u32; SUITS];
        let mut shifted_ranks = [0u32; SUITS];
        for &card in cards.iter() {
            assert!((card as usize) < CARDS, "invalid card {}", card);
            let rank = card_rank(card) as u32;
            let suit = card_suit(card) as usize;
            let rank_bit = 1u32 << rank;
            assert!(ranks[suit] & rank_bit == 0, "duplicate card in round");
            ranks[suit] |= rank_bit;
            let used_before = ((rank_bit - 1) & state.used_ranks[suit]).count_ones();
            shifted_ranks[suit] |= rank_bit >> used_before;
        }

        for i in 0..SUITS {
            assert!(state.used_ranks[i] & ranks[i] == 0, "duplicate card across rounds");
            let used_size = state.used_ranks[i].count_ones() as usize;
            let this_size = ranks[i].count_ones() as usize;
            state.suit_index[i] +=
                state.suit_multiplier[i] * rank_set_to_index[shifted_ranks[i] as usize];
            state.suit_multiplier[i] *= NCR_RANKS[RANKS - used_size][this_size];
            state.used_ranks[i] |= ranks[i];
        }

        // Update permutation index with this round's count tuple.
        let mut remaining = n_cards as u32;
        for i in 0..(SUITS - 1) {
            let this_size = ranks[i].count_ones();
            state.permutation_index += state.permutation_multiplier * this_size;
            state.permutation_multiplier *= remaining + 1;
            remaining -= this_size;
        }

        let configuration_idx =
            self.permutation_to_configuration[round][state.permutation_index as usize] as usize;
        let pi_idx = self.permutation_to_pi[round][state.permutation_index as usize] as usize;
        let equal_index = self.configuration_to_equal[round][configuration_idx] as usize;
        let offset = self.configuration_to_offset[round][configuration_idx];
        let pi = &SUIT_PERMUTATIONS[pi_idx];

        // Apply pi permutation to suit_index and suit_multiplier.
        let mut suit_index: [u64; SUITS] = [0; SUITS];
        let mut suit_multiplier: [u64; SUITS] = [0; SUITS];
        for i in 0..SUITS {
            suit_index[i] = state.suit_index[pi[i] as usize];
            suit_multiplier[i] = state.suit_multiplier[pi[i] as usize];
        }

        let mut index: u128 = offset as u128;
        let mut multiplier: u128 = 1;
        let mut i = 0;
        while i < SUITS {
            let (part, size, next_i) = if i + 1 < SUITS && EQUAL[equal_index][i + 1] {
                if i + 2 < SUITS && EQUAL[equal_index][i + 2] {
                    if i + 3 < SUITS && EQUAL[equal_index][i + 3] {
                        // Four equal suits.
                        sort_swap(&mut suit_index, i, i + 1);
                        sort_swap(&mut suit_index, i + 2, i + 3);
                        sort_swap(&mut suit_index, i, i + 2);
                        sort_swap(&mut suit_index, i + 1, i + 3);
                        sort_swap(&mut suit_index, i + 1, i + 2);
                        let part = suit_index[i]
                            + n_choose_k_group(suit_index[i + 1] + 1, 2)
                            + n_choose_k_group(suit_index[i + 2] + 2, 3)
                            + n_choose_k_group(suit_index[i + 3] + 3, 4);
                        let size = n_choose_k_group(suit_multiplier[i] + 3, 4);
                        (part, size, i + 4)
                    } else {
                        // Three equal suits.
                        sort_swap(&mut suit_index, i, i + 1);
                        sort_swap(&mut suit_index, i, i + 2);
                        sort_swap(&mut suit_index, i + 1, i + 2);
                        let part = suit_index[i]
                            + n_choose_k_group(suit_index[i + 1] + 1, 2)
                            + n_choose_k_group(suit_index[i + 2] + 2, 3);
                        let size = n_choose_k_group(suit_multiplier[i] + 2, 3);
                        (part, size, i + 3)
                    }
                } else {
                    // Two equal suits.
                    sort_swap(&mut suit_index, i, i + 1);
                    let part = suit_index[i] + n_choose_k_group(suit_index[i + 1] + 1, 2);
                    let size = n_choose_k_group(suit_multiplier[i] + 1, 2);
                    (part, size, i + 2)
                }
            } else {
                // No equal suits at this position.
                (suit_index[i], suit_multiplier[i], i + 1)
            };

            index += multiplier * part as u128;
            multiplier *= size as u128;
            i = next_i;
        }
        u64::try_from(index).expect("hand index overflowed u64")
    }

    /// Forward-index the entire card sequence, filling in
    /// `indices[r]` for each round and returning the final round's
    /// index.
    pub fn index_all(&self, cards: &[Card], indices: &mut [u64]) -> u64 {
        assert_eq!(indices.len(), self.rounds);
        let mut state = IndexerState::new();
        let mut offset = 0usize;
        for r in 0..self.rounds {
            let n = self.cards_per_round[r] as usize;
            indices[r] = self.next_round(&cards[offset..offset + n], &mut state);
            offset += n;
        }
        indices[self.rounds - 1]
    }

    /// Convenience: forward-index and return only the last round's
    /// index.
    pub fn index_last(&self, cards: &[Card]) -> u64 {
        let mut indices = vec![0u64; self.rounds];
        self.index_all(cards, &mut indices)
    }
}

#[inline]
fn sort_swap(arr: &mut [u64; SUITS], a: usize, b: usize) {
    if arr[a] > arr[b] {
        arr.swap(a, b);
    }
}

// =====================================================================
// Inverse: hand_unindex
// =====================================================================

impl HandIndexer {
    /// Recover the canonical card sequence corresponding to `index`
    /// in round `round`. Returns `None` if `index` is out of range.
    pub fn unindex(&self, round: usize, index: u64) -> Option<Vec<Card>> {
        if round >= self.rounds || index >= self.round_size[round] {
            return None;
        }

        // Binary search to find configuration_idx.
        let mut low = 0usize;
        let mut high = self.configurations[round].len();
        let mut configuration_idx = 0usize;
        while low < high {
            let mid = (low + high) / 2;
            if self.configuration_to_offset[round][mid] <= index {
                configuration_idx = mid;
                low = mid + 1;
            } else {
                high = mid;
            }
        }
        let mut index = index - self.configuration_to_offset[round][configuration_idx];

        // Recover per-suit suit_index by decomposing each group.
        let mut suit_index = [0u64; SUITS];
        let mut i = 0usize;
        while i < SUITS {
            // Find run of equal configs starting at i.
            let mut j = i + 1;
            while j < SUITS
                && self.configurations[round][configuration_idx][j]
                    == self.configurations[round][configuration_idx][i]
            {
                j += 1;
            }
            let group_size = j - i;
            let suit_size = self.configuration_to_suit_size[round][configuration_idx][i];

            let group_total = n_choose_k_group(suit_size + group_size as u64 - 1, group_size);
            let mut group_index = index % group_total;
            index /= group_total;

            // Decompose group_index into a multiset of group_size
            // colex indices, each in [0, suit_size).
            for k in i..(j - 1) {
                let g = j - k;
                let mut lo = 0u64;
                let mut hi = suit_size;
                let mut chosen = 0u64;
                while lo < hi {
                    let mid = (lo + hi) / 2;
                    if n_choose_k_group(mid + g as u64 - 1, g) <= group_index {
                        chosen = mid;
                        lo = mid + 1;
                    } else {
                        hi = mid;
                    }
                }
                suit_index[k] = chosen;
                group_index -= n_choose_k_group(chosen + g as u64 - 1, g);
            }
            suit_index[j - 1] = group_index;
            i = j;
        }

        // Decompose each suit_index by round into rank sets, then
        // map back through used-ranks to actual card discriminants.
        let (_rank_set_to_index, index_to_rank_set) = &*COLEX_TABLES;
        let nth_unset = &*NTH_UNSET;
        let total_cards: usize = self.cards_per_round[..=round]
            .iter()
            .map(|&n| n as usize)
            .sum();
        let mut cards = vec![0u8; total_cards];
        let mut location: Vec<u8> = self.round_start.clone();
        for s in 0..SUITS {
            let mut used: u32 = 0;
            let mut consumed: u32 = 0;
            let mut idx = suit_index[s];
            for r in 0..=round {
                let n = ((self.configurations[round][configuration_idx][s]
                    >> (ROUND_SHIFT * (self.rounds as u32 - r as u32 - 1)))
                    & ROUND_MASK) as usize;
                let round_size = NCR_RANKS[RANKS - consumed as usize][n];
                let round_idx = (idx % round_size) as usize;
                idx /= round_size;
                consumed += n as u32;
                let shifted_cards = index_to_rank_set[n][round_idx];
                let mut shifted = shifted_cards as u32;
                let mut rank_set: u32 = 0;
                for _ in 0..n {
                    let shifted_card = shifted & shifted.wrapping_neg();
                    shifted ^= shifted_card;
                    let card_rank = nth_unset[used as usize][shifted_card.trailing_zeros() as usize];
                    rank_set |= 1 << card_rank;
                    let pos = location[r] as usize;
                    cards[pos] = card_make(s as u8, card_rank);
                    location[r] += 1;
                }
                used |= rank_set;
            }
        }

        Some(cards)
    }
}

// =====================================================================
// Tests
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    // ------- Static tables ----------------------------------------

    #[test]
    fn ncr_pascal_triangle() {
        let t = &*NCR_RANKS;
        // First row.
        assert_eq!(t[0][0], 1);
        // Diagonal.
        for i in 0..=RANKS {
            assert_eq!(t[i][i], 1, "C({}, {}) should be 1", i, i);
            assert_eq!(t[i][0], 1, "C({}, 0) should be 1", i);
        }
        // Known values.
        assert_eq!(t[13][2], 78);
        assert_eq!(t[13][3], 286);
        assert_eq!(t[13][5], 1287);
        assert_eq!(t[13][6], 1716);
        // Symmetry.
        for i in 0..=RANKS {
            for j in 0..=i {
                assert_eq!(t[i][j], t[i][i - j], "C({}, {}) != C({}, {})", i, j, i, i - j);
            }
        }
    }

    #[test]
    fn nth_unset_examples() {
        let t = &*NTH_UNSET;
        // mask = 0 → all bits are unset.
        for j in 0..RANKS {
            assert_eq!(t[0][j], j as u8);
        }
        // mask = 1 (bit 0 set) → first unset is bit 1.
        assert_eq!(t[1][0], 1);
        assert_eq!(t[1][1], 2);
        assert_eq!(t[1][12], 0xff); // only 12 unset bits, the 13th doesn't exist
        // mask = all bits set (low 13) → no unset bits.
        let all = (1u32 << RANKS) as usize - 1;
        for j in 0..RANKS {
            assert_eq!(t[all][j], 0xff);
        }
    }

    #[test]
    fn equal_table_matches_definition() {
        let t = &*EQUAL;
        for i in 0..(1 << (SUITS - 1)) {
            assert!(!t[i][0]); // index 0 is unused / always false
            for j in 1..SUITS {
                let expect = (i >> (j - 1)) & 1 != 0;
                assert_eq!(t[i][j], expect, "EQUAL[{}][{}]", i, j);
            }
        }
    }

    #[test]
    fn colex_round_trip_all_13bit_sets() {
        let (rank_set_to_index, index_to_rank_set) = &*COLEX_TABLES;
        for mask in 0..(1 << RANKS) {
            let idx = rank_set_to_index[mask];
            let popcount = (mask as u32).count_ones() as usize;
            let recovered = index_to_rank_set[popcount][idx as usize];
            assert_eq!(
                recovered as usize, mask,
                "colex round-trip failed for mask {}",
                mask
            );
        }
    }

    #[test]
    fn colex_index_is_dense() {
        // For each popcount k, the indices used should cover 0..C(13,k).
        let (rank_set_to_index, _) = &*COLEX_TABLES;
        for k in 0..=RANKS {
            let mut seen: HashSet<u64> = HashSet::new();
            for mask in 0..(1 << RANKS) {
                if (mask as u32).count_ones() as usize == k {
                    seen.insert(rank_set_to_index[mask]);
                }
            }
            let expected = NCR_RANKS[RANKS][k];
            assert_eq!(seen.len() as u64, expected, "colex density mismatch at k={}", k);
            for v in &seen {
                assert!(*v < expected, "colex index {} out of range for k={}", v, k);
            }
        }
    }

    #[test]
    fn suit_permutations_is_a_bijection() {
        let perms = &*SUIT_PERMUTATIONS;
        assert_eq!(perms.len(), 24); // 4!
        let mut unique: HashSet<[u8; SUITS]> = HashSet::new();
        for p in perms {
            // Each permutation is a bijection on 0..SUITS.
            let mut seen = [false; SUITS];
            for &s in p {
                assert!(!seen[s as usize], "perm {:?} repeats suit", p);
                seen[s as usize] = true;
            }
            for &b in &seen {
                assert!(b, "perm {:?} skipped a suit", p);
            }
            unique.insert(*p);
        }
        assert_eq!(unique.len(), 24);
    }

    // ------- n_choose_k_group --------------------------------------

    #[test]
    fn n_choose_k_group_basics() {
        assert_eq!(n_choose_k_group(0, 0), 1);
        assert_eq!(n_choose_k_group(5, 0), 1);
        assert_eq!(n_choose_k_group(0, 5), 0);
        assert_eq!(n_choose_k_group(5, 5), 1);
        assert_eq!(n_choose_k_group(13, 2), 78);
        assert_eq!(n_choose_k_group(13, 3), 286);
        assert_eq!(n_choose_k_group(52, 2), 1326);
        // Multiset combination with 3 equal slots from 13 options:
        assert_eq!(n_choose_k_group(13 + 2, 3), 455);
        // Multiset combination with 2 equal slots from 13 options:
        assert_eq!(n_choose_k_group(13 + 1, 2), 91);
    }

    // ------- Configuration enumeration -----------------------------

    #[test]
    fn enumerate_preflop_configurations_3() {
        // 3 cards in one round, 4 suits → configs (3,0,0,0),
        // (2,1,0,0), (1,1,1,0). Count = 3.
        let mut count = 0usize;
        enumerate_configurations(1, &[3], |round, _config| {
            assert_eq!(round, 0);
            count += 1;
        });
        assert_eq!(count, 3);
    }

    #[test]
    fn enumerate_holdem_preflop_configurations_2() {
        // 2 cards in one round, 4 suits → configs (2,0,0,0),
        // (1,1,0,0). Count = 2.
        let mut count = 0usize;
        enumerate_configurations(1, &[2], |_round, _config| {
            count += 1;
        });
        assert_eq!(count, 2);
    }

    #[test]
    fn holdem_preflop_iso_class_count_169() {
        // Hold'em preflop has 169 strategically-distinct hands
        // (13 pocket pairs + 78 suited + 78 offsuit). The indexer
        // for rounds=[2] should produce exactly 169.
        let indexer = HandIndexer::init(&[2]).expect("init");
        assert_eq!(indexer.size(0), 169, "holdem preflop iso count");
    }

    #[test]
    fn holdem_preflop_plus_flop_iso_count() {
        // Hold'em preflop + flop iso class count is a known number:
        // 1,286,792. Source: Waugh 2013 paper.
        let indexer = HandIndexer::init(&[2, 3]).expect("init");
        assert_eq!(
            indexer.size(1),
            1_286_792,
            "holdem preflop+flop iso count (Waugh 2013 table)"
        );
    }

    #[test]
    fn holdem_through_river_iso_count() {
        // Full holdem (preflop + flop + turn + river) iso class
        // count: 2,428,287,420. Source: Waugh 2013 paper.
        let indexer = HandIndexer::init(&[2, 3, 1, 1]).expect("init");
        assert_eq!(indexer.size(3), 2_428_287_420);
    }

    // ------- Round trip ---------------------------------------------

    #[test]
    fn round_trip_holdem_preflop_all_169() {
        // For preflop, iterate every canonical index 0..169, unindex
        // to cards, re-index, assert we get the same canonical
        // index back.
        let indexer = HandIndexer::init(&[2]).expect("init");
        for idx in 0..indexer.size(0) {
            let cards = indexer.unindex(0, idx).expect("unindex");
            assert_eq!(cards.len(), 2);
            let mut indices = vec![0u64; 1];
            indexer.index_all(&cards, &mut indices);
            assert_eq!(
                indices[0], idx,
                "round-trip mismatch at idx {}: cards = {:?}",
                idx, cards
            );
        }
    }

    #[test]
    fn round_trip_oh_bidding_2p_3trick() {
        // OH 2p 3-trick bidding-only is rounds = [3, 1]. The
        // resulting iso class count should match the OH hand-rolled
        // enumerator's 63,193 canonical (hand × face_up_rank)
        // tuples. Plus we round-trip every index.
        let indexer = HandIndexer::init(&[3, 1]).expect("init");
        let total = indexer.size(1);
        assert_eq!(
            total, 63_193,
            "OH 2p 3-trick (hand + face_up) iso count must match the \
             hand-rolled enumerator's count (= 13 face_ups × 4861 \
             canonical hands)"
        );
        // Round-trip a sample of indices (full sweep would take a
        // while; trust the count + the holdem sweep above).
        for idx in (0..total).step_by((total as usize / 200).max(1) as usize) {
            let cards = indexer.unindex(1, idx).expect("unindex");
            assert_eq!(cards.len(), 4);
            let mut indices = vec![0u64; 2];
            indexer.index_all(&cards, &mut indices);
            assert_eq!(indices[1], idx, "round-trip mismatch at idx {}", idx);
        }
    }

    #[test]
    fn round_trip_oh_bidding_2p_1trick() {
        // OH 2p 1-trick: rounds = [1, 1]. Iso class count should be
        // 25 canonical hands × 13 face_ups = ... well, that's the
        // raw count; with canonicalisation it's the deduped count.
        let indexer = HandIndexer::init(&[1, 1]).expect("init");
        // Sanity: round 0 (just 1 card) → 1 iso class (suits all
        // equivalent, one rank → 13 ranks but they're not symmetric
        // under suit perm... wait).
        //
        // For round 0 with cards_per_round=[1]: each card is a
        // single (rank, suit). Under suit perm (all 4 suits
        // equivalent at this stage), the iso class depends only on
        // rank. So size(0) = RANKS = 13.
        assert_eq!(indexer.size(0), RANKS as u64);
        // For round 1 with [1, 1]: 2 cards revealed, possibly same
        // suit or different. Iso classes = ?
        // Round-trip everything to be sure.
        for idx in 0..indexer.size(1) {
            let cards = indexer.unindex(1, idx).expect("unindex");
            assert_eq!(cards.len(), 2);
            let mut indices = vec![0u64; 2];
            indexer.index_all(&cards, &mut indices);
            assert_eq!(indices[1], idx, "round-trip mismatch at idx {}: cards={:?}", idx, cards);
        }
    }

    #[test]
    fn round_trip_oh_full_2p_1trick() {
        // OH 2p 1-trick full game: rounds = [1, 1, 1, 1] (hand +
        // face_up + 2 plays).
        let indexer = HandIndexer::init(&[1, 1, 1, 1]).expect("init");
        let total = indexer.size(3);
        assert!(total > 0);
        // Round-trip a small sample.
        for idx in (0..total).step_by((total as usize / 100).max(1) as usize) {
            let cards = indexer.unindex(3, idx).expect("unindex");
            assert_eq!(cards.len(), 4);
            let mut indices = vec![0u64; 4];
            indexer.index_all(&cards, &mut indices);
            assert_eq!(indices[3], idx, "round-trip mismatch at idx {}", idx);
        }
    }

    #[test]
    fn unindex_out_of_range_returns_none() {
        let indexer = HandIndexer::init(&[2]).expect("init");
        assert!(indexer.unindex(0, indexer.size(0)).is_none());
        assert!(indexer.unindex(1, 0).is_none());
    }

    #[test]
    fn init_rejects_invalid_configs() {
        assert!(HandIndexer::init(&[]).is_none());
        assert!(HandIndexer::init(&[CARDS as u8 + 1]).is_none());
        let too_many: Vec<u8> = vec![1; MAX_ROUNDS + 1];
        assert!(HandIndexer::init(&too_many).is_none());
        // Round with > MAX_CARDS_PER_ROUND.
        assert!(HandIndexer::init(&[(MAX_CARDS_PER_ROUND + 1) as u8]).is_none());
    }

    #[test]
    fn unindex_produces_unique_canonical_forms() {
        // For a small game, iterate every index, unindex, re-index,
        // and assert: (a) the round-trip closes, (b) the unindexed
        // cards across all idxs are unique (different idxs give
        // different card sequences).
        let indexer = HandIndexer::init(&[2, 1]).expect("init");
        let total = indexer.size(1);
        let mut seen: HashSet<Vec<u8>> = HashSet::new();
        for idx in 0..total {
            let cards = indexer.unindex(1, idx).expect("unindex");
            // Within a round, the cards may be in any order (since
            // within-round is unordered). Sort within each round for
            // a stable signature.
            let mut sig = cards.clone();
            sig[..2].sort();
            assert!(seen.insert(sig.clone()), "duplicate canonical form at idx {}: {:?}", idx, sig);
            let mut indices = vec![0u64; 2];
            indexer.index_all(&cards, &mut indices);
            assert_eq!(indices[1], idx);
        }
        assert_eq!(seen.len() as u64, total);
    }
}
