use std::fmt::{Debug, Display, Write};

use num_derive::{FromPrimitive, ToPrimitive};
use num_traits::{FromPrimitive, ToPrimitive};
use serde::{Deserialize, Serialize};

use crate::game::Action;

#[derive(PartialEq, Clone, Copy, Serialize, Deserialize, Eq, FromPrimitive, ToPrimitive)]
pub enum EAction {
    Pickup,
    Pass,
    Clubs,
    Spades,
    Hearts,
    Diamonds,
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
    PrivateNC,
    PrivateTC,
    PrivateJC,
    PrivateQC,
    PrivateKC,
    PrivateAC,
    PrivateNS,
    PrivateTS,
    PrivateJS,
    PrivateQS,
    PrivateKS,
    PrivateAS,
    PrivateNH,
    PrivateTH,
    PrivateJH,
    PrivateQH,
    PrivateKH,
    PrivateAH,
    PrivateND,
    PrivateTD,
    PrivateJD,
    PrivateQD,
    PrivateKD,
    PrivateAD,
    /// Value to differentiate discard states from player 0 states
    DiscardMarker,
}

impl EAction {
    pub fn card(&self) -> Card {
        use EAction::*;
        match self {
            NC | PrivateNC => Card::NC,
            TC | PrivateTC => Card::TC,
            JC | PrivateJC => Card::JC,
            QC | PrivateQC => Card::QC,
            KC | PrivateKC => Card::KC,
            AC | PrivateAC => Card::AC,
            NS | PrivateNS => Card::NS,
            TS | PrivateTS => Card::TS,
            JS | PrivateJS => Card::JS,
            QS | PrivateQS => Card::QS,
            KS | PrivateKS => Card::KS,
            AS | PrivateAS => Card::AS,
            NH | PrivateNH => Card::NH,
            TH | PrivateTH => Card::TH,
            JH | PrivateJH => Card::JH,
            QH | PrivateQH => Card::QH,
            KH | PrivateKH => Card::KH,
            AH | PrivateAH => Card::AH,
            ND | PrivateND => Card::ND,
            TD | PrivateTD => Card::TD,
            JD | PrivateJD => Card::JD,
            QD | PrivateQD => Card::QD,
            KD | PrivateKD => Card::KD,
            AD | PrivateAD => Card::AD,
            _ => panic!("can't get card on: {:?}", self),
        }
    }

    pub fn public_action(card: Card) -> Self {
        match card {
            Card::NC => EAction::NC,
            Card::TC => EAction::TC,
            Card::JC => EAction::JC,
            Card::QC => EAction::QC,
            Card::KC => EAction::KC,
            Card::AC => EAction::AC,
            Card::NS => EAction::NS,
            Card::TS => EAction::TS,
            Card::JS => EAction::JS,
            Card::QS => EAction::QS,
            Card::KS => EAction::KS,
            Card::AS => EAction::AS,
            Card::NH => EAction::NH,
            Card::TH => EAction::TH,
            Card::JH => EAction::JH,
            Card::QH => EAction::QH,
            Card::KH => EAction::KH,
            Card::AH => EAction::AH,
            Card::ND => EAction::ND,
            Card::TD => EAction::TD,
            Card::JD => EAction::JD,
            Card::QD => EAction::QD,
            Card::KD => EAction::KD,
            Card::AD => EAction::AD,
        }
    }

    pub fn private_action(card: Card) -> Self {
        match card {
            Card::NC => EAction::PrivateNC,
            Card::TC => EAction::PrivateTC,
            Card::JC => EAction::PrivateJC,
            Card::QC => EAction::PrivateQC,
            Card::KC => EAction::PrivateKC,
            Card::AC => EAction::PrivateAC,
            Card::NS => EAction::PrivateNS,
            Card::TS => EAction::PrivateTS,
            Card::JS => EAction::PrivateJS,
            Card::QS => EAction::PrivateQS,
            Card::KS => EAction::PrivateKS,
            Card::AS => EAction::PrivateAS,
            Card::NH => EAction::PrivateNH,
            Card::TH => EAction::PrivateTH,
            Card::JH => EAction::PrivateJH,
            Card::QH => EAction::PrivateQH,
            Card::KH => EAction::PrivateKH,
            Card::AH => EAction::PrivateAH,
            Card::ND => EAction::PrivateND,
            Card::TD => EAction::PrivateTD,
            Card::JD => EAction::PrivateJD,
            Card::QD => EAction::PrivateQD,
            Card::KD => EAction::PrivateKD,
            Card::AD => EAction::PrivateAD,
        }
    }

    pub fn is_public(&self) -> bool {
        !matches!(
            self,
            EAction::PrivateNC
                | EAction::PrivateTC
                | EAction::PrivateJC
                | EAction::PrivateQC
                | EAction::PrivateKC
                | EAction::PrivateAC
                | EAction::PrivateNS
                | EAction::PrivateTS
                | EAction::PrivateJS
                | EAction::PrivateQS
                | EAction::PrivateKS
                | EAction::PrivateAS
                | EAction::PrivateNH
                | EAction::PrivateTH
                | EAction::PrivateJH
                | EAction::PrivateQH
                | EAction::PrivateKH
                | EAction::PrivateAH
                | EAction::PrivateND
                | EAction::PrivateTD
                | EAction::PrivateJD
                | EAction::PrivateQD
                | EAction::PrivateKD
                | EAction::PrivateAD
        )
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

impl From<EAction> for Action {
    fn from(val: EAction) -> Self {
        let v: u8 = ToPrimitive::to_u8(&val).unwrap();
        Action(v)
    }
}

impl From<Action> for EAction {
    fn from(value: Action) -> Self {
        FromPrimitive::from_u8(value.0).unwrap()
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
