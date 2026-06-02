//! Proof-of-concept: direct Waugh-based istate→slot indexer for OH,
//! verified against the existing iterator's enumeration.
//!
//! For each (num_players, n_tricks, max_cards_played) config:
//!   1. Build a WaughOhIndexer.
//!   2. Enumerate iso-canonical istates via the existing iterator.
//!   3. For each, compute Waugh slot — assert in-range + unique.
//!   4. Report slot utilisation (used vs total).

use games::{
    gamestates::oh_hell::{
        actions::{OHCard, OH_DECK_SIZE},
        iterator::OhHellIsomorphicIStateIterator,
    },
    iso::hand_indexer::{HandIndexer, IndexerState},
    istate::IStateKey,
};

fn oh_to_waugh(c: OHCard) -> u8 {
    let d = c as u8;
    let suit = d / 13;
    let rank = d % 13;
    (rank << 2) | suit
}

struct WaughOhIndexer {
    num_players: usize,
    n_tricks: usize,
    max_cards: usize,
    bid_base: u64,
    bid_full: u64,
    waugh: HandIndexer,
    bidding_offsets: Vec<u64>,
    bidding_size: u64,
    depth_offsets: Vec<u64>,
    total: u64,
}

struct Parsed {
    hand: Vec<OHCard>,
    face_up: OHCard,
    bids: Vec<u8>,
    plays: Vec<OHCard>,
}

impl WaughOhIndexer {
    fn new(num_players: usize, n_tricks: usize, max_cards: usize) -> Self {
        let bid_base = (n_tricks + 1) as u64;
        let bid_full = bid_base.pow(num_players as u32);

        let mut rounds = vec![n_tricks as u8, 1];
        for _ in 0..max_cards {
            rounds.push(1);
        }
        let waugh = HandIndexer::init(&rounds).expect("init");
        let waugh_size_1 = waugh.size(1);

        let mut bidding_offsets = Vec::with_capacity(num_players + 1);
        let mut running = 0u64;
        for p in 0..num_players {
            bidding_offsets.push(running);
            running += bid_base.pow(p as u32) * waugh_size_1;
        }
        bidding_offsets.push(running);
        let bidding_size = running;

        let mut depth_offsets = Vec::with_capacity(max_cards + 1);
        let mut running = 0u64;
        for d in 0..max_cards {
            depth_offsets.push(running);
            running += bid_full * waugh.size(1 + d);
        }
        depth_offsets.push(running);

        let total = bidding_size + running;

        Self {
            num_players, n_tricks, max_cards,
            bid_base, bid_full,
            waugh,
            bidding_offsets, bidding_size,
            depth_offsets, total,
        }
    }

    fn parse(&self, istate: &IStateKey) -> Parsed {
        let mut hand = Vec::with_capacity(self.n_tricks);
        for i in 0..self.n_tricks {
            hand.push(OHCard::from_index(istate[i].0).unwrap());
        }
        let face_up = OHCard::from_index(istate[self.n_tricks].0).unwrap();
        let mut bids = Vec::new();
        let mut plays = Vec::new();
        for i in (self.n_tricks + 1)..istate.len() {
            let d = istate[i].0;
            if d >= OH_DECK_SIZE as u8 {
                bids.push(d - OH_DECK_SIZE as u8);
            } else {
                plays.push(OHCard::from_index(d).unwrap());
            }
        }
        Parsed { hand, face_up, bids, plays }
    }

    fn encode_bids(&self, bids: &[u8]) -> u64 {
        let mut idx = 0u64;
        let mut mul = 1u64;
        for &b in bids {
            idx += b as u64 * mul;
            mul *= self.bid_base;
        }
        idx
    }

    fn waugh_idx_through_round(&self, parsed: &Parsed, target_round: usize) -> u64 {
        // Walk Waugh round-by-round up to and including `target_round`,
        // using `next_round` directly. We can't call `index_all` because
        // the indexer was built with extra play rounds (to support the
        // deepest training depth); when an istate hasn't filled those
        // rounds yet, we stop early. cards-per-round layout:
        //   round 0 = n_tricks (hand)
        //   round 1 = 1 (face_up)
        //   round 2+ = 1 (each play)
        let mut state = IndexerState::new();
        let mut idx = 0u64;

        // Round 0: hand cards.
        let hand_cards: Vec<u8> = parsed.hand.iter().map(|c| oh_to_waugh(*c)).collect();
        idx = self.waugh.next_round(&hand_cards, &mut state);
        if target_round == 0 {
            return idx;
        }

        // Round 1: face_up.
        idx = self.waugh.next_round(&[oh_to_waugh(parsed.face_up)], &mut state);
        if target_round == 1 {
            return idx;
        }

        // Rounds 2..=target_round: one play card each.
        for d in 0..(target_round - 1) {
            idx = self.waugh.next_round(&[oh_to_waugh(parsed.plays[d])], &mut state);
        }
        idx
    }

    fn index(&self, istate: &IStateKey) -> u64 {
        let parsed = self.parse(istate);
        if parsed.plays.is_empty() && parsed.bids.len() < self.num_players {
            let perspective = parsed.bids.len();
            let bid_idx = self.encode_bids(&parsed.bids);
            let waugh_idx = self.waugh_idx_through_round(&parsed, 1);
            let waugh_size_1 = self.waugh.size(1);
            self.bidding_offsets[perspective] + bid_idx * waugh_size_1 + waugh_idx
        } else {
            let depth = parsed.plays.len();
            assert!(
                depth < self.max_cards,
                "depth {} >= max {} unsupported (this iter shouldn't be emitted)",
                depth, self.max_cards
            );
            let bid_idx = self.encode_bids(&parsed.bids);
            let waugh_idx = self.waugh_idx_through_round(&parsed, 1 + depth);
            let waugh_size_d = self.waugh.size(1 + depth);
            self.bidding_size
                + self.depth_offsets[depth]
                + bid_idx * waugh_size_d
                + waugh_idx
        }
    }

    fn len(&self) -> u64 {
        self.total
    }
}

fn verify_config(np: usize, nt: usize, max_cards: usize) {
    let indexer = WaughOhIndexer::new(np, nt, max_cards);
    let iter = OhHellIsomorphicIStateIterator::full_game_via_waugh(np, nt, max_cards);
    let istates: Vec<IStateKey> = iter.into_iter().collect();

    let mut slots: std::collections::HashSet<u64> = std::collections::HashSet::new();
    let mut max_seen = 0u64;
    let mut collision_example: Option<(IStateKey, u64)> = None;
    let mut out_of_range_example: Option<(IStateKey, u64)> = None;

    for istate in &istates {
        let slot = indexer.index(istate);
        if slot >= indexer.len() {
            if out_of_range_example.is_none() {
                out_of_range_example = Some((*istate, slot));
            }
            continue;
        }
        max_seen = max_seen.max(slot);
        if !slots.insert(slot) {
            if collision_example.is_none() {
                collision_example = Some((*istate, slot));
            }
        }
    }

    let mark = if collision_example.is_none() && out_of_range_example.is_none() {
        "✓"
    } else {
        "✗"
    };
    println!(
        "{} {}p_{}t_max{}: iter={} slots_used={}/{} max_slot={}{}",
        mark, np, nt, max_cards,
        istates.len(), slots.len(), indexer.len(), max_seen,
        if let Some((_, s)) = collision_example {
            format!("  COLLISION at slot={}", s)
        } else if let Some((_, s)) = out_of_range_example {
            format!("  OUT_OF_RANGE slot={} >= {}", s, indexer.len())
        } else {
            String::new()
        }
    );
}

/// Verify iso equivalence on raw (non-canonicalised) istates: for each
/// pair of iterator-emitted istates, verify Waugh slot is unique. Then
/// for a sample of (raw game state, suit permutation σ) pairs, verify
/// the raw and σ-permuted istates map to the same Waugh slot.
fn verify_iso_on_raw(np: usize, nt: usize, max_cards: usize) {
    use games::{
        gamestates::oh_hell::{
            actions::{OHAction, OHCard, OHSuit, OH_DECK, OH_DECK_SIZE},
            OhHell,
        },
        Action, GameState,
    };

    let indexer = WaughOhIndexer::new(np, nt, max_cards);
    let mut rng = 0xC0FFEEu64;
    let mut next_rand = || {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        rng
    };
    fn perm_card(c: OHCard, perm: &[u8; 4]) -> OHCard {
        let d = c as u8;
        let suit = (d / 13) as usize;
        let rank = d % 13;
        OHCard::from_index((perm[suit] as u8) * 13 + rank).unwrap()
    }

    // Random suit permutations.
    let perms: Vec<[u8; 4]> = vec![
        [0, 1, 2, 3], // identity
        [1, 0, 2, 3], // swap S↔C
        [2, 3, 0, 1], // swap (S↔H, C↔D)
        [3, 2, 1, 0], // reverse
        [1, 2, 3, 0], // cyclic
    ];

    let mut tested = 0usize;
    let mut mismatches = 0usize;

    // Generate a handful of game states, get istate from each perspective,
    // for each perspective × suit_perm σ verify slot stability.
    let n_games = 200;
    for _ in 0..n_games {
        let r = next_rand();
        let seed = r as usize;
        // Build a random game by playing through to a bidding or play decision.
        let mut gs = OhHell::new_state(np, nt);
        let mut acts = Vec::new();
        let mut step = 0usize;
        while !gs.is_terminal() {
            gs.legal_actions(&mut acts);
            let pick = (seed.wrapping_add(step.wrapping_mul(73))) % acts.len();
            gs.apply_action(acts[pick]);
            step += 1;
            // Stop when we reach a CFR-relevant decision: bidding or
            // early-play (depth < max_cards).
            let np_u8 = np as u8;
            let _ = np_u8;
            if !gs.is_chance_node() {
                // Check phase: if play and depth >= max_cards, advance.
                use games::gamestates::oh_hell::OHPhase;
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

        for perm in &perms {
            // Build a permuted game state by applying perm to every card
            // action and replay.
            let mut gs_p = OhHell::new_state(np, nt);
            let history = gs.key();
            for a in history.iter().copied() {
                let oa = OHAction::from(a);
                let new_a = match oa {
                    OHAction::Card(c) => OHAction::Card(perm_card(c, perm)).into(),
                    OHAction::Bid(_) => a,
                };
                gs_p.apply_action(new_a);
            }
            let raw_perm = gs_p.istate_key(perspective);

            let slot_raw = indexer.index(&raw);
            let slot_perm = indexer.index(&raw_perm);

            if slot_raw != slot_perm {
                if mismatches < 3 {
                    println!(
                        "  mismatch ({}p_{}t_max{}): perm={:?} slot_raw={} slot_perm={} (perspective {})",
                        np, nt, max_cards, perm, slot_raw, slot_perm, perspective
                    );
                }
                mismatches += 1;
            }
            tested += 1;
        }
    }
    let mark = if mismatches == 0 { "✓" } else { "✗" };
    println!(
        "{} iso_raw {}p_{}t_max{}: tested {} (perspective, σ) pairs; mismatches={}",
        mark, np, nt, max_cards, tested, mismatches
    );
}

fn main() {
    let configs = [
        (2, 1, 0),
        (2, 2, 0), (2, 2, 1), (2, 2, 2),
        (2, 3, 0), (2, 3, 1),
        (3, 1, 0),
        (3, 2, 0), (3, 2, 1),
        (3, 3, 0), (3, 3, 1),
    ];
    for (np, nt, m) in configs {
        verify_config(np, nt, m);
    }
    println!();
    println!("=== iso equivalence on raw (non-canonical) istates ===");
    for (np, nt, m) in configs {
        verify_iso_on_raw(np, nt, m);
    }
}
