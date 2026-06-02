use std::sync::OnceLock;

use boomphf::Mphf;
use games::{
    gamestates::{
        bluff::Bluff,
        euchre::{
            actions::{Card as ECard, EAction, Suit as ESuit},
            isomorphic::normalize_euchre_istate,
            iterator::EuchreIsomorphicIStateIterator,
        },
        kuhn_poker::KuhnPoker,
        oh_hell::actions::{OHCard, OH_DECK_SIZE},
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
    WaughEuchre(WaughEuchreIndexer),
}

impl Indexer {
    /// May return None if the key isn't in the original function, but this isn't guaranteed
    pub fn index(&self, key: &IStateKey) -> Option<usize> {
        match self {
            Indexer::Phf(p) => p.index(key),
            Indexer::WaughOh(w) => Some(w.index(key) as usize),
            Indexer::WaughEuchre(w) => Some(w.index(key) as usize),
        }
    }

    /// Returns the total length of the indexer
    pub fn len(&self) -> usize {
        match self {
            Indexer::Phf(p) => p.len(),
            Indexer::WaughOh(w) => w.len() as usize,
            Indexer::WaughEuchre(w) => w.len() as usize,
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

    pub fn euchre(max_cards_played: usize) -> Self {
        // TODO: in the future can use make it so the hashing happens in stages so that later istates are offset from others as a way to save space
        // Or can pass in the max num cards as a parameter
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

    /// Build a Waugh-based direct indexer for Euchre.
    ///
    /// **Phase 1 scaffold** — see the roadmap in `database/indexer.rs`.
    /// `index()` currently panics; do NOT use for training until Phase 2
    /// (slot computation + L-Bauer + Discard + Play) is complete. The
    /// constructor and bidding-state machinery are in place so the
    /// migration tool and tests can be built alongside Phase 2.
    pub fn euchre_waugh(max_cards_played: usize) -> Self {
        Indexer::WaughEuchre(WaughEuchreIndexer::new(max_cards_played))
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
///     slots per depth.
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
    bid_full: u64,
    waugh: HandIndexer,
    waugh_size: Vec<u64>,       // waugh_size[r] = waugh.size(r), r ∈ [0, max_cards+2)
    bidding_offsets: Vec<u64>,  // [num_players + 1]
    bidding_size: u64,
    depth_offsets: Vec<u64>,    // [max_cards + 1]
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
        let mut num_bids = 0u8;
        let mut num_plays = 0usize;
        for i in (n_tricks + 1)..key.len() {
            let d = key[i].0;
            if d >= OH_DECK_SIZE as u8 {
                num_bids += 1;
            } else {
                num_plays += 1;
            }
        }

        if num_plays == 0 && (num_bids as usize) < self.num_players {
            // Bidding istate. perspective = number of prior bids seen.
            let perspective = num_bids as usize;
            let bid_idx = encode_bids_from_istate(key, n_tricks, perspective, rt.bid_base);
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
            let bid_idx = encode_bids_from_istate(key, n_tricks, self.num_players, rt.bid_base);
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
        let bid_full = bid_base.pow(num_players as u32);

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
            total: bidding_size + play_size,
        }
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

fn encode_bids_from_istate(
    key: &IStateKey,
    n_tricks: usize,
    num_bids: usize,
    bid_base: u64,
) -> u64 {
    let tail_start = n_tricks + 1;
    let mut idx = 0u64;
    let mut mul = 1u64;
    let mut seen = 0;
    for i in tail_start..key.len() {
        let d = key[i].0;
        if d >= OH_DECK_SIZE as u8 {
            if seen >= num_bids {
                break;
            }
            let b = d - OH_DECK_SIZE as u8;
            idx += (b as u64) * mul;
            mul *= bid_base;
            seen += 1;
        }
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

// =====================================================================
// Waugh-based direct indexer for Euchre (work in progress)
// =====================================================================
//
// Roadmap — what's done and what's not.
//
// DONE (Phase 1):
//   * Bidding state machine (`EuchreBidState`) — finite enum over every
//     point at which CFR can have a non-Play istate. Total ≤ 64 variants
//     so the encoding can be a small integer table.
//   * `parse_euchre_bid_state(&IStateKey, n_cards_per_hand)` — derive the
//     bid state from an istate's tail. Independently testable; does NOT
//     depend on L-Bauer or slot layout.
//   * `WaughEuchreIndexer` struct + `Indexer::euchre_waugh` constructor,
//     wired into the `Indexer` enum.
//   * Slot layout sketch (per shard): bidding-state-major × waugh-hand-iso
//     minor. Computed in `WaughEuchreRuntime::build`.
//
// TODO (Phase 2 — required for any training to be correct):
//   * `index()` body that computes the slot from a parsed istate.
//     Currently returns 0 with a panic — explicitly NOT a quiet stub so
//     mis-wired training fails loudly.
//   * L-Bauer preprocessor — map (cards, declared_trump) → (cards with
//     off-color jack reassigned to trump's suit). Required for any
//     post-trump-declaration bid state (Alone, Discard) and all Play
//     istates.
//   * Discard slot layout — dealer's istate after Pickup has 6 cards in
//     hand (5 + picked-up face_up). Requires a second HandIndexer with
//     round 0 = 6.
//   * Play phase (depth d = 1..max_cards_played) — append play rounds
//     to the HandIndexer config, encode plays in order, similar to OH.
//
// TODO (Phase 3 — migration):
//   * `examples/migrate_euchre_phf_to_waugh.rs` — load legacy PHF +
//     mmap, enumerate via the iterator, copy each populated InfoState
//     from old slot to new Waugh slot.

/// Every distinct "what's been decided in bidding" state CFR can encounter.
///
/// Phases the istate can be at when no Play action has happened:
///   * Round 1 (Pickup vs Pass), pre-Pickup: 0..=3 prior passes.
///   * Round 2 (ChooseTrump vs Pass), pre-call: 4 R1 passes + 0..=3 R2
///     passes.
///   * Discard (dealer only): trump was declared via Pickup; dealer is
///     about to drop one of their 6 cards. Distinguished by which R1
///     slot Pickup'd — 4 sub-variants — because that determines who
///     leads first trick.
///   * Alone (caller): trump was declared (via Pickup or R2 call) and
///     the caller has yet to decide alone-or-not. Distinguished by the
///     calling slot × calling phase.
///
/// The encoding stores the *bid prefix* but NOT the perspective; the
/// perspective is implicit in the istate's hand contents and is folded
/// into the hand-iso Waugh index, not the bid state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EuchreBidState {
    /// Round 1 pickup decision pending. `passes_so_far` ∈ {0,1,2,3}.
    R1Pending { passes_so_far: u8 },
    /// Round 2 trump-choice pending after 4 R1 passes. `r2_passes` ∈ {0,1,2,3}.
    R2Pending { r2_passes: u8 },
    /// Dealer in Discard phase after `r1_pickup_seat` ∈ {0,1,2,3} called Pickup.
    Discard { r1_pickup_seat: u8 },
    /// Caller in Alone phase. `caller_seat` ∈ {0,1,2,3}.
    /// `via_pickup` = true if Pickup path (dealer discarded), else R2 path
    /// (some `chosen_trump` was declared — we only need the seat for the
    /// slot because the trump suit is derivable from the face-up sharding +
    /// the istate's R2 call action).
    Alone { caller_seat: u8, via_pickup: bool },
}

impl EuchreBidState {
    /// Total number of distinct bid states — used to size the per-shard
    /// slot allocation.
    pub const COUNT: usize =
        4    // R1Pending: 4
        + 4  // R2Pending: 4
        + 4  // Discard: 4
        + 8; // Alone: 4 seats × 2 paths

    /// Stable index into `[0, COUNT)`.
    pub fn to_idx(self) -> usize {
        match self {
            EuchreBidState::R1Pending { passes_so_far } => passes_so_far as usize,
            EuchreBidState::R2Pending { r2_passes } => 4 + r2_passes as usize,
            EuchreBidState::Discard { r1_pickup_seat } => 8 + r1_pickup_seat as usize,
            EuchreBidState::Alone { caller_seat, via_pickup } => {
                12 + (caller_seat as usize) * 2 + via_pickup as usize
            }
        }
    }
}

/// Parse the istate's tail to recover the bidding state. Returns `None` if
/// the istate is in the Play phase (which this Phase 1 scaffold does NOT
/// yet handle — see roadmap above).
///
/// `n_hand_cards` is the size of the hand block at the start of the istate
/// (5 for standard Euchre; the constant is passed in to keep the parser
/// independent of game-specific globals).
pub fn parse_euchre_bid_state(key: &IStateKey, n_hand_cards: usize) -> Option<EuchreBidState> {
    // Tail layout after [hand (n_hand_cards), face_up (1)]:
    //   bidding actions: Pass / Pickup / Clubs / Spades / Hearts / Diamonds
    //   Alone (or Pass meaning NotAlone) once trump declared
    //   DiscardMarker pseudo-action (dealer view in Discard phase) or the
    //   actual discarded card (post-Discard view, currently unused by CFR)
    //
    // We walk the tail and track which sub-phase we're in.
    let tail_start = n_hand_cards + 1;
    let mut r1_passes = 0u8;
    let mut r1_pickup_at: Option<u8> = None;
    let mut saw_discard_marker = false;
    let mut r2_passes = 0u8;
    let mut r2_call_at: Option<u8> = None;
    let mut seen_alone_decision = false;
    let mut play_seen = false;

    for i in tail_start..key.len() {
        let ea = EAction::from(key[i]);
        match ea {
            EAction::Pass => {
                if r1_pickup_at.is_some() || r2_call_at.is_some() {
                    // A Pass *after* trump declaration is the "NotAlone" choice.
                    seen_alone_decision = true;
                } else if r1_passes < 4 && r2_call_at.is_none() && r2_passes == 0 {
                    // Still in R1.
                    r1_passes += 1;
                    if r1_passes > 4 {
                        // shouldn't happen — defensive
                        return None;
                    }
                } else {
                    r2_passes += 1;
                }
            }
            EAction::Pickup => {
                r1_pickup_at = Some(r1_passes);
            }
            EAction::Clubs | EAction::Spades | EAction::Hearts | EAction::Diamonds => {
                r2_call_at = Some(r2_passes);
            }
            EAction::Alone => {
                seen_alone_decision = true;
            }
            EAction::DiscardMarker => {
                saw_discard_marker = true;
            }
            _ if (ea as u8) < (EAction::DiscardMarker as u8) => {
                // Card action — either a discard (dealer view, post-DiscardMarker)
                // or a play. Either way we're past the bid state.
                if saw_discard_marker {
                    // Discard happened: now we'd be in Alone or Play. Treat as
                    // post-discard for state purposes.
                    seen_alone_decision = saw_discard_marker; // dealer's Alone view
                } else {
                    play_seen = true;
                }
            }
            _ => {}
        }
    }

    if play_seen {
        return None; // play phase — not handled in Phase 1
    }

    // Case analysis on what we observed.
    if r1_pickup_at.is_none() && r2_call_at.is_none() {
        // R1 still pending (no trump declared yet).
        if r1_passes < 4 {
            return Some(EuchreBidState::R1Pending { passes_so_far: r1_passes });
        }
        // 4 R1 passes done, R2 pending.
        if r2_passes < 4 {
            return Some(EuchreBidState::R2Pending { r2_passes });
        }
        return None; // all 8 pass — game void, no istate emitted by iterator
    }

    if let Some(seat) = r1_pickup_at {
        // Pickup path. Discard pending or Alone pending.
        if saw_discard_marker && !seen_alone_decision {
            return Some(EuchreBidState::Discard { r1_pickup_seat: seat });
        }
        // Either non-dealer view (no DiscardMarker, no Alone yet) or dealer
        // view past Discard (saw_discard_marker, no Alone yet either).
        // Both correspond to the Alone-pending state with caller = pickup_seat.
        return Some(EuchreBidState::Alone {
            caller_seat: seat,
            via_pickup: true,
        });
    }

    if let Some(r2_seat) = r2_call_at {
        return Some(EuchreBidState::Alone {
            caller_seat: r2_seat,
            via_pickup: false,
        });
    }

    None
}

/// Direct istate→slot indexer for Euchre using Waugh-2013 multi-round
/// hand isomorphism. **Work in progress — see the roadmap comment above
/// for what's implemented and what isn't.**
///
/// BLOCKING ISSUE discovered in Phase 2a (commit pending):
///
/// A vanilla `HandIndexer::init(&[5, 1])` over (hand, face_up) iso-reduces
/// under the full 4-suit symmetric group S₄ (24 permutations). The
/// existing Euchre iterator + PHF uses a STRICTER iso reduction — only
/// color-preserving suit permutations are considered iso (Z₂×Z₂ = 4
/// permutations: identity, swap_black {S↔C}, swap_red {H↔D}, both).
/// Concretely: hands like {AC, JH, QH, KH, AH} with face_up=NS and
/// {AH, JC, QC, KC, AC} with face_up=NS are iso under S₄ (swap C↔H) but
/// NOT iso under Euchre's Z₂×Z₂ — the iterator emits both, and a naive
/// Waugh `index_last` collapses them to the same slot, breaking
/// bijection.
///
/// The fix (Phase 2a-2, queued — needs design + implementation):
///
/// Split cards by color before indexing. Use two HandIndexers per shard:
///   * `waugh_black` over `(black_hand_count, 1)` rounds — covers
///     (Spades-or-Clubs hand cards, face_up which is always Spades in
///     the post-sharder layout). Allows S↔C swap; treats H,D as the
///     always-empty suits.
///   * `waugh_red` over `(red_hand_count,)` rounds — covers Hearts-or-
///     Diamonds hand cards. Allows H↔D swap; S,C empty.
///
/// Slot within shard = combo_offset[hand_black_count]
///                    + black_idx × red_size_at_combo + red_idx.
///
/// This matches Euchre's exact iso reduction. Until that's built and
/// passes the bijection test against the iterator, `index()` panics.
///
/// Slot layout per face-up shard (planned):
///   * For each `bid_state ∈ EuchreBidState`, a contiguous block of size
///     `color_split_size` (post-discard / play rounds add per-depth
///     blocks similar to OH).
///   * Shards are stacked: slot = shard_offset + intra_shard_slot.
///
/// The sharding mirrors `Indexer::euchre`: 6 shards, one per face-up
/// rank ∈ {NS, TS, JS, QS, KS, AS}.
#[derive(Serialize, Deserialize)]
pub struct WaughEuchreIndexer {
    max_cards_played: usize,
    #[serde(skip)]
    runtime: OnceLock<WaughEuchreRuntime>,
}

/// Per-shard color-split runtime. See the BLOCKING ISSUE / PATH-2
/// CORRECTION block above for design rationale.
struct WaughEuchreRuntime {
    /// `waugh_black[k]` for `k ∈ {0..=5}`. Black hand-card count.
    ///   * k = 0: rounds = `[1]` (just face_up).
    ///   * k > 0: rounds = `[k, 1]` (hand black cards + face_up).
    waugh_black: Vec<HandIndexer>,
    /// `waugh_red[k]` for `k ∈ {0..=5}`. Red hand-card count is `5 - k`
    /// in our indexer indexing convention (`k` is the black count, the
    /// red count derives from `5 - k`).
    ///   * 5 - k = 0: dummy 1-card indexer that's never queried.
    ///   * 5 - k > 0: rounds = `[5-k]`.
    waugh_red: Vec<HandIndexer>,
    /// `waugh_black_size[k]` = iso class count for `waugh_black[k]`.
    waugh_black_size: Vec<u64>,
    /// `waugh_red_size[k]` = iso class count for `waugh_red[k]`. For
    /// `5 - k = 0` this is 1 (the single "empty red hand" iso class).
    waugh_red_size: Vec<u64>,
    /// Cumulative offset of each `k` block within a (shard, bid_state)
    /// section. `combo_offsets[k] = Σ_{k'<k} (waugh_black_size[k'] ×
    /// waugh_red_size[k'])`. `combo_offsets[6]` is the total
    /// `bid_state_size`.
    combo_offsets: Vec<u64>,
    /// Total slots within one (shard, bid_state) block.
    bid_state_size: u64,
    /// Per-bid-state offset within a shard.
    bid_state_offsets: [u64; EuchreBidState::COUNT],
    /// Total slots within one shard.
    shard_size: u64,
    /// 6 shards × `shard_size` (matches `Indexer::euchre`'s sharding).
    total: u64,
}

impl WaughEuchreIndexer {
    pub fn new(max_cards_played: usize) -> Self {
        let s = Self {
            max_cards_played,
            runtime: OnceLock::new(),
        };
        s.runtime(); // eagerly build
        s
    }

    fn runtime(&self) -> &WaughEuchreRuntime {
        self.runtime
            .get_or_init(|| WaughEuchreRuntime::build(self.max_cards_played))
    }

    pub fn len(&self) -> u64 {
        self.runtime().total
    }

    pub fn index(&self, key: &IStateKey) -> u64 {
        // CORRECTION (Phase 2a roll-back): full-S₄ iso is wrong even for
        // pre-trump bidding. The L-Bauer (off-color trump-color jack)
        // creates Z₂×Z₂-only symmetry: hands {AH,KH,QH,JH,JD} and
        // {AH,KH,QH,JH,JC} are S₄-iso under C↔D but NOT strategically
        // iso (under H trump the former has 5 trumps via JD-Left-Bauer
        // and the latter has 4). The PHF's color-preserving Z₂×Z₂
        // reduction is the correct one.
        //
        // Re-pivoting to Path 1 (color-split indexer):
        //   * waugh_black on (black hand cards + face_up if black) for
        //     S↔C iso.
        //   * waugh_red on (red hand cards + face_up if red) for H↔D
        //     iso.
        //   * Slot encodes both indices plus the (black_count, red_count)
        //     combo (which color holds face_up, hand split by color).
        //
        // The color-split implementation below realises this design.
        let rt = self.runtime();
        let bid_state = match parse_euchre_bid_state(key, 5) {
            Some(s) => s,
            None => panic!(
                "WaughEuchreIndexer: istate has no recoverable bid state \
                 (either malformed or Play phase, which Phase 2c doesn't yet handle)"
            ),
        };
        match bid_state {
            EuchreBidState::R1Pending { .. } | EuchreBidState::R2Pending { .. } => {}
            EuchreBidState::Alone { .. } => panic!(
                "WaughEuchreIndexer: Alone bid state queued for Phase 2b"
            ),
            EuchreBidState::Discard { .. } => panic!(
                "WaughEuchreIndexer: Discard bid state queued for Phase 2c"
            ),
        }

        // D₄ canonicalisation:
        //   step 1: if face_up is red (H or D), apply swap_color
        //           (S↔H, C↔D) to every card. After this face_up is
        //           always black. This kills the swap_color iso element.
        //   step 2: split hand by color. Feed (hand_black, face_up) to
        //           a per-k black HandIndexer (it reduces by S↔C iso
        //           internally because the H,D positions are always
        //           empty). Feed hand_red to a per-k red HandIndexer
        //           (it reduces by H↔D iso). Combined this kills the
        //           residual Z₂×Z₂.
        let face_up_bit = key[5].0;
        let face_up_rank = face_up_bit % 8;
        let face_up_suit = face_up_bit / 8;
        let face_up_is_red = face_up_suit >= 2;

        let normalize_suit = |s: u8| -> u8 {
            if !face_up_is_red {
                s
            } else if s < 2 {
                s + 2
            } else {
                s - 2
            }
        };

        let canonical_face_up_suit = normalize_suit(face_up_suit);
        let face_up_waugh = (face_up_rank << 2) | canonical_face_up_suit;

        let mut hand_black: Vec<u8> = Vec::with_capacity(5);
        let mut hand_red: Vec<u8> = Vec::with_capacity(5);
        for i in 0..5 {
            let bit = key[i].0;
            let rank = bit % 8;
            let new_suit = normalize_suit(bit / 8);
            let waugh_card = (rank << 2) | new_suit;
            if new_suit < 2 {
                hand_black.push(waugh_card);
            } else {
                hand_red.push(waugh_card);
            }
        }
        let k = hand_black.len();

        // Waugh_black: feed (hand_black, face_up). For k = 0 the indexer
        // is the 1-card form so we only feed the face_up.
        let black_indexer = &rt.waugh_black[k];
        let black_idx = if k == 0 {
            let mut state = IndexerState::new();
            black_indexer.next_round(&[face_up_waugh], &mut state)
        } else {
            let mut state = IndexerState::new();
            black_indexer.next_round(&hand_black, &mut state);
            black_indexer.next_round(&[face_up_waugh], &mut state)
        };

        // Waugh_red: feed hand_red. Empty hand_red gets slot 0 (the
        // single empty-hand iso class).
        let red_idx = if hand_red.is_empty() {
            0
        } else {
            let red_indexer = &rt.waugh_red[k];
            let mut state = IndexerState::new();
            red_indexer.next_round(&hand_red, &mut state)
        };

        let shard = face_up_rank as u64;
        let red_size = rt.waugh_red_size[k];
        shard * rt.shard_size
            + rt.bid_state_offsets[bid_state.to_idx()]
            + rt.combo_offsets[k]
            + black_idx * red_size
            + red_idx
    }
}

/// Convert an istate entry's `Action(u8)` (a bit index 0..32) to Waugh's
/// `(rank << 2) | suit` encoding. Euchre cards occupy bit indices
/// `suit*8 + rank_in_suit` where `rank_in_suit ∈ [0, 6)` (0=9 ... 5=A)
/// and `suit ∈ [0, 4)` (0=S, 1=C, 2=H, 3=D).
#[inline]
fn euchre_istate_entry_to_waugh_card(a: Action) -> u8 {
    let bit_idx = a.0;
    debug_assert!(
        bit_idx < 32 && (bit_idx % 8) < 6,
        "euchre card bit_idx={} is not a card position",
        bit_idx
    );
    let suit = bit_idx / 8;
    let rank = bit_idx % 8;
    (rank << 2) | suit
}

/// Compute Waugh's iso-class index through round 1 (hand + face_up) for an
/// Euchre istate that's been through the sharder normalizer. The istate
/// layout is `[hand_0 .. hand_4, face_up, ...bid tail]`.
fn compute_euchre_waugh_idx_5_1(key: &IStateKey, waugh: &HandIndexer) -> u64 {
    let mut state = IndexerState::new();
    let hand: [u8; 5] = [
        euchre_istate_entry_to_waugh_card(key[0]),
        euchre_istate_entry_to_waugh_card(key[1]),
        euchre_istate_entry_to_waugh_card(key[2]),
        euchre_istate_entry_to_waugh_card(key[3]),
        euchre_istate_entry_to_waugh_card(key[4]),
    ];
    waugh.next_round(&hand, &mut state);
    let face_up = euchre_istate_entry_to_waugh_card(key[5]);
    waugh.next_round(&[face_up], &mut state)
}

impl WaughEuchreRuntime {
    fn build(_max_cards_played: usize) -> Self {
        const N_HAND: usize = 5;
        let mut waugh_black = Vec::with_capacity(N_HAND + 1);
        let mut waugh_red = Vec::with_capacity(N_HAND + 1);
        let mut waugh_black_size = Vec::with_capacity(N_HAND + 1);
        let mut waugh_red_size = Vec::with_capacity(N_HAND + 1);

        for k in 0..=N_HAND {
            // Black indexer: k hand cards + 1 face_up. For k = 0 there
            // are no hand cards so we collapse to a 1-card indexer over
            // the face_up alone.
            let black_indexer = if k == 0 {
                HandIndexer::init(&[1]).expect("Waugh black init k=0")
            } else {
                HandIndexer::init(&[k as u8, 1]).expect("Waugh black init")
            };
            let black_size = if k == 0 {
                black_indexer.size(0)
            } else {
                black_indexer.size(1)
            };
            waugh_black_size.push(black_size);
            waugh_black.push(black_indexer);

            // Red indexer: (5 - k) hand cards. For 5 - k = 0 we use a
            // dummy indexer and a slot count of 1 (the empty red hand
            // iso class).
            let red_count = N_HAND - k;
            let (red_indexer, red_size) = if red_count == 0 {
                (
                    HandIndexer::init(&[1]).expect("Waugh red dummy init"),
                    1u64,
                )
            } else {
                let idx = HandIndexer::init(&[red_count as u8]).expect("Waugh red init");
                let sz = idx.size(0);
                (idx, sz)
            };
            waugh_red_size.push(red_size);
            waugh_red.push(red_indexer);
        }

        // Combo offsets: cumulative (black × red) sizes across k.
        let mut combo_offsets = Vec::with_capacity(N_HAND + 2);
        let mut running = 0u64;
        for k in 0..=N_HAND {
            combo_offsets.push(running);
            running += waugh_black_size[k] * waugh_red_size[k];
        }
        combo_offsets.push(running);
        let bid_state_size = running;

        // Per-bid-state offsets within a shard. Phase 2 only fills
        // R1Pending / R2Pending; other variants are still panic-stubbed
        // but we reserve space for them so the layout is stable.
        let mut bid_state_offsets = [0u64; EuchreBidState::COUNT];
        let mut running = 0u64;
        for slot in bid_state_offsets.iter_mut() {
            *slot = running;
            running += bid_state_size;
        }
        let shard_size = running;
        let total = 6 * shard_size;

        Self {
            waugh_black,
            waugh_red,
            waugh_black_size,
            waugh_red_size,
            combo_offsets,
            bid_state_size,
            bid_state_offsets,
            shard_size,
            total,
        }
    }
}

/// Map the off-color jack to the trump suit when `trump` is declared.
///
/// In Euchre the J of the same color as the trump suit (the "L-Bauer") is
/// treated as a trump card. For the Waugh canonicalisation to capture the
/// L-Bauer's true-trump status, the card's suit label must be reassigned
/// from its native suit to the trump suit before being fed to
/// `HandIndexer`. This is a card-level rewrite: rank stays the same,
/// `suit` becomes the trump's suit.
///
/// Returns the input cards with L-Bauer remapped (or unchanged if no trump
/// is declared yet).
#[allow(dead_code)] // wired up in Phase 2
pub fn apply_l_bauer(cards: &[ECard], trump: Option<ESuit>) -> Vec<ECard> {
    let Some(trump) = trump else {
        return cards.to_vec();
    };
    let same_color_jack = match trump {
        ESuit::Spades => Some(ECard::JC),
        ESuit::Clubs => Some(ECard::JS),
        ESuit::Hearts => Some(ECard::JD),
        ESuit::Diamonds => Some(ECard::JH),
    };
    let trump_jack = match trump {
        ESuit::Spades => ECard::JS,
        ESuit::Clubs => ECard::JC,
        ESuit::Hearts => ECard::JH,
        ESuit::Diamonds => ECard::JD,
    };
    cards
        .iter()
        .map(|&c| {
            if Some(c) == same_color_jack {
                trump_jack
            } else {
                c
            }
        })
        .collect()
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

#[cfg(test)]
mod waugh_euchre_tests {
    use super::*;
    use games::gamestates::euchre::actions::{Card as ECard, Suit as ESuit};

    /// `EuchreBidState::to_idx` is bijective on [0, COUNT). All 16 distinct
    /// variants land at distinct indices in [0, 20).
    #[test]
    fn euchre_bid_state_indices_unique() {
        let mut seen = std::collections::HashSet::new();
        let states: Vec<EuchreBidState> = (0..4u8)
            .map(|p| EuchreBidState::R1Pending { passes_so_far: p })
            .chain((0..4u8).map(|r| EuchreBidState::R2Pending { r2_passes: r }))
            .chain((0..4u8).map(|s| EuchreBidState::Discard { r1_pickup_seat: s }))
            .chain((0..4u8).flat_map(|s| {
                [true, false]
                    .into_iter()
                    .map(move |v| EuchreBidState::Alone { caller_seat: s, via_pickup: v })
            }))
            .collect();
        assert_eq!(states.len(), EuchreBidState::COUNT);
        for s in &states {
            let idx = s.to_idx();
            assert!(idx < EuchreBidState::COUNT, "idx {} out of range", idx);
            assert!(seen.insert(idx), "collision at idx {}: {:?}", idx, s);
        }
        assert_eq!(seen.len(), EuchreBidState::COUNT);
    }

    /// L-Bauer preprocessor: with spades trump, the JC (clubs jack, the
    /// off-color jack) gets remapped to JS. Other cards untouched. With
    /// no trump declared, all cards untouched.
    #[test]
    fn l_bauer_remaps_off_color_jack() {
        // Spades trump → JC becomes JS.
        let hand = [ECard::JC, ECard::AH, ECard::KS, ECard::TD, ECard::AS];
        let out = apply_l_bauer(&hand, Some(ESuit::Spades));
        assert_eq!(out, vec![ECard::JS, ECard::AH, ECard::KS, ECard::TD, ECard::AS]);

        // Diamonds trump → JH becomes JD.
        let hand = [ECard::JH, ECard::QH];
        let out = apply_l_bauer(&hand, Some(ESuit::Diamonds));
        assert_eq!(out, vec![ECard::JD, ECard::QH]);

        // No trump → no remap.
        let hand = [ECard::JC, ECard::JS, ECard::JH, ECard::JD];
        let out = apply_l_bauer(&hand, None);
        assert_eq!(out, hand.to_vec());

        // Trump-suit jack is unchanged (no off-color match).
        let hand = [ECard::JS]; // already trump's jack
        let out = apply_l_bauer(&hand, Some(ESuit::Spades));
        assert_eq!(out, vec![ECard::JS]);
    }

    /// The constructor builds a non-empty runtime and reports a positive
    /// slot count.
    #[test]
    fn waugh_euchre_runtime_builds() {
        let idx = WaughEuchreIndexer::new(0);
        assert!(idx.len() > 0);
        let idx = WaughEuchreIndexer::new(1);
        assert!(idx.len() > 0);
    }

    /// `euchre_istate_entry_to_waugh_card` is bijective on the 24 valid
    /// Euchre card bit indices: distinct cards map to distinct Waugh
    /// encodings.
    #[test]
    fn euchre_to_waugh_card_bijection() {
        let mut seen = std::collections::HashSet::new();
        // Card bit indices: suit (0..4) × rank-in-suit (0..6).
        for suit in 0..4u8 {
            for rank in 0..6u8 {
                let bit = suit * 8 + rank;
                let w = euchre_istate_entry_to_waugh_card(Action(bit));
                assert!(
                    seen.insert(w),
                    "collision at suit={} rank={} bit={} → w={}",
                    suit, rank, bit, w
                );
            }
        }
        assert_eq!(seen.len(), 24);
    }

    /// Walk every R1Pending / R2Pending istate the Euchre iterator
    /// emits for the NS face_up shard. With the D₄ color-split indexer,
    /// the iterator's iso reduction = our iso reduction, so we expect:
    ///   * Every Waugh slot is in `[0, indexer.len())`.
    ///   * **Bijection** — every iterator emission gets a unique Waugh
    ///     slot.
    #[test]
    fn waugh_euchre_r1_r2_bijection_ns_shard() {
        use games::gamestates::euchre::{
            actions::EAction, iterator::EuchreIsomorphicIStateIterator,
        };

        let indexer = WaughEuchreIndexer::new(0);
        let total = indexer.len();

        let mut slots: std::collections::HashSet<u64> = std::collections::HashSet::new();
        let mut r1_r2_count = 0usize;

        for istate in EuchreIsomorphicIStateIterator::with_face_up(0, &[EAction::NS]) {
            let bid_state = parse_euchre_bid_state(&istate, 5);
            let in_phase = matches!(
                bid_state,
                Some(EuchreBidState::R1Pending { .. })
                    | Some(EuchreBidState::R2Pending { .. })
            );
            if !in_phase {
                continue;
            }

            let slot = indexer.index(&istate);
            assert!(
                slot < total,
                "slot {} >= total {} for istate {:?}",
                slot, total, istate
            );
            slots.insert(slot);
            r1_r2_count += 1;
        }

        assert!(r1_r2_count > 0, "iterator emitted zero R1/R2 istates");
        // D₄ design: iterator's iso reduction == ours. We expect
        // bijection — each iterator emission gets a unique Waugh slot.
        assert_eq!(
            slots.len(),
            r1_r2_count,
            "expected bijection (D₄ iso match); got unique slots={} \
             but emissions={}",
            slots.len(),
            r1_r2_count
        );
    }

    /// Iso-on-raw under D₄ (the 8 color-preserving suit permutations).
    /// Random R1/R2 game states under each D₄ element should land on
    /// the same Waugh slot. This is the correctness invariant for the
    /// color-split indexer.
    #[test]
    fn waugh_euchre_r1_r2_iso_under_d4() {
        use games::gamestates::euchre::{actions::EAction, EPhase, Euchre};
        use games::GameState;
        let _ = EAction::Pass; // touch import to silence unused

        let indexer = WaughEuchreIndexer::new(0);

        // The 8 elements of D₄ = preserve color partition {{S,C},{H,D}}.
        // Each perm sends each suit somewhere consistent with the
        // partition: black ↔ black or black ↔ red (as a pair).
        fn perms() -> Vec<[u8; 4]> {
            vec![
                [0, 1, 2, 3], // identity
                [1, 0, 2, 3], // swap_black: S↔C
                [0, 1, 3, 2], // swap_red: H↔D
                [1, 0, 3, 2], // swap_both
                [2, 3, 0, 1], // swap_color: S↔H, C↔D
                [3, 2, 0, 1], // swap_color × swap_black
                [2, 3, 1, 0], // swap_color × swap_red
                [3, 2, 1, 0], // swap_color × swap_both
            ]
        }

        fn perm_action(a: Action, perm: &[u8; 4]) -> Action {
            // Card bit indices: suit * 8 + rank_in_suit ∈ [0, 6).
            // Non-card bits (Pickup/Pass/etc.) stay put.
            let bit = a.0;
            if bit >= 32 {
                return a;
            }
            let suit = bit / 8;
            let rank = bit % 8;
            if rank < 6 {
                // Real card — remap suit.
                Action((perm[suit as usize]) * 8 + rank)
            } else {
                // Suit-call bit (Spades/Clubs/Hearts/Diamonds at rank=6) —
                // also remaps to the permuted suit. Pickup/Pass/etc. live
                // at rank=7 across suits and shouldn't be permuted (they
                // happen to be the SAME action regardless of suit).
                if rank == 6 {
                    Action((perm[suit as usize]) * 8 + rank)
                } else {
                    a
                }
            }
        }

        let all_perms = perms();
        let mut rng: u64 = 0xA1B2C3D4;
        let mut next = || {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            rng
        };

        for _ in 0..50 {
            let seed = next() as usize;
            // Build a random game and stop at an R1/R2 decision.
            let mut gs = Euchre::new_state();
            let mut acts = Vec::new();
            let mut step = 0usize;
            let mut reached_decision = false;
            while !gs.is_terminal() {
                gs.legal_actions(&mut acts);
                let pick = (seed.wrapping_add(step.wrapping_mul(73))) % acts.len();
                gs.apply_action(acts[pick]);
                step += 1;
                if !gs.is_chance_node() {
                    // Stop at Pickup or ChooseTrump phase.
                    let p = gs.phase();
                    if p == EPhase::Pickup || p == EPhase::ChooseTrump {
                        reached_decision = true;
                        break;
                    }
                }
            }
            if !reached_decision {
                continue;
            }

            let perspective = gs.cur_player();
            let raw = gs.istate_key(perspective);
            // Sanity: parse_euchre_bid_state should return R1/R2.
            let bs = parse_euchre_bid_state(&raw, 5);
            if !matches!(
                bs,
                Some(EuchreBidState::R1Pending { .. })
                    | Some(EuchreBidState::R2Pending { .. })
            ) {
                continue;
            }
            let slot_raw = indexer.index(&raw);

            for perm in &all_perms {
                // Apply σ to every action in `raw` to build a permuted
                // istate. The bid-state and shard should be invariant
                // under σ (no card actions in the R1/R2 tail).
                let mut permuted = IStateKey::default();
                for a in raw.iter() {
                    permuted.push(perm_action(*a, perm));
                }
                // Re-sort the hand block since suit perm shuffles ranks.
                permuted.sort_range(0, 5.min(permuted.len()));
                let slot_perm = indexer.index(&permuted);
                assert_eq!(
                    slot_raw, slot_perm,
                    "iso violation: perm={:?} raw={:?} permuted={:?}",
                    perm, raw, permuted
                );
            }
        }
    }

    /// Negative control: under a non-D₄ permutation (single S↔H swap,
    /// which breaks the color partition), iso must NOT hold for at
    /// least some istates — otherwise our reduction would be coarser
    /// than D₄ and over-collapsing again.
    #[test]
    fn waugh_euchre_r1_r2_breaks_iso_under_non_d4() {
        use games::gamestates::euchre::{EPhase, Euchre};
        use games::GameState;

        let indexer = WaughEuchreIndexer::new(0);
        let non_d4: [u8; 4] = [2, 1, 0, 3]; // S↔H only (not D₄)

        fn perm_action(a: Action, perm: &[u8; 4]) -> Action {
            let bit = a.0;
            if bit >= 32 {
                return a;
            }
            let suit = bit / 8;
            let rank = bit % 8;
            if rank <= 6 {
                Action(perm[suit as usize] * 8 + rank)
            } else {
                a
            }
        }

        let mut rng: u64 = 0xDEADBEEF;
        let mut next = || {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            rng
        };

        let mut differing_count = 0usize;
        let mut tested = 0usize;
        for _ in 0..200 {
            let seed = next() as usize;
            let mut gs = Euchre::new_state();
            let mut acts = Vec::new();
            let mut step = 0usize;
            let mut reached = false;
            while !gs.is_terminal() {
                gs.legal_actions(&mut acts);
                let pick = (seed.wrapping_add(step.wrapping_mul(73))) % acts.len();
                gs.apply_action(acts[pick]);
                step += 1;
                if !gs.is_chance_node() {
                    let p = gs.phase();
                    if p == EPhase::Pickup || p == EPhase::ChooseTrump {
                        reached = true;
                        break;
                    }
                }
            }
            if !reached {
                continue;
            }
            let perspective = gs.cur_player();
            let raw = gs.istate_key(perspective);
            let bs = parse_euchre_bid_state(&raw, 5);
            if !matches!(
                bs,
                Some(EuchreBidState::R1Pending { .. })
                    | Some(EuchreBidState::R2Pending { .. })
            ) {
                continue;
            }
            let slot_raw = indexer.index(&raw);
            let mut permuted = IStateKey::default();
            for a in raw.iter() {
                permuted.push(perm_action(*a, &non_d4));
            }
            permuted.sort_range(0, 5.min(permuted.len()));
            let slot_perm = indexer.index(&permuted);
            if slot_raw != slot_perm {
                differing_count += 1;
            }
            tested += 1;
        }

        assert!(tested > 0, "couldn't sample any R1/R2 istates");
        assert!(
            differing_count > 0,
            "non-D₄ perm (S↔H only) produced same slot on every test ({} samples); \
             our reduction is too coarse",
            tested
        );
    }
}
