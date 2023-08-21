use crate::{
    game::{euchre::deck::CardLocation, Action},
    istate::NormalizedAction,
};

use super::{
    actions::{Card, EAction, Suit},
    deck::Deck,
    EuchreGameState,
};

const JACK_RANK: usize = 2;
const SUITS: &[Suit] = &[Suit::Spades, Suit::Clubs, Suit::Hearts, Suit::Diamonds];
const WORD_SIZE: usize = 4;
const MAX_WORDS: usize = 7;
type Locations = u32;

// Pre-computed ordering of cards by value
use Card::*;
const SPADES_NONE: &[Card] = &[NS, TS, JS, QS, KS, AS];
const CLUBS_NONE: &[Card] = &[NC, TC, JC, QC, KC, AC];
const HEARTS_NONE: &[Card] = &[NH, TH, JH, QH, KH, AH];
const DIAMONDS_NONE: &[Card] = &[ND, TD, JD, QD, KD, AD];

const SPADES_CLUBS: &[Card] = &[NS, TS, QS, KS, AS];
const CLUBS_CLUBS: &[Card] = &[NC, TC, QC, KC, AC, JS, JC];

const SPADES_SPADES: &[Card] = &[NS, TS, QS, KS, AS, JC, JS];
const CLUBS_SPADES: &[Card] = &[NC, TC, QC, KC, AC];

const HEARTS_HEARTS: &[Card] = &[NH, TH, QH, KH, AH, JD, JH];
const DIAMONDS_HEARTS: &[Card] = &[ND, TD, QD, KD, AD];

const HEARTS_DIAMONDS: &[Card] = &[NH, TH, QH, KH, AH];
const DIAMONDS_DIAMONDS: &[Card] = &[ND, TD, QD, KD, AD, JH, JD];

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
        // todo: could do this in a single pass if needed for performance reasons
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
pub(super) fn get_cards(suit: Suit, trump: Option<Suit>) -> &'static [Card] {
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

/// Normalizes the suit to have Spades always be the faceup card.
pub fn normalize_action(action: Action, gs: &EuchreGameState) -> NormalizedAction {
    let face_up_suit = gs.face_up().map(|c| c.suit());
    if face_up_suit.is_none() {
        return NormalizedAction::new(action);
    }

    NormalizedAction::new(transform(action, face_up_suit.unwrap()))
}

pub fn denormalize_action(action: NormalizedAction, gs: &EuchreGameState) -> Action {
    let face_up_suit = gs.face_up().map(|c| c.suit());
    if face_up_suit.is_none() {
        return action.get();
    }

    transform(action.get(), face_up_suit.unwrap())
}

fn transform(action: Action, face_up_suit: Suit) -> Action {
    // We can apply the transform again to denormalize the action
    use EAction::*;
    let ea = EAction::from(action);
    let new_action = match ea {
        NC | TC | JC | QC | KC | AC | NS | TS | JS | QS | KS | AS | NH | TH | JH | QH | KH | AH
        | ND | TD | JD | QD | KD | AD => {
            EAction::public_action(transform_card(ea.card(), face_up_suit))
        }

        PrivateNC | PrivateTC | PrivateJC | PrivateQC | PrivateKC | PrivateAC | PrivateNS
        | PrivateTS | PrivateJS | PrivateQS | PrivateKS | PrivateAS | PrivateNH | PrivateTH
        | PrivateJH | PrivateQH | PrivateKH | PrivateAH | PrivateND | PrivateTD | PrivateJD
        | PrivateQD | PrivateKD | PrivateAD => {
            EAction::private_action(transform_card(ea.card(), face_up_suit))
        }

        Spades => transform_suit(Suit::Spades, face_up_suit).into(),
        Clubs => transform_suit(Suit::Clubs, face_up_suit).into(),
        Hearts => transform_suit(Suit::Hearts, face_up_suit).into(),
        Diamonds => transform_suit(Suit::Diamonds, face_up_suit).into(),
        _ => ea,
    };

    new_action.into()
}

/// Function to normalize and denormalize cards. Calling this function
/// on an already normalized card reverses the normaliztion
fn transform_card(c: Card, face_up_suit: Suit) -> Card {
    let new_suit = transform_suit(c.suit(), face_up_suit);
    c.to_suit(new_suit)
}

fn transform_suit(s: Suit, face_up_suit: Suit) -> Suit {
    use Suit::*;
    match (face_up_suit, s) {
        (Clubs, Clubs) => Spades,
        (Clubs, Spades) => Clubs,
        (Clubs, Hearts) => Hearts,
        (Clubs, Diamonds) => Diamonds,
        (Spades, Clubs) => Clubs,
        (Spades, Spades) => Spades,
        (Spades, Hearts) => Hearts,
        (Spades, Diamonds) => Diamonds,
        (Hearts, Clubs) => Diamonds,
        (Hearts, Spades) => Hearts,
        (Hearts, Hearts) => Spades,
        (Hearts, Diamonds) => Clubs,
        (Diamonds, Clubs) => Hearts,
        (Diamonds, Spades) => Diamonds,
        (Diamonds, Hearts) => Clubs,
        (Diamonds, Diamonds) => Spades,
    }
}

#[cfg(test)]
mod tests {
    use crate::game::euchre::{
        actions::{Card, EAction, Suit},
        deck::{CardLocation, Deck, CARDS},
        ismorphic::{iso_deck, swap_loc},
    };

    use super::{transform, transform_card};

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
    fn test_swap_loc() {
        let mut l = 0b1010_0101;
        swap_loc(&mut l, 0, 1);
        assert_eq!(l, 0b0101_1010);

        let mut l = 0;
        swap_loc(&mut l, 0, 1);
        assert_eq!(l, 0);
    }

    #[test]
    fn test_normalize_denormalize() {
        for suit in [Suit::Spades, Suit::Clubs, Suit::Hearts, Suit::Diamonds] {
            for c in CARDS {
                let normalized = transform_card(*c, suit);
                let denormalized = transform_card(normalized, suit);
                assert_eq!(denormalized, *c, "{} with face up suit {}", *c, suit)
            }
        }

        for suit in [
            EAction::Spades,
            EAction::Clubs,
            EAction::Hearts,
            EAction::Diamonds,
        ] {
            for face_up in [Suit::Spades, Suit::Clubs, Suit::Hearts, Suit::Diamonds] {
                let normalized = transform(suit.into(), face_up);
                let denormalized = transform(normalized, face_up);
                assert_eq!(denormalized, suit.into())
            }
        }
    }
}
