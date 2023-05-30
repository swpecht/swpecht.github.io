use crate::game::euchre::deck::CardLocation;

use super::{
    actions::{Card, Suit},
    deck::Deck,
};

const JACK_RANK: usize = 2;
const SUITS: &[Suit] = &[Suit::Spades, Suit::Clubs, Suit::Hearts, Suit::Diamonds];
const RANKS: &[usize] = &[0, 1, 2, 3, 4, 5];

const CARDS: &[Card] = &[
    Card::NC,
    Card::TC,
    Card::JC,
    Card::QC,
    Card::KC,
    Card::AC,
    Card::NS,
    Card::TS,
    Card::JS,
    Card::QS,
    Card::KS,
    Card::AS,
    Card::NH,
    Card::TH,
    Card::JH,
    Card::QH,
    Card::KH,
    Card::AH,
    Card::ND,
    Card::TD,
    Card::JD,
    Card::QD,
    Card::KD,
    Card::AD,
];

pub(super) fn iso_deck(deck: Deck, trump: Option<Suit>) -> [[CardLocation; 7]; 4] {
    let mut locations = [[CardLocation::None; 7]; 4];

    // todo: build this without spacing to begin with somehow? Can we do this in a single pass without need to "squish" things later
    for &c in CARDS {
        let (s, r) = get_index(c, trump);
        locations[s][r] = deck[c];
    }

    if trump.is_some() {
        // todo: be smater to downshift some things if there is no trump, really just can't move the jacks and above
        for suit_locations in locations.iter_mut() {
            let mut i = 0;
            let mut last_card = suit_locations.len();
            // We downshift cards that are in the None location. For example,a 10 is as valuable in future hands as a 9
            // if the 9 has been played already
            while i < last_card {
                if suit_locations[i] == CardLocation::None {
                    suit_locations[i..].rotate_left(1);
                    last_card -= 1;
                } else {
                    i += 1;
                }
            }
        }
    } else {
        // we can only swap things around the jacks
        for suit_locations in locations.iter_mut() {
            for i in 0..suit_locations.len() - 1 {
                if i != JACK_RANK && i + 1 != JACK_RANK && suit_locations[i] == CardLocation::None {
                    suit_locations.swap(i, i + 1);
                }
            }
        }
    }

    fn get_count(x: &[CardLocation]) -> usize {
        x.iter().filter(|x| **x != CardLocation::None).count()
    }

    if let Some(trump) = trump {
        // put trump in the first spot
        locations.swap(0, trump as usize);
        // sort everything else
        locations[1..].sort_by_key(|a| get_count(a));
    } else {
        // sort everything
        locations.sort_by_key(|a| get_count(a));
    }

    locations
}

fn get_index(c: Card, trump: Option<Suit>) -> (usize, usize) {
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

#[cfg(test)]
mod tests {
    use crate::game::euchre::{
        actions::{Card, Suit},
        deck::{CardLocation, Deck},
        ismorphic::iso_deck,
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
}
