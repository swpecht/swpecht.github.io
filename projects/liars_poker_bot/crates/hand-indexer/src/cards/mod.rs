use itertools::{Combinations, Itertools};
use std::fmt::Debug;

use self::cardset::CardSet;

const SPADES: u64 = 0b1111111111111;
const CLUBS: u64 = SPADES << 13;
const HEARTS: u64 = CLUBS << 13;
const DIAMONDS: u64 = HEARTS << 13;

const MAX_CARDS: usize = 64;

pub mod cardset;

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
    next_candidate_set: Option<[CardSet; R]>,
    deck: Deck,
}

impl<const R: usize> DealEnumerationIterator<R> {
    pub fn new(deck: Deck, cards_per_round: [usize; R]) -> Self {
        let mut first_candidate_set = [CardSet::default(); R];
        let mut i = 0;
        for r in 0..R {
            for _ in 0..cards_per_round[r] {
                first_candidate_set[r].insert(Card::new(i));
                i += 1;
            }
        }

        assert!(is_valid(first_candidate_set, CardSet::all()));

        Self {
            cards_per_round,
            deck,
            next_candidate_set: Some(first_candidate_set),
        }
    }
}

impl<const R: usize> Iterator for DealEnumerationIterator<R> {
    type Item = [CardSet; R];

    fn next(&mut self) -> Option<Self::Item> {
        let mut candidate = self.next_candidate_set?;

        while !is_valid(candidate, self.deck.remaining_cards) {
            candidate = incremenet_deal(candidate, self.deck.remaining_cards)?;
        }

        let cur_set = candidate;
        self.next_candidate_set = incremenet_deal(candidate, self.deck.remaining_cards);
        Some(cur_set)
    }
}

/// Returns the valid possible cards for each round
fn round_valid_cards<const R: usize>(deal: [CardSet; R], starting_valid: CardSet) -> [CardSet; R] {
    let mut round_valid_cards = [CardSet::default(); R];
    round_valid_cards[0] = starting_valid;
    for r in 1..R {
        round_valid_cards[r] = round_valid_cards[r - 1];
        round_valid_cards[r].remove_all(deal[r - 1]);
    }
    round_valid_cards
}

fn is_valid<const R: usize>(deal: [CardSet; R], valid_cards: CardSet) -> bool {
    let mut check_set = CardSet::default();
    for set in deal {
        check_set.insert_all(set);
    }

    let each_round_different_cards = deal.iter().map(|x| x.len()).sum::<usize>() == check_set.len();
    let each_card_valid = deal.into_iter().all(|x| valid_cards.constains_all(x));

    each_round_different_cards && each_card_valid
}

/// Increments the set index, "carrying" when the last digit gets to
/// MAX_CARDS
fn increment_cardset(set: CardSet) -> Option<CardSet> {
    // TODO: how do we allow this to wrap for later rounds? -- start at 0 each time we go through a new round?
    increment_cardset_r(set, MAX_CARDS)
}

fn increment_cardset_r(mut set: CardSet, max_rank: usize) -> Option<CardSet> {
    let last = set.highest()?.rank();
    // handle the simple case where no carrying occurs
    if last + 1 < max_rank {
        set = set.increment_highest()?;
        return Some(set);
    }

    // recursively do all the carrying for the base index
    set.pop_highest();
    set = increment_cardset_r(set, max_rank - 1)?;

    if set.highest()?.rank() + 1 < max_rank {
        set.insert(Card::new(set.highest()?.rank() + 1));
        Some(set)
    } else {
        // no further indexes are possible
        None
    }
}

fn incremenet_deal<const R: usize>(
    mut deal: [CardSet; R],
    deck_cards: CardSet,
) -> Option<[CardSet; R]> {
    let mut cards_per_round = [0; R];
    for r in 0..R {
        cards_per_round[r] = deal[r].len();
    }

    let mut updated_round = R;
    for r in (0..R).rev() {
        if let Some(set) = increment_cardset(deal[r]) {
            deal[r] = set;
            updated_round = r;
            break;
        } else if r == 0 {
            // if we fail to find an increment for round 0, we're at the end of the iterator
            return None;
        }
    }

    // fill the forward rounds with the lowest index and values
    for r in (updated_round + 1)..R {
        let mut valid_cards = round_valid_cards(deal, deck_cards);
        let mut new_set = CardSet::default();
        // As an optimization, we start the sets with the lowest possible valid cards rather than just the lowest cards
        for _ in 0..cards_per_round[r] {
            new_set.insert(valid_cards[r].pop_lowest().unwrap())
        }
        deal[r] = new_set;
    }

    Some(deal)
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

    #[test]
    fn test_enumerate_deals() {
        assert_eq!(count_combinations([1]), 52);

        // 52 choose 2 for the pockets cards in hold em
        assert_eq!(count_combinations([2]), 1326);

        assert_eq!(count_combinations([2, 2]), 1_624_350);
        // Flop: 52 choose 2 * 50 choose 3
        assert_eq!(count_combinations([2, 3]), 25_989_600);

        // TODO: move to test rather than integration tests given run time
        // // Turn: 52 choose 2 * 50 choose 3 * 47
        // assert_eq!(count_combinations([2, 3, 1]), 1_221_511_200);
        // // River: 52 choose 2 * 50 choose 3 * 47 * 46
        // assert_eq!(count_combinations([2, 3, 1, 1]), 56_189_515_200);
    }

    fn count_combinations<const R: usize>(cards_per_round: [usize; R]) -> usize {
        let deck = Deck::standard();
        let mut count = 0;

        for _ in DealEnumerationIterator::new(deck, cards_per_round) {
            count += 1;
        }

        count
    }
}
