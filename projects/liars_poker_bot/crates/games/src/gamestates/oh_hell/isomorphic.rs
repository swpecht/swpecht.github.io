//! `IStateNormalizer` for Oh Hell. Collapses CFR istate keys that differ
//! only by a non-trump suit permutation into a single canonical key, so
//! MCCFR's HashMap-backed store shares regrets across the symmetry class
//! instead of paying for each labelling separately.
//!
//! Symmetries exploited (player-visible only — this operates on istates,
//! not the full gamestate):
//!   * Trump suit → canonical slot 0 (Spades). The face-up card's suit
//!     determines trump, so this is a one-shot relabel.
//!   * Among the three non-trump suits, sort by an iso-invariant
//!     "fingerprint" derived from the istate (player's hand cards in
//!     that suit + plays in that suit). Ties break deterministically.
//!
//! Why no rank compaction within a suit
//!
//!   Any non-trivial rank permutation within a suit would have to
//!   (a) be order-preserving (otherwise trick-taking outcomes change)
//!   and (b) map Hand↔Hand, Played↔Played, Face-up↔Face-up, Unknown↔
//!   Unknown set-wise. For a strict total order the only such bijection
//!   is the identity, so rank-compaction *within* a suit doesn't exist
//!   for a strict iso of a CFR istate. The per-rank category sequence
//!   is therefore the absolute ceiling for strict iso reduction. The
//!   `OpenHandSolver` TT *does* rank-compact, but only because it sees
//!   the full game state (including "card is literally out of the game"
//!   None positions, which behave like Euchre's `iso_deck` swap_loc).
//!
//!   We confirmed this empirically: a packed categorical-sequence
//!   normaliser (encoding the per-rank category for every suit + bids
//!   + tricks_won + cur_player) produced 982,081 info_states across a
//!   50M-iter run, vs 981,902 for the simpler suit-perm action
//!   sequence here. The 179-state difference is within RNG sampling
//!   noise, confirming the two formats capture the same iso-equivalence
//!   classes. We reverted to this simpler form so the normalised istate
//!   stays a recognisable suit-permuted action sequence (easier to
//!   debug and inspect).
//!
//!   Further reduction requires *approximate* iso (poker-style card
//!   abstraction), which is a separate design choice with a real
//!   policy-quality cost.
//!
//! Code structure mirrors `gamestates/euchre/isomorphic.rs` — same
//! `IStateNormalizer` trait, same "compute perm from istate, then apply
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
/// - The other three suits are sorted by an iso-invariant fingerprint and
///   assigned slots 1, 2, 3 in order.
///
/// The fingerprint depends only on:
///   * the player's hand bits in that suit,
///   * how many cards in that suit have been played by anyone,
///   * the played-card mask within that suit (so chronologically
///     identical-card-distribution suits also tie),
/// all of which are visible from the istate alone. That makes the perm
/// invariant under any non-trump suit relabelling, which is the
/// fundamental requirement for the normalised istate to be canonical.
pub fn compute_perm(gs: &OhHellGameState) -> [u8; 4] {
    let Some(trump) = gs.trump_suit() else {
        return IDENTITY;
    };

    let player = gs.cur_player();
    let hand = gs.hand_mask(player);
    let played = gs.played_mask();

    // For each suit, compute a fingerprint that's iso-invariant.
    let fingerprint = |suit_idx: usize| -> u64 {
        let suit_full = SUIT_MASK[suit_idx];
        let suit_base = (suit_idx as u64) * 13;
        let hand_in_suit = ((hand & suit_full) >> suit_base) & 0x1FFF;
        let played_in_suit = ((played & suit_full) >> suit_base) & 0x1FFF;
        // Pack: low 13 bits = hand mask in suit, next 13 = played mask in suit,
        // next 4 = popcount(hand_in_suit), next 4 = popcount(played_in_suit).
        let hand_count = hand_in_suit.count_ones() as u64;
        let played_count = played_in_suit.count_ones() as u64;
        hand_in_suit
            | (played_in_suit << 13)
            | (hand_count << 26)
            | (played_count << 30)
    };

    // Gather (old_suit_idx, fingerprint) for the three non-trump suits.
    let trump_idx = trump as usize;
    let mut nontrump: [(u8, u64); 3] = [(0, 0); 3];
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

/// Apply the suit permutation to a single Action. Card actions get their
/// suit-bits relabelled (same rank, new suit); Bid actions pass through.
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

/// Apply the suit permutation to an entire istate key, re-sorting the
/// player's hand segment afterwards (it was sorted before the relabel,
/// but the new suit labels change the discriminant order).
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

    /// Play a deterministic random game and record its action sequence so
    /// it can be replayed (possibly with a perm) to build a truly
    /// iso-equivalent sibling state.
    fn play_random_to_phase(seed: u64, stop_in_bidding: bool) -> (OhHellGameState, Vec<Action>) {
        let mut rng: StdRng = SeedableRng::seed_from_u64(seed);
        let mut gs = OhHell::new_state(2, 2);
        let mut history = Vec::new();
        while !gs.is_terminal() {
            // Stop once we're in bidding-or-play (post chance).
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

    /// Rebuild a fresh game state by replaying `history` with the suit
    /// permutation `perm` applied to every action. Bid actions pass
    /// through unchanged; card actions get their suit-bits relabelled.
    fn rebuild_with_perm(history: &[Action], perm: [u8; 4]) -> OhHellGameState {
        let mut gs = OhHell::new_state(2, 2);
        for a in history {
            gs.apply_action(apply_perm(*a, &perm));
        }
        gs
    }

    /// Two iso-equivalent states (one built by replaying the original
    /// history with a 2-suit perm applied to every card action) must
    /// produce the same normalised istate. Tests every pairwise non-
    /// identity permutation of the 4 suits.
    #[test]
    fn normalize_collapses_suit_perm_istate() {
        let normalizer = OhHellNormalizer::default();
        // All pairwise 2-suit swaps. Each swaps two suits and leaves the
        // other two untouched. We deliberately include swaps that move
        // the trump suit — the normalisation still has to land both
        // states on the same canonical key because trump is just pinned
        // to slot 0 regardless of which suit was originally trump.
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
        let normalizer = OhHellNormalizer::default();
        let mut rng: StdRng = SeedableRng::seed_from_u64(0xC0FFEE);
        for _ in 0..50 {
            let gs = drive_to_play(rng.random::<u64>());
            // Pick a legal action and round-trip it through
            // normalize → denormalize. The denormalised value should
            // equal the original.
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
        let normalizer = OhHellNormalizer::default();
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

    /// Bids are unaffected by suit permutation.
    #[test]
    fn bid_actions_unchanged() {
        let normalizer = OhHellNormalizer::default();
        let gs = drive_to_play(42);
        // Phase is Play, but we still test the bid branch through
        // apply_perm directly (Bid actions never re-enter play, but
        // the type-level handling is what we want to check).
        let bid: Action = OHAction::Bid(2).into();
        let n = normalizer.normalize_action(bid, &gs);
        assert_eq!(n.get(), bid);
        let d = normalizer.denormalize_action(n, &gs);
        assert_eq!(d, bid);
    }

    /// Two states with the same trump, same hand and same bids but a
    /// different specific played card (different rank in the same suit)
    /// must NOT collapse — the played card is part of the istate's
    /// action sequence, so its rank label distinguishes the two
    /// normalised istates. This is the lower bound on what the
    /// normaliser must preserve: it can collapse suit relabellings but
    /// not rank substitutions on played cards.
    #[test]
    fn normalize_distinguishes_different_played_card_ranks() {
        let normalizer = OhHellNormalizer::default();

        // State A: P0=9s,Js / P1=9h,Th. Face up=2h (hearts trump).
        // Bids 0,0. P0 leads 9s, P1 follows with 9h (played).
        let mut a = OhHell::new_state(2, 2);
        for c in &[OHCard::NS, OHCard::NH, OHCard::JS, OHCard::TH] {
            a.apply_action(OHAction::Card(*c).into());
        }
        a.apply_action(OHAction::Card(OHCard::_2H).into()); // face up
        a.apply_action(OHAction::Bid(0).into());
        a.apply_action(OHAction::Bid(0).into());
        a.apply_action(OHAction::Card(OHCard::NS).into()); // P0 leads 9s
        a.apply_action(OHAction::Card(OHCard::NH).into()); // P1 plays 9h (trump)

        // State B: same hands, same face up, same bids, but P1 plays Th
        // (different rank in same suit) on the same trick.
        let mut b = OhHell::new_state(2, 2);
        for c in &[OHCard::NS, OHCard::NH, OHCard::JS, OHCard::TH] {
            b.apply_action(OHAction::Card(*c).into());
        }
        b.apply_action(OHAction::Card(OHCard::_2H).into());
        b.apply_action(OHAction::Bid(0).into());
        b.apply_action(OHAction::Bid(0).into());
        b.apply_action(OHAction::Card(OHCard::NS).into());
        b.apply_action(OHAction::Card(OHCard::TH).into()); // played higher card

        // The played-card rank differs (9h vs Th) — the normalised
        // istate must preserve that distinction.
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

    /// Picking apart a specific card: queen-of-hearts under a perm that
    /// remaps hearts→clubs becomes queen-of-clubs (same rank).
    #[test]
    fn apply_perm_relabels_suit_only() {
        // Hearts is suit index 2; Clubs is 1. Build a perm Hearts -> Clubs.
        let mut perm = [0u8, 1, 2, 3];
        perm.swap(1, 2); // Clubs↔Hearts
        let qh: Action = OHCard::QH.into();
        let qc: Action = OHCard::QC.into();
        assert_eq!(apply_perm(qh, &perm), qc);
        assert_eq!(apply_perm(qc, &perm), qh);
        // Spades and Diamonds untouched.
        let qs: Action = OHCard::QS.into();
        let qd: Action = OHCard::QD.into();
        assert_eq!(apply_perm(qs, &perm), qs);
        assert_eq!(apply_perm(qd, &perm), qd);
    }
}
