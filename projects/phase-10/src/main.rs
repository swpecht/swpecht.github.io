use std::collections::HashMap;

use num_derive::FromPrimitive;

use rand::prelude::ThreadRng;
use rand::seq::SliceRandom;
use rand::thread_rng;

use log::info;

#[derive(Debug, FromPrimitive, PartialEq, Eq, Hash, Clone, Copy)]
enum Face {
    One = 1,
    Two,
    Three,
    Four,
    Five,
    Six,
    Seven,
    Eight,
    Nine,
    Ten,
    Eleven,
    Twelve,
    Wild,
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
enum Card {
    Regular(Face),
    Wild,
}

enum Action {
    Take,
    Draw,
}

/// Deck overview: 108 cards
/// 96 numbered: 2 of each value from 1-12 in each of the four colors
/// 8 wild
/// 4 skip cards
///
/// Looking at:
/// * Phase 7: 2 sets of 4
/// * Phase 9: 1 set of 5, 1 set of 2
/// * Phase 10: 1 set of 5, 1 set of 3
///
/// Ref: https://en.wikipedia.org/wiki/Phase_10
///
/// Strategies:
/// * Greedily make pairs
/// * Greedily take cards going for larger set
/// * ...
///
/// Design:
/// * Store cards as list
/// * Each card is the index, state is the value
///
///
/// Starting with a hardcoded policy and score. Could we do this with ML and instead not have to write this?
///
fn main() {
    env_logger::init();

    let mut rng = thread_rng();
    const NUM_PLAYS: i32 = 10000;
    let mut total_turns = 0;

    for _ in 0..NUM_PLAYS {
        total_turns += play_game(&mut rng);
    }

    println!(
        "Average number of turns: {}",
        total_turns as f64 / NUM_PLAYS as f64
    )
}

/// Return the number of turns to get phase
fn play_game(rng: &mut ThreadRng) -> i32 {
    let mut draw_pile = create_deck();
    let mut discard_pile = Vec::new();
    draw_pile.shuffle(rng);

    // Deal 10 cards
    let mut hand = Vec::new();
    for _ in 0..10 {
        hand.push(draw_card(&mut draw_pile, &mut discard_pile, rng));
    }
    info!("{:?}", hand);

    // Main gameplay loop
    let mut turn_count = 0;
    loop {
        turn_count += 1;

        let candidate = draw_card(&mut draw_pile, &mut discard_pile, rng);
        match take_or_draw(&hand, candidate) {
            Action::Take => discard_pile.push(discard(&mut hand, candidate)),
            Action::Draw => {
                // Draw before the discard
                let c = draw_card(&mut draw_pile, &mut discard_pile, rng);
                discard_pile.push(candidate);
                discard_pile.push(discard(&mut hand, c))
            }
        }

        if evaluate(&hand) {
            break;
        }
    }
    info!("{:?}", hand);

    return turn_count;
}

/// Determine if should take face up card or draw
fn take_or_draw(hand: &Vec<Card>, candidate_card: Card) -> Action {
    match hand.contains(&candidate_card) {
        true => Action::Take,
        _ => Action::Draw,
    }
}

/// Returns the discarded card
fn discard(hand: &mut Vec<Card>, candidate_card: Card) -> Card {
    if hand.contains(&candidate_card) || candidate_card == Card::Wild {
        hand.push(candidate_card);
        for i in 0..hand.len() {
            if hand[i] != candidate_card {
                return hand.remove(i);
            }
        }
    }
    return candidate_card;
}

/// Return true if the game is over, evaluating a hand for
/// Phase 9.
///
/// TODO: implement other phases
///
/// To handle wild cards, could evaluate based on on points,
/// e.g. 5 of a kind is 5 points (only 1), 3 of a kind is 3 points (only 1)
fn evaluate(hand: &Vec<Card>) -> bool {
    let mut counts = HashMap::new();
    for c in hand {
        if let Some(&count) = counts.get(&c) {
            counts.insert(c, count + 1)
        } else {
            counts.insert(c, 1)
        };
    }

    let mut num_wilds = *counts.get(&Card::Wild).unwrap_or(&0);
    counts.remove(&Card::Wild);

    // We only need to check the top 2 most common cards for a match. We can greedily
    // consume the wild cards to try to make a match.
    let mut histogram = counts.values().collect::<Vec<&i32>>();
    histogram.sort();

    let five_candidate = *histogram.pop().unwrap_or(&0);
    let set_5 = (five_candidate + num_wilds) >= 5;
    num_wilds = num_wilds - (5 - five_candidate); // consume the used wild cards

    let three_candidate = *histogram.pop().unwrap_or(&0);
    let set_3 = (three_candidate + num_wilds) >= 3;

    return set_5 && set_3;
}

/// Returns a card from the top of the deck.
///
/// TODO: implement re-shuffling deck if draw pile is empty
fn draw_card(draw_pile: &mut Vec<Card>, discard_pile: &mut Vec<Card>, rng: &mut ThreadRng) -> Card {
    if draw_pile.len() == 0 {
        info!("reshuffling discard pile");
        for _ in 0..discard_pile.len() {
            let c = discard_pile.pop().unwrap();
            draw_pile.push(c);
        }
        draw_pile.shuffle(rng);
    }

    let c = draw_pile.pop().unwrap();
    info!("drew card: {:?}", c);
    return c;
}

fn create_deck() -> Vec<Card> {
    info!("creating deck");

    let mut deck = Vec::new();

    // Add each face card twice
    for f in 1..13 {
        for _ in 0..4 {
            // Colors
            for _ in 0..2 {
                let card = Card::Regular(num::FromPrimitive::from_i32(f).unwrap());
                deck.push(card);
            }
        }
    }

    // Add wild cards
    for _ in 0..8 {
        deck.push(Card::Wild)
    }

    // Should be 104 cards. The 108 total deck size less the 4 skip cards
    assert_eq!(deck.len(), 104);

    return deck;
}

#[cfg(test)]
mod tests {
    use crate::{evaluate, Card, Face};

    #[test]
    fn test_evaluate() {
        let mut hand = vec![
            Card::Regular(Face::One),
            Card::Regular(Face::One),
            Card::Regular(Face::One),
            Card::Regular(Face::One),
            Card::Regular(Face::Two),
            Card::Regular(Face::Two),
        ];
        assert!(!evaluate(&hand));

        hand.push(Card::Regular(Face::One));
        assert!(!evaluate(&hand));

        hand.push(Card::Regular(Face::Two));
        assert!(evaluate(&hand));

        // Check if still works even if 5 of each
        hand.push(Card::Regular(Face::Two));
        hand.push(Card::Regular(Face::Two));
        assert!(evaluate(&hand));
    }

    #[test]
    fn test_evaluate_with_wild() {
        let hand = vec![
            Card::Regular(Face::One),
            Card::Regular(Face::One),
            Card::Regular(Face::One),
            Card::Regular(Face::One),
            Card::Regular(Face::Two),
            Card::Regular(Face::Two),
            Card::Wild,
            Card::Wild,
        ];

        assert!(evaluate(&hand));
    }
}
