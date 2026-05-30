//! `IStateNormalizer` for Oh Hell.
//!
//! Collapses CFR istate keys that differ only by a non-trump suit
//! permutation into a single canonical key, so MCCFR's HashMap-backed
//! store shares regrets across the symmetry class instead of paying
//! for each labelling separately.
//!
//! ## How it works
//!
//! For each non-trump suit, compute a fingerprint that captures
//! everything iso-invariant about how cards in that suit appear in
//! the perspective player's istate. Sort the three non-trump suits
//! by this fingerprint ascending; that determines the canonical
//! relabelling. Trump is pinned to canonical slot 0 (Spades).
//!
//! The fingerprint has four parts, packed into a `u128` so they
//! sort lex-style:
//!
//! ```text
//!   bits   0..13  : hand-in-suit rank mask
//!   bits  13..26  : played-in-suit rank mask
//!   bits  26..30  : hand-in-suit count (popcount of the hand mask)
//!   bits  30..34  : played-in-suit count
//!   bits  64..96  : first-play-position (or u32::MAX if no plays)
//! ```
//!
//! The first-play-position is the **key** fix relative to the
//! purely-set-based fingerprint we shipped first. Without it, two
//! states that differ only in the *order* plays were made share the
//! same fingerprint (same final card distribution) and therefore the
//! same perm, leaving the order in the canonical IStateKey — which
//! produces two distinct canonical forms for one true iso class.
//!
//! The 3p 1-trick repro (uncovered by cross-checking against the
//! Waugh `HandIndexer` in `crate::iso::hand_indexer`):
//!
//! ```text
//!   hand=5♠, face_up=TS (trump),  bids=[1, 0, 0]
//!   Scenario A: plays = [TD, TH]   (Diamonds played first)
//!   Scenario B: plays = [TH, TD]   (Hearts played first)
//! ```
//!
//! A and B are related by the Hearts↔Diamonds swap (a valid OH iso
//! symmetry — trump is Spades). They must collapse to one
//! canonical IStateKey. With the set-only fingerprint Diamonds and
//! Hearts had identical fingerprints in both scenarios, so the perm
//! came out the same and the order remained — two distinct canonical
//! forms. With `first_play_position` included, Diamonds has
//! position 0 in A but position 1 in B (vice versa for Hearts), so
//! the perm differs between A and B in exactly the way needed to
//! make them produce the same canonical sequence.
//!
//! ## Other notes
//!
//! Why no rank compaction within a suit: any non-trivial rank
//! permutation within a suit would have to (a) be order-preserving
//! and (b) map Hand↔Hand, Played↔Played, Face-up↔Face-up,
//! Unknown↔Unknown set-wise. For a strict total order the only such
//! bijection is the identity, so rank-compaction within a suit
//! doesn't exist for a strict iso of a CFR istate. (The
//! `OpenHandSolver` TT does rank-compact, but only because it sees
//! the full game state, including "card is literally out of the
//! game" None positions — which behave like Euchre's `iso_deck`
//! swap_loc.) The per-rank category sequence is the absolute
//! ceiling for strict iso reduction; this implementation hits it
//! once the play-order ambiguity is resolved.
//!
//! Further reduction requires *approximate* iso (poker-style card
//! abstraction), which is a separate design choice with a real
//! policy-quality cost.
//!
//! Code structure mirrors `gamestates/euchre/isomorphic.rs` — same
//! `IStateNormalizer` trait, same "compute perm from gs, then apply
//! per-action" pattern.

use crate::{
    gamestates::oh_hell::{
        actions::{OHAction, OHSuit, OH_DECK_SIZE},
        OhHellGameState, SUIT_MASK,
    },
    istate::{IStateKey, IStateNormalizer, NormalizedAction, NormalizedIstate},
    Action, GameState,
};

/// Identity for cases where we can't (yet) safely canonicalise — i.e. the
/// face-up card hasn't been dealt so trump isn't yet known.
const IDENTITY: [u8; 4] = [0, 1, 2, 3];

#[derive(Default, Clone)]
pub struct OhHellNormalizer;

impl IStateNormalizer<OhHellGameState> for OhHellNormalizer {
    fn normalize_action(&self, action: Action, gs: &OhHellGameState) -> NormalizedAction {
        let perm = compute_perm(gs);
        NormalizedAction::new(apply_perm(action, &perm))
    }

    fn denormalize_action(&self, action: NormalizedAction, gs: &OhHellGameState) -> Action {
        let perm = compute_perm(gs);
        let inv = invert_perm(&perm);
        apply_perm(action.get(), &inv)
    }

    fn normalize_istate(
        &self,
        istate: &IStateKey,
        gs: &OhHellGameState,
    ) -> NormalizedIstate {
        let perm = compute_perm(gs);
        NormalizedIstate::new(apply_perm_to_istate(istate, &perm, gs.n_tricks()))
    }
}

/// Compute the canonical suit permutation: `perm[old_suit] = new_suit`.
///
/// - `perm[trump_suit] = 0` (Spades is the canonical trump slot).
/// - The other three suits are sorted by an iso-invariant fingerprint
///   and assigned slots 1, 2, 3 in order.
///
/// The fingerprint depends only on data visible from the istate alone,
/// so the perm is invariant under any non-trump suit relabelling.
pub fn compute_perm(gs: &OhHellGameState) -> [u8; 4] {
    let Some(trump) = gs.trump_suit() else {
        return IDENTITY;
    };

    let perspective = gs.cur_player();
    let hand = gs.hand_mask(perspective);
    let played = gs.played_mask();
    let n_tricks = gs.n_tricks();
    let istate = gs.istate_key(perspective);

    // Walk the play portion of the istate to record the first-play
    // position per suit. The istate layout is:
    //   [0..n_tricks)         hand cards (in some sorted order)
    //   [n_tricks]            face-up
    //   [n_tricks+1..]        bids (discriminant ≥ OH_DECK_SIZE) and
    //                         plays (discriminant < OH_DECK_SIZE),
    //                         interleaved-but-actually-bids-first
    //                         since OH transitions through bidding
    //                         entirely before play starts.
    //
    // The play position we record is the index *among play cards
    // only* — bids don't shift it. `u32::MAX` = no play seen in
    // that suit yet (sorts after every real position).
    let mut first_play_pos: [u32; 4] = [u32::MAX; 4];
    let mut play_idx: u32 = 0;
    for a in istate.iter().skip(n_tricks + 1) {
        let d = a.0;
        if (d as usize) >= OH_DECK_SIZE {
            // Bid action — no card revealed, position unchanged.
            continue;
        }
        let suit = (d / 13) as usize;
        if first_play_pos[suit] == u32::MAX {
            first_play_pos[suit] = play_idx;
        }
        play_idx += 1;
    }

    // For each non-trump suit, compute the fingerprint. Packed as
    // u128 so the lex comparison on the tuple comes out in one
    // primitive comparison.
    let fingerprint = |suit_idx: usize| -> u128 {
        let suit_full = SUIT_MASK[suit_idx];
        let suit_base = (suit_idx as u64) * 13;
        let hand_in_suit = ((hand & suit_full) >> suit_base) & 0x1FFF;
        let played_in_suit = ((played & suit_full) >> suit_base) & 0x1FFF;
        let hand_count = hand_in_suit.count_ones() as u64;
        let played_count = played_in_suit.count_ones() as u64;
        let lo: u64 = hand_in_suit
            | (played_in_suit << 13)
            | (hand_count << 26)
            | (played_count << 30);
        let hi: u64 = first_play_pos[suit_idx] as u64;
        (lo as u128) | ((hi as u128) << 64)
    };

    // Gather (old_suit_idx, fingerprint) for the three non-trump suits.
    let trump_idx = trump as usize;
    let mut nontrump: [(u8, u128); 3] = [(0, 0); 3];
    let mut k = 0;
    for s in 0..4 {
        if s != trump_idx {
            nontrump[k] = (s as u8, fingerprint(s));
            k += 1;
        }
    }
    // Sort by fingerprint ascending, tie-broken by old index for
    // determinism.
    nontrump.sort_by_key(|&(idx, fp)| (fp, idx));

    let mut perm = [0u8; 4];
    perm[trump_idx] = 0;
    perm[nontrump[0].0 as usize] = 1;
    perm[nontrump[1].0 as usize] = 2;
    perm[nontrump[2].0 as usize] = 3;
    perm
}

/// Apply the suit permutation to a single Action. Card actions get
/// their suit-bits relabelled (same rank, new suit); Bid actions pass
/// through.
#[inline]
pub fn apply_perm(action: Action, perm: &[u8; 4]) -> Action {
    let oa = OHAction::from(action);
    match oa {
        OHAction::Card(c) => {
            let id = c as u8;
            let suit_idx = (id / 13) as usize;
            let rank_idx = id % 13;
            let new_suit = perm[suit_idx];
            let new_id = new_suit * 13 + rank_idx;
            debug_assert!((new_id as usize) < OH_DECK_SIZE);
            Action(new_id)
        }
        OHAction::Bid(_) => action,
    }
}

/// Apply the suit permutation to an entire istate key, re-sorting
/// the perspective's hand segment afterwards (sort order changes
/// when suit discriminants shift).
pub fn apply_perm_to_istate(
    istate: &IStateKey,
    perm: &[u8; 4],
    n_tricks: usize,
) -> IStateKey {
    let mut out = IStateKey::default();
    for a in istate.iter() {
        out.push(apply_perm(*a, perm));
    }
    let n_hand = n_tricks.min(out.len());
    out.sort_range(0, n_hand);
    out
}

/// Compute the inverse permutation: if `perm[i] = j` then `inv[j] = i`.
#[inline]
pub fn invert_perm(perm: &[u8; 4]) -> [u8; 4] {
    let mut inv = [0u8; 4];
    for (old, &new) in perm.iter().enumerate() {
        inv[new as usize] = old as u8;
    }
    inv
}

/// Sanity helper: the canonical trump after normalisation is always
/// `OHSuit::Spades`. Used internally by tests.
#[allow(dead_code)]
pub(crate) fn canonical_trump() -> OHSuit {
    OHSuit::Spades
}

#[allow(dead_code)]
pub(crate) fn debug_perm(gs: &OhHellGameState) -> [u8; 4] {
    compute_perm(gs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        actions,
        gamestates::oh_hell::{actions::OHCard, OHPhase, OhHell},
    };
    use rand::{rngs::StdRng, seq::IndexedRandom, RngExt, SeedableRng};

    fn play_random_to_phase(seed: u64, stop_in_bidding: bool) -> (OhHellGameState, Vec<Action>) {
        let mut rng: StdRng = SeedableRng::seed_from_u64(seed);
        let mut gs = OhHell::new_state(2, 2);
        let mut history = Vec::new();
        while !gs.is_terminal() {
            if !gs.is_chance_node()
                && (gs.phase() == OHPhase::Play || stop_in_bidding)
            {
                break;
            }
            let acts = actions!(gs);
            let a = *acts.choose(&mut rng).unwrap();
            gs.apply_action(a);
            history.push(a);
        }
        (gs, history)
    }

    fn drive_to_play(seed: u64) -> OhHellGameState {
        play_random_to_phase(seed, false).0
    }

    fn rebuild_with_perm(history: &[Action], perm: [u8; 4]) -> OhHellGameState {
        let mut gs = OhHell::new_state(2, 2);
        for a in history {
            gs.apply_action(apply_perm(*a, &perm));
        }
        gs
    }

    /// Two iso-equivalent states (one built by replaying the original
    /// history with a 2-suit perm applied to every card action) must
    /// produce the same normalised istate.
    #[test]
    fn normalize_collapses_suit_perm_istate() {
        let normalizer = OhHellNormalizer;
        let perms: &[[u8; 4]] = &[
            [1, 0, 2, 3], // S↔C
            [2, 1, 0, 3], // S↔H
            [3, 1, 2, 0], // S↔D
            [0, 2, 1, 3], // C↔H
            [0, 3, 2, 1], // C↔D
            [0, 1, 3, 2], // H↔D
        ];
        for seed in 0..50u64 {
            let (gs, history) = play_random_to_phase(seed, false);
            for perm in perms {
                let sibling = rebuild_with_perm(&history, *perm);
                let p_gs = gs.cur_player();
                let p_sib = sibling.cur_player();
                assert_eq!(p_gs, p_sib, "cur_player must agree across iso-rebuild");
                let na = normalizer
                    .normalize_istate(&gs.istate_key(p_gs), &gs)
                    .get();
                let nb = normalizer
                    .normalize_istate(&sibling.istate_key(p_sib), &sibling)
                    .get();
                assert_eq!(
                    na, nb,
                    "normalised istate differs for seed={} perm={:?}",
                    seed, perm
                );
            }
        }
    }

    #[test]
    fn perm_inverse_round_trip_actions() {
        let normalizer = OhHellNormalizer;
        let mut rng: StdRng = SeedableRng::seed_from_u64(0xC0FFEE);
        for _ in 0..50 {
            let gs = drive_to_play(rng.random::<u64>());
            let acts = actions!(gs);
            for &a in acts.iter().take(5) {
                let n = normalizer.normalize_action(a, &gs);
                let d = normalizer.denormalize_action(n, &gs);
                assert_eq!(d, a, "round-trip broken for action {:?}", a);
            }
        }
    }

    #[test]
    fn normalization_preserves_legal_action_count() {
        let normalizer = OhHellNormalizer;
        let mut rng: StdRng = SeedableRng::seed_from_u64(123);
        for _ in 0..50 {
            let gs = drive_to_play(rng.random::<u64>());
            let acts = actions!(gs);
            let normalised: std::collections::HashSet<Action> = acts
                .iter()
                .map(|&a| normalizer.normalize_action(a, &gs).get())
                .collect();
            assert_eq!(
                normalised.len(),
                acts.len(),
                "normalisation collapsed legal actions"
            );
        }
    }

    #[test]
    fn trump_lands_in_canonical_slot() {
        let mut rng: StdRng = SeedableRng::seed_from_u64(7);
        for _ in 0..20 {
            let gs = drive_to_play(rng.random::<u64>());
            let perm = compute_perm(&gs);
            let trump = gs.trump_suit().unwrap();
            assert_eq!(
                perm[trump as usize], 0,
                "trump suit didn't land in slot 0 (perm={:?}, trump={:?})",
                perm, trump
            );
        }
    }

    #[test]
    fn bid_actions_unchanged() {
        let normalizer = OhHellNormalizer;
        let gs = drive_to_play(42);
        let bid: Action = OHAction::Bid(2).into();
        let n = normalizer.normalize_action(bid, &gs);
        assert_eq!(n.get(), bid);
        let d = normalizer.denormalize_action(n, &gs);
        assert_eq!(d, bid);
    }

    #[test]
    fn normalize_distinguishes_different_played_card_ranks() {
        let normalizer = OhHellNormalizer;

        let mut a = OhHell::new_state(2, 2);
        for c in &[OHCard::NS, OHCard::NH, OHCard::JS, OHCard::TH] {
            a.apply_action(OHAction::Card(*c).into());
        }
        a.apply_action(OHAction::Card(OHCard::_2H).into());
        a.apply_action(OHAction::Bid(0).into());
        a.apply_action(OHAction::Bid(0).into());
        a.apply_action(OHAction::Card(OHCard::NS).into());
        a.apply_action(OHAction::Card(OHCard::NH).into());

        let mut b = OhHell::new_state(2, 2);
        for c in &[OHCard::NS, OHCard::NH, OHCard::JS, OHCard::TH] {
            b.apply_action(OHAction::Card(*c).into());
        }
        b.apply_action(OHAction::Card(OHCard::_2H).into());
        b.apply_action(OHAction::Bid(0).into());
        b.apply_action(OHAction::Bid(0).into());
        b.apply_action(OHAction::Card(OHCard::NS).into());
        b.apply_action(OHAction::Card(OHCard::TH).into());

        let p = a.cur_player();
        let na = normalizer
            .normalize_istate(&a.istate_key(p), &a)
            .get();
        let nb = normalizer
            .normalize_istate(&b.istate_key(p), &b)
            .get();
        assert_ne!(
            na, nb,
            "normaliser must distinguish states whose played-card rank differs"
        );
    }

    #[test]
    fn apply_perm_relabels_suit_only() {
        let mut perm = [0u8, 1, 2, 3];
        perm.swap(1, 2);
        let qh: Action = OHCard::QH.into();
        let qc: Action = OHCard::QC.into();
        assert_eq!(apply_perm(qh, &perm), qc);
        assert_eq!(apply_perm(qc, &perm), qh);
        let qs: Action = OHCard::QS.into();
        let qd: Action = OHCard::QD.into();
        assert_eq!(apply_perm(qs, &perm), qs);
        assert_eq!(apply_perm(qd, &perm), qd);
    }

    /// The bug that prompted this rewrite: a Hearts↔Diamonds play-
    /// order swap in 3p × 1-trick that the set-only fingerprint left
    /// in two distinct canonical forms. With first-play-position now
    /// in the fingerprint, both scenarios must collapse to one
    /// canonical IStateKey.
    #[test]
    fn collapses_hearts_diamonds_play_order_swap() {
        let normalizer = OhHellNormalizer;

        // Build the two scenarios as 3p × 1-trick games. Perspective
        // = P2; hand = 5♠; face-up = TS (trump = Spades); bids =
        // [1, 0, 0]; plays differ only in order.
        let mut a = OhHell::new_state(3, 1);
        a.apply_action(OHAction::Card(OHCard::TD).into()); // P0 deal
        a.apply_action(OHAction::Card(OHCard::TH).into()); // P1 deal
        a.apply_action(OHAction::Card(OHCard::_5S).into()); // P2 deal
        a.apply_action(OHAction::Card(OHCard::TS).into()); // face-up
        a.apply_action(OHAction::Bid(1).into());
        a.apply_action(OHAction::Bid(0).into());
        a.apply_action(OHAction::Bid(0).into());
        a.apply_action(OHAction::Card(OHCard::TD).into()); // P0 lead
        a.apply_action(OHAction::Card(OHCard::TH).into()); // P1 follow

        let mut b = OhHell::new_state(3, 1);
        b.apply_action(OHAction::Card(OHCard::TH).into()); // P0 deal (swapped)
        b.apply_action(OHAction::Card(OHCard::TD).into()); // P1 deal (swapped)
        b.apply_action(OHAction::Card(OHCard::_5S).into());
        b.apply_action(OHAction::Card(OHCard::TS).into());
        b.apply_action(OHAction::Bid(1).into());
        b.apply_action(OHAction::Bid(0).into());
        b.apply_action(OHAction::Bid(0).into());
        b.apply_action(OHAction::Card(OHCard::TH).into()); // P0 lead (swapped)
        b.apply_action(OHAction::Card(OHCard::TD).into()); // P1 follow (swapped)

        assert_eq!(a.cur_player(), 2);
        assert_eq!(b.cur_player(), 2);
        let p = 2;
        let na = normalizer.normalize_istate(&a.istate_key(p), &a).get();
        let nb = normalizer.normalize_istate(&b.istate_key(p), &b).get();
        assert_eq!(
            na, nb,
            "Hearts↔Diamonds play-order swap must collapse to a single \
             canonical IStateKey"
        );
    }
}
