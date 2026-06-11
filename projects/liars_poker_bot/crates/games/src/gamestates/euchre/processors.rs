use itertools::Itertools;

use crate::{
    gamestates::euchre::{
        actions::EAction,
        deck::{self},
        isomorphic::get_cards,
        EuchreGameState,
    },
    Action, GameState, Player,
};

use super::{actions::Card, EPhase};

/// Euchre specific processor for open hand solver
pub fn process_euchre_actions(gs: &EuchreGameState, actions: &mut Vec<Action>) {
    match gs.phase() {
        EPhase::Discard => process_discard_actions(gs, actions),
        EPhase::Play => process_play_actions(gs, actions),
        _ => {}
    };
}

/// Evaluate if the euchre game is already over. For example, if a play has the highest trump card, their team is guaranteed
/// to get at least one more trick
pub fn euchre_early_terminate(gs: &EuchreGameState) -> bool {
    if gs.is_terminal() {
        return true;
    }

    // only do this when a trick is over, otherwise might miss played cards
    // also only valid for play phase
    if !gs.is_start_of_trick() || gs.phase != EPhase::Play {
        return false;
    }


    let mut future_score = gs.tricks_won;
    let mut highest = None;
    let mut i = 0;

    while let Some((p, _)) = get_n_highest_trump(gs, i) {
        if let Some(highest) = highest {
            if highest != p {
                break;
            }
        } else {
            highest = Some(p);
        }

        future_score[p % 2] += 1;
        if future_score[p % 2] == 5 // won all five tricks
            // won more than 3 and opponent has won at least 1
                || (future_score[p % 2] >= 3 && future_score[(p + 1) % 2] > 0)
        {
            return true;
        }
        i += 1;
    }

    false
}

/// Sound (pessimistic, optimistic) bounds on the terminal value of this
/// state for `maximizing_player`'s team, used by the alpha-beta solver to
/// prune subtrees whose score range is already decided.
///
/// With `a` tricks for team 0, `b` for team 1 and `r = 5 - a - b` still
/// open, every achievable final split is `(a + k, 5 - a - k)` for
/// `k ∈ [0, r]`. The score function maps each split to one of a handful
/// of values (±1, ±2, ±march), so min/max over the splits bound the true
/// value exactly. Once a team has 3+ tricks and the other has 1+, the
/// score is fully locked and the whole subtree collapses — including
/// mid-trick nodes, which the transposition table never caches.
pub fn euchre_value_bounds(gs: &EuchreGameState, maximizing_player: Player) -> (f64, f64) {
    if gs.phase() != EPhase::Play {
        return (f64::NEG_INFINITY, f64::INFINITY);
    }
    let (a, b) = (gs.tricks_won[0], gs.tricks_won[1]);
    let r = 5 - a - b;
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for k in 0..=r {
        let s = gs.score(a + k, 5 - a - k);
        lo = lo.min(s);
        hi = hi.max(s);
    }
    if maximizing_player % 2 == 0 {
        (lo, hi)
    } else {
        (-hi, -lo)
    }
}

fn process_play_actions(gs: &EuchreGameState, actions: &mut Vec<Action>) {
    // if have the highest trump, and it's a new trick, likely want to play that, evaluate it first
    evaluate_highest_trump_first(gs, actions);

    // if leading and have off suit ace, use that

    // remove actions that are the same (e.g. cards are next to each other)
    remove_equivlent_cards(gs, actions);
}

fn process_discard_actions(gs: &EuchreGameState, actions: &mut Vec<Action>) {
    // We cannot simply remove the picked up card from the action list since there are some times when it is advantageous to discard it. For example:
    // "QcKcKs9dTd|9cTcAc9sAh|JcTsJhJdKd|AsQhKhQdAd|Js|PT|"
    // Instead, we just evaluate it last
    evaluate_picked_up_card_last(gs, actions);

    // remove actions that are the same (e.g. cards are next to each other)
    remove_equivlent_cards(gs, actions);
}

/// Get the owner and card for the nth highest trump in the game, doesn't account for played cards
///
/// 0 is the highest
fn get_n_highest_trump(gs: &EuchreGameState, n: usize) -> Option<(Player, Card)> {
    let trump = gs.trump?;
    // Precompute each active player's hand mask once. Skipping Deck::get (which walks all 10
    // card locations per call) in favour of bitmask AND on the four player hands keeps this
    // function O(trump_cards) instead of O(trump_cards × 10).
    let sitting_out = gs.sitting_out_player();
    let p0 = if sitting_out == Some(0) { 0 } else { gs.deck.get_all(deck::CardLocation::Player0).raw_mask() };
    let p1 = if sitting_out == Some(1) { 0 } else { gs.deck.get_all(deck::CardLocation::Player1).raw_mask() };
    let p2 = if sitting_out == Some(2) { 0 } else { gs.deck.get_all(deck::CardLocation::Player2).raw_mask() };
    let p3 = if sitting_out == Some(3) { 0 } else { gs.deck.get_all(deck::CardLocation::Player3).raw_mask() };
    let active_hands = p0 | p1 | p2 | p3;

    let mut count = 0;
    for c in get_cards(trump, gs.trump).iter().rev() {
        let mask = *c as u32;
        if mask & active_hands == 0 {
            // Card is played, face up, discarded, or held by the sitting-out player. Skip.
            continue;
        }
        if count == n {
            // Determine which active player holds this card. At most four cheap checks.
            let player: Player = if mask & p0 != 0 {
                0
            } else if mask & p1 != 0 {
                1
            } else if mask & p2 != 0 {
                2
            } else {
                debug_assert!(mask & p3 != 0);
                3
            };
            return Some((player, *c));
        }
        count += 1;
    }
    None
}

fn evaluate_highest_trump_first(gs: &EuchreGameState, actions: &mut Vec<Action>) {
    if gs.is_start_of_trick() {
        if let Some((player, card)) = get_n_highest_trump(gs, 0) {
            if player == gs.cur_player() {
                let idx = actions
                    .iter()
                    .find_position(|&x| EAction::from(*x).card() == card)
                    .unwrap()
                    .0;
                actions.swap(0, idx);
                assert!(
                    !actions.is_empty(),
                    "found empty actions evaluating: {}",
                    gs
                );
            }
        }
    }
}

fn evaluate_picked_up_card_last(gs: &EuchreGameState, actions: &mut Vec<Action>) {
    let face_up = gs
        .face_up()
        .expect("can't call faceup before deal finished");
    let idx = actions
        .iter()
        .find_position(|&x| EAction::from(*x).card() != face_up)
        .unwrap()
        .0;

    let last = actions.len() - 1;
    actions.swap(idx, last);
}

/// Removes cards that are equivalent. For example, If a player has the 9s and Ts, each card will play
/// the same way. We don't need to evaluate both.
fn remove_equivlent_cards(gs: &EuchreGameState, actions: &mut Vec<Action>) {
    // Precompute per-location masks once (instead of doing Deck::get — which walks all 10
    // locations — inside a tight per-action loop). With bitmask ops the inner check for each
    // candidate card becomes a handful of AND instructions instead of ~6 Hand::contains calls.
    let cur_hand: u32 = gs.deck.get_all(deck::CardLocation::from(gs.cur_player())).raw_mask();
    let all_hands: u32 = gs.deck.get_all(deck::CardLocation::Player0).raw_mask()
        | gs.deck.get_all(deck::CardLocation::Player1).raw_mask()
        | gs.deck.get_all(deck::CardLocation::Player2).raw_mask()
        | gs.deck.get_all(deck::CardLocation::Player3).raw_mask();
    let visible: u32 = gs.deck.get_all(deck::CardLocation::Played(0)).raw_mask()
        | gs.deck.get_all(deck::CardLocation::Played(1)).raw_mask()
        | gs.deck.get_all(deck::CardLocation::Played(2)).raw_mask()
        | gs.deck.get_all(deck::CardLocation::Played(3)).raw_mask()
        | gs.deck.get_all(deck::CardLocation::FaceUp).raw_mask();
    // "Chain breaker" = any card that isn't None. If the next higher card in a suit is held by
    // another player OR visible on the table, the equivalence chain is broken and we must
    // keep the current card. Excludes cur_hand because we check that first.
    let chain_breaker: u32 = (all_hands & !cur_hand) | visible;

    actions.retain(|x| {
        let ea = EAction::from(*x);
        if ea == EAction::Pass {
            // Sentinel Pass plays for the sit-out partner aren't a card; keep as-is.
            return true;
        }
        let c = ea.card();
        let same_suit = get_cards(gs.get_suit(c), gs.trump);
        let idx = same_suit
            .iter()
            .position(|&x| x == c)
            .expect("card must be in its own suit list");
        for next in &same_suit[idx + 1..] {
            let mask = *next as u32;
            if mask & cur_hand != 0 {
                // Current player owns the next higher card: this one is redundant, remove it.
                return false;
            }
            if mask & chain_breaker != 0 {
                // Next higher card is held by another player or visible on the table.
                // Chain is broken: keep this card.
                return true;
            }
            // Otherwise the next card is None (discarded) — look further up the suit.
        }
        // No card above this one, keep it.
        true
    });
}

/// Returns true if >= n cards have been played in the play phase. If n=0
/// returns true as soon as entering the play phase
pub fn post_cards_played(gs: &EuchreGameState, n: usize) -> bool {
    gs.phase() == EPhase::Play && gs.cards_played >= n
}

#[cfg(test)]
mod tests {
    use itertools::Itertools;

    use crate::{
        actions,
        gamestates::euchre::{
            actions::EAction, processors::evaluate_highest_trump_first, Euchre, EuchreGameState,
        },
        GameState,
    };

    use super::{euchre_value_bounds, remove_equivlent_cards};

    /// `euchre_value_bounds` must bracket the true terminal value along
    /// every random playout, for every perspective player.
    #[test]
    fn value_bounds_bracket_terminal_values() {
        use rand::{rngs::StdRng, seq::IndexedRandom, SeedableRng};
        let mut rng: StdRng = SeedableRng::seed_from_u64(0xE0B);
        for _ in 0..100 {
            let mut gs = Euchre::new_state();
            let mut bounds_along_path: Vec<[(f64, f64); 4]> = Vec::new();
            while !gs.is_terminal() {
                if gs.phase() == super::EPhase::Play {
                    let mut snapshot = [(0.0, 0.0); 4];
                    for (p, s) in snapshot.iter_mut().enumerate() {
                        *s = euchre_value_bounds(&gs, p);
                    }
                    bounds_along_path.push(snapshot);
                }
                let acts = actions!(gs);
                let a = *acts.choose(&mut rng).unwrap();
                gs.apply_action(a);
            }
            for p in 0..4 {
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

    #[test]
    fn test_highest_trump() {
        // Shouldn't do any filtering here as player 3 has the highest card
        let gs = EuchreGameState::from("KcTsJsQsAd|9cTcAcKsAs|ThKh9dJdKd|JcJhQhAhQd|Qc|PT|Ah|P|");
        let mut actions = Vec::new();
        gs.legal_actions(&mut actions);
        let old_actions = actions.clone();
        evaluate_highest_trump_first(&gs, &mut actions);
        assert_eq!(actions, old_actions);

        let gs =
            EuchreGameState::from("KcTsJsQsQd|9cTcAcKsAs|ThKh9dJdKd|JcJhQhAhAd|Qc|PT|Ah|P|QdAs9dAd");
        let mut actions = Vec::new();
        gs.legal_actions(&mut actions);
        evaluate_highest_trump_first(&gs, &mut actions);
        assert_eq!(
            actions.into_iter().map(EAction::from).collect_vec(),
            vec![EAction::JC, EAction::QC, EAction::JH, EAction::QH]
        );

        // Not leading, so should just return all actions
        let gs = EuchreGameState::from(
            "KcTsJsQsAd|9cTcAcKsAs|ThKh9dJdKd|JcJhQhAhQd|Qc|T|Jc|P|JsAcThQc|KcTcKhJh|Ts",
        );
        let mut actions = Vec::new();
        gs.legal_actions(&mut actions);
        let old_actions = actions.clone();
        evaluate_highest_trump_first(&gs, &mut actions);
        assert_eq!(actions, old_actions);
    }

    #[test]
    fn test_remove_equivalent_cards() {
        // shouldn't remove any cards
        let gs = EuchreGameState::from("Kc9sQsAsAd|9cTcAcKsJs|ThKh9dJdKd|TsJhQhAhQd|Qc|PT|Ah|P|");
        let mut actions = Vec::new();
        gs.legal_actions(&mut actions);
        let old_actions = actions.clone();
        remove_equivlent_cards(&gs, &mut actions);
        assert_eq!(
            actions.into_iter().map(EAction::from).collect_vec(),
            old_actions.into_iter().map(EAction::from).collect_vec(),
        );

        let gs = EuchreGameState::from("KcTsJsQsAd|9cTcAcKsAs|ThKh9dJdKd|JcJhQhAhQd|Qc|PT|Ah|P|");
        let mut actions = Vec::new();
        gs.legal_actions(&mut actions);
        remove_equivlent_cards(&gs, &mut actions);
        assert_eq!(
            actions.into_iter().map(EAction::from).collect_vec(),
            vec![
                EAction::JS, // a club since clubs is trump
                EAction::QS,
                EAction::KC,
                EAction::AD
            ]
        );
    }
}
