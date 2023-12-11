use std::fmt::Debug;

use super::{Card, Suit};

#[derive(Clone, Copy, Default, PartialEq, PartialOrd)]
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
