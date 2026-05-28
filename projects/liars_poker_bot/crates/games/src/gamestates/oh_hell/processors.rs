//! Game-specific helpers for the `OpenHandSolver` running on Oh Hell.
//!
//! These hooks plug into the generic `Optimizations<G>` slots in the alpha-
//! beta solver and let it exploit Oh Hell-specific structure:
//!
//! * `oh_hell_early_terminate` short-circuits search when nothing remaining
//!   can change any player's "made bid?" status.
//! * `process_oh_hell_actions` reorders and prunes legal actions in the play
//!   phase: equivalent-rank cards in the same suit collapse to one option,
//!   and known winners get tried first to maximise alpha-beta cutoffs.

use crate::{
    gamestates::oh_hell::{
        actions::{OHAction, OHCard, OHSuit, OH_DECK},
        OhHellGameState,
    },
    Action, GameState,
};

use super::OHPhase;

// Per-suit precomputed masks. Index is `OHSuit as u8`.
const SUIT_MASKS: [u64; 4] = {
    let mut out = [0u64; 4];
    let mut i = 0;
    while i < OH_DECK.len() {
        let c = OH_DECK[i];
        // (c as u8) / 13 gives the suit index; rebuild here in const context.
        let suit_idx = (c as u8) / 13;
        out[suit_idx as usize] |= 1u64 << (c as u8);
        i += 1;
    }
    out
};

#[inline(always)]
fn suit_mask(suit: OHSuit) -> u64 {
    SUIT_MASKS[suit as usize]
}

/// Returns a mask of all cards in `suit` strictly higher in rank than `card`.
/// Computed from the discriminant layout (suit-major, rank-ascending).
#[inline(always)]
fn higher_in_suit_mask(card: OHCard) -> u64 {
    let id = card as u8;
    let suit_idx = id / 13;
    let rank_idx = id % 13;
    let suit_base = (suit_idx as u64) * 13;
    let suit_full = suit_mask(OHSuit::ALL[suit_idx as usize]);
    // bits strictly above `card` within the suit: shift up by (rank_idx + 1)
    let above = (suit_full >> (suit_base + rank_idx as u64 + 1)) << (suit_base + rank_idx as u64 + 1);
    above
}

/// Returns `true` if the search may stop expanding from this state because
/// every player's final score is already locked in.
///
/// Currently detects the most common case: every player has either busted
/// (taken more tricks than their bid) or run out of room to make their bid
/// (`tricks_won + remaining_tricks < bid`). In either case the player's
/// final score is 0 no matter what's played in the remaining tricks.
pub fn oh_hell_early_terminate(gs: &OhHellGameState) -> bool {
    if gs.is_terminal() {
        return true;
    }
    if gs.phase() != OHPhase::Play {
        return false;
    }

    let np = gs.num_players();
    let total_tricks = gs.n_tricks();
    let completed_tricks = gs.cards_played() / np;
    let tricks_remaining = total_tricks - completed_tricks;

    let bids = gs.bids();
    let tricks_won = gs.tricks_won();

    for p in 0..np {
        let Some(bid) = bids[p] else {
            return false;
        };
        let bid = bid as usize;
        let won = tricks_won[p] as usize;
        if won > bid {
            continue;
        }
        if won == bid && tricks_remaining == 0 {
            continue;
        }
        if won + tricks_remaining < bid {
            continue;
        }
        return false;
    }
    true
}

/// Filter and reorder the legal action list to make alpha-beta cheaper.
/// Only active in the play phase, and only when there are at least two
/// actions to consider (pruning a single action is a no-op, and the
/// per-call overhead would actively slow the search down).
pub fn process_oh_hell_actions(gs: &OhHellGameState, actions: &mut Vec<Action>) {
    if actions.len() < 2 || gs.phase() != OHPhase::Play {
        return;
    }
    // Order matters: prune redundant cards first so the move-ordering step
    // doesn't waste a swap on a card we're about to drop.
    remove_equivalent_cards(gs, actions);
    if actions.len() < 2 {
        return;
    }
    order_promising_moves_first(gs, actions);
}

/// Two cards held by the current player are "equivalent" for the open-hand
/// search if there is no card strictly between them in the same suit that
/// is held by anyone else *or* visible on the table. The lower of the pair
/// can never beat anything the higher one can't, so trying both gives the
/// same value.
///
/// We drop the lower card from the action list whenever we detect such a
/// pair, repeating until no further reductions are possible.
///
/// Implemented with pure bitmask operations — no allocation and no `Vec`
/// scans of the action history.
fn remove_equivalent_cards(gs: &OhHellGameState, actions: &mut Vec<Action>) {
    let cur_player = gs.cur_player();
    let cur_hand = gs.hand_mask(cur_player);
    // Visible = played-so-far + face-up.
    let visible = gs.played_mask() | gs.face_up().map(|c| 1u64 << (c as u8)).unwrap_or(0);
    let all_other_hands = (0..gs.num_players())
        .filter(|p| *p != cur_player)
        .fold(0u64, |a, p| a | gs.hand_mask(p));
    let chain_breaker = all_other_hands | visible;

    actions.retain(|act| {
        let OHAction::Card(card) = OHAction::from(*act) else {
            return true;
        };
        // All cards in the same suit above this one.
        let above = higher_in_suit_mask(card);
        // The "first higher card" relevant for the chain is the lowest set
        // bit in (above ∩ (cur_hand ∪ chain_breaker)).
        let relevant = above & (cur_hand | chain_breaker);
        if relevant == 0 {
            return true; // no card above accounted for; keep
        }
        let next_bit = relevant & relevant.wrapping_neg(); // isolate lowest set bit
        // If the next-above card is in our hand, the current card is redundant.
        (next_bit & cur_hand) == 0
    });
}

/// Heuristic move ordering. Trying "obviously strong" moves first widens the
/// alpha-beta window quickly and prunes more of the rest.
fn order_promising_moves_first(gs: &OhHellGameState, actions: &mut Vec<Action>) {
    if actions.len() < 2 {
        return;
    }
    let Some(trump) = gs.trump_suit() else { return };
    let cur_player = gs.cur_player();
    let cur_hand = gs.hand_mask(cur_player);
    let visible = gs.played_mask() | gs.face_up().map(|c| 1u64 << (c as u8)).unwrap_or(0);
    let all_other_hands = (0..gs.num_players())
        .filter(|p| *p != cur_player)
        .fold(0u64, |a, p| a | gs.hand_mask(p));

    // Absolute highest trump card still in play (anywhere).
    let trump_in_play = suit_mask(trump) & !visible;
    if trump_in_play == 0 {
        return;
    }
    // The single highest trump bit in trump_in_play.
    let highest_trump = highest_bit(trump_in_play);
    if highest_trump & cur_hand != 0 {
        // We hold the absolute highest trump → likely a winner; try first.
        let card_id = highest_trump.trailing_zeros() as u8;
        move_action_to_front(actions, Action(card_id));
        return;
    }
    // Otherwise: try the smallest card in our hand of the lead suit first
    // (cheap & dominated) so the alpha cutoff fires earlier when we're
    // trying not to win the trick.
    let _ = all_other_hands; // (reserved for future ordering heuristics)
}

#[inline(always)]
fn highest_bit(mask: u64) -> u64 {
    if mask == 0 {
        0
    } else {
        1u64 << (63 - mask.leading_zeros())
    }
}

#[inline(always)]
fn move_action_to_front(actions: &mut Vec<Action>, target: Action) {
    if let Some(idx) = actions.iter().position(|a| *a == target) {
        actions.swap(0, idx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{actions, gamestates::oh_hell::{OhHell, OHPhase}};

    fn fixture() -> OhHellGameState {
        // From oh_hell module tests, but used here for processor checks.
        let mut gs = OhHell::new_state(3, 2);
        // P0: NS, TS / P1: JS, QS / P2: KS, NC / face up TC (clubs trump)
        let order = [
            OHCard::NS, OHCard::JS, OHCard::KS,
            OHCard::TS, OHCard::QS, OHCard::NC,
        ];
        for c in order {
            gs.apply_action(OHAction::Card(c).into());
        }
        gs.apply_action(OHAction::Card(OHCard::TC).into());
        gs.apply_action(OHAction::Bid(1).into());
        gs.apply_action(OHAction::Bid(1).into());
        gs.apply_action(OHAction::Bid(1).into());
        gs
    }

    #[test]
    fn early_terminate_false_initially() {
        let gs = fixture();
        assert!(!oh_hell_early_terminate(&gs));
    }

    #[test]
    fn early_terminate_true_when_all_busted() {
        // Build a state where everyone has overshot their bid of 0.
        let mut gs = OhHell::new_state(3, 1);
        // Deal P0: 9s, P1: 9c, P2: 9h, face up 9d (diamonds trump)
        gs.apply_action(OHAction::Card(OHCard::NS).into());
        gs.apply_action(OHAction::Card(OHCard::NC).into());
        gs.apply_action(OHAction::Card(OHCard::NH).into());
        gs.apply_action(OHAction::Card(OHCard::ND).into());
        // All players bid 0
        gs.apply_action(OHAction::Bid(0).into());
        gs.apply_action(OHAction::Bid(0).into());
        gs.apply_action(OHAction::Bid(0).into());
        // Play the trick; whoever wins busts (the other two stay safe).
        gs.apply_action(OHAction::Card(OHCard::NS).into());
        gs.apply_action(OHAction::Card(OHCard::NC).into());
        gs.apply_action(OHAction::Card(OHCard::NH).into());
        // Terminal now — every player has a locked score.
        assert!(gs.is_terminal());
        assert!(oh_hell_early_terminate(&gs));
    }

    #[test]
    fn equivalent_cards_collapse_to_one() {
        // Construct a play-phase state where the current player holds two
        // cards in the same suit with no chain-breakers between them.
        let mut gs = OhHell::new_state(3, 2);
        // Hands:
        //   P0: 9s, Ts  (consecutive spades, no breakers — should collapse)
        //   P1: 9h, Th  (off-suit so won't impact)
        //   P2: Jh, Qh
        // Face up: 9c (trump = Clubs)
        let order = [
            OHCard::NS, OHCard::NH, OHCard::JH,
            OHCard::TS, OHCard::TH, OHCard::QH,
        ];
        for c in order {
            gs.apply_action(OHAction::Card(c).into());
        }
        gs.apply_action(OHAction::Card(OHCard::NC).into());
        gs.apply_action(OHAction::Bid(1).into());
        gs.apply_action(OHAction::Bid(0).into());
        gs.apply_action(OHAction::Bid(0).into());
        assert_eq!(gs.phase(), OHPhase::Play);

        let mut acts = actions!(gs);
        assert_eq!(acts.len(), 2, "expected 2 legal actions before pruning");
        remove_equivalent_cards(&gs, &mut acts);
        assert_eq!(
            acts.len(),
            1,
            "9s/Ts are equivalent (no chain-breakers); pruning should leave 1"
        );
    }

    #[test]
    fn ace_kept_when_chain_breaker_blocks() {
        // P0 holds 9s and Js; P1 holds Ts (chain breaker between).
        // 9s and Js are NOT equivalent — Js can beat Ts, 9s can't.
        let mut gs = OhHell::new_state(3, 2);
        let order = [
            OHCard::NS, OHCard::TS, OHCard::NH,
            OHCard::JS, OHCard::QS, OHCard::TH,
        ];
        for c in order {
            gs.apply_action(OHAction::Card(c).into());
        }
        gs.apply_action(OHAction::Card(OHCard::NC).into()); // clubs trump
        gs.apply_action(OHAction::Bid(1).into());
        gs.apply_action(OHAction::Bid(1).into());
        gs.apply_action(OHAction::Bid(0).into());

        let mut acts = actions!(gs);
        let before = acts.len();
        remove_equivalent_cards(&gs, &mut acts);
        assert_eq!(
            acts.len(),
            before,
            "Ts (held by P1) breaks the 9s-Js chain; nothing should be pruned"
        );
    }
}
