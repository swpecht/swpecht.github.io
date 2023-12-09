use itertools::{Combinations, Itertools};
use std::fmt::Debug;

const SPADES: u64 = 0b1111111111111;
const CLUBS: u64 = SPADES << 13;
const HEARTS: u64 = CLUBS << 13;
const DIAMONDS: u64 = HEARTS << 13;

const MAX_CARDS: usize = 64;

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

#[derive(Clone, Copy, Default)]
pub struct CardSet(u64);

impl CardSet {
    pub fn insert(&mut self, c: Card) {
        self.0 |= c.0;
    }

    pub fn highest(&self) -> Card {
        let rank = 64 - self.0.leading_zeros() - 1;
        Card(1 << rank)
    }

    pub fn remove(&mut self, card: Card) {
        self.0 &= !card.0;
    }

    pub fn contains(&self, card: Card) -> bool {
        self.0 & card.0 > 0
    }
}

impl Debug for CardSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{:#b}", self.0))
    }
}

/// A bit mask determining which cards are in a suit
#[derive(Clone, Copy)]
pub struct Suit(u64);

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
            remaining_cards: CardSet(!(!0 << 52)),
            suits: [Suit(SPADES), Suit(CLUBS), Suit(HEARTS), Suit(DIAMONDS)],
        };
        deck.validate();
        deck
    }

    /// Returns a euchre deck
    pub fn euchre() -> Self {
        todo!()
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

    // Enumerates all possible combination of cards from the deck
    // ordering within a round doesn't matter. No simplifications are made, e.g. As is different from Ac
    // todo: should we make this an iterator
    pub fn enumerate_deals<const N: usize>(
        &self,
        cards_per_round: [usize; N],
    ) -> Vec<[CardSet; N]> {
        todo!()
    }

    /// Returns the lowest rank card in the deck by representation, this
    /// does not necessarily correspond to a cards value in a given game
    fn lowest(&self) -> Card {
        let rank = self.remaining_cards.0.trailing_zeros();
        Card(1 << rank)
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

pub struct DeckIterator {
    deck: Deck,
}

impl Iterator for DeckIterator {
    type Item = Card;

    fn next(&mut self) -> Option<Self::Item> {
        self.deck.pop()
    }
}

/// Enumerates over all possible deals
///
/// Want to store an array or iterators for the different combinations of cards
struct DealEnumerationIterator<const R: usize> {
    cards_per_round: [usize; R],
    /// the index for the next card set
    /// these are the counter variables if generating in a loop
    cur_set_index: [Vec<usize>; R],
    deck: Deck,
}

impl<const R: usize> DealEnumerationIterator<R> {
    pub fn new(deck: Deck, cards_per_round: [usize; R]) -> Self {
        let mut first_index = std::array::from_fn(|_| Vec::new());
        let mut i = 0;
        for r in 0..R {
            for _ in 0..cards_per_round[r] {
                first_index[r].push(i);
                i += 1;
            }
        }

        Self {
            cards_per_round,
            deck,
            cur_set_index: first_index,
        }
    }
}

impl<const R: usize> Iterator for DealEnumerationIterator<R> {
    type Item = [CardSet; R];

    fn next(&mut self) -> Option<Self::Item> {
        let mut cur_set = [CardSet::default(); R];

        let mut candidate_index = self.cur_set_index.clone();
        loop {
            if let Some(next_set) = index_to_card_set(&candidate_index[0], &self.deck) {
                cur_set[0] = next_set;
                break;
            }
            candidate_index[0] = increment_set_index(&candidate_index[0])?;
        }

        self.cur_set_index[0] = increment_set_index(&candidate_index[0]).unwrap();
        Some(cur_set)
    }
}

/// Increments the set index, "carrying" when the last digit gets to
/// MAX_CARDS
fn increment_set_index(index: &[usize]) -> Option<Vec<usize>> {
    increment_set_index_r(index, MAX_CARDS)
}

fn increment_set_index_r(index: &[usize], max_rank: usize) -> Option<Vec<usize>> {
    let last = index.last()?;
    // handle the simple case where no carrying occurs
    if last + 1 < max_rank {
        let mut new_index = index.to_vec();
        *new_index.last_mut()? += 1;
        return Some(new_index);
    }

    // recursively do all the carrying for the base index
    let mut index_start = index.to_vec();
    index_start.pop();
    let mut new_index = increment_set_index_r(&index_start, max_rank - 1)?;

    if new_index.last()? + 1 < max_rank {
        new_index.push(new_index.last()? + 1);
        Some(new_index)
    } else {
        // no further indexes are possible
        None
    }
}

fn index_to_card_set(index: &[usize], deck: &Deck) -> Option<CardSet> {
    let mut set = CardSet::default();

    for c in index.iter().map(|x| Card::new(*x)) {
        if !deck.remaining_cards.contains(c) {
            return None;
        }
        set.insert(c)
    }
    Some(set)
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
        assert_eq!(set.highest(), Card(0b10));
        set.insert(Card(0b100));
        assert_eq!(set.highest(), Card(0b100));
        set.insert(Card(0b1));
        assert_eq!(set.highest(), Card(0b100));
    }

    #[test]
    fn test_enumerate_deals() {
        // 52 choose 2 for the pockets cards in hold em
        assert_eq!(count_combinations([2]), 1326);
        // Flop: 52 choose 2 * 50 choose 3
        assert_eq!(count_combinations([2, 3]), 25_989_600);
        // Turn: 52 choose 2 * 50 choose 3 * 47
        assert_eq!(count_combinations([2, 3, 1]), 1221511200);
        // River: 52 choose 2 * 50 choose 3 * 47 * 46
        assert_eq!(count_combinations([2, 3, 1, 1]), 56_189_515_200);
    }

    fn count_combinations<const R: usize>(cards_per_round: [usize; R]) -> usize {
        let deck = Deck::standard();
        let mut count = 0;

        for c in DealEnumerationIterator::new(deck, cards_per_round) {
            println!("{:?}", c);
            count += 1;
        }

        count
    }
}
