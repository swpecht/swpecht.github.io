use std::{cmp::Ordering, collections::HashSet};

use crate::cards::{Suit, SPADES};

use super::{cardset::CardSet, Card, Deck, MAX_CARDS};

pub struct DeckIterator {
    pub(super) deck: Deck,
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
pub struct DealEnumerationIterator<const R: usize> {
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

    // This doesn't need to be recalculated each loop since we're going over them in reverse order and later
    // rounds can't impact earlier rounds
    let valid_cards = round_valid_cards(deal, deck_cards);
    let mut updated_round = R;
    'round_loop: for r in (0..R).rev() {
        loop {
            let set = match (increment_cardset(deal[r]), r) {
                (Some(x), _) => x,
                (None, 0) => return None, // if can't increment round 0 anymore, we're at the ned of the iterator
                (None, _) => continue 'round_loop,
            };

            deal[r] = set;
            if valid_cards[r].constains_all(set) {
                updated_round = r;
                break 'round_loop;
            }
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

/// Iterates over all possible isomorphic deals of a deck -- suit can be changed, but rank cannot
///
/// Follows the definition in Kevin's paper
pub struct IsomorphicDealIterator<const R: usize> {
    deal_enumerator: DealEnumerationIterator<R>,
    previous_deals: HashSet<[CardSet; R]>,
}

impl<const R: usize> IsomorphicDealIterator<R> {
    pub fn new(deck: Deck, cards_per_round: [usize; R]) -> Self {
        let deal_enumerator = DealEnumerationIterator::new(deck, cards_per_round);

        Self {
            deal_enumerator,
            previous_deals: HashSet::new(),
        }
    }
}

impl<const R: usize> Iterator for IsomorphicDealIterator<R> {
    type Item = [CardSet; R];

    fn next(&mut self) -> Option<Self::Item> {
        let mut iso_deal;
        loop {
            let next_deal = self.deal_enumerator.next()?;
            iso_deal = isomorphic(next_deal, &self.deal_enumerator.deck);

            if !self.previous_deals.contains(&iso_deal) {
                self.previous_deals.insert(iso_deal);
                break;
            }
        }

        Some(iso_deal)
    }
}

// Adjusts suits on the cardset to make the deal isomorphic, specicially, we
// make the lowest suit be the highest suit configurations
fn isomorphic<const R: usize>(deal: [CardSet; R], deck: &Deck) -> [CardSet; R] {
    assert_eq!(
        deck.suits[0],
        Suit(SPADES),
        "only support the standard, contiguous suits for now"
    );

    let counts = suit_counts(deal, deck);
    let mut indexes = [0, 1, 2, 3];
    let card_arrays = card_array(deal);
    // sort by suit counts first
    // if the counts are equal, we sort by the cards with earlier rounds having priorty
    indexes.sort_by(|&a, &b| match counts[a].cmp(&counts[b]) {
        Ordering::Greater => Ordering::Greater,
        Ordering::Less => Ordering::Less,
        Ordering::Equal => card_arrays[a].cmp(&card_arrays[b]),
    });
    indexes.reverse();

    let mut iso_deal = [CardSet::default(); R];
    for r in 0..R {
        let array = to_array(deal[r].0);
        let mut sorted_array = [0; 4];
        for s in 0..4 {
            sorted_array[s] = array[indexes[s]];
        }
        iso_deal[r] = CardSet(to_u64(sorted_array));
    }

    iso_deal
}

pub fn to_array(v: u64) -> [u16; 4] {
    unsafe { std::mem::transmute(v) }
}

pub fn to_u64(v: [u16; 4]) -> u64 {
    unsafe { std::mem::transmute(v) }
}

/// Convert a deal to suit counts (round configs)
fn suit_counts<const R: usize>(deal: [CardSet; R], deck: &Deck) -> [[usize; R]; 4] {
    let mut counts = [[0; R]; 4];

    for (s, suit) in deck.suits.iter().enumerate() {
        for (r, set) in deal.iter().enumerate() {
            counts[s][r] = set.count(suit);
        }
    }

    counts
}

fn card_array<const R: usize>(deal: [CardSet; R]) -> [[u16; R]; 4] {
    let mut cards = [[0; R]; 4];

    for (r, set) in deal.iter().enumerate() {
        let array = to_array(set.0);
        for s in 0..4 {
            cards[s][r] = array[s];
        }
    }

    cards
}

#[cfg(test)]
mod tests {

    use std::collections::HashSet;

    use crate::{
        cards::{cardset::to_cardset, Deck},
        rankset::RankSet,
        HandIndexer,
    };

    use super::*;

    #[test]
    fn test_iso_deal() {
        // if only 1 suit, every suit should be changed to the lowest suit
        let deck = Deck::standard();
        for s in deck.suits {
            let set = CardSet(s.0);
            assert_eq!(isomorphic([set], &deck)[0].0, deck.suits[0].0);
        }
    }

    #[test]
    fn test_enumerate_deals() {
        let deck = Deck::standard();
        assert_eq!(DealEnumerationIterator::new(deck, [1]).count(), 52);

        // 52 choose 2 for the pockets cards in hold em
        assert_eq!(DealEnumerationIterator::new(deck, [2]).count(), 1326);
        assert_eq!(
            DealEnumerationIterator::new(deck, [2, 2]).count(),
            1_624_350
        );
    }

    #[test]
    fn test_count_iso_deals() {
        let deck = Deck::standard();

        // the rejection criteria are wrong, since we don't actually enumerate from lowest to highest -- could change so that this is the case
        // right now we increment the highest bit
        // can look at just using a hash set for this for now? -- could we use bit runs to keep track of what we've seen?
        // whats the memory on this look like?
        // TODOs:
        // * Fix it so it works with hashsets
        // * Look at more efificient hashsets if needed to make it work, or fixing the low to high thing

        assert_eq!(IsomorphicDealIterator::new(deck, [1]).count(), 13);
        assert_eq!(IsomorphicDealIterator::new(deck, [2]).count(), 169);

        // TODO: Move additioanl tests to integration tests
    }

    fn count_combinations<const R: usize>(cards_per_round: [usize; R]) -> usize {
        let deck = Deck::standard();
        let mut count = 0;

        for _ in DealEnumerationIterator::new(deck, cards_per_round) {
            count += 1;
        }

        count
    }

    #[test]
    fn test_with_hand_indexer() {
        let deck = Deck::standard();

        for deal in IsomorphicDealIterator::new(deck, [2]) {
            let counts = suit_counts(deal, &deck);
            assert!(counts[0][0] == 2 || (counts[0][0] == 1 && counts[1][0] == 1));
            let array = to_array(deal[0].0);
            assert!(array[0] >= array[1]);
        }

        assert_eq!(IsomorphicDealIterator::new(deck, [2]).count(), 169);

        let mut iso_deals = HashSet::new();
        let indexer = HandIndexer::<13, 4>::new(&[2]);
        for i in 0..200 {
            let hand = indexer.unindex_hand(i, vec![vec![1], vec![1]]).unwrap();
            assert_eq!(hand.len(), 2);
            let set = to_cardset(&hand, &deck);
            assert_eq!(set.len(), 1);
            iso_deals.insert(set[0]);
        }
        assert_eq!(iso_deals.len(), 91);

        for deal in IsomorphicDealIterator::new(deck, [2]) {
            if deal[0].count(&deck.suits[0]) != 1 {
                continue; // only care about 1, 1 suit config
            }

            if !iso_deals.contains(&deal[0]) {
                let hand: [RankSet; 4] = deal[0].into();
                let index = indexer.index_hand(vec![hand.to_vec()]);
                let unindexed_hand = indexer.unindex_hand(index, vec![vec![1], vec![1]]).unwrap();
                let unindex_deal = to_cardset(&unindexed_hand, &deck);
                println!("got: {:?}, should: {:?}", deal[0], unindex_deal[0]);
            }
        }

        ///
        /// got:    0b10000000000000001
        /// should: 0b100000000000000001

        println!("indexer sets");
    }
}
