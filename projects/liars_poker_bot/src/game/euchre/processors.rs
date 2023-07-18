use itertools::Itertools;
use rand::rngs::StdRng;

use crate::game::{
    euchre::{
        actions::EAction,
        deck::{self},
        ismorphic::get_cards,
        EuchreGameState,
    },
    Action, GameState, Player,
};

use super::{actions::Card, EPhase, Euchre};

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
    if !gs.is_trick_over() || gs.phase != EPhase::Play {
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
    let deck = gs.deck;
    let mut owner = None;
    let mut highest_trump = None;
    let mut count = 0;
    for c in get_cards(trump, gs.trump).iter().rev() {
        let loc = deck[*c];

        use deck::CardLocation::*;
        match loc {
            Player0 | Player1 | Player2 | Player3 => {
                if n == count {
                    owner = loc.to_player();
                    highest_trump = Some(*c);
                    break;
                } else {
                    count += 1;
                }
            }
            Played(_) | FaceUp | None => {}
        }
    }

    owner.map(|owner| (owner, highest_trump.unwrap()))
}

fn evaluate_highest_trump_first(gs: &EuchreGameState, actions: &mut Vec<Action>) {
    if gs.is_trick_over() {
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
    let face_up = gs.face_up();
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
    actions.retain(|x| find_next_card_owner(EAction::from(*x).card(), gs) != Some(gs.cur_player()));
}

fn find_next_card_owner(c: Card, gs: &EuchreGameState) -> Option<Player> {
    // we must use the effective suit of the card here (from the gamestate)
    let same_suit = get_cards(gs.get_suit(c), gs.trump);
    let idx = same_suit
        .iter()
        .find_position(|&x| *x == c)
        .unwrap_or_else(|| {
            panic!(
                "couldn't find {} in {:?} for {}\n{:?}",
                c, same_suit, gs, gs
            )
        })
        .0;

    for card in same_suit[idx + 1..].iter() {
        let loc = gs.deck[*card];
        use deck::CardLocation::*;
        match loc {
            Player0 | Player1 | Player2 | Player3 => return loc.to_player(),
            // If in play or face up, can't do the optimizations, so we return none
            Played(_) | FaceUp => return Option::None,
            None => continue, // if already played, we can look for the next card up
        };
    }

    None
}

pub fn post_bidding_phase(gs: &EuchreGameState) -> bool {
    match gs.phase() {
        EPhase::DealHands => false,
        EPhase::DealFaceUp => false,
        EPhase::Pickup => false,
        EPhase::ChooseTrump => false,
        EPhase::Discard => true,
        EPhase::Play => true,
    }
}

#[cfg(test)]
mod tests {
    use itertools::Itertools;

    use crate::game::{
        euchre::{
            actions::{Card, EAction},
            processors::evaluate_highest_trump_first,
            EuchreGameState,
        },
        Action, GameState,
    };

    use super::remove_equivlent_cards;

    #[test]
    fn test_highest_trump() {
        // Shouldn't do any filtering here as player 3 has the highest card
        let gs = EuchreGameState::from("KcTsJsQsAd|9cTcAcKsAs|ThKh9dJdKd|JcJhQhAhQd|Qc|PT|Ah|");
        let mut actions = Vec::new();
        gs.legal_actions(&mut actions);
        let old_actions = actions.clone();
        evaluate_highest_trump_first(&gs, &mut actions);
        assert_eq!(actions, old_actions);

        let gs =
            EuchreGameState::from("KcTsJsQsQd|9cTcAcKsAs|ThKh9dJdKd|JcJhQhAhAd|Qc|PT|Ah|QdAsThAd");
        let mut actions = Vec::new();
        gs.legal_actions(&mut actions);
        evaluate_highest_trump_first(&gs, &mut actions);
        assert_eq!(
            actions,
            vec![Action(102), Action(103), Action(114), Action(115)]
        );

        // Not leading, so should just return all actions
        let gs = EuchreGameState::from(
            "KcTsJsQsAd|9cTcAcKsAs|ThKh9dJdKd|JcJhQhAhQd|Qc|T|Jc|JsAcThQc|KcTcKhJh|Ts",
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
        let gs = EuchreGameState::from("Kc9sQsAsAd|9cTcAcKsJs|ThKh9dJdKd|TsJhQhAhQd|Qc|PT|Ah|");
        let mut actions = Vec::new();
        gs.legal_actions(&mut actions);
        let old_actions = actions.clone();
        remove_equivlent_cards(&gs, &mut actions);
        assert_eq!(
            actions.into_iter().map(EAction::from).collect_vec(),
            old_actions.into_iter().map(EAction::from).collect_vec(),
        );

        let gs = EuchreGameState::from("KcTsJsQsAd|9cTcAcKsAs|ThKh9dJdKd|JcJhQhAhQd|Qc|PT|Ah|");
        let mut actions = Vec::new();
        gs.legal_actions(&mut actions);
        remove_equivlent_cards(&gs, &mut actions);
        assert_eq!(
            actions.into_iter().map(EAction::from).collect_vec(),
            vec![
                EAction::Play { c: Card::KC },
                EAction::Play { c: Card::JS }, // a club since clubs is trump
                EAction::Play { c: Card::QS },
                EAction::Play { c: Card::AD }
            ]
        );
    }
}
