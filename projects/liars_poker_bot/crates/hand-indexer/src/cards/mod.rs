use itertools::{Combinations, Itertools};
use std::fmt::Debug;

use self::{cardset::CardSet, iterators::DeckIterator};

const SPADES: u64 = 0b1111111111111111;
const CLUBS: u64 = SPADES << 16;
const HEARTS: u64 = CLUBS << 16;
const DIAMONDS: u64 = HEARTS << 16;

pub(super) const MAX_CARDS: usize = 64;

pub mod cardset;
pub mod iterators;

/// Represents a single card
///
/// Cards are represented as a bit flipped in a u64
#[derive(PartialEq, Clone, Copy)]
pub struct Card(u64);

impl Card {
    pub fn new(idx: usize) -> Self {
        Card(1 << idx)
    }

    pub fn rank(&self) -> usize {
        self.0.trailing_zeros() as usize
    }
}

impl Debug for Card {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{:#b}", self.0))
    }
}

/// A bit mask determining which cards are in a suit
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Suit(u64);

impl Debug for Suit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{:#b}", self.0))
    }
}

/// Contains information about possible configurations of cards,
/// e.g. which cards are valid, what are the suits, etc.
#[derive(Copy, Clone)]
pub struct Deck {
    remaining_cards: CardSet,
    suits: [Suit; 4],
}

impl Deck {
    /// Returns a standard 52 card playing deck
    pub fn standard() -> Self {
        let deck = Self {
            remaining_cards: CardSet(
                0b0001111111111111000111111111111100011111111111110001111111111111,
            ),
            suits: [Suit(SPADES), Suit(CLUBS), Suit(HEARTS), Suit(DIAMONDS)],
        };
        assert_eq!(deck.remaining_cards.len(), 52);
        deck.validate();
        deck
    }

    /// Returns a euchre deck
    pub fn euchre() -> Self {
        let deck = Self {
            remaining_cards: CardSet(
                0b0000000000111111000000000011111100000000001111110000000000111111,
            ),
            suits: [Suit(SPADES), Suit(CLUBS), Suit(HEARTS), Suit(DIAMONDS)],
        };
        assert_eq!(deck.remaining_cards.len(), 24);
        deck.validate();
        deck
    }

    /// Returns if a given configuration is valid
    fn validate(&self) {
        // ensure no overlap in suits
        let mut all_suits = 0;
        for s in &self.suits {
            all_suits |= s.0;
        }
        assert_eq!(
            all_suits.count_ones(),
            self.suits.iter().map(|x| x.0.count_ones()).sum()
        );
    }

    /// Returns the lowest rank card in the deck by representation, this
    /// does not necessarily correspond to a cards value in a given game
    fn lowest(&self) -> Card {
        self.remaining_cards.lowest().unwrap()
    }

    fn pop(&mut self) -> Option<Card> {
        if self.is_empty() {
            return None;
        }
        let c = self.lowest();
        self.remaining_cards.remove(c);
        Some(c)
    }

    pub fn len(&self) -> usize {
        self.remaining_cards.0.count_ones() as usize
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Remove all cards lower than and including c from the deck
    pub fn remove_lower(&mut self, c: Card) {
        let rank = c.rank();
        self.remaining_cards.0 &= !0 << (rank + 1);
    }
}

impl IntoIterator for Deck {
    type Item = Card;

    type IntoIter = DeckIterator;

    fn into_iter(self) -> Self::IntoIter {
        DeckIterator { deck: self }
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_basic_deck() {
        let mut deck = Deck::standard();
        assert_eq!(deck.lowest(), Card(0b1));
        deck.remaining_cards.remove(Card(0b1));
        assert_eq!(deck.lowest(), Card(0b10));

        assert_eq!(Deck::standard().into_iter().count(), 52);

        let mut set = CardSet::default();
        set.insert(Card(0b10));
        assert_eq!(set.highest().unwrap(), Card(0b10));
        set.insert(Card(0b100));
        assert_eq!(set.highest().unwrap(), Card(0b100));
        set.insert(Card(0b1));
        assert_eq!(set.highest().unwrap(), Card(0b100));
    }
}
