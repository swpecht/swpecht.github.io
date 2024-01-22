use crate::{
    gamestates::euchre::deck::CardLocation,
    istate::{IStateKey, IStateNormalizer, NormalizedAction, NormalizedIstate},
    Action, GameState,
};

use super::{
    actions::{Card, EAction, Suit, UNSUITED_ACTION_MASK},
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
        let transform = norm_transform(&gs.istate_key(gs.cur_player()));
        NormalizedAction::new(transform(EAction::from(action)).into())
    }

    fn denormalize_action(&self, action: NormalizedAction, gs: &EuchreGameState) -> Action {
        let transform = denorm_transform(&gs.istate_key(gs.cur_player()));
        transform(EAction::from(action.get())).into()
    }

    fn normalize_istate(
        &self,
        istate: &crate::istate::IStateKey,
        _gs: &EuchreGameState,
    ) -> crate::istate::NormalizedIstate {
        let new_istate = normalize_euchre_istate(istate);
        NormalizedIstate::new(new_istate)
    }
}

/// Converts an istate into it's isomorphic form:
/// * Hand is sorted
/// * Spades is always the face up card
pub fn normalize_euchre_istate(istate: &IStateKey) -> IStateKey {
    let transform = norm_transform(istate);

    let mut new_istate = IStateKey::default();
    for a in istate.into_iter().map(EAction::from) {
        let norm_a = transform(a).into();
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

fn face_up(istate: &IStateKey) -> Suit {
    EAction::from(istate[5]).card().suit()
}

fn suit_order(istate: &IStateKey) -> [u8; 4] {
    let mut visible_cards = 0;

    for a in istate.into_iter().map(EAction::from) {
        visible_cards |= a as u32;
    }
    // remove the unsuited actions
    visible_cards &= !UNSUITED_ACTION_MASK;

    visible_cards.to_ne_bytes()
}

fn norm_transform(istate: &IStateKey) -> fn(EAction) -> EAction {
    // no transform without a face up card
    if istate.len() < 6 {
        return |x| x;
    }

    let face_up_suit = face_up(istate);
    let suit_order = suit_order(istate);

    match (
        face_up_suit,
        suit_order[3] > suit_order[2],
        suit_order[1] > suit_order[0],
    ) {
        (Suit::Spades, false, _) => |x| x, // no translation needed for spades
        (Suit::Spades, true, _) => |x| x.swap_red(), // normalize red cards
        (Suit::Clubs, false, _) => |x| x.swap_black(),
        (Suit::Clubs, true, _) => |x| x.swap_black().swap_red(),
        (Suit::Hearts, _, false) => |x| x.swap_color(),
        (Suit::Hearts, _, true) => |x| x.swap_color().swap_red(),
        (Suit::Diamonds, _, false) => |x| x.swap_color().swap_black(),
        (Suit::Diamonds, _, true) => |x| x.swap_color().swap_black().swap_red(),
    }
}

/// Performs the reverse of the norm transform
fn denorm_transform(istate: &IStateKey) -> fn(EAction) -> EAction {
    // no transform without a face up card
    if istate.len() < 6 {
        return |x| x;
    }

    let face_up_suit = face_up(istate);
    let suit_order = suit_order(istate);

    match (
        face_up_suit,
        suit_order[3] > suit_order[2],
        suit_order[1] > suit_order[0],
    ) {
        (Suit::Spades, false, _) => |x| x,
        (Suit::Spades, true, _) => |x| x.swap_red(),
        (Suit::Clubs, false, _) => |x| x.swap_black(),
        (Suit::Clubs, true, _) => |x| x.swap_red().swap_black(),
        (Suit::Hearts, _, false) => |x| x.swap_color(),
        (Suit::Hearts, _, true) => |x| x.swap_red().swap_color(),
        (Suit::Diamonds, _, false) => |x| x.swap_black().swap_color(),
        (Suit::Diamonds, _, true) => |x| x.swap_black().swap_red().swap_color(),
    }
}

#[cfg(test)]
mod tests {
    use itertools::Itertools;

    use crate::{
        actions,
        gamestates::euchre::{
            actions::{Card, EAction, Suit},
            deck::{CardLocation, Deck},
            ismorphic::{iso_deck, swap_loc, LossyEuchreNormalizer},
            EuchreGameState,
        },
        istate::{IStateKey, IStateNormalizer},
        GameState,
    };

    use super::*;

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
    fn test_normalize_edge_cases() {
        validate_norm_denorm("Qs9cJcAhKd|JsTdJdQdAd|KsAsAcJhQh|TsTcKc9h9d|Kh|PPPPP");
        validate_norm_denorm("TsAsQc9hAh|9sTcQhQdAd|Qs9cKcKhJd|JsKsJcAc9d|Th|T|")
    }

    fn validate_norm_denorm(state_string: &str) {
        let gs = EuchreGameState::from(state_string);
        let normalizer = EuchreNormalizer::default();
        let actions = actions!(gs)
            .into_iter()
            .map(EAction::from)
            .sorted()
            .collect_vec();
        let normed_actions = actions
            .iter()
            .map(|x| normalizer.normalize_action(Action::from(*x), &gs))
            .map(|x| EAction::from(x.get()))
            .sorted()
            .collect_vec();
        let denormed_actions = normed_actions
            .iter()
            .map(|x| normalizer.denormalize_action(NormalizedAction::new(Action::from(*x)), &gs))
            .map(EAction::from)
            .sorted()
            .collect_vec();

        assert_eq!(denormed_actions, actions, "{}", state_string);
    }

    #[test]
    fn test_lossy_normalizer() {
        let normalizer = LossyEuchreNormalizer::default();

        let gs = EuchreGameState::from("Qc9sTs9hAh|9cKsThQhTd|KcAsJhKhQd|AcJs9dAdJd|Qs");
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
            JS, JC, NH, AH, JD, QS, Pickup, NH, NS, KS, AS, JS, JC, TS, NH, NC,
        ];
        assert_eq!(
            normalizer.normalize_istate(&key, &gs).get(),
            IStateKey::from(should.as_slice())
        );
    }
}
