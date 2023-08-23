use std::{fmt::Debug, mem};

use anyhow::{bail, Ok};
use num_traits::{FromPrimitive, ToPrimitive};
use serde::{Deserialize, Serialize};

use crate::game::Player;

use super::actions::Card;

pub const CARDS: &[Card] = &[
    Card::NC,
    Card::TC,
    Card::JC,
    Card::QC,
    Card::KC,
    Card::AC,
    Card::NS,
    Card::TS,
    Card::JS,
    Card::QS,
    Card::KS,
    Card::AS,
    Card::NH,
    Card::TH,
    Card::JH,
    Card::QH,
    Card::KH,
    Card::AH,
    Card::ND,
    Card::TD,
    Card::JD,
    Card::QD,
    Card::KD,
    Card::AD,
];

#[derive(Default, Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum CardLocation {
    Player0,
    Player1,
    Player2,
    Player3,
    Played(Player),
    FaceUp,
    #[default]
    None,
}

impl CardLocation {
    fn idx(&self) -> usize {
        match self {
            CardLocation::Player0 => 0,
            CardLocation::Player1 => 1,
            CardLocation::Player2 => 2,
            CardLocation::Player3 => 3,
            CardLocation::Played(0) => 4,
            CardLocation::Played(1) => 5,
            CardLocation::Played(2) => 6,
            CardLocation::Played(3) => 7,
            CardLocation::FaceUp => 8,
            CardLocation::None => 9,
            _ => panic!("invalid played"),
        }
    }

    fn from_idx(idx: usize) -> Self {
        use CardLocation::*;
        match idx {
            0 => Player0,
            1 => Player1,
            2 => Player2,
            3 => Player3,
            4 => Played(0),
            5 => Played(1),
            6 => Played(2),
            7 => Played(3),
            8 => FaceUp,
            9 => None,
            _ => panic!("invalid index"),
        }
    }

    pub fn to_player(self) -> Option<Player> {
        match self {
            CardLocation::Player0 => Some(0),
            CardLocation::Player1 => Some(1),
            CardLocation::Player2 => Some(2),
            CardLocation::Player3 => Some(3),
            _ => None,
        }
    }
}

impl From<Player> for CardLocation {
    fn from(value: Player) -> Self {
        match value {
            0 => Self::Player0,
            1 => Self::Player1,
            2 => Self::Player2,
            3 => Self::Player3,
            _ => panic!("only support converting to player values"),
        }
    }
}

/// Track location of all euchre cards
#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Deserialize, Hash)]
pub(super) struct Deck {
    locations: [Hand; 10],
}

impl Default for Deck {
    fn default() -> Self {
        let mut locations: [Hand; 10] = Default::default();
        for c in CARDS {
            locations[CardLocation::None.idx()].add(*c)
        }

        Self { locations }
    }
}

impl Deck {
    /// Return the face up card if it exists
    pub fn face_up(&self) -> Option<Card> {
        let hand = self.locations[CardLocation::FaceUp.idx()];
        hand.card()
    }

    pub fn played(&self, player: Player) -> Option<Card> {
        let hand = self.locations[CardLocation::Played(player).idx()];
        hand.card()
    }

    pub fn set(&mut self, card: Card, loc: CardLocation) {
        // remove the card everywhere
        for locs in self.locations.iter_mut() {
            locs.remove(card);
        }

        // then set its final spot
        self.locations[loc.idx()].add(card);
    }

    pub fn get(&self, card: Card) -> CardLocation {
        for (i, hand) in self.locations.iter().enumerate() {
            if hand.contains(card) {
                return CardLocation::from_idx(i);
            }
        }

        panic!("Card not found in deck")
    }

    pub fn get_all(&self, loc: CardLocation) -> Hand {
        self.locations[loc.idx()]
    }

    /// Moves a card from the players hand to the play location
    pub fn play(&mut self, card: Card, player: Player) -> anyhow::Result<()> {
        let player_loc = CardLocation::from(player);

        let player_hand = &mut self.locations[player_loc.idx()];
        if !player_hand.contains(card) {
            let cards = player_hand.cards();
            let loc = self.get(card);
            bail!(
                "Tried to play a card not in the players hand. Card: {}, location: {:?}, hand: {:?}",
                card,
                loc,
                cards
            );
        }

        player_hand.remove(card);

        let played_hand = &mut self.locations[CardLocation::Played(player).idx()];
        played_hand.add(card);

        if !played_hand.len() == 1 {
            bail!("Attempted to play more than one card");
        }

        Ok(())
    }
}

/// Performant representation of collection of cards using a bit mask
#[derive(Copy, Clone, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Hand {
    mask: u32,
}

impl Hand {
    /// Return a hand containing all cards
    pub fn all_cards() -> Self {
        Self {
            mask: 0b00000000111111111111111111111111,
        }
    }

    /// Adds a card to the hand
    pub fn add(&mut self, card: Card) {
        self.mask |= ToPrimitive::to_u32(&card).unwrap();
    }

    pub fn remove(&mut self, card: Card) {
        self.mask &= !ToPrimitive::to_u32(&card).unwrap();
    }

    /// Remove all cards in hand from self
    pub fn remove_all(&mut self, hand: Hand) {
        self.mask &= !hand.mask;
    }

    pub fn add_all(&mut self, hand: Hand) {
        self.mask |= hand.mask;
    }

    pub fn contains(&self, card: Card) -> bool {
        self.mask & (card as u32) > 0
    }

    pub fn len(&self) -> usize {
        self.mask.count_ones() as usize
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn cards(&self) -> Vec<Card> {
        let mut mask = self.mask;
        let mut cards = Vec::with_capacity(mask.count_ones() as usize);

        while mask.count_ones() > 0 {
            let bit_index = mask.trailing_zeros();
            let card_rep = 1 << bit_index;
            mask &= !card_rep;
            cards.push(FromPrimitive::from_u32(card_rep).unwrap())
        }

        cards
    }

    /// Returns a single card if there is only one in the hand
    pub fn card(&self) -> Option<Card> {
        if self.len() != 1 {
            return None;
        }
        FromPrimitive::from_u32(self.mask)
    }

    #[deprecated]
    pub fn mask(&self) -> u32 {
        self.mask
    }

    pub fn from_mask(mask: u32) -> Self {
        Self { mask }
    }
}

impl Debug for Hand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{:?}", self.cards()))
    }
}

impl IntoIterator for Hand {
    type Item = Card;

    type IntoIter = HandIterator;

    fn into_iter(self) -> Self::IntoIter {
        HandIterator { mask: self.mask }
    }
}

pub struct HandIterator {
    mask: u32,
}

impl Iterator for HandIterator {
    type Item = Card;

    fn next(&mut self) -> Option<Self::Item> {
        if self.mask.count_ones() == 0 {
            return None;
        }

        let bit_index = self.mask.trailing_zeros();
        let card_rep = 1 << bit_index;
        self.mask &= !card_rep;

        // For performance purposes, we directly case the memory in the constructed
        // mask to a card
        let card;
        unsafe {
            card = mem::transmute_copy(&card_rep);
        }
        Some(card)
    }
}

#[cfg(test)]
mod tests {
    use crate::game::euchre::actions::Card::*;

    use super::Hand;

    #[test]
    fn test_hand() {
        let mut hand = Hand::default();

        assert_eq!(hand.len(), 0);
        hand.add(JS);
        hand.add(TD);

        assert_eq!(hand.len(), 2);
        assert!(hand.contains(JS));
        assert!(hand.contains(TD));
        assert!(!hand.contains(QS));

        assert_eq!(hand.cards(), vec![JS, TD])
    }
}
