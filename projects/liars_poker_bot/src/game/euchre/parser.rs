use std::ops::Deref;

use serde::{Deserialize, Serialize};

use crate::game::{Action, Player};

use super::{actions::EAction, CARDS_PER_HAND};

#[derive(Copy, Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub(super) enum EuchreParserState {
    DealPlayers(usize),
    DealFaceUp,
    Discard,
    PickupChoice(Player),
    CallChoice(Player),
    Play(usize),
    Terminal,
}

impl EuchreParserState {
    pub fn next(&self, a: Action) -> Self {
        match self {
            Self::DealPlayers(c) => self.next_deal(a, *c),
            Self::DealFaceUp => self.next_deal_face_up(a),
            Self::Discard => self.next_discard(a),
            Self::PickupChoice(p) => self.next_pickup_choice(a, *p),
            Self::CallChoice(p) => self.next_call_choice(a, *p),
            Self::Play(c) => self.next_play(a, *c),
            Self::Terminal => panic!("can't progress terminal state"),
        }
    }

    pub fn is_visible(&self, p: Player) -> bool {
        match self {
            EuchreParserState::DealPlayers(c) => {
                *c >= p * CARDS_PER_HAND && *c < (p + 1) * CARDS_PER_HAND
            }
            EuchreParserState::DealFaceUp => true,
            EuchreParserState::Discard => p == 3, // only visible to dealer
            EuchreParserState::PickupChoice(_) => true,
            EuchreParserState::CallChoice(_) => true,
            EuchreParserState::Play(_) => true,
            EuchreParserState::Terminal => false, // no one sees this
        }
    }

    fn next_deal(&self, a: Action, c: usize) -> Self {
        assert!(EAction::from(a).is_card());
        if c == 19 {
            Self::DealFaceUp
        } else {
            Self::DealPlayers(c + 1)
        }
    }

    fn next_deal_face_up(&self, a: Action) -> Self {
        assert!(EAction::from(a).is_card());
        Self::PickupChoice(0)
    }

    fn next_discard(&self, a: Action) -> Self {
        assert!(EAction::from(a).is_card());
        Self::Play(0)
    }

    fn next_pickup_choice(&self, a: Action, prev_player: Player) -> Self {
        match (EAction::from(a), prev_player) {
            (EAction::Pickup, _) => Self::Discard,
            (EAction::Pass, x) if x < 3 => Self::PickupChoice(x + 1),
            (EAction::Pass, 3) => Self::CallChoice(0),
            _ => panic!(
                "invalid action for pickup, got: {:?} with previous player: {}",
                EAction::from(a),
                prev_player
            ),
        }
    }

    fn next_call_choice(&self, a: Action, prev_player: Player) -> Self {
        match (EAction::from(a), prev_player) {
            (EAction::Clubs, _)
            | (EAction::Hearts, _)
            | (EAction::Diamonds, _)
            | (EAction::Spades, _) => Self::Play(0),
            (EAction::Pass, x) if x < 3 => Self::CallChoice(x + 1),
            _ => panic!("invalid action for call"),
        }
    }

    fn next_play(&self, a: Action, c: usize) -> Self {
        if c < 20 {
            Self::Play(c + 1)
        } else {
            Self::Terminal
        }
    }
}

/// Consumes a series of actions to track the state of a euchre game
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub(super) struct EuchreParser {
    pub history: Vec<EuchreParserState>,
}

impl EuchreParser {
    pub fn consume(&mut self, a: Action) {
        let last = self.history[self.history.len() - 1];
        self.history.push(last.next(a));
    }

    // undo the last action
    pub fn undo(&mut self) -> EuchreParserState {
        self.history.pop().unwrap()
    }
}

impl Default for EuchreParser {
    fn default() -> Self {
        Self {
            history: vec![EuchreParserState::DealPlayers(0)],
        }
    }
}

impl Deref for EuchreParser {
    type Target = Vec<EuchreParserState>;

    fn deref(&self) -> &Self::Target {
        &self.history
    }
}
