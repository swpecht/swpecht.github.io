use std::fmt::{Debug, Display, Write};

use serde::{Deserialize, Serialize};

use crate::game::Action;

pub(super) const CARD_PER_SUIT: u8 = 6;

#[derive(PartialEq, Clone, Copy, Serialize, Deserialize, Eq)]
pub enum EAction {
    Pickup,
    Pass,
    Clubs,
    Spades,
    Hearts,
    Diamonds,
    DealPlayer { c: Card },
    DealFaceUp { c: Card },
    Discard { c: Card },
    Play { c: Card },
}

impl EAction {
    pub fn card(&self) -> Card {
        match self {
            EAction::Discard { c } | EAction::DealPlayer { c } | EAction::Play { c } => *c,
            _ => panic!("can't get card on: {:?}", self),
        }
    }
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Debug)]
pub enum Card {
    NS,
    TS,
    JS,
    QS,
    KS,
    AS,
    NC,
    TC,
    JC,
    QC,
    KC,
    AC,
    NH,
    TH,
    JH,
    QH,
    KH,
    AH,
    ND,
    TD,
    JD,
    QD,
    KD,
    AD,
}

impl Card {
    pub(super) fn suit(&self) -> Suit {
        match self {
            Card::NS | Card::TS | Card::JS | Card::QS | Card::KS | Card::AS => Suit::Spades,
            Card::NC | Card::TC | Card::JC | Card::QC | Card::KC | Card::AC => Suit::Clubs,
            Card::NH | Card::TH | Card::JH | Card::QH | Card::KH | Card::AH => Suit::Hearts,
            Card::ND | Card::TD | Card::JD | Card::QD | Card::KD | Card::AD => Suit::Diamonds,
        }
    }

    pub(super) fn rank(&self) -> u8 {
        *self as u8 % CARD_PER_SUIT
    }
}

impl From<u8> for Card {
    fn from(value: u8) -> Self {
        match value {
            0 => Self::NS,
            1 => Self::TS,
            2 => Self::JS,
            3 => Self::QS,
            4 => Self::KS,
            5 => Self::AS,
            6 => Self::NC,
            7 => Self::TC,
            8 => Self::JC,
            9 => Self::QC,
            10 => Self::KC,
            11 => Self::AC,
            12 => Self::NH,
            13 => Self::TH,
            14 => Self::JH,
            15 => Self::QH,
            16 => Self::KH,
            17 => Self::AH,
            18 => Self::ND,
            19 => Self::TD,
            20 => Self::JD,
            21 => Self::QD,
            22 => Self::KD,
            23 => Self::AD,
            _ => panic!("invalid value to conver to card: {}", value),
        }
    }
}

impl From<&str> for Card {
    fn from(value: &str) -> Self {
        match value {
            "9S" => Self::NS,
            "TS" => Self::TS,
            "JS" => Self::JS,
            "QS" => Self::QS,
            "KS" => Self::KS,
            "AS" => Self::AS,
            "9C" => Self::NC,
            "TC" => Self::TC,
            "JC" => Self::JC,
            "QC" => Self::QC,
            "KC" => Self::KC,
            "AC" => Self::AC,
            "9H" => Self::NH,
            "TH" => Self::TH,
            "JH" => Self::JH,
            "QH" => Self::QH,
            "KH" => Self::KH,
            "AH" => Self::AH,
            "9D" => Self::ND,
            "TD" => Self::TD,
            "JD" => Self::JD,
            "QD" => Self::QD,
            "KD" => Self::KD,
            "AD" => Self::AD,
            _ => panic!("invalud card string: {}", value),
        }
    }
}

impl Display for Card {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Card::NS => write!(f, "9S"),
            Card::TS => write!(f, "TS"),
            Card::JS => write!(f, "JS"),
            Card::QS => write!(f, "QS"),
            Card::KS => write!(f, "KS"),
            Card::AS => write!(f, "AS"),
            Card::NC => write!(f, "9C"),
            Card::TC => write!(f, "TC"),
            Card::JC => write!(f, "JC"),
            Card::QC => write!(f, "QC"),
            Card::KC => write!(f, "KC"),
            Card::AC => write!(f, "AC"),
            Card::NH => write!(f, "9H"),
            Card::TH => write!(f, "TH"),
            Card::JH => write!(f, "JH"),
            Card::QH => write!(f, "QH"),
            Card::KH => write!(f, "KH"),
            Card::AH => write!(f, "AH"),
            Card::ND => write!(f, "9D"),
            Card::TD => write!(f, "TD"),
            Card::JD => write!(f, "JD"),
            Card::QD => write!(f, "QD"),
            Card::KD => write!(f, "KD"),
            Card::AD => write!(f, "AD"),
        }
    }
}

impl From<EAction> for Action {
    fn from(val: EAction) -> Self {
        let v: u8 = match val {
            EAction::Pickup => 0,
            EAction::Pass => 1,
            EAction::Clubs => 2,
            EAction::Spades => 3,
            EAction::Hearts => 4,
            EAction::Diamonds => 5,
            EAction::DealPlayer { c: x } => 50 + x as u8,
            EAction::Play { c: x } => 100 + x as u8,
            EAction::Discard { c: x } => 150 + x as u8,
            EAction::DealFaceUp { c: x } => 200 + x as u8,
        };
        Action(v)
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
            x if x >= 200 => EAction::DealFaceUp {
                c: Card::from(x - 50),
            },
            x if x >= 150 => EAction::Discard {
                c: Card::from(x - 50),
            },
            x if x >= 100 => EAction::Play {
                c: Card::from(x - 50),
            },
            x if x >= 50 => EAction::DealPlayer {
                c: Card::from(x - 50),
            },
            _ => panic!("invalid action to cast: {}", value),
        }
    }
}

impl Display for EAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        EAction_fmt(self, f)
    }
}

impl Debug for EAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        EAction_fmt(self, f)
    }
}

fn EAction_fmt(v: &EAction, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match v {
        EAction::Clubs => f.write_char('C'),
        EAction::Spades => f.write_char('S'),
        EAction::Hearts => f.write_char('H'),
        EAction::Diamonds => f.write_char('D'),
        EAction::Pickup => f.write_char('T'),
        EAction::Pass => f.write_char('P'),
        EAction::Play { c: x } => f.write_str(&x.to_string()),
        EAction::DealPlayer { c: x } => f.write_str(&x.to_string()),
        EAction::Discard { c: x } => f.write_str(&x.to_string()),
        EAction::DealFaceUp { c: x } => f.write_str(&x.to_string()),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Copy, Serialize, Deserialize)]
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
    put_card(c, &mut out);
    out.to_string()
}

fn put_card(c: u8, out: &mut str) {
    assert_eq!(out.len(), 2);

    let suit_char = match c / CARD_PER_SUIT {
        x if x == Suit::Clubs as u8 => 'C',
        x if x == Suit::Hearts as u8 => 'H',
        x if x == Suit::Spades as u8 => 'S',
        x if x == Suit::Diamonds as u8 => 'D',
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
