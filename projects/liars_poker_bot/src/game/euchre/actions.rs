use std::fmt::{Display, Write};

use crate::game::Action;

pub(super) const CARD_PER_SUIT: usize = 6;

#[derive(PartialEq, Clone, Copy, Debug)]
pub(super) enum EAction {
    Pickup,
    Pass,
    Clubs,
    Spades,
    Hearts,
    Diamonds,
    Card { a: u8 },
}

impl EAction {
    pub(super) fn get_suit(&self) -> Suit {
        let card_index = match self {
            EAction::Card { a: x } => *x,
            _ => panic!("can only get the suit of a card action"),
        };

        return match card_index as usize / CARD_PER_SUIT {
            x if x == Suit::Clubs as usize => Suit::Clubs,
            x if x == Suit::Hearts as usize => Suit::Hearts,
            x if x == Suit::Spades as usize => Suit::Spades,
            x if x == Suit::Diamonds as usize => Suit::Diamonds,
            _ => panic!("invalid card"),
        };
    }

    pub(super) fn get_face(&self) -> Face {
        let card_index = match self {
            EAction::Card { a: x } => *x,
            _ => panic!("can only get the suit of a card action"),
        };

        return match card_index as usize % CARD_PER_SUIT {
            0 => Face::N,
            1 => Face::T,
            2 => Face::J,
            3 => Face::Q,
            4 => Face::K,
            5 => Face::A,
            _ => panic!("invalid card index: {}", card_index),
        };
    }
}

impl Into<Action> for EAction {
    fn into(self) -> Action {
        let v: u8 = match self {
            EAction::Pickup => 0,
            EAction::Pass => 1,
            EAction::Clubs => 2,
            EAction::Spades => 3,
            EAction::Hearts => 4,
            EAction::Diamonds => 5,
            EAction::Card { a: x } => 6 + x as u8,
        };
        return Action(v);
    }
}

impl From<Action> for EAction {
    fn from(value: Action) -> Self {
        match value.0 {
            0 => EAction::Pickup,
            1 => EAction::Pass,
            2 => EAction::Clubs,
            3 => EAction::Spades,
            4 => EAction::Hearts,
            5 => EAction::Diamonds,
            x if x >= 6 && x <= 24 + 6 => EAction::Card { a: x - 6 },
            _ => panic!("invalud action to cast: {}", value),
        }
    }
}

impl Display for EAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EAction::Clubs => f.write_char('C'),
            EAction::Spades => f.write_char('S'),
            EAction::Hearts => f.write_char('H'),
            EAction::Diamonds => f.write_char('D'),
            EAction::Pickup => f.write_char('T'),
            EAction::Pass => f.write_char('P'),
            EAction::Card { a: c } => f.write_str(&format_card(*c)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Copy)]
pub(super) enum Suit {
    Clubs,
    Spades,
    Hearts,
    Diamonds,
}

#[derive(PartialEq, Eq)]
pub(super) enum Face {
    N,
    T,
    J,
    Q,
    K,
    A,
}

impl Display for Suit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let c = match self {
            Suit::Clubs => 'C',
            Suit::Spades => 'S',
            Suit::Hearts => 'H',
            Suit::Diamonds => 'D',
        };

        f.write_char(c)
    }
}

/// Populates a string buffer with formated card. Must be 2 characters long
fn format_card(c: u8) -> String {
    let mut out = "XX".to_string();
    put_card(c as usize, &mut out);
    return out.to_string();
}

fn put_card(c: usize, out: &mut str) {
    assert_eq!(out.len(), 2);

    let suit_char = match c / CARD_PER_SUIT {
        x if x == Suit::Clubs as usize => 'C',
        x if x == Suit::Hearts as usize => 'H',
        x if x == Suit::Spades as usize => 'S',
        x if x == Suit::Diamonds as usize => 'D',
        _ => panic!("invalid card"),
    };

    let num_char = match c % CARD_PER_SUIT {
        0 => '9',
        1 => 'T',
        2 => 'J',
        3 => 'Q',
        4 => 'K',
        5 => 'A',
        _ => panic!("invalid card"),
    };

    let s_bytes: &mut [u8] = unsafe { out.as_bytes_mut() };
    assert_eq!(s_bytes.len(), 2);
    // we've made sure this is safe.
    s_bytes[0] = num_char as u8;
    s_bytes[1] = suit_char as u8;
}