use crate::game::euchre::deck::CardLocation;

use super::{
    actions::{Card as C, Suit},
    deck::Deck,
};

const JACK_RANK: usize = 2;
const SUITS: &[Suit] = &[Suit::Spades, Suit::Clubs, Suit::Hearts, Suit::Diamonds];
const RANKS: &[usize] = &[0, 1, 2, 3, 4, 5];
const WORD_SIZE: usize = 4;
const MAX_WORDS: usize = 7;
type Locations = u32;

// Pre-computed ordering of cards by value
const SPADES_NONE: &[C] = &[C::NS, C::TS, C::JS, C::QS, C::KS, C::AS];
const CLUBS_NONE: &[C] = &[C::NC, C::TC, C::JC, C::QC, C::KC, C::AC];
const HEARTS_NONE: &[C] = &[C::NH, C::TH, C::JH, C::QH, C::KH, C::AH];
const DIAMONDS_NONE: &[C] = &[C::ND, C::TD, C::JD, C::QD, C::KD, C::AD];

const SPADES_CLUBS: &[C] = &[C::NS, C::TS, C::QS, C::KS, C::AS];
const CLUBS_CLUBS: &[C] = &[C::NC, C::TC, C::QC, C::KC, C::AC, C::JS, C::JC];

const SPADES_SPADES: &[C] = &[C::NS, C::TS, C::QS, C::KS, C::AS, C::JC, C::JS];
const CLUBS_SPADES: &[C] = &[C::NC, C::TC, C::QC, C::KC, C::AC];

const HEARTS_HEARTS: &[C] = &[C::NH, C::TH, C::QH, C::KH, C::AH, C::JD, C::JH];
const DIAMONDS_HEARTS: &[C] = &[C::ND, C::TD, C::QD, C::KD, C::AD];

const HEARTS_DIAMONDS: &[C] = &[C::NH, C::TH, C::QH, C::KH, C::AH];
const DIAMONDS_DIAMONDS: &[C] = &[C::ND, C::TD, C::QD, C::KD, C::AD, C::JH, C::JD];

pub(super) fn iso_deck(deck: Deck, trump: Option<Suit>) -> [Locations; 4] {
    let mut locations = [0; 4];
    // if we have trump, skip all the none locations, we can fully compress
    if trump.is_some() {
        for s in SUITS {
            for c in get_cards(*s, trump) {
                if deck[*c] != CardLocation::None {
                    locations[*s as usize] <<= WORD_SIZE;
                    locations[*s as usize] |= location_mask(deck[*c])
                }
            }
        }
    } else {
        // todo: could do this in a single path if needed for performance reasons
        // if no trump, we load everything and then swap around the jacks
        for s in SUITS {
            for c in get_cards(*s, trump) {
                locations[*s as usize] <<= WORD_SIZE;
                locations[*s as usize] |= location_mask(deck[*c])
            }
        }

        // we can only swap things around the jacks
        for suit_locations in locations.iter_mut() {
            for i in 0..MAX_WORDS - 1 {
                if i != JACK_RANK
                    && i + 1 != JACK_RANK
                    && is_equal(suit_locations, i, CardLocation::None)
                {
                    swap_loc(suit_locations, i, i + 1);
                }
            }
        }
    }

    if let Some(trump) = trump {
        // put trump in the first spot
        locations.swap(0, trump as usize);
        // sort everything else
        locations[1..].sort();
    } else {
        // sort everything
        locations.sort();
    }

    locations
}

fn get_index(c: C, trump: Option<Suit>) -> (usize, usize) {
    let rank = c.rank() as usize;
    let suit = c.suit();

    if trump.is_none() {
        return (suit as usize, rank);
    }

    let trump = trump.unwrap();
    let is_trump = suit == trump;
    let is_trump_color = suit.other_color() == trump || is_trump;

    match (is_trump, is_trump_color, rank) {
        (true, true, JACK_RANK) => (suit as usize, 6),
        (false, true, JACK_RANK) => (trump as usize, 5),
        (_, true, x) if x > JACK_RANK => (suit as usize, rank - 1),
        (_, true, x) if x < JACK_RANK => (suit as usize, rank),
        (false, false, _) => (suit as usize, rank),
        _ => panic!(
            "invalid card: {}, is_trump: {}, is_trump_color: {}, rank: {}",
            c, is_trump, is_trump_color, rank
        ),
    }
}

/// Returns the bit mask associated with each location.
///
/// Results are arbitrary.
fn location_mask(loc: CardLocation) -> Locations {
    match loc {
        CardLocation::Player0 => 0b1000,
        CardLocation::Player1 => 0b0001,
        CardLocation::Player2 => 0b0010,
        CardLocation::Player3 => 0b0011,
        CardLocation::Played(_) => 0b0100,
        CardLocation::FaceUp => 0b0101,
        CardLocation::None => 0b0000,
    }
}

/// Deletes the ith word, downshift all following elements
pub fn downshift(l: &mut Locations, i: usize) {
    let m = mask(i);
    let x = *l & m;
    *l &= !m;
    *l >>= 4;
    *l |= x;
}
fn mask(i: usize) -> Locations {
    if i == 0 {
        0
    } else {
        u32::MAX >> (32 - (i * WORD_SIZE))
    }
}

fn set_loc(l: &mut Locations, i: usize, loc: CardLocation) {
    let m = 0b1111 << (i * WORD_SIZE);
    *l &= !m;
    let word_mask = location_mask(loc) << (i * WORD_SIZE);
    *l |= word_mask;
}

/// Swap words
///
/// http://graphics.stanford.edu/~seander/bithacks.html#SwappingBitsXOR
fn swap_loc(l: &mut Locations, a: usize, b: usize) {
    let i = a * WORD_SIZE;
    let j = b * WORD_SIZE;
    let x = ((*l >> i) ^ (*l >> j)) & ((1 << WORD_SIZE) - 1);
    *l ^= (x << i) | (x << j);
}

/// Returns if card location in position i equals loc
fn is_equal(l: &Locations, i: usize, loc: CardLocation) -> bool {
    ((*l >> (i * WORD_SIZE)) & 0b1111) == location_mask(loc)
}

/// Return all cards, in order from lowest to highest of a suit for a given trump
fn get_cards(suit: Suit, trump: Option<Suit>) -> &'static [C] {
    match (suit, trump) {
        (Suit::Clubs, Some(Suit::Clubs)) => CLUBS_CLUBS,
        (Suit::Clubs, Some(Suit::Spades)) => CLUBS_SPADES,
        (Suit::Spades, Some(Suit::Spades)) => SPADES_SPADES,
        (Suit::Spades, Some(Suit::Clubs)) => SPADES_CLUBS,
        (Suit::Hearts, Some(Suit::Hearts)) => HEARTS_HEARTS,
        (Suit::Hearts, Some(Suit::Diamonds)) => HEARTS_DIAMONDS,
        (Suit::Diamonds, Some(Suit::Diamonds)) => DIAMONDS_DIAMONDS,
        (Suit::Diamonds, Some(Suit::Hearts)) => DIAMONDS_HEARTS,
        // No trump or off color
        (Suit::Clubs, _) => CLUBS_NONE,
        (Suit::Spades, _) => SPADES_NONE,
        (Suit::Diamonds, _) => DIAMONDS_NONE,
        (Suit::Hearts, _) => HEARTS_NONE,
    }
}

#[cfg(test)]
mod tests {
    use crate::game::euchre::{
        actions::{Card, Suit},
        deck::{CardLocation, Deck},
        ismorphic::{downshift, iso_deck, set_loc, swap_loc},
    };

    #[test]
    fn test_deck_iso_no_trump() {
        let mut d1 = Deck::default();

        d1[Card::NS] = CardLocation::Player0;

        let mut d2 = d1;

        assert_eq!(iso_deck(d1, None), iso_deck(d2, None));
        d2[Card::TS] = CardLocation::Player0;

        assert!(iso_deck(d1, None) != iso_deck(d2, None));
        d2[Card::NS] = CardLocation::None;

        assert_eq!(iso_deck(d1, None), iso_deck(d1, None));
    }

    #[test]
    fn test_deck_iso_across_suit() {
        let mut d1 = Deck::default();
        d1[Card::NS] = CardLocation::Player0;
        d1[Card::TS] = CardLocation::Player0;
        d1[Card::JC] = CardLocation::Player1;

        let mut d2 = Deck::default();
        d2[Card::NH] = CardLocation::Player0;
        d2[Card::TH] = CardLocation::Player0;
        d2[Card::JD] = CardLocation::Player1;

        // both have 2 lowest cards across suit
        assert_eq!(iso_deck(d1, None), iso_deck(d2, None));

        assert!(iso_deck(d1, Some(Suit::Spades)) != iso_deck(d2, None));

        assert_eq!(
            iso_deck(d1, Some(Suit::Spades)),
            iso_deck(d2, Some(Suit::Hearts))
        );
    }

    #[test]
    fn test_deck_iso_trump() {
        let mut d1 = Deck::default();

        d1[Card::NS] = CardLocation::Player0;
        d1[Card::TS] = CardLocation::Player0;

        let mut d2 = d1;

        assert_eq!(
            iso_deck(d1, Some(Suit::Spades)),
            iso_deck(d2, Some(Suit::Spades))
        );
        d2[Card::JS] = CardLocation::Player0;

        assert!(iso_deck(d1, Some(Suit::Spades)) != iso_deck(d2, Some(Suit::Spades)));
        d2[Card::NS] = CardLocation::None;
        // player 0  still has the 2 highest spades
        assert_eq!(
            iso_deck(d1, Some(Suit::Spades)),
            iso_deck(d2, Some(Suit::Spades))
        );
        d2[Card::JC] = CardLocation::Player0;
        d2[Card::TS] = CardLocation::None;
        // player 0  still has the 2 highest spades, JC and JS
        assert_eq!(
            iso_deck(d1, Some(Suit::Spades)),
            iso_deck(d2, Some(Suit::Spades))
        );

        // this persists even if we deal some other cards
        d1[Card::TH] = CardLocation::Player1;
        d2[Card::TH] = CardLocation::Player1;
        assert_eq!(
            iso_deck(d1, Some(Suit::Spades)),
            iso_deck(d2, Some(Suit::Spades))
        );
    }

    #[test]
    fn test_downshift() {
        let mut l = 0;
        downshift(&mut l, 1);
        assert_eq!(l, 0);

        let mut l = 0b0001;
        downshift(&mut l, 0);
        assert_eq!(l, 0);

        let mut l = 0b00010000;
        downshift(&mut l, 0);
        assert_eq!(l, 0b0001);

        let mut l = 0b0010_0000_0101_0001;
        downshift(&mut l, 2);
        assert_eq!(l, 0b0010_0101_0001);
    }

    #[test]
    fn test_set_loc() {
        let mut l = 0;
        set_loc(&mut l, 1, CardLocation::Player1);
        assert_eq!(l, 0b00010000);
    }

    #[test]
    fn test_swap_loc() {
        let mut l = 0b1010_0101;
        swap_loc(&mut l, 0, 1);
        assert_eq!(l, 0b0101_1010);

        let mut l = 0;
        swap_loc(&mut l, 0, 1);
        assert_eq!(l, 0);
    }
}
