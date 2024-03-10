#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Card(u64);

impl Card {
    pub fn new(rank: Rank, suit: Suit) -> Card {
        let mut card_data: [u16; 4] = [0; 4];
        card_data[suit as usize] = rank.into();
        card_data.into()
    }
}

impl From<[u16; 4]> for Card {
    fn from(value: [u16; 4]) -> Self {
        unsafe { std::mem::transmute(value) }
    }
}

/// Collection of Cards
#[derive(Default, Clone, Copy)]
pub struct Hand(u64);

impl Hand {
    /// Full deck of 52 playing cards
    pub fn standard() -> Self {
        Self(0b0111111111111100011111111111110001111111111111000111111111111100)
    }

    pub fn len(&self) -> usize {
        self.0.count_ones() as usize
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn remove(&mut self, card: Card) {
        self.0 &= !card.0
    }
}

impl Iterator for Hand {
    type Item = Card;

    fn next(&mut self) -> Option<Self::Item> {
        if self.is_empty() {
            return None;
        }

        let card = Card(1 << self.0.trailing_zeros());
        self.remove(card);
        Some(card)
    }
}

#[repr(u16)]
pub enum Rank {
    Two = 1 << 2,
    Three = 1 << 3,
    Four = 1 << 4,
    Five = 1 << 5,
    Six = 1 << 6,
    Seven = 1 << 7,
    Eight = 1 << 8,
    Nine = 1 << 9,
    Ten = 1 << 10,
    Jack = 1 << 11,
    Queen = 1 << 12,
    King = 1 << 13,
    Ace = 1 << 14,
}

impl From<Rank> for u16 {
    fn from(value: Rank) -> Self {
        value as u16
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Suit {
    Spade = 0,
    Clubs = 1,
    Diamonds = 2,
    Hearts = 3,
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn test_standard_deck() {
        let deck = Hand::standard();
        assert_eq!(deck.len(), 52);

        let mut deck_set = HashSet::new();
        deck.for_each(|c| {
            deck_set.insert(c);
        });

        let mut should_set = HashSet::new();
        for suit in [Suit::Spade, Suit::Clubs, Suit::Diamonds, Suit::Hearts] {
            use Rank::*;
            for rank in [
                Two, Three, Four, Five, Six, Seven, Eight, Nine, Ten, Jack, Queen, King, Ace,
            ] {
                should_set.insert(Card::new(rank, suit));
            }
        }

        assert_eq!(deck_set, should_set);
    }
}
