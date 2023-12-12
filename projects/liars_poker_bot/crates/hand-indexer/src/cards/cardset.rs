use std::{fmt::Debug, path::Display};

use itertools::Itertools;

use crate::{cards::SPADES, rankset::RankSet, Rank};

use super::{Card, Deck, Suit};

#[derive(Clone, Copy, Default, PartialEq, PartialOrd, Hash, Eq, Ord)]
pub struct CardSet(pub(super) u64);

impl CardSet {
    /// Create a CardSet with all cards
    pub fn all() -> Self {
        Self(!0)
    }

    pub fn pop_highest(&mut self) -> Option<Card> {
        let highest = self.highest()?;
        self.remove(highest);
        Some(highest)
    }

    pub fn pop_lowest(&mut self) -> Option<Card> {
        let lowest = self.lowest()?;
        self.remove(lowest);
        Some(lowest)
    }

    pub fn insert(&mut self, c: Card) {
        self.0 |= c.0;
    }

    pub fn insert_all(&mut self, set: CardSet) {
        self.0 |= set.0;
    }

    pub fn highest(&self) -> Option<Card> {
        if self.is_empty() {
            return None;
        }

        let rank = 64 - self.0.leading_zeros() - 1;
        Some(Card(1 << rank))
    }

    pub fn lowest(&self) -> Option<Card> {
        if self.is_empty() {
            return None;
        }

        let rank = self.0.trailing_zeros();
        Some(Card(1 << rank))
    }

    pub fn increment_highest(&self) -> Option<CardSet> {
        let mut new = *self;
        let highest_mask = new.highest()?.0;
        new.0 &= !highest_mask;
        new.0 |= highest_mask.checked_shl(1)?;
        Some(new)
    }

    pub fn remove(&mut self, card: Card) {
        self.0 &= !card.0;
    }

    pub fn remove_all(&mut self, set: CardSet) {
        self.0 &= !set.0;
    }

    pub fn constains_all(&self, set: CardSet) -> bool {
        self.0 | set.0 == self.0
    }

    pub fn contains(&self, card: Card) -> bool {
        self.0 & card.0 > 0
    }

    pub fn is_empty(&self) -> bool {
        self.0 == 0
    }

    pub fn len(&self) -> usize {
        self.0.count_ones() as usize
    }

    /// Returns the number of items in self that are also in `set`
    pub fn count(&self, suit: &Suit) -> usize {
        (self.0 & suit.0).count_ones() as usize
    }
}

impl Debug for CardSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{:#b}", self.0))
    }
}

/// Convert a vector of ranksets into a Cardset with a decks suit configuration
pub(super) fn to_cardset(ranksets: &Vec<Vec<RankSet>>, deck: &Deck) -> Vec<CardSet> {
    let mut sets = Vec::new();
    let rounds = ranksets.iter().map(|x| x.len()).max().unwrap();

    for r in 0..rounds {
        let mut set = CardSet::default();
        let mut suit_offset = 0;
        for (s, rankset) in deck.suits.iter().zip(ranksets) {
            let cards = rankset.get(r).copied().unwrap_or_default();
            set.0 |= (cards.0 as u64) << suit_offset;
            suit_offset += s.0.count_ones();
        }
        sets.push(set);
    }

    sets
}

impl From<CardSet> for [RankSet; 4] {
    fn from(value: CardSet) -> Self {
        unsafe { std::mem::transmute(value.0) }
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_cardset() {
        let mut set = CardSet::default();
        set.insert(Card::new(1));
        set.insert(Card::new(5));

        set = set.increment_highest().unwrap();
        assert_eq!(set.highest().unwrap(), Card::new(6));
        assert!(set.contains(Card::new(1)));

        assert_eq!(set.lowest().unwrap(), Card::new(1));
        assert_eq!(set.pop_lowest().unwrap(), Card::new(1));
        assert_eq!(set.lowest().unwrap(), Card::new(6));
    }
}
