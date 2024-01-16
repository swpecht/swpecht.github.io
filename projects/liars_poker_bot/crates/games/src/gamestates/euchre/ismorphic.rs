use crate::{
    gamestates::euchre::deck::CardLocation,
    istate::{IStateKey, IStateNormalizer, NormalizedAction, NormalizedIstate},
    Action,
};

use super::{
    actions::{Card, EAction, Suit},
    deck::Deck,
    EPhase, EuchreGameState,
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
        let none_cards = deck.get_all(CardLocation::None);
        for s in SUITS {
            for c in get_cards(*s, trump) {
                if !none_cards.contains(*c) {
                    locations[*s as usize] <<= WORD_SIZE;
                    locations[*s as usize] |= location_mask(deck.get(*c))
                }
            }
        }
    } else {
        // todo: could do this in a single pass if needed for performance reasons
        // if no trump, we load everything and then swap around the jacks
        for s in SUITS {
            for c in get_cards(*s, trump) {
                locations[*s as usize] <<= WORD_SIZE;
                locations[*s as usize] |= location_mask(deck.get(*c))
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

#[derive(Default, Clone)]
pub struct EuchreNormalizer {}

impl IStateNormalizer<EuchreGameState> for EuchreNormalizer {
    /// Normalizes the suit to have Spades always be the faceup card.
    fn normalize_action(&self, action: Action, gs: &EuchreGameState) -> NormalizedAction {
        let face_up_suit = gs.face_up().map(|c| c.suit());
        if face_up_suit.is_none() {
            return NormalizedAction::new(action);
        }

        NormalizedAction::new(transform(action, face_up_suit.unwrap()))
    }

    fn denormalize_action(&self, action: NormalizedAction, gs: &EuchreGameState) -> Action {
        let face_up_suit = gs.face_up().map(|c| c.suit());
        if face_up_suit.is_none() {
            return action.get();
        }

        transform(action.get(), face_up_suit.unwrap())
    }

    fn normalize_istate(
        &self,
        istate: &crate::istate::IStateKey,
        gs: &EuchreGameState,
    ) -> crate::istate::NormalizedIstate {
        let new_istate = normalize_euchre_istate(istate);
        NormalizedIstate::new(new_istate)
    }
}

/// Converts an istate into it's isomorphic form:
/// * Hand is sorted
/// * Spades is always the face up card
pub fn normalize_euchre_istate(istate: &IStateKey) -> IStateKey {
    if istate.len() <= 6 {
        let mut new_istate = *istate;
        new_istate.sort_range(0, 5.min(new_istate.len()));
        return new_istate;
    }

    let mut new_istate = IStateKey::default();
    let face_up_suit = EAction::from(istate[5]).card().suit();

    for a in *istate {
        let norm_a = transform(a, face_up_suit);
        new_istate.push(norm_a);
    }

    // re-sort the hand
    new_istate.sort_range(0, 5.min(new_istate.len()));
    new_istate
}

/// Normalizes the play cards for euchre to normalize the istate
#[derive(Default, Clone)]
pub struct LossyEuchreNormalizer {
    baseline: EuchreNormalizer,
}

impl IStateNormalizer<EuchreGameState> for LossyEuchreNormalizer {
    fn normalize_action(&self, action: Action, gs: &EuchreGameState) -> NormalizedAction {
        self.baseline.normalize_action(action, gs)
    }

    fn denormalize_action(&self, action: NormalizedAction, gs: &EuchreGameState) -> Action {
        self.baseline.denormalize_action(action, gs)
    }

    /// Only exactly tracks the cards in our hand, face up card, trump card, and the lead suit in a trick.
    ///
    /// Everything else is replaced with a 9 of the relevant suit
    fn normalize_istate(&self, istate: &IStateKey, gs: &EuchreGameState) -> NormalizedIstate {
        let mut new_key = self.baseline.normalize_istate(istate, gs).get();

        if gs.phase() != EPhase::Play {
            return NormalizedIstate::new(new_key);
        }

        let len = new_key.len();
        let cards_played = gs.cards_played;
        let trump = gs.trump.unwrap();
        let mut lead_suit = Suit::Spades;

        new_key
            .iter_mut()
            .skip(len - cards_played)
            .enumerate()
            .for_each(|(i, x)| {
                let a = EAction::from(*x);
                if i % 4 == 0 {
                    lead_suit = gs.get_suit(a.card());
                }
                let new_a = match a {
                    a if gs.get_suit(a.card()) == trump => a,
                    a if gs.get_suit(a.card()) == lead_suit => a,
                    a if a.card().suit() == Suit::Clubs => EAction::NC,
                    a if a.card().suit() == Suit::Spades => EAction::NS,
                    a if a.card().suit() == Suit::Diamonds => EAction::ND,
                    a if a.card().suit() == Suit::Hearts => EAction::NH,
                    _ => panic!("invalid action to convert: {}", a),
                };

                *x = new_a.into();
            });

        NormalizedIstate::new(new_key)
    }
}

fn transform(action: Action, face_up_suit: Suit) -> Action {
    // We can apply the transform again to denormalize the action
    use EAction::*;
    let ea = EAction::from(action);
    let new_action = match ea {
        NC | TC | JC | QC | KC | AC | NS | TS | JS | QS | KS | AS | NH | TH | JH | QH | KH | AH
        | ND | TD | JD | QD | KD | AD => transform_card(ea.card(), face_up_suit).into(),

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
    use crate::{
        gamestates::euchre::{
            actions::{Card, EAction, Suit},
            deck::{CardLocation, Deck, CARDS},
            ismorphic::{iso_deck, swap_loc, LossyEuchreNormalizer},
            EuchreGameState,
        },
        istate::{IStateKey, IStateNormalizer},
        GameState,
    };

    use super::{transform, transform_card};

    #[test]
    fn test_deck_iso_no_trump() {
        let mut d1 = Deck::default();

        d1.set(Card::NS, CardLocation::Player0);

        let mut d2 = d1;

        assert_eq!(iso_deck(d1, None), iso_deck(d2, None));
        d2.set(Card::TS, CardLocation::Player0);

        assert!(iso_deck(d1, None) != iso_deck(d2, None));
        d2.set(Card::NS, CardLocation::None);

        assert_eq!(iso_deck(d1, None), iso_deck(d1, None));
    }

    #[test]
    fn test_deck_iso_across_suit() {
        let mut d1 = Deck::default();
        d1.set(Card::NS, CardLocation::Player0);
        d1.set(Card::TS, CardLocation::Player0);
        d1.set(Card::JC, CardLocation::Player1);

        let mut d2 = Deck::default();
        d2.set(Card::NH, CardLocation::Player0);
        d2.set(Card::TH, CardLocation::Player0);
        d2.set(Card::JD, CardLocation::Player1);

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

        d1.set(Card::NS, CardLocation::Player0);
        d1.set(Card::TS, CardLocation::Player0);

        let mut d2 = d1;

        assert_eq!(
            iso_deck(d1, Some(Suit::Spades)),
            iso_deck(d2, Some(Suit::Spades))
        );
        d2.set(Card::JS, CardLocation::Player0);

        assert!(iso_deck(d1, Some(Suit::Spades)) != iso_deck(d2, Some(Suit::Spades)));
        d2.set(Card::NS, CardLocation::None);
        // player 0  still has the 2 highest spades
        assert_eq!(
            iso_deck(d1, Some(Suit::Spades)),
            iso_deck(d2, Some(Suit::Spades))
        );
        d2.set(Card::JC, CardLocation::Player0);
        d2.set(Card::TS, CardLocation::None);
        // player 0  still has the 2 highest spades, JC and JS
        assert_eq!(
            iso_deck(d1, Some(Suit::Spades)),
            iso_deck(d2, Some(Suit::Spades))
        );

        // this persists even if we deal some other cards
        d1.set(Card::TH, CardLocation::Player1);
        d2.set(Card::TH, CardLocation::Player1);
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

    #[test]
    fn test_lossy_normalizer() {
        let normalizer = LossyEuchreNormalizer::default();

        let gs = EuchreGameState::from("Qc9sTs9dAd|9cKsThQhTd|KcAsJhKhQd|AcJs9hAhJd|Qs");
        let key = gs.istate_key(0);
        assert_eq!(
            gs.istate_key(0),
            normalizer.normalize_istate(&key, &gs).get()
        );

        let gs = EuchreGameState::from(
            "Qc9sTs9dAd|9cKsThQhTd|KcAsJhKhQd|JcJs9hAhJd|Qs|T|9h|9sKsAsJs|JcTsQhKc",
        );
        let key = gs.istate_key(3);

        use EAction::*;
        let should = &[
            JC, JS, NH, AH, JD, QS, Pickup, NH, NS, KS, AS, JS, JC, TS, NH, NC,
        ];
        assert_eq!(
            normalizer.normalize_istate(&key, &gs).get(),
            IStateKey::from(should.as_slice())
        );
    }
}
