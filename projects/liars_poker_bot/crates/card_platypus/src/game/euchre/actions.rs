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
    DealPlayer {
        c: Card,
    },
    DealFaceUp {
        c: Card,
    },
    Discard {
        c: Card,
    },
    Play {
        c: Card,
    },
    /// Value to differentiate discard states from player 0 states
    DiscardMarker,
}

impl EAction {
    pub fn card(&self) -> Card {
        match self {
            EAction::Discard { c }
            | EAction::DealPlayer { c }
            | EAction::Play { c }
            | EAction::DealFaceUp { c } => *c,
            _ => panic!("can't get card on: {:?}", self),
        }
    }
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Debug, Hash)]
pub enum Card {
    NC,
    TC,
    JC,
    QC,
    KC,
    AC,
    NS,
    TS,
    JS,
    QS,
    KS,
    AS,
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
    pub fn suit(&self) -> Suit {
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
}

impl From<u8> for Card {
    fn from(value: u8) -> Self {
        match value {
            0 => Self::NC,
            1 => Self::TC,
            2 => Self::JC,
            3 => Self::QC,
            4 => Self::KC,
            5 => Self::AC,
            6 => Self::NS,
            7 => Self::TS,
            8 => Self::JS,
            9 => Self::QS,
            10 => Self::KS,
            11 => Self::AS,
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
            _ => panic!("invalid value to convert to card: {}", value),
        }
    }
}

impl From<Card> for u8 {
    fn from(value: Card) -> Self {
        value as u8
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
            EAction::DiscardMarker => 255,
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
            255 => EAction::DiscardMarker,
            x if x >= 250 => panic!("invalid action: {}", x),
            x if x >= 200 => EAction::DealFaceUp {
                c: Card::from(x - 200),
            },
            x if x >= 150 => EAction::Discard {
                c: Card::from(x - 150),
            },
            x if x >= 100 => EAction::Play {
                c: Card::from(x - 100),
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
        EAction::Play { c: x } => f.write_str(&x.to_string()),
        EAction::DealPlayer { c: x } => f.write_str(&x.to_string()),
        EAction::Discard { c: x } => f.write_str(&x.to_string()),
        EAction::DealFaceUp { c: x } => f.write_str(&x.to_string()),
        EAction::DiscardMarker => f.write_str(""),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Copy, Serialize, Deserialize, Hash)]
pub enum Suit {
    Clubs = 0,
    Spades,
    Hearts,
    Diamonds,
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
