use itertools::Itertools;

use crate::game::{
    euchre::{actions::EAction, deck, ismorphic::get_cards, EuchreGameState},
    Action, GameState, Player,
};

use super::{actions::Card, EPhase};

/// Euchre specific processor for open hand solver
pub fn process_euchre_actions(gs: &EuchreGameState, actions: &mut Vec<Action>) {
    match gs.phase() {
        EPhase::DealHands => {}
        EPhase::DealFaceUp => {}
        EPhase::Pickup => {}
        EPhase::Discard => process_discard_actions(gs, actions),
        EPhase::ChooseTrump => {}
        EPhase::Play => process_play_actions(gs, actions),
    };
}

fn process_play_actions(gs: &EuchreGameState, actions: &mut Vec<Action>) {
    // if have the highest trump, and it's a new trick, likely want to play that, evaluate it first
    if gs.is_trick_over() {
        if let Some((player, card)) = get_player_highest_trump(gs) {
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

    // if leading and have off suit ace, use that

    // remove actions that are the same (e.g. cards are next to each other)
}

fn process_discard_actions(gs: &EuchreGameState, actions: &mut Vec<Action>) {
    // We cannot simply remove the picked up card from the action list since there are some times when it is advantageous to discard it. For example:
    // "QcKcKs9dTd|9cTcAc9sAh|JcTsJhJdKd|AsQhKhQdAd|Js|PT|"
    // Instead, we just evaluate it last
    let face_up = gs.face_up();
    let idx = actions
        .iter()
        .find_position(|&x| EAction::from(*x).card() != face_up)
        .unwrap()
        .0;

    let last = actions.len() - 1;
    actions.swap(idx, last);

    // remove actions that are the same (e.g. cards are next to each other)
}

fn get_player_highest_trump(gs: &EuchreGameState) -> Option<(Player, Card)> {
    let trump = gs.trump.unwrap();
    let deck = gs.deck;
    let mut owner = None;
    let mut highest_trump = None;
    for c in get_cards(trump, gs.trump).iter().rev() {
        let loc = deck[*c];

        use deck::CardLocation::*;
        match loc {
            Player0 | Player1 | Player2 | Player3 => {
                owner = loc.to_player();
                highest_trump = Some(*c);
                break;
            }
            Played(_) | FaceUp | None => {}
        }
    }

    owner.map(|owner| (owner, highest_trump.unwrap()))
}

#[cfg(test)]
mod tests {
    use crate::{
        actions,
        game::{
            euchre::{
                actions::{Card, EAction},
                processors::process_euchre_actions,
                EuchreGameState,
            },
            Action, GameState,
        },
    };

    #[test]
    fn test_highest_trump() {
        // Shouldn't do any filtering here as player 3 has the highest card
        let gs = EuchreGameState::from("KcTsJsQsAd|9cTcAcKsAs|ThKh9dJdKd|JcJhQhAhQd|Qc|PT|Ah|");
        let mut actions = Vec::new();
        gs.legal_actions(&mut actions);
        let old_actions = actions.clone();
        process_euchre_actions(&gs, &mut actions);
        assert_eq!(actions, old_actions);

        let gs =
            EuchreGameState::from("KcTsJsQsQd|9cTcAcKsAs|ThKh9dJdKd|JcJhQhAhAd|Qc|PT|Ah|QdAsThAd");
        let mut actions = Vec::new();
        gs.legal_actions(&mut actions);
        process_euchre_actions(&gs, &mut actions);
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
        process_euchre_actions(&gs, &mut actions);
        assert_eq!(actions, old_actions);
    }
}
