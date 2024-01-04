use std::fmt::Debug;

use crate::rankset::RankSet;

use super::{Card, Deck, Suit, MAX_CARDS_PER_SUIT};

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

    /// Returns the next highest CardSet if one exists
    pub fn next(&self) -> Option<CardSet> {
        let mut ranks = <[RankSet; 4]>::from(*self);

        // try to increment the earliest rank set
        let mut i = 0;
        loop {
            if let Some(new) = increment_rankset(ranks[i]) {
                ranks[i] = new;
                break;
            }
            i += 1;

            // Can't increment anything
            if i >= 4 {
                return None;
            }
        }

        // todo: figure out how to jump to the next suit

        // set all the earlier ones to the smallest setting
        for s in 0..i {
            let num_ranks = ranks[i].len();
            ranks[i] = RankSet(!(!0 << num_ranks));
        }

        Some(ranks.into())
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

impl From<[RankSet; 4]> for CardSet {
    fn from(value: [RankSet; 4]) -> Self {
        unsafe { std::mem::transmute(value) }
    }
}

impl From<CardSet> for [u16; 4] {
    fn from(value: CardSet) -> Self {
        unsafe { std::mem::transmute(value.0) }
    }
}

/// Increments the set index, "carrying" when the last digit gets to
/// MAX_CARDS_PER_SUIT
fn increment_rankset(set: RankSet) -> Option<RankSet> {
    increment_rankset_r(set, MAX_CARDS_PER_SUIT as u8)
}

fn increment_rankset_r(mut set: RankSet, max_rank: u8) -> Option<RankSet> {
    let last = set.largest()?;
    // handle the simple case where no carrying occurs
    if last + 1 < max_rank {
        let highest_mask = 1 << last;
        set.0 &= !highest_mask;
        set.0 |= highest_mask.checked_shl(1)?;
        return Some(set);
    }

    // recursively do all the carrying for the base index
    set.pop_largest();
    set = increment_rankset_r(set, max_rank - 1)?;

    if set.largest()? + 1 < max_rank {
        set.insert(set.largest()? + 1);
        Some(set)
    } else {
        // no further indexes are possible
        None
    }
}

pub struct CardSetIterator {
    remaining_cards: CardSet,
}

impl Iterator for CardSetIterator {
    type Item = Card;

    fn next(&mut self) -> Option<Self::Item> {
        self.remaining_cards.pop_lowest()
    }
}

impl IntoIterator for CardSet {
    type Item = Card;

    type IntoIter = CardSetIterator;

    fn into_iter(self) -> Self::IntoIter {
        CardSetIterator {
            remaining_cards: self,
        }
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
