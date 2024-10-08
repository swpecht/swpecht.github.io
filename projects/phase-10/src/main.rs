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
    Skip,
}

enum Action {
    Take,
    Draw,
}

/// Store results of a game run
struct RunStats {
    turns_to_win: i32,
}

fn main() {
    env_logger::init();

    let mut rng = thread_rng();
    const NUM_PLAYS: usize = 100000;
    const ENABLE_ANTAG_DISCARD: bool = true;
    // let policy = greedy_5_after_n;
    // let policy = take_if_pair;
    let runs: Vec<(&str, for<'r> fn(&'r Vec<Card>, Card) -> Action)> = vec![
        ("Greedy pairs", greedy_pairs),
        ("Greedy 5 after 4", greedy_5_after_4),
        ("Greedy 5 after 3", greedy_5_after_3),
        ("Hide until 4", hide_until_4),
        ("Hide until 3", hide_until_3),
    ];

    for r in runs {
        println!("{}", r.0);
        let mut run_tape = Vec::with_capacity(NUM_PLAYS);
        for _ in 0..NUM_PLAYS {
            let stats = play_game(&mut rng, r.1, ENABLE_ANTAG_DISCARD);
            run_tape.push(stats.turns_to_win);
        }

        println!("Average number of turns: {}", mean(&run_tape));
        println!("Median turns: {}", median(&mut run_tape));
        println!();
    }
}

/// Returns the median and sorts the array
fn median(array: &mut Vec<i32>) -> f64 {
    array.sort();
    if (array.len() % 2) == 0 {
        let ind_left = array.len() / 2 - 1;
        let ind_right = array.len() / 2;
        (array[ind_left] + array[ind_right]) as f64 / 2.0
    } else {
        array[(array.len() / 2)] as f64
    }
}

fn mean(array: &Vec<i32>) -> f64 {
    array.iter().sum::<i32>() as f64 / array.len() as f64
}

/// Return the number of turns to get phase
fn play_game<F>(rng: &mut ThreadRng, take_policy: F, antagonistic_discard: bool) -> RunStats
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
    let mut taken_cards = Vec::new();

    loop {
        turn_count += 1;

        // Keep drawing new cards until we get to one we haven't taken before
        let mut candidate = draw_card(&mut draw_pile, &mut discard_pile, rng);
        while taken_cards.contains(&candidate) && antagonistic_discard {
            candidate = draw_card(&mut draw_pile, &mut discard_pile, rng);
        }

        // Includes some baseline policy decisions:
        // * Always take a wild card
        // * Always draw when a skip card comes up
        match (candidate, take_policy(&hand, candidate)) {
            (Card::Wild, _) => hand.push(candidate),
            (Card::Skip, _) | (_, Action::Draw) => {
                // Draw before the discard
                let c = draw_card(&mut draw_pile, &mut discard_pile, rng);
                discard_pile.push(candidate);
                hand.push(c);
            }
            (_, Action::Take) => {
                hand.push(candidate);
                taken_cards.push(candidate)
            }
        }

        discard_pile.push(discard(&mut hand));

        // Cycle the cards as if another player went
        let c = draw_card(&mut draw_pile, &mut discard_pile, rng);
        discard_pile.push(c);

        assert_eq!(hand.len(), 10);
        if evaluate(&hand) {
            break;
        }
    }
    info!("{:?}", hand);

    return RunStats {
        turns_to_win: turn_count,
    };
}

/// Take a card if a copy exists in the hand, otherwise, draw
fn greedy_pairs(hand: &Vec<Card>, candidate_card: Card) -> Action {
    match hand.contains(&candidate_card) {
        true => Action::Take,
        _ => Action::Draw,
    }
}

/// Two phases:
/// * If no n of a kind in hand, take if pair
/// * If n of a kind or more, draw card
fn greedy_5_after_n(hand: &Vec<Card>, candidate_card: Card, target_n: i32) -> Action {
    let counts = get_counts(&hand);
    let (_, mcount) = counts[counts.len() - 1]; // end of list has highest count

    if mcount < target_n {
        return greedy_pairs(hand, candidate_card);
    }

    for (card, count) in counts {
        match (card, count) {
            // Check to ensure don't already have 5 of a kind
            (x, n) if x == candidate_card && n >= target_n && n < 5 => return Action::Take,
            _ => continue,
        };
    }

    return Action::Draw;
}

fn greedy_5_after_3(hand: &Vec<Card>, candidate_card: Card) -> Action {
    greedy_5_after_n(hand, candidate_card, 3)
}

fn greedy_5_after_4(hand: &Vec<Card>, candidate_card: Card) -> Action {
    greedy_5_after_n(hand, candidate_card, 4)
}

fn hide_until_3(hand: &Vec<Card>, candidate_card: Card) -> Action {
    return hide_until_n(hand, candidate_card, 3);
}

fn hide_until_4(hand: &Vec<Card>, candidate_card: Card) -> Action {
    return hide_until_n(hand, candidate_card, 4);
}

fn hide_until_n(hand: &Vec<Card>, candidate_card: Card, target_n: i32) -> Action {
    let counts = get_counts(&hand);
    let (_, mcount) = counts[counts.len() - 1]; // end of list has highest count

    if mcount <= target_n {
        return Action::Draw;
    }

    for (card, count) in counts {
        match (card, count) {
            // Check to ensure don't already have 5 of a kind
            (x, n) if x == candidate_card && n >= target_n && n < 5 => return Action::Take,
            _ => continue,
        };
    }

    return Action::Draw;
}

/// Returns a sorted list from lowest to highest by frequency of cards.
///
/// Exclude wild cards
fn get_counts(cards: &Vec<Card>) -> Vec<(Card, i32)> {
    let mut counts = HashMap::new();
    for c in cards {
        if *c == Card::Wild || *c == Card::Skip {
            // Don't get counts for wilds or skips
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
    // Always discard a skip card if possible
    if let Some(i) = hand.into_iter().position(|x| *x == Card::Skip) {
        return hand.remove(i);
    }

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

    for _ in 0..4 {
        deck.push(Card::Wild);
    }

    // Should be 108 total deck size
    // https://en.wikipedia.org/wiki/Phase_10
    assert_eq!(deck.len(), 108);

    return deck;
}

#[cfg(test)]
mod tests {
    use crate::{discard, evaluate, Card, Face};

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

    #[test]
    fn test_discard() {
        let mut hand = vec![
            Card::Regular(Face::One),
            Card::Regular(Face::Two),
            Card::Regular(Face::Two),
            Card::Regular(Face::Three),
            Card::Regular(Face::Three),
            Card::Regular(Face::Three),
            Card::Skip,
            Card::Wild,
            Card::Wild,
        ];

        assert_eq!(discard(&mut hand), Card::Skip);
        assert_eq!(discard(&mut hand), Card::Regular(Face::One));
        assert_eq!(discard(&mut hand), Card::Regular(Face::Two));
        assert_eq!(discard(&mut hand), Card::Regular(Face::Two));
        assert_eq!(discard(&mut hand), Card::Regular(Face::Three));
    }
}
