use std::ops::{Index, IndexMut};

use serde::{Deserialize, Serialize};

use super::actions::Card;

#[derive(Default, Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum CardLocation {
    Player0,
    Player1,
    Player2,
    Player3,
    FaceUp,
    #[default]
    None,
}

/// Track location of all euchre cards
#[derive(Default, Debug, PartialEq, Eq, Clone, Copy, Serialize, Deserialize)]
pub(super) struct Deck {
    locations: [CardLocation; 24],
}

impl Index<Card> for Deck {
    type Output = CardLocation;

    fn index(&self, index: Card) -> &Self::Output {
        &self.locations[index as usize]
    }
}

impl IndexMut<Card> for Deck {
    fn index_mut(&mut self, index: Card) -> &mut Self::Output {
        &mut self.locations[index as usize]
    }
}

impl<'a> IntoIterator for &'a Deck {
    type Item = (Card, &'a CardLocation);

    type IntoIter = DeckIterator<'a>;

    fn into_iter(self) -> Self::IntoIter {
        DeckIterator {
            deck: self,
            index: 0,
        }
    }
}

pub(super) struct DeckIterator<'a> {
    deck: &'a Deck,
    index: usize,
}

impl<'a> Iterator for DeckIterator<'a> {
    type Item = (Card, &'a CardLocation);

    fn next(&mut self) -> Option<Self::Item> {
        if self.index == self.deck.locations.len() {
            return None;
        }

        let location = &self.deck.locations[self.index];
        let card = Card::from(self.index as u8);
        self.index += 1;
        Some((card, location))
    }
}
