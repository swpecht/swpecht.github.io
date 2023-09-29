use std::fmt::{Debug, Display, Write};

use num_derive::{FromPrimitive, ToPrimitive};
use num_traits::FromPrimitive;
use serde::{Deserialize, Serialize};

use crate::game::Action;

#[derive(
    PartialEq, Clone, Copy, Serialize, Deserialize, Eq, FromPrimitive, ToPrimitive, PartialOrd, Ord,
)]
#[repr(u32)]
pub enum EAction {
    NC = Card::NC as u32,
    TC = Card::TC as u32,
    JC = Card::JC as u32,
    QC = Card::QC as u32,
    KC = Card::KC as u32,
    AC = Card::AC as u32,
    NS = Card::NS as u32,
    TS = Card::TS as u32,
    JS = Card::JS as u32,
    QS = Card::QS as u32,
    KS = Card::KS as u32,
    AS = Card::AS as u32,
    NH = Card::NH as u32,
    TH = Card::TH as u32,
    JH = Card::JH as u32,
    QH = Card::QH as u32,
    KH = Card::KH as u32,
    AH = Card::AH as u32,
    ND = Card::ND as u32,
    TD = Card::TD as u32,
    JD = Card::JD as u32,
    QD = Card::QD as u32,
    KD = Card::KD as u32,
    AD = Card::AD as u32,
    Pickup = 0b1000000000000000000000000,
    Pass = 0b10000000000000000000000000,
    Clubs = 0b100000000000000000000000000,
    Spades = 0b1000000000000000000000000000,
    Hearts = 0b10000000000000000000000000000,
    Diamonds = 0b100000000000000000000000000000,
    /// Value to differentiate discard states from player 0 states
    DiscardMarker = 0b1000000000000000000000000000000,
}

impl EAction {
    pub fn card(&self) -> Card {
        unsafe { std::mem::transmute(*self) }
    }
}

impl From<EAction> for Action {
    fn from(val: EAction) -> Self {
        let v: u8 = (val as u32).trailing_zeros().try_into().unwrap();
        Action(v)
    }
}

impl From<Action> for EAction {
    fn from(value: Action) -> Self {
        let repr = 1 << value.0;
        unsafe { std::mem::transmute(repr) }
    }
}

impl From<Card> for EAction {
    fn from(value: Card) -> Self {
        unsafe { std::mem::transmute(value) }
    }
}

impl From<Suit> for EAction {
    fn from(value: Suit) -> Self {
        match value {
            Suit::Clubs => EAction::Clubs,
            Suit::Spades => EAction::Spades,
            Suit::Hearts => EAction::Hearts,
            Suit::Diamonds => EAction::Diamonds,
        }
    }
}

impl Display for EAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        eaction_fmt(self, f)
    }
}

impl Debug for EAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        eaction_fmt(self, f)
    }
}

fn eaction_fmt(v: &EAction, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match v {
        EAction::Clubs => f.write_char('C'),
        EAction::Spades => f.write_char('S'),
        EAction::Hearts => f.write_char('H'),
        EAction::Diamonds => f.write_char('D'),
        EAction::Pickup => f.write_char('T'),
        EAction::Pass => f.write_char('P'),
        EAction::DiscardMarker => f.write_str("|Dis|"),
        _ => f.write_str(&v.card().to_string()),
    }
}

pub const CLUBS_MASK: u32 = 0b00000000000000000000000000111111;
pub const SPADES_MASK: u32 = 0b00000000000000000000111111000000;
pub const HEART_MASK: u32 = 0b00000000000000111111000000000000;
pub const DIAMONDS_MASK: u32 = 0b00000000111111000000000000000000;

/// Represent cards in a deck, represented as a bitmask
#[derive(
    Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Debug, Hash, FromPrimitive, ToPrimitive,
)]
#[repr(u32)]
pub enum Card {
    NC = 0b1,
    TC = 0b10,
    JC = 0b100,
    QC = 0b1000,
    KC = 0b10000,
    AC = 0b100000,
    NS = 0b1000000,
    TS = 0b10000000,
    JS = 0b100000000,
    QS = 0b1000000000,
    KS = 0b10000000000,
    AS = 0b100000000000,
    NH = 0b1000000000000,
    TH = 0b10000000000000,
    JH = 0b100000000000000,
    QH = 0b1000000000000000,
    KH = 0b10000000000000000,
    AH = 0b100000000000000000,
    ND = 0b1000000000000000000,
    TD = 0b10000000000000000000,
    JD = 0b100000000000000000000,
    QD = 0b1000000000000000000000,
    KD = 0b10000000000000000000000,
    AD = 0b100000000000000000000000,
}

impl Card {
    pub fn mask(&self) -> u32 {
        *self as u32
    }

    pub fn suit(&self) -> Suit {
        let suit_id = (*self as u32).trailing_zeros() / 6;
        FromPrimitive::from_u32(suit_id).unwrap()
    }

    pub fn to_idx(&self) -> usize {
        match self {
            Card::NC => 0,
            Card::TC => 1,
            Card::JC => 2,
            Card::QC => 3,
            Card::KC => 4,
            Card::AC => 5,
            Card::NS => 6,
            Card::TS => 7,
            Card::JS => 8,
            Card::QS => 9,
            Card::KS => 10,
            Card::AS => 11,
            Card::NH => 12,
            Card::TH => 13,
            Card::JH => 14,
            Card::QH => 15,
            Card::KH => 16,
            Card::AH => 17,
            Card::ND => 18,
            Card::TD => 19,
            Card::JD => 20,
            Card::QD => 21,
            Card::KD => 22,
            Card::AD => 23,
        }
    }

    pub fn icon(&self) -> &str {
        match self {
            Card::NC => "🃙",
            Card::TC => "🃚",
            Card::JC => "🃛",
            Card::QC => "🃝",
            Card::KC => "🃞",
            Card::AC => "🃑",
            Card::NS => "🂩",
            Card::TS => "🂪",
            Card::JS => "🂫",
            Card::QS => "🂭",
            Card::KS => "🂮",
            Card::AS => "🂡",
            Card::NH => "🂹",
            Card::TH => "🂺",
            Card::JH => "🂻",
            Card::QH => "🂽",
            Card::KH => "🂾",
            Card::AH => "🂱",
            Card::ND => "🃉",
            Card::TD => "🃊",
            Card::JD => "🃋",
            Card::QD => "🃍",
            Card::KD => "🃎",
            Card::AD => "🃁",
        }
    }

    /// Returns a card of the same rank for the new suit
    pub fn to_suit(&self, suit: Suit) -> Card {
        use Card::*;

        match suit {
            Suit::Clubs => match self {
                NC => NC,
                TC => TC,
                JC => JC,
                QC => QC,
                KC => KC,
                AC => AC,
                NS => NC,
                TS => TC,
                JS => JC,
                QS => QC,
                KS => KC,
                AS => AC,
                NH => NC,
                TH => TC,
                JH => JC,
                QH => QC,
                KH => KC,
                AH => AC,
                ND => NC,
                TD => TC,
                JD => JC,
                QD => QC,
                KD => KC,
                AD => AC,
            },
            Suit::Spades => match self {
                NC => NS,
                TC => TS,
                JC => JS,
                QC => QS,
                KC => KS,
                AC => AS,
                NS => NS,
                TS => TS,
                JS => JS,
                QS => QS,
                KS => KS,
                AS => AS,
                NH => NS,
                TH => TS,
                JH => JS,
                QH => QS,
                KH => KS,
                AH => AS,
                ND => NS,
                TD => TS,
                JD => JS,
                QD => QS,
                KD => KS,
                AD => AS,
            },
            Suit::Hearts => match self {
                NC => NH,
                TC => TH,
                JC => JH,
                QC => QH,
                KC => KH,
                AC => AH,
                NS => NH,
                TS => TH,
                JS => JH,
                QS => QH,
                KS => KH,
                AS => AH,
                NH => NH,
                TH => TH,
                JH => JH,
                QH => QH,
                KH => KH,
                AH => AH,
                ND => NH,
                TD => TH,
                JD => JH,
                QD => QH,
                KD => KH,
                AD => AH,
            },
            Suit::Diamonds => match self {
                NC => ND,
                TC => TD,
                JC => JD,
                QC => QD,
                KC => KD,
                AC => AD,
                NS => ND,
                TS => TD,
                JS => JD,
                QS => QD,
                KS => KD,
                AS => AD,
                NH => ND,
                TH => TD,
                JH => JD,
                QH => QD,
                KH => KD,
                AH => AD,
                ND => ND,
                TD => TD,
                JD => JD,
                QD => QD,
                KD => KD,
                AD => AD,
            },
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
            "9s" => Self::NS,
            "Ts" => Self::TS,
            "Js" => Self::JS,
            "Qs" => Self::QS,
            "Ks" => Self::KS,
            "As" => Self::AS,
            "9c" => Self::NC,
            "Tc" => Self::TC,
            "Jc" => Self::JC,
            "Qc" => Self::QC,
            "Kc" => Self::KC,
            "Ac" => Self::AC,
            "9h" => Self::NH,
            "Th" => Self::TH,
            "Jh" => Self::JH,
            "Qh" => Self::QH,
            "Kh" => Self::KH,
            "Ah" => Self::AH,
            "9d" => Self::ND,
            "Td" => Self::TD,
            "Jd" => Self::JD,
            "Qd" => Self::QD,
            "Kd" => Self::KD,
            "Ad" => Self::AD,
            _ => panic!("invalid card string: {}", value),
        }
    }
}

impl Display for Card {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Card::NS => write!(f, "9s"),
            Card::TS => write!(f, "Ts"),
            Card::JS => write!(f, "Js"),
            Card::QS => write!(f, "Qs"),
            Card::KS => write!(f, "Ks"),
            Card::AS => write!(f, "As"),
            Card::NC => write!(f, "9c"),
            Card::TC => write!(f, "Tc"),
            Card::JC => write!(f, "Jc"),
            Card::QC => write!(f, "Qc"),
            Card::KC => write!(f, "Kc"),
            Card::AC => write!(f, "Ac"),
            Card::NH => write!(f, "9h"),
            Card::TH => write!(f, "Th"),
            Card::JH => write!(f, "Jh"),
            Card::QH => write!(f, "Qh"),
            Card::KH => write!(f, "Kh"),
            Card::AH => write!(f, "Ah"),
            Card::ND => write!(f, "9d"),
            Card::TD => write!(f, "Td"),
            Card::JD => write!(f, "Jd"),
            Card::QD => write!(f, "Qd"),
            Card::KD => write!(f, "Kd"),
            Card::AD => write!(f, "Ad"),
        }
    }
}

#[derive(
    Debug, Clone, PartialEq, Eq, Copy, Serialize, Deserialize, Hash, FromPrimitive, ToPrimitive,
)]
pub enum Suit {
    Clubs = 0,
    Spades,
    Hearts,
    Diamonds,
}

impl Suit {
    pub fn icon(&self) -> &str {
        match self {
            Suit::Clubs => "♣",
            Suit::Spades => "♠",
            Suit::Hearts => "♥",
            Suit::Diamonds => "♦",
        }
    }
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

#[cfg(test)]
mod tests {
    use crate::game::{
        euchre::{
            actions::{Card, EAction},
            deck::CARDS,
        },
        Action,
    };

    #[test]
    fn test_euchre_actions() {
        assert_eq!(EAction::JS as u32, Card::JS as u32);
        assert_eq!(EAction::JS, Card::JS.into());

        let a: Action = EAction::JS.into();
        assert_eq!(EAction::from(a), EAction::JS);

        for c in CARDS {
            let ea = EAction::from(*c);
            let a = Action::from(ea);
            let ea2 = EAction::from(a);
            let card = ea2.card();
            assert_eq!(card, *c);
        }
    }
}
