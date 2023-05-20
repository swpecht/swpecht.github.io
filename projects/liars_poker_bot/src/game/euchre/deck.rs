use std::ops::{Deref, Index, IndexMut};

use serde::{Deserialize, Serialize};

use crate::game::Player;

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

impl Deref for Deck {
    type Target = [CardLocation; 24];

    fn deref(&self) -> &Self::Target {
        &self.locations
    }
}
