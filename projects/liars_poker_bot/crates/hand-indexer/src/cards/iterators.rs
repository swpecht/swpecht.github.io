use crate::{
    cards::{Suit, SPADES},
    configurations::RoundConfig,
};

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
struct DealEnumerationIterator<const R: usize> {
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
    last_deal: Option<[CardSet; R]>,
}

impl<const R: usize> IsomorphicDealIterator<R> {
    pub fn new(deck: Deck, cards_per_round: [usize; R]) -> Self {
        let deal_enumerator = DealEnumerationIterator::new(deck, cards_per_round);

        Self {
            deal_enumerator,
            last_deal: None,
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

            // the deal enumerator is guaranteed to go over deals from lowest to highest
            // if the deal is lower than the last deal, then we have seen it before and can skip it
            // TODO: investigate if we can just return the first time seeing a lower deal
            match self.last_deal {
                Some(ld) => {
                    if iso_deal > ld {
                        self.last_deal = Some(iso_deal);
                        break;
                    }
                }
                None => self.last_deal = Some(iso_deal),
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
    indexes.sort_by_key(|&x| &counts[x]);
    indexes.reverse();

    let mut iso_deal = [CardSet::default(); R];
    for (new_index, old_index) in indexes.into_iter().enumerate() {
        for r in 0..R {
            // get the bits for just the new suit in the right place, then put them in the iso deal
            let mut new_suit = swap_bits(deal[r].0, old_index * 13, new_index * 13, 13);
            // todo -- something is wrong here, old index passes the naive test, but it seems like it should be new index
            new_suit &= deck.suits[new_index].0;
            iso_deal[r].0 |= new_suit;
        }
    }

    iso_deal
}

/// Swap the `n` consecutive bits between index `i` and `j` in `b`
///
/// Adapted from: https://graphics.stanford.edu/~seander/bithacks.html#SwappingBitsXOR
fn swap_bits(b: u64, i: usize, j: usize, n: usize) -> u64 {
    // unsigned int i, j; // positions of bit sequences to swap
    // unsigned int n;    // number of consecutive bits in each sequence
    // unsigned int b;    // bits to swap reside in b
    // unsigned int r;    // bit-swapped result goes here
    // unsigned int x = ((b >> i) ^ (b >> j)) & ((1U << n) - 1); // XOR temporary
    // r = b ^ ((x << i) | (x << j));

    let x = ((b >> i) ^ (b >> j)) & ((1 << n) - 1); // XOR temporary
    b ^ ((x << i) | (x << j))
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

#[cfg(test)]
mod tests {

    use crate::cards::Deck;

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
    fn test_swap_bits() {
        assert_eq!(
            swap_bits(0b1111111111111, 0, 13, 13),
            0b11111111111110000000000000
        );

        assert_eq!(swap_bits(0b11111, 13, 13, 13), 0b11111);
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

    #[test]
    fn test_count_iso_deals() {
        let deck = Deck::standard();

        assert_eq!(IsomorphicDealIterator::new(deck, [2]).count(), 169);
        assert_eq!(IsomorphicDealIterator::new(deck, [2, 3]).count(), 1_286_792);

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
}
