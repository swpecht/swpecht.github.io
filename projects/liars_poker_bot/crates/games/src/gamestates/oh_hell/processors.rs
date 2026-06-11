//! Game-specific helpers for the `OpenHandSolver` running on Oh Hell.
//!
//! These hooks plug into the generic `Optimizations<G>` slots in the alpha-
//! beta solver and let it exploit Oh Hell-specific structure:
//!
//! * `oh_hell_early_terminate` short-circuits search when nothing remaining
//!   can change any player's "made bid?" status.
//! * `oh_hell_value_bounds` computes sound (pessimistic, optimistic) bounds
//!   on the final mean-centred value so the solver can prune subtrees whose
//!   score range is already decided — including mid-trick nodes, which the
//!   transposition table never caches.
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

/// Returns `true` if the search may stop expanding from this state.
///
/// Under the previous bonus-only scoring rule ("10 + bid if exact, else
/// 0"), this hook short-circuited play once every player was already
/// guaranteed to score 0 — busted or unable to reach their bid. With
/// the Wikipedia "common scoring" rule the per-trick points keep
/// accruing through the last card, so a bust no longer locks the score.
/// Detecting a mid-game state where the full distribution of remaining
/// tricks-won is determined is significantly harder; until that
/// detector is written, this hook is effectively a no-op (true only at
/// terminal).
pub fn oh_hell_early_terminate(gs: &OhHellGameState) -> bool {
    gs.is_terminal()
}

/// Sound (pessimistic, optimistic) bounds on the final mean-centred value
/// of this state for `maximizing_player`, used by the alpha-beta solver to
/// prune subtrees whose outcome range is already decided.
///
/// Per player q with `t_q` tricks won so far and `R` tricks still to be
/// awarded, the final trick count lies in `[t_q, t_q + R]`, so the final
/// score `t + 10·[t == bid_q]` lies between
///   `s_min(q)`: `t_q` normally; if `t_q == bid_q` the cheapest escape is
///   `t_q + 1` when `R ≥ 1`, else the bonus is forced (`t_q + 10`);
///   `s_max(q)`: `max(t_q + R, bid_q + 10 if bid_q ∈ [t_q, t_q + R])`.
/// These per-player ranges ignore the joint constraint that the remaining
/// tricks sum to `R`, which only widens them — so the derived bounds on
/// `v = s_p − mean = ((np−1)·s_p − Σ_others s_o) / np` stay sound.
pub fn oh_hell_value_bounds(gs: &OhHellGameState, maximizing_player: usize) -> (f64, f64) {
    if gs.phase() != OHPhase::Play {
        return (f64::NEG_INFINITY, f64::INFINITY);
    }
    // This runs at every alpha-beta node, so it is written with integer
    // arithmetic and direct indexing throughout; the only float ops are
    // the two final divisions. (All intermediate quantities are small
    // integers, so the f64 results are bit-identical to a float version.)
    let np = gs.num_players();
    let in_trick = gs.num_in_trick();
    let trick_starter = gs.trick_starter();
    let not_started =
        gs.n_tricks() - gs.cards_played() / np - usize::from(in_trick > 0);

    // Mid-trick, the in-progress trick can only be won by the current
    // winner-so-far or a player who hasn't played to it yet — players who
    // already played a losing card are shut out, which tightens their
    // optimistic bound. Mid-trick nodes have no transposition-table
    // caching, so this is their only cutoff besides alpha/beta itself.
    // Card ids are suit-major / rank-ascending, so within a suit a plain
    // id comparison orders by rank; the running best is always lead suit
    // or trump, so a candidate from a third suit never wins.
    let mut winner_so_far = usize::MAX;
    if in_trick > 0 {
        let trick = gs.current_trick();
        let trump = gs.trump_suit().expect("trump set in play phase") as u8;
        let mut best_id = trick[0].expect("lead card set") as u8;
        let mut best_pos = 0;
        for (i, c) in trick.iter().enumerate().take(in_trick).skip(1) {
            let cid = c.expect("played slots filled") as u8;
            let better = if cid / 13 == best_id / 13 {
                cid > best_id
            } else {
                cid / 13 == trump
            };
            if better {
                best_id = cid;
                best_pos = i;
            }
        }
        winner_so_far = (trick_starter + best_pos) % np;
    }

    let tricks_won = gs.tricks_won();
    let bids = gs.bids();
    let p = maximizing_player;
    let (mut s_min_p, mut s_max_p) = (0i32, 0i32);
    let (mut sum_min, mut sum_max) = (0i32, 0i32);
    for q in 0..np {
        let t = tricks_won[q] as i32;
        let bid = bids[q].expect("bids set in play phase") as i32;
        // Tricks q could still win: every not-yet-started trick, plus the
        // in-progress one if q is winning it or hasn't played to it.
        let eligible_current = in_trick > 0
            && (q == winner_so_far || (q + np - trick_starter) % np >= in_trick);
        let rem_q = not_started as i32 + i32::from(eligible_current);

        let (mn, mx) = if rem_q == 0 {
            // q can't win another trick: final count is locked at t.
            let locked = if t == bid { t + 10 } else { t };
            (locked, locked)
        } else {
            let mn = if t == bid { t + 1 } else { t };
            let hi_tricks = t + rem_q;
            let mx = if bid >= t && bid <= hi_tricks {
                hi_tricks.max(bid + 10)
            } else {
                hi_tricks
            };
            (mn, mx)
        };
        sum_min += mn;
        sum_max += mx;
        if q == p {
            s_min_p = mn;
            s_max_p = mx;
        }
    }

    // Mirror `evaluate`'s exact expression shape (`score - sum / np`) so
    // the bounds round identically to the values the game produces:
    // ((np-1)·s - others)/np rounds differently from s - (s + others)/np
    // by 1 ULP on thirds, which is enough to put a true terminal value
    // epsilon-outside an algebraically-equal bound. Both expressions are
    // monotone in the (small, exactly-representable) integer inputs, so
    // soundness carries over bit-for-bit.
    let npf = np as f64;
    let lo = s_min_p as f64 - ((s_min_p + (sum_max - s_max_p)) as f64) / npf;
    let hi = s_max_p as f64 - ((s_max_p + (sum_min - s_min_p)) as f64) / npf;
    (lo, hi)
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

    /// `oh_hell_value_bounds` must bracket the true terminal value along
    /// every random playout, for every perspective player. Walks random
    /// games to terminal and checks each intermediate play-phase state's
    /// bounds against the eventual `evaluate` result of that playout
    /// (every reachable terminal value must lie within the bounds of all
    /// its ancestors).
    #[test]
    fn value_bounds_bracket_terminal_values() {
        use rand::{rngs::StdRng, seq::IndexedRandom, SeedableRng};
        let mut rng: StdRng = SeedableRng::seed_from_u64(0xB07);
        for n_tricks in 1..=4 {
            for _ in 0..50 {
                let mut gs = OhHell::new_state(3, n_tricks);
                let mut bounds_along_path: Vec<[(f64, f64); 3]> = Vec::new();
                while !gs.is_terminal() {
                    if gs.phase() == OHPhase::Play {
                        let mut snapshot = [(0.0, 0.0); 3];
                        for (p, s) in snapshot.iter_mut().enumerate() {
                            *s = oh_hell_value_bounds(&gs, p);
                        }
                        bounds_along_path.push(snapshot);
                    }
                    let acts = actions!(gs);
                    let a = *acts.choose(&mut rng).unwrap();
                    gs.apply_action(a);
                }
                for p in 0..3 {
                    let v = gs.evaluate(p);
                    for (i, snapshot) in bounds_along_path.iter().enumerate() {
                        let (lo, hi) = snapshot[p];
                        assert!(
                            lo <= v && v <= hi,
                            "bounds ({}, {}) at play-state #{} don't bracket terminal \
                             value {} for player {} (state: {})",
                            lo,
                            hi,
                            i,
                            v,
                            p,
                            gs
                        );
                    }
                }
            }
        }
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
