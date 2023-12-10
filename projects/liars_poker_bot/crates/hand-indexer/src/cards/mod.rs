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

    pub fn remove_all(&mut self, set: CardSet) {
        self.0 &= !set.0;
    }

    pub fn constains_all(&self, set: CardSet) -> bool {
        self.0 | set.0 == self.0
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
    next_candidate_idx: [Vec<usize>; R],
    last_set: Option<[CardSet; R]>,
    deck: Deck,
}

impl<const R: usize> DealEnumerationIterator<R> {
    pub fn new(deck: Deck, cards_per_round: [usize; R]) -> Self {
        let mut first_index = std::array::from_fn(|_| Vec::new());
        for r in 0..R {
            for i in 0..cards_per_round[r] {
                first_index[r].push(i);
            }
        }

        Self {
            cards_per_round,
            deck,
            next_candidate_idx: first_index,
            last_set: None,
        }
    }
}

impl<const R: usize> Iterator for DealEnumerationIterator<R> {
    type Item = [CardSet; R];

    fn next(&mut self) -> Option<Self::Item> {
        // todo: handle the first iteration
        let valid_cards;
        if let Some(ls) = self.last_set {
            valid_cards = round_valid_cards(ls, self.deck.remaining_cards);
        } else {
            let mut next_set = [CardSet::default(); R];
            for r in 0..R {
                let valid_cards = round_valid_cards(next_set, self.deck.remaining_cards);
                (next_set[r], self.next_candidate_idx[r]) =
                    find_next_cardset(self.next_candidate_idx[r].clone(), valid_cards[r])?;
            }

            self.last_set = Some(next_set);

            return self.last_set;
        }

        let mut next_set = self.last_set.unwrap();
        let mut updated_round = R;
        for r in (0..R).rev() {
            if let Some((round_set, next_rnd_idx)) =
                find_next_cardset(self.next_candidate_idx[r].clone(), valid_cards[r])
            {
                next_set[r] = round_set;
                self.next_candidate_idx[r] = next_rnd_idx;
                updated_round = r;
                break;
            } else if r == 0 {
                // if we fail to find an increment for round 0, we're at the end of the iterator
                return None;
            }
        }

        // fill the forward rounds with the lowest index and values
        for r in (updated_round + 1)..R {
            let valid_cards = round_valid_cards(next_set, self.deck.remaining_cards);
            let candidate_index = (0..self.cards_per_round[r]).collect_vec();
            (next_set[r], self.next_candidate_idx[r]) =
                find_next_cardset(candidate_index, valid_cards[r])?;
        }

        self.last_set = Some(next_set);

        self.last_set
    }
}

fn find_next_cardset(
    mut candidate_index: Vec<usize>,
    valid_cards: CardSet,
) -> Option<(CardSet, Vec<usize>)> {
    let set;

    loop {
        if let Some(found_set) = index_to_card_set(&candidate_index, &valid_cards) {
            set = found_set;
            break;
        }
        candidate_index = increment_index_round(candidate_index)?;
    }

    let next_candidate_index = increment_index_round(candidate_index).unwrap();
    Some((set, next_candidate_index))
}

/// Returns the valid possible cards for each round
fn round_valid_cards<const R: usize>(set: [CardSet; R], starting_valid: CardSet) -> [CardSet; R] {
    let mut round_valid_cards = [CardSet::default(); R];
    round_valid_cards[0] = starting_valid;
    for r in 1..R {
        round_valid_cards[r] = round_valid_cards[r - 1];
        round_valid_cards[r].remove_all(set[r - 1]);
    }
    round_valid_cards
}

/// Increments the set index, "carrying" when the last digit gets to
/// MAX_CARDS
fn increment_index_round(index: Vec<usize>) -> Option<Vec<usize>> {
    // TODO: how do we allow this to wrap for later rounds? -- start at 0 each time we go through a new round?
    increment_index_round_r(index, MAX_CARDS)
}

fn increment_index_round_r(mut index: Vec<usize>, max_rank: usize) -> Option<Vec<usize>> {
    let last = index.last()?;
    // handle the simple case where no carrying occurs
    if last + 1 < max_rank {
        *index.last_mut()? += 1;
        return Some(index);
    }

    // recursively do all the carrying for the base index
    index.pop();
    let mut index = increment_index_round_r(index, max_rank - 1)?;

    if index.last()? + 1 < max_rank {
        index.push(index.last()? + 1);
        Some(index)
    } else {
        // no further indexes are possible
        None
    }
}

fn index_to_card_set(index: &[usize], valid_cards: &CardSet) -> Option<CardSet> {
    let mut set = CardSet::default();

    for c in index.iter().map(|x| Card::new(*x)) {
        set.insert(c)
    }

    if !valid_cards.constains_all(set) {
        return None;
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
        assert_eq!(count_combinations([1]), 52);

        // 52 choose 2 for the pockets cards in hold em
        assert_eq!(count_combinations([2]), 1326);

        assert_eq!(count_combinations([2, 2]), 1_624_350);
        // // Flop: 52 choose 2 * 50 choose 3
        // assert_eq!(count_combinations([2, 3]), 25_989_600);
        // // Turn: 52 choose 2 * 50 choose 3 * 47
        // assert_eq!(count_combinations([2, 3, 1]), 1_221_511_200);
        // // River: 52 choose 2 * 50 choose 3 * 47 * 46
        // assert_eq!(count_combinations([2, 3, 1, 1]), 56_189_515_200);
        todo!()
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
