use std::sync::OnceLock;

use boomphf::Mphf;
use games::{
    gamestates::{
        bluff::Bluff,
        euchre::{
            actions::EAction,
            isomorphic::normalize_euchre_istate,
            iterator::EuchreIsomorphicIStateIterator,
        },
        kuhn_poker::KuhnPoker,
        oh_hell::actions::OH_DECK_SIZE,
    },
    iso::hand_indexer::{HandIndexer, IndexerState},
    istate::IStateKey,
    iterator::IStateIterator,
    Action,
};
use itertools::Itertools;
use serde::{Deserialize, Serialize};

use crate::collections::mmapvec::MMapVec;

const GAMMA: f64 = 1.7;

#[derive(Clone, Copy, Serialize, Deserialize)]
enum Sharder {
    Euchre,
    NoOp,
}

impl Sharder {
    fn shard(&self, istate: &IStateKey) -> Option<(usize, IStateKey)> {
        match self {
            Sharder::Euchre => euchre_sharder(istate),
            Sharder::NoOp => Some((0, *istate)),
        }
    }
}

/// Two backends:
///   * `Phf` — boomphf over an enumerated iso-class set. Used for
///     Kuhn Poker, Bluff(1,1), Euchre. Requires building the PHF up
///     front, which dominates startup time for large games.
///   * `WaughOh` — direct Waugh-2013 multi-round hand isomorphism for
///     Oh Hell. Closed-form O(1) slot lookup; no enumeration, no
///     PHF, no on-disk indexer state to serialize. Supports any
///     `(num_players, n_tricks, max_cards_played)` including the
///     `3p × 3-trick × max=2` config that the iterator can't reach.
#[derive(Serialize, Deserialize)]
pub enum Indexer {
    Phf(PhfIndexer),
    WaughOh(WaughOhIndexer),
}

impl Indexer {
    /// May return None if the key isn't in the original function, but this isn't guaranteed
    pub fn index(&self, key: &IStateKey) -> Option<usize> {
        match self {
            Indexer::Phf(p) => p.index(key),
            Indexer::WaughOh(w) => Some(w.index(key) as usize),
        }
    }

    /// Returns the total length of the indexer
    pub fn len(&self) -> usize {
        match self {
            Indexer::Phf(p) => p.len(),
            Indexer::WaughOh(w) => w.len() as usize,
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Parse an `indexer` file written before commit 2f075ed, when
    /// `Indexer` was still a bare struct rather than the
    /// `Phf`/`WaughOh` enum. The old layout matches today's
    /// `PhfIndexer` field-for-field, so we deserialize into a local
    /// mirror and wrap it in the `Phf` variant. Used by the migration
    /// tool (and the runtime load fallback) so the existing trained
    /// Euchre weight files don't need to be retrained when we switch
    /// the on-disk format.
    pub fn from_legacy_struct_json(json: &str) -> anyhow::Result<Self> {
        // Mirror of the pre-enum `Indexer` struct fields. The two
        // private-to-this-module types (`Mphf<IStateKey>`, `Sharder`)
        // implement `Deserialize`, so serde walks the JSON exactly the
        // way it used to.
        #[derive(Deserialize)]
        struct LegacyIndexer {
            phf: Mphf<IStateKey>,
            shard_len: usize,
            num_shards: usize,
            sharder: Sharder,
        }
        let l: LegacyIndexer = serde_json::from_str(json)?;
        Ok(Indexer::Phf(PhfIndexer {
            phf: l.phf,
            shard_len: l.shard_len,
            num_shards: l.num_shards,
            sharder: l.sharder,
        }))
    }

    /// Build a PHF-backed indexer for Euchre by enumerating every
    /// reachable istate via the isomorphic-istate iterator and feeding
    /// them to `Mphf::new`. Build is O(reachable istates) (slow for
    /// large `max_cards_played`) but lookup is a single MPHF hash and
    /// the slot space is compact (1:1 with reachable istates).
    ///
    /// ## Why PHF and not a Waugh-style direct indexer
    ///
    /// A Waugh-style multi-round HandIndexer works cleanly for Oh Hell
    /// (see `oh_hell_full_game`) because OH has no cards whose
    /// identity changes based on declared trump and no dealer-private
    /// state. It worked there: cleaner code, no enumeration build,
    /// unlocked configs PHF couldn't fit (3p × 3t × max=2).
    ///
    /// Trying the same for Euchre (see git history through commits
    /// e717999..d0c876a) ran into enough Euchre-specific complications
    /// that the result was *harder* to reason about than this PHF, not
    /// easier. Concretely:
    ///
    ///   * The L-Bauer (off-color trump-color jack acts as trump) means
    ///     the iso group on hands is Z₂×Z₂ pre-trump and Z₂ post-trump,
    ///     not the full S₄ that a vanilla HandIndexer reduces by. Hands
    ///     like {AH, KH, QH, JH, JD} vs {AH, KH, QH, JH, JC} are S₄-iso
    ///     under C↔D but NOT strategically iso (under H trump the first
    ///     has 5 trumps via JD-Left-Bauer, the second has 4). A naive
    ///     Waugh `index_last` collapses them; a correct color-split
    ///     HandIndexer is much more code per phase.
    ///
    ///   * Dealer-private discard creates iterator-emitted istates that
    ///     iso-norm cannot deduplicate by suit_order alone (two
    ///     "different discards" that look identical to the canonical
    ///     suit-mask). Each had to be encoded with extra dimensions to
    ///     keep bijection vs the iterator.
    ///
    ///   * The iterator's `legal_actions_choose_trump` allows R2-Pass
    ///     counts up to 7 (giving 86k "Stick-the-Dealer-relaxed"
    ///     istates per face-up shard at max=0) that real Euchre's
    ///     `apply_action_choose_trump` rejects. The Waugh path had to
    ///     special-case-filter these; the PHF path just hashes whatever
    ///     the iterator emits, so it's a non-issue.
    ///
    ///   * The bid history has many sub-phases (R1, R2, Discard,
    ///     Alone-pending dealer-own, Alone-pending dealer-observer,
    ///     Alone-pending non-dealer, post-Alone-decision) each with
    ///     different perspective / hand-reconstruction rules. The
    ///     Waugh indexer ended up at ~600 lines with 39 bid_state
    ///     variants (heading to ~87 for full Play coverage) — the PHF
    ///     handles all of them with `iterator → Mphf::new`.
    ///
    /// PHF builds are slow but tractable, the on-disk slot space is
    /// compact, and the correctness story is "by construction" rather
    /// than "match the iterator's iso reduction across every
    /// sub-phase." If a future config exhausts PHF buildability,
    /// revisit Waugh-with-color-split as a per-phase indexer; until
    /// then this is the simpler answer.
    pub fn euchre(max_cards_played: usize) -> Self {
        let istate_iter =
            EuchreIsomorphicIStateIterator::with_face_up(max_cards_played, &[EAction::NS]);
        // Use an mmap vector as this collection may not fit into memory. This is also
        // more performant than the chunked iterator approach as we do not have an efficient method to
        // find the nth item for the iterator -- a common call in later rounds of the phf.
        let istates = MMapVec::from_iter(istate_iter);
        let phf = Mphf::new(GAMMA, &istates);

        Indexer::Phf(PhfIndexer {
            phf,
            shard_len: istates.len(),
            num_shards: 6, // one for each possible face up card
            sharder: Sharder::Euchre,
        })
    }

    pub fn kuhn_poker() -> Self {
        let istate_iter = IStateIterator::new(KuhnPoker::new_state());
        let istates = istate_iter.collect_vec();
        let phf = Mphf::new(GAMMA, &istates);
        Indexer::Phf(PhfIndexer {
            phf,
            shard_len: istates.len(),
            num_shards: 1,
            sharder: Sharder::NoOp,
        })
    }

    pub fn bluff_11() -> Self {
        let istate_iter = IStateIterator::new(Bluff::new_state(1, 1));
        let istates = istate_iter.collect_vec();
        let phf = Mphf::new(GAMMA, &istates);
        Indexer::Phf(PhfIndexer {
            phf,
            shard_len: istates.len(),
            num_shards: 1,
            sharder: Sharder::NoOp,
        })
    }

    /// Build a Waugh-based direct indexer for an Oh Hell configuration.
    /// O(1) per slot lookup; no enumeration or PHF construction.
    /// Same slot space as the iterator-built PHF (verified by
    /// `examples/waugh_oh_indexer_poc.rs`).
    pub fn oh_hell_full_game(
        num_players: usize,
        n_tricks: usize,
        max_cards_played: usize,
    ) -> Self {
        Indexer::WaughOh(WaughOhIndexer::new(num_players, n_tricks, max_cards_played))
    }
}

/// PHF-backed indexer for games small enough to pre-enumerate.
#[derive(Serialize, Deserialize)]
pub struct PhfIndexer {
    phf: Mphf<IStateKey>,
    shard_len: usize,
    num_shards: usize,
    /// Returns the normalized istatekey and the associated shard.
    /// Shards keep similar istates near each other in the database.
    sharder: Sharder,
}

impl PhfIndexer {
    pub fn index(&self, key: &IStateKey) -> Option<usize> {
        let (shard, normed) = self.sharder.shard(key)?;
        self.phf
            .try_hash(&normed)
            .map(|x| x as usize + (shard * self.shard_len))
    }

    pub fn len(&self) -> usize {
        self.shard_len * self.num_shards
    }
}

// =====================================================================
// Waugh-based direct indexer for Oh Hell
// =====================================================================

/// Direct istate→slot indexer for Oh Hell using Waugh-2013 multi-round
/// hand isomorphism. Slot layout:
///   * `[0 .. bidding_size)` — bidding istates. Sub-layout by
///     perspective: perspective `p`'s slice is at
///     `bidding_offsets[p] .. bidding_offsets[p+1]` and contains
///     `(n_tricks+1)^p × waugh_size_1` slots
///     (one per prior-bid sequence × canonical (hand, face_up)).
///   * `[bidding_size .. total)` — play istates. Sub-layout by depth
///     `d ∈ [0, max_cards_played)`: `bid_full × waugh.size(1+d)`
///     slots per depth, where `bid_full` counts only hook-legal
///     complete bid profiles (total ≠ n_tricks).
///
/// The HandIndexer (and the per-round-size cache) is rebuilt lazily on
/// first use after deserialisation; only `(num_players, n_tricks,
/// max_cards_played)` is on disk.
#[derive(Serialize, Deserialize)]
pub struct WaughOhIndexer {
    num_players: usize,
    n_tricks: usize,
    max_cards: usize,
    #[serde(skip)]
    runtime: OnceLock<WaughOhRuntime>,
}

struct WaughOhRuntime {
    bid_base: u64,
    /// Number of hook-legal complete bid profiles: all `(n_tricks+1)^np`
    /// sequences minus those summing to `n_tricks` (the dealer hook
    /// forbids the last bid from making the total hit `n_tricks`).
    bid_full: u64,
    waugh: HandIndexer,
    waugh_size: Vec<u64>,       // waugh_size[r] = waugh.size(r), r ∈ [0, max_cards+2)
    bidding_offsets: Vec<u64>,  // [num_players + 1]
    bidding_size: u64,
    depth_offsets: Vec<u64>,    // [max_cards + 1]
    /// sum_counts[k][s] = number of k-bid sequences (each in
    /// [0, n_tricks]) summing to exactly s. Used to rank hook-legal
    /// bid profiles.
    sum_counts: Vec<Vec<u64>>,
    total: u64,
}

impl WaughOhIndexer {
    pub fn new(num_players: usize, n_tricks: usize, max_cards: usize) -> Self {
        let s = Self {
            num_players,
            n_tricks,
            max_cards,
            runtime: OnceLock::new(),
        };
        s.runtime(); // eagerly build
        s
    }

    fn runtime(&self) -> &WaughOhRuntime {
        self.runtime
            .get_or_init(|| WaughOhRuntime::build(self.num_players, self.n_tricks, self.max_cards))
    }

    pub fn len(&self) -> u64 {
        self.runtime().total
    }

    pub fn index(&self, key: &IStateKey) -> u64 {
        let rt = self.runtime();
        let n_tricks = self.n_tricks;
        // Walk the istate tail to split bids from plays. Layout:
        //   [0 .. n_tricks)          hand cards
        //   [n_tricks]               face_up
        //   [n_tricks + 1 ..]        bids (discriminant ≥ OH_DECK_SIZE)
        //                            then plays (discriminant < OH_DECK_SIZE)
        let mut bids: Vec<u8> = Vec::with_capacity(self.num_players);
        let mut num_plays = 0usize;
        for i in (n_tricks + 1)..key.len() {
            let d = key[i].0;
            if d >= OH_DECK_SIZE as u8 {
                bids.push(d - OH_DECK_SIZE as u8);
            } else {
                num_plays += 1;
            }
        }

        if num_plays == 0 && bids.len() < self.num_players {
            // Bidding istate. perspective = number of prior bids seen.
            let perspective = bids.len();
            let bid_idx = encode_bids(&bids, rt.bid_base);
            let waugh_idx = compute_waugh_idx(key, n_tricks, 0, &rt.waugh);
            rt.bidding_offsets[perspective] + bid_idx * rt.waugh_size[1] + waugh_idx
        } else {
            // Play istate at depth = num_plays.
            let depth = num_plays;
            debug_assert!(
                depth < self.max_cards,
                "play istate at depth {} >= max_cards {} (out of CFR scope)",
                depth,
                self.max_cards,
            );
            let bid_idx = rt.rank_legal_bid_profile(&bids, n_tricks);
            let waugh_idx = compute_waugh_idx(key, n_tricks, depth, &rt.waugh);
            rt.bidding_size
                + rt.depth_offsets[depth]
                + bid_idx * rt.waugh_size[1 + depth]
                + waugh_idx
        }
    }
}

impl WaughOhRuntime {
    fn build(num_players: usize, n_tricks: usize, max_cards: usize) -> Self {
        let bid_base = (n_tricks + 1) as u64;
        // sum_counts[k][s]: k bids in [0, n_tricks] summing to s.
        let mut sum_counts = vec![vec![0u64; num_players * n_tricks + 1]; num_players + 1];
        sum_counts[0][0] = 1;
        for k in 1..=num_players {
            for s in 0..=k * n_tricks {
                sum_counts[k][s] = (0..=n_tricks.min(s))
                    .map(|d| sum_counts[k - 1][s - d])
                    .sum();
            }
        }
        // Complete bid profiles, minus those the dealer hook forbids
        // (total == n_tricks).
        let bid_full = bid_base.pow(num_players as u32) - sum_counts[num_players][n_tricks];

        let mut rounds = vec![n_tricks as u8, 1];
        for _ in 0..max_cards {
            rounds.push(1);
        }
        let waugh = HandIndexer::init(&rounds).expect("Waugh indexer init");
        let n_rounds = 2 + max_cards;
        let waugh_size: Vec<u64> = (0..n_rounds).map(|r| waugh.size(r)).collect();

        let mut bidding_offsets = Vec::with_capacity(num_players + 1);
        let mut running = 0u64;
        for p in 0..num_players {
            bidding_offsets.push(running);
            running += bid_base.pow(p as u32) * waugh_size[1];
        }
        bidding_offsets.push(running);
        let bidding_size = running;

        let mut depth_offsets = Vec::with_capacity(max_cards + 1);
        let mut running = 0u64;
        for d in 0..max_cards {
            depth_offsets.push(running);
            running += bid_full * waugh_size[1 + d];
        }
        depth_offsets.push(running);
        let play_size = running;

        Self {
            bid_base,
            bid_full,
            waugh,
            waugh_size,
            bidding_offsets,
            bidding_size,
            depth_offsets,
            sum_counts,
            total: bidding_size + play_size,
        }
    }

    /// Rank a complete bid profile among the hook-legal profiles,
    /// preserving the little-endian mixed-radix order of
    /// `encode_bids_from_istate` restricted to legal profiles. Walks
    /// digits from most significant (last bid) down, counting the legal
    /// profiles under every smaller digit choice.
    fn rank_legal_bid_profile(&self, bids: &[u8], n_tricks: usize) -> u64 {
        let mut rank = 0u64;
        let mut prefix_sum = 0usize;
        for i in (0..bids.len()).rev() {
            for d in 0..bids[i] as usize {
                let free = self.bid_base.pow(i as u32);
                let illegal = n_tricks
                    .checked_sub(prefix_sum + d)
                    .and_then(|need| self.sum_counts[i].get(need))
                    .copied()
                    .unwrap_or(0);
                rank += free - illegal;
            }
            prefix_sum += bids[i] as usize;
        }
        rank
    }
}

/// Convert an OH istate's hand + face_up + first `depth` plays into
/// Waugh card encoding and compute the iso-class index through round
/// `1 + depth`.
fn compute_waugh_idx(key: &IStateKey, n_tricks: usize, depth: usize, waugh: &HandIndexer) -> u64 {
    let mut state = IndexerState::new();
    let mut idx;

    // Round 0: hand cards.
    let mut hand = Vec::with_capacity(n_tricks);
    for i in 0..n_tricks {
        hand.push(oh_disc_to_waugh(key[i].0));
    }
    idx = waugh.next_round(&hand, &mut state);
    if depth == 0 && n_tricks == 0 {
        return idx;
    }

    // Round 1: face_up.
    idx = waugh.next_round(&[oh_disc_to_waugh(key[n_tricks].0)], &mut state);
    if depth == 0 {
        return idx;
    }

    // Rounds 2..=(1+depth): plays in order.
    // Plays start in the istate tail AFTER all bids.
    let tail_start = n_tricks + 1;
    let mut play_iter = key
        .iter()
        .skip(tail_start)
        .filter(|a| a.0 < OH_DECK_SIZE as u8)
        .copied();
    for _ in 0..depth {
        let play = play_iter.next().expect("missing play card at depth");
        idx = waugh.next_round(&[oh_disc_to_waugh(play.0)], &mut state);
    }
    idx
}

/// Little-endian mixed-radix encoding of a (possibly partial) bid
/// sequence. Used for bidding-phase istates, where any prefix is
/// reachable (the hook only constrains the final bid).
fn encode_bids(bids: &[u8], bid_base: u64) -> u64 {
    let mut idx = 0u64;
    let mut mul = 1u64;
    for &b in bids {
        idx += (b as u64) * mul;
        mul *= bid_base;
    }
    idx
}

/// OHCard discriminant (`suit * 13 + rank`) → Waugh card encoding
/// (`(rank << 2) | suit`).
fn oh_disc_to_waugh(d: u8) -> u8 {
    let suit = d / 13;
    let rank = d % 13;
    (rank << 2) | suit
}

fn euchre_sharder(istate: &IStateKey) -> Option<(usize, IStateKey)> {
    let mut normed = normalize_euchre_istate(istate);
    let face_up = *normed.get(5)?;
    normed.swap(Action::from(EAction::NS), face_up); // swap to be an istate with ns as the face up card
    normed.sort_range(0, 5.min(normed.len()));

    let face_up = EAction::from(face_up);
    use EAction::*;
    let shard = match face_up {
        NS => 0,
        TS => 1,
        JS => 2,
        QS => 3,
        KS => 4,
        AS => 5,
        _ => panic!("found non-spades face up card after normalization"),
    };

    Some((shard, normed))
}

#[cfg(test)]
mod waugh_oh_tests {
    use super::*;
    use games::gamestates::oh_hell::{
        actions::{OHAction, OHCard, OH_DECK},
        iterator::OhHellIsomorphicIStateIterator,
        OhHell,
    };
    use games::GameState;

    /// Helper: convert a Waugh card encoding back to OHCard.
    fn waugh_card_to_oh(w: u8) -> OHCard {
        let suit = w & 3;
        let rank = w >> 2;
        OHCard::from_index(suit * 13 + rank).unwrap()
    }

    /// Every iterator-emitted istate gets a unique slot in [0, total).
    fn check_bijection(np: usize, nt: usize, max_cards: usize) {
        let indexer = Indexer::oh_hell_full_game(np, nt, max_cards);
        let total = indexer.len();
        let mut seen = std::collections::HashSet::new();
        let mut count = 0;
        for istate in OhHellIsomorphicIStateIterator::full_game_via_waugh(np, nt, max_cards) {
            let slot = indexer.index(&istate).expect("indexable");
            assert!(
                slot < total,
                "{}p_{}t_max{}: slot {} out of range (total {})",
                np, nt, max_cards, slot, total
            );
            assert!(
                seen.insert(slot),
                "{}p_{}t_max{}: collision at slot {}",
                np, nt, max_cards, slot
            );
            count += 1;
        }
        assert_eq!(
            seen.len(),
            total,
            "{}p_{}t_max{}: iter emitted {} but indexer has {} slots",
            np, nt, max_cards, count, total
        );
    }

    #[test]
    fn waugh_oh_bijection_smoke() {
        check_bijection(2, 1, 0);
        check_bijection(2, 2, 0);
        check_bijection(2, 2, 1);
        check_bijection(3, 1, 0);
        check_bijection(3, 2, 0);
        check_bijection(3, 2, 1);
    }

    /// Iso-permuted raw istates land on the same slot.
    #[test]
    fn waugh_oh_iso_on_raw() {
        let configs = [(2, 1, 0), (2, 2, 1), (3, 2, 1)];
        let perms: [[u8; 4]; 4] = [
            [0, 1, 2, 3],
            [1, 0, 2, 3],
            [2, 3, 0, 1],
            [3, 2, 1, 0],
        ];

        for (np, nt, max_cards) in configs {
            let indexer = Indexer::oh_hell_full_game(np, nt, max_cards);
            let mut rng: u64 = 0xC0FFEE;
            let mut next = || {
                rng ^= rng << 13;
                rng ^= rng >> 7;
                rng ^= rng << 17;
                rng
            };

            for _ in 0..50 {
                let seed = next() as usize;
                let mut gs = OhHell::new_state(np, nt);
                let mut acts = Vec::new();
                let mut step: usize = 0;
                while !gs.is_terminal() {
                    gs.legal_actions(&mut acts);
                    let pick = (seed.wrapping_add(step.wrapping_mul(73))) % acts.len();
                    gs.apply_action(acts[pick]);
                    step += 1;
                    use games::gamestates::oh_hell::OHPhase;
                    if !gs.is_chance_node() {
                        if gs.phase() == OHPhase::Play && gs.cards_played() >= max_cards {
                            continue;
                        }
                        break;
                    }
                }
                if gs.is_terminal() || gs.is_chance_node() {
                    continue;
                }
                let perspective = gs.cur_player();
                let raw = gs.istate_key(perspective);
                let slot_raw = indexer.index(&raw).expect("indexable");

                for perm in &perms {
                    let mut gs_p = OhHell::new_state(np, nt);
                    for a in gs.key().iter().copied() {
                        let oa = OHAction::from(a);
                        let new_a = match oa {
                            OHAction::Card(c) => {
                                let d = c as u8;
                                let suit = (d / 13) as usize;
                                let rank = d % 13;
                                let new_c = OHCard::from_index(perm[suit] * 13 + rank).unwrap();
                                OHAction::Card(new_c).into()
                            }
                            OHAction::Bid(_) => a,
                        };
                        gs_p.apply_action(new_a);
                    }
                    let raw_p = gs_p.istate_key(perspective);
                    let slot_perm = indexer.index(&raw_p).expect("indexable");
                    assert_eq!(
                        slot_raw, slot_perm,
                        "iso mismatch ({}p_{}t_max{}): perm={:?}",
                        np, nt, max_cards, perm
                    );
                }
            }
        }
    }

    /// Verify the waugh_card_to_oh helper round-trips against
    /// oh_disc_to_waugh (used as a basic encoding sanity).
    #[test]
    fn waugh_oh_encoding_round_trip() {
        for c in OH_DECK.iter() {
            let d = *c as u8;
            let w = oh_disc_to_waugh(d);
            let back = waugh_card_to_oh(w);
            assert_eq!(back, *c);
        }
    }
}

