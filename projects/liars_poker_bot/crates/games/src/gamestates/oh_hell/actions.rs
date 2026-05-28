use std::fmt::{Display, Write};

use serde::{Deserialize, Serialize};

use crate::Action;

/// All 52 playing cards. Discriminants are laid out as `suit * 13 + (rank - 2)`,
/// so spades occupy 0..13, clubs 13..26, hearts 26..39, diamonds 39..52.
///
/// Cards inside a suit are ordered lowest-to-highest (Two..Ace).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[repr(u8)]
#[rustfmt::skip]
#[allow(non_camel_case_types)]
pub enum OHCard {
    // Spades
    _2S = 0,  _3S, _4S, _5S, _6S, _7S, _8S, NS, TS, JS, QS, KS, AS,
    // Clubs
    _2C = 13, _3C, _4C, _5C, _6C, _7C, _8C, NC, TC, JC, QC, KC, AC,
    // Hearts
    _2H = 26, _3H, _4H, _5H, _6H, _7H, _8H, NH, TH, JH, QH, KH, AH,
    // Diamonds
    _2D = 39, _3D, _4D, _5D, _6D, _7D, _8D, ND, TD, JD, QD, KD, AD,
}

pub const OH_DECK_SIZE: usize = 52;

pub const OH_DECK: [OHCard; OH_DECK_SIZE] = {
    use OHCard::*;
    [
        _2S, _3S, _4S, _5S, _6S, _7S, _8S, NS, TS, JS, QS, KS, AS,
        _2C, _3C, _4C, _5C, _6C, _7C, _8C, NC, TC, JC, QC, KC, AC,
        _2H, _3H, _4H, _5H, _6H, _7H, _8H, NH, TH, JH, QH, KH, AH,
        _2D, _3D, _4D, _5D, _6D, _7D, _8D, ND, TD, JD, QD, KD, AD,
    ]
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[repr(u8)]
pub enum OHSuit {
    Spades = 0,
    Clubs = 1,
    Hearts = 2,
    Diamonds = 3,
}

impl OHSuit {
    pub const ALL: [OHSuit; 4] = [
        OHSuit::Spades,
        OHSuit::Clubs,
        OHSuit::Hearts,
        OHSuit::Diamonds,
    ];

    pub fn from_index(i: u8) -> OHSuit {
        match i {
            0 => OHSuit::Spades,
            1 => OHSuit::Clubs,
            2 => OHSuit::Hearts,
            3 => OHSuit::Diamonds,
            _ => panic!("invalid suit index: {}", i),
        }
    }
}

impl Display for OHSuit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let c = match self {
            OHSuit::Spades => 's',
            OHSuit::Clubs => 'c',
            OHSuit::Hearts => 'h',
            OHSuit::Diamonds => 'd',
        };
        f.write_char(c)
    }
}

impl OHCard {
    /// Returns the suit of this card. Computed directly from the discriminant.
    pub fn suit(self) -> OHSuit {
        OHSuit::from_index((self as u8) / 13)
    }

    /// Returns the rank (2..=14, with 11/12/13/14 = J/Q/K/A).
    pub fn rank(self) -> u8 {
        (self as u8) % 13 + 2
    }

    /// Build a card from a (rank, suit) pair. `rank` must be in 2..=14.
    pub fn make(rank: u8, suit: OHSuit) -> OHCard {
        assert!((2..=14).contains(&rank), "rank out of range: {}", rank);
        OHCard::from_index((suit as u8) * 13 + (rank - 2))
            .expect("computed index in range")
    }

    pub fn from_index(i: u8) -> Option<OHCard> {
        OH_DECK.get(i as usize).copied()
    }
}

impl Display for OHCard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let r = match self.rank() {
            n @ 2..=9 => (b'0' + n) as char,
            10 => 'T',
            11 => 'J',
            12 => 'Q',
            13 => 'K',
            14 => 'A',
            _ => unreachable!(),
        };
        write!(f, "{}{}", r, self.suit())
    }
}

/// First bid action's discriminant. Cards occupy [0, OH_DECK_SIZE);
/// bids occupy [BID_BASE, BID_BASE + max_bids].
pub const BID_BASE: u8 = OH_DECK_SIZE as u8;

/// An Oh Hell action: either playing/dealing a card, or making a bid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OHAction {
    Card(OHCard),
    Bid(u8),
}

impl From<OHAction> for Action {
    fn from(value: OHAction) -> Self {
        match value {
            OHAction::Card(c) => Action(c as u8),
            OHAction::Bid(n) => Action(BID_BASE + n),
        }
    }
}

impl From<OHCard> for OHAction {
    fn from(value: OHCard) -> Self {
        OHAction::Card(value)
    }
}

impl From<OHCard> for Action {
    fn from(value: OHCard) -> Self {
        Action(value as u8)
    }
}

impl From<Action> for OHAction {
    fn from(value: Action) -> Self {
        if (value.0 as usize) < OH_DECK_SIZE {
            OHAction::Card(OHCard::from_index(value.0).expect("valid card index"))
        } else {
            OHAction::Bid(value.0 - BID_BASE)
        }
    }
}

impl Display for OHAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OHAction::Card(c) => write!(f, "{}", c),
            OHAction::Bid(n) => write!(f, "B{}", n),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deck_has_52_cards() {
        assert_eq!(OH_DECK_SIZE, 52);
        assert_eq!(OH_DECK.len(), 52);
    }

    #[test]
    fn deck_has_no_duplicates() {
        use std::collections::HashSet;
        let set: HashSet<OHCard> = OH_DECK.iter().copied().collect();
        assert_eq!(set.len(), 52);
    }

    #[test]
    fn deck_has_13_of_each_suit() {
        for suit in OHSuit::ALL {
            let count = OH_DECK.iter().filter(|c| c.suit() == suit).count();
            assert_eq!(count, 13, "suit {} has {} cards", suit, count);
        }
    }

    #[test]
    fn deck_has_four_of_each_rank() {
        for rank in 2..=14u8 {
            let count = OH_DECK.iter().filter(|c| c.rank() == rank).count();
            assert_eq!(count, 4, "rank {} has {} cards", rank, count);
        }
    }

    #[test]
    fn card_action_roundtrip_all_52() {
        for &c in &OH_DECK {
            let a: Action = OHAction::Card(c).into();
            let back: OHAction = a.into();
            assert_eq!(back, OHAction::Card(c));
        }
    }

    #[test]
    fn bid_action_roundtrip() {
        for n in 0u8..=13 {
            let a: Action = OHAction::Bid(n).into();
            let back: OHAction = a.into();
            assert_eq!(back, OHAction::Bid(n));
        }
    }

    #[test]
    fn cards_sort_below_bids() {
        let card_max: Action = OHAction::Card(OHCard::AD).into();
        let bid_min: Action = OHAction::Bid(0).into();
        assert!(card_max < bid_min);
    }

    #[test]
    fn card_display_examples() {
        assert_eq!(OHCard::NS.to_string(), "9s");
        assert_eq!(OHCard::_2S.to_string(), "2s");
        assert_eq!(OHCard::TS.to_string(), "Ts");
        assert_eq!(OHCard::AH.to_string(), "Ah");
        assert_eq!(OHCard::_7D.to_string(), "7d");
        assert_eq!(OHCard::JC.to_string(), "Jc");
        assert_eq!(OHCard::AS.to_string(), "As");
    }

    #[test]
    fn suit_assignments_full_deck() {
        assert_eq!(OHCard::_2S.suit(), OHSuit::Spades);
        assert_eq!(OHCard::AS.suit(), OHSuit::Spades);
        assert_eq!(OHCard::NC.suit(), OHSuit::Clubs);
        assert_eq!(OHCard::AC.suit(), OHSuit::Clubs);
        assert_eq!(OHCard::_5H.suit(), OHSuit::Hearts);
        assert_eq!(OHCard::AD.suit(), OHSuit::Diamonds);
    }

    #[test]
    fn make_recovers_originals() {
        for &c in &OH_DECK {
            assert_eq!(OHCard::make(c.rank(), c.suit()), c);
        }
    }
}
