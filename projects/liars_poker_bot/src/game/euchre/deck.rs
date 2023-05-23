use std::ops::{Index, IndexMut};

use serde::{Deserialize, Serialize};

use crate::game::Player;

use super::actions::{Card, Suit, CARD_PER_SUIT};

const CARDS: &[Card] = &[
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

const SUITS: &[Suit] = &[Suit::Clubs, Suit::Diamonds, Suit::Spades, Suit::Hearts];
const JACK_RANK: u8 = 2;
const LEFT_RANK: usize = 6;
const RIGHT_RANK: usize = 7;

#[derive(Default, Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum CardLocation {
    Player0,
    Player1,
    Player2,
    Player3,
    FaceUp,
    #[default]
    None,
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
#[derive(Default, Debug, PartialEq, Eq, Clone, Copy, Serialize, Deserialize, Hash)]
pub(super) struct Deck {
    locations: [[CardLocation; 6]; 4],
    trump: Option<Suit>,
}

impl Deck {
    pub fn set_trump(&mut self, suit: Option<Suit>) {
        self.trump = suit;
    }

    /// Returns an isomorphic representation of the deck
    pub fn isomorphic_rep(&self) -> Self {
        let mut iso = *self;

        for &s in SUITS {
            if Some(s) == self.trump || Some(s.other_color()) == self.trump {
                continue; // don't do anything to trump colored suits yet
            }
            let mut r = 0;
            let mut last_card = CARD_PER_SUIT;
            // We downshift cards that are in the None location. These a 10 is as valuable in future hands as a 9
            // if the 9 has been played already
            while r < last_card {
                if iso.locations[s as usize][r as usize] == CardLocation::None {
                    iso.locations[s as usize][r as usize..].rotate_left(1);
                    last_card -= 1;
                } else {
                    r += 1;
                }
            }
        }

        // handle detecting trump for downshifting here

        iso
    }

    fn get_index(&self, c: Card) -> (usize, usize) {
        let rank = c.rank();
        let suit = c.suit();

        (suit as usize, rank as usize)
    }
}

impl Index<Card> for Deck {
    type Output = CardLocation;

    fn index(&self, index: Card) -> &Self::Output {
        let index = self.get_index(index);
        &self.locations[index.0][index.1]
    }
}

impl IndexMut<Card> for Deck {
    fn index_mut(&mut self, index: Card) -> &mut Self::Output {
        let index = self.get_index(index);
        &mut self.locations[index.0][index.1]
    }
}

impl<'a> IntoIterator for &'a Deck {
    type Item = (Card, CardLocation);

    type IntoIter = DeckIterator<'a>;

    fn into_iter(self) -> Self::IntoIter {
        DeckIterator {
            deck: self,
            index: 0,
        }
    }
}

pub struct DeckIterator<'a> {
    deck: &'a Deck,
    index: usize,
}

impl<'a> Iterator for DeckIterator<'a> {
    type Item = (Card, CardLocation);

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= CARDS.len() {
            return None;
        }

        let c = CARDS[self.index];
        let loc = self.deck[c];
        self.index += 1;
        Some((c, loc))
    }
}
