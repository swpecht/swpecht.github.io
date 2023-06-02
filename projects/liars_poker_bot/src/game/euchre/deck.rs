use std::ops::{Index, IndexMut};

use serde::{Deserialize, Serialize};

use crate::game::Player;

use super::actions::Card;

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
