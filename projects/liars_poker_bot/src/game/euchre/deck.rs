use std::ops::{Index, IndexMut};

use serde::{Deserialize, Serialize};

use crate::game::Player;

use super::actions::{Card, Suit, CARD_PER_SUIT};

const JACK_RANK: usize = 2;
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
    locations: [[CardLocation; 8]; 4],
    trump: Option<Suit>,
}

impl Deck {
    /// Return a deck with the new trump
    pub fn with_new_trump(self, trump: Option<Suit>) -> Self {
        let mut new = Deck {
            trump,
            ..Default::default()
        };
        for &c in CARDS {
            new[c] = self[c];
        }

        new
    }

    /// Returns an isomorphic representation of the deck
    pub fn isomorphic_rep(&self) -> Self {
        let mut iso = *self;

        for &s in SUITS {
            let mut r = 0;
            let mut last_card = 8;
            // We downshift cards that are in the None location. For example,a 10 is as valuable in future hands as a 9
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
        let rank = c.rank() as usize;
        let suit = c.suit();

        if self.trump.is_none() {
            return (suit as usize, rank);
        }

        let trump = self.trump.unwrap();
        let is_trump = suit == trump;
        let is_trump_color = suit.other_color() == self.trump.unwrap() || is_trump;

        match (is_trump, is_trump_color, rank) {
            (true, true, JACK_RANK) => (suit as usize, 7),
            (false, true, JACK_RANK) => (trump as usize, 6),
            (_, true, x) if x > JACK_RANK => (suit as usize, rank - 1),
            (_, true, x) if x < JACK_RANK => (suit as usize, rank),
            (false, false, _) => (suit as usize, rank),
            _ => panic!(
                "invalid card: {}, is_trump: {}, is_trump_color: {}, rank: {}",
                c, is_trump, is_trump_color, rank
            ),
        }
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

#[cfg(test)]
mod tests {
    use crate::game::euchre::{
        actions::{Card, Suit},
        deck::{CardLocation, Deck},
    };

    #[test]
    fn test_deck_iso_no_trump() {
        let mut d1 = Deck::default();

        d1[Card::NS] = CardLocation::Player0;
        d1[Card::TS] = CardLocation::Player0;

        let mut d2 = d1;

        assert_eq!(d1.isomorphic_rep(), d2.isomorphic_rep());
        d2[Card::JS] = CardLocation::Player0;

        assert!(d1.isomorphic_rep() != d2.isomorphic_rep());
        d2[Card::NS] = CardLocation::None;

        assert_eq!(d1.isomorphic_rep(), d2.isomorphic_rep());
    }

    #[test]
    fn test_deck_iso_across_suit() {
        todo!()
    }

    #[test]
    fn test_deck_iso_trump() {
        let mut d1 = Deck::default();

        d1[Card::NS] = CardLocation::Player0;
        d1[Card::TS] = CardLocation::Player0;
        d1 = d1.with_new_trump(Some(Suit::Spades));

        let mut d2 = d1;

        assert_eq!(d1.isomorphic_rep(), d2.isomorphic_rep());
        d2[Card::JS] = CardLocation::Player0;

        assert!(d1.isomorphic_rep() != d2.isomorphic_rep());
        d2[Card::NS] = CardLocation::None;
        // player 0  still has the 2 highest spades
        assert_eq!(d1.isomorphic_rep(), d2.isomorphic_rep());
        d2[Card::JC] = CardLocation::Player0;
        d2[Card::TS] = CardLocation::None;
        // player 0  still has the 2 highest spades, JC and JS
        assert_eq!(d1.isomorphic_rep(), d2.isomorphic_rep());

        // this persists even if we deal some other cards
        d1[Card::TH] = CardLocation::Player1;
        d2[Card::TH] = CardLocation::Player1;
        assert_eq!(d1.isomorphic_rep(), d2.isomorphic_rep());
    }
}
