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
    const NUM_PLAYS: i32 = 100000;
    let mut total_turns = 0;
    let policy = take_if_no_n_of_kind;

    for _ in 0..NUM_PLAYS {
        total_turns += play_game(&mut rng, policy);
    }

    println!(
        "Average number of turns: {}",
        total_turns as f64 / NUM_PLAYS as f64
    )
}

/// Return the number of turns to get phase
fn play_game<F>(rng: &mut ThreadRng, take_policy: F) -> i32
where
    F: Fn(&Vec<Card>, Card) -> Action,
{
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
        match take_policy(&hand, candidate) {
            Action::Take => hand.push(candidate),
            Action::Draw => {
                // Draw before the discard
                let c = draw_card(&mut draw_pile, &mut discard_pile, rng);
                discard_pile.push(candidate);
                hand.push(c);
            }
        }

        discard_pile.push(discard(&mut hand));

        assert_eq!(hand.len(), 10);
        if evaluate(&hand) {
            break;
        }
    }
    info!("{:?}", hand);

    return turn_count;
}

/// Take a card if a copy exists in the hand, otherwise, draw
fn take_if_pair(hand: &Vec<Card>, candidate_card: Card) -> Action {
    match hand.contains(&candidate_card) || candidate_card == Card::Wild {
        true => Action::Take,
        _ => Action::Draw,
    }
}

/// Two phases:
/// * If no n of a kind in hand, take if pair
/// * If n of a kind or more, draw card
fn take_if_no_n_of_kind(hand: &Vec<Card>, candidate_card: Card) -> Action {
    let counts = get_counts(&hand);
    let (mcard, mcount) = counts[counts.len() - 1];

    if mcount < 3 {
        return take_if_pair(hand, candidate_card);
    }

    // If it's part of the max set or wild, take it
    return match candidate_card {
        x if x == mcard => Action::Take,
        Card::Wild => Action::Take,
        _ => Action::Draw,
    };
}

/// Returns a sorted list from lowest to highest by frequency of cards.
///
/// Exclude wild cards
fn get_counts(cards: &Vec<Card>) -> Vec<(Card, i32)> {
    let mut counts = HashMap::new();
    for c in cards {
        if *c == Card::Wild {
            // Don't get counts for wildcards
            continue;
        }
        if let Some(&count) = counts.get(c) {
            counts.insert(*c, count + 1)
        } else {
            counts.insert(*c, 1)
        };
    }

    let mut result = Vec::new();
    for (k, v) in counts {
        result.push((k, v));
    }

    result.sort_by(|&a, &b| a.1.cmp(&b.1));

    return result;
}

/// Returns the discarded card.
///
/// Discards the least common non-wild card in the hand
fn discard(hand: &mut Vec<Card>) -> Card {
    let mut counts: HashMap<Card, usize> = HashMap::new();
    let hand_size = hand.len();

    for c in hand.into_iter() {
        if *c == Card::Wild {
            // Don't get counts for wildcards
            continue;
        }
        if let Some(&count) = counts.get(&c) {
            counts.insert(*c, count + 1)
        } else {
            counts.insert(*c, 1)
        };
    }

    let min_count = *counts.values().min().unwrap();
    for i in 0..hand_size {
        let count = *counts.get(&hand[i]).unwrap_or(&1);
        if count == min_count {
            return hand.remove(i);
        }
    }

    assert!(false); // Should never get here
    return hand.remove(0);
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
