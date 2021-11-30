---
layout: post
title:  "How to not lose at Phase 10"
date:   2021-11-30 00:00:00 +0000
categories: project-log
---

## Context

Phase 10 is a rummy-like cardgame where the player needs to accomplish various goals (or phases) to progress. For example, the player may need to get 2 sets of 3 cards or a 7 card straight. Wikipedia gives a good overview of [the rules](https://en.wikipedia.org/wiki/Phase_10).

## Problem

While playing with family over Thanksgiving, I was unsure on the best approach to accomplishing phases 9 and 10 of the game. These phases require the player to get 1 set of 5 and 1 set of 2 (or 3). Specifically, I was trying to decide between:

1. Greedily taking cards from the discard pile to start pairs, hurting my near term chances of getting to the set of 5, but giving me more options in the future.
2. Only taking cards from the discard pile when they helped me get to sets of 3 or 4 and instead taking my chances with the draw pile.

As you can imagine, these decisions are incredbily high stakes, especially when playing with family.

What is the best policy for determining when to take the face up card or to draw a new card? Where best lets us complete Phase 10 (1 set of 5 and 1 set of 3) in the smallest number of turns.

## Approach

I built a simulation for Phase 10 in Rust ([code on Github](https://github.com/swpecht/swpecht.github.io/tree/master/projects/phase-10)). I chose Rust for fun.

The main area of interest in the 'take-policy' -- the set or rules that determine when to take the face-up discarded card or to draw an unknown card\[0\].

For each take-policy, I run 100k simulations and compare the median and average turns to win. See below for an overview of the policies.

### Baseline policy

Rules that apply no matter what we're doing:

* Always take wild cards
* Always draw if a skip card is on the discard pile. Skip cards can't contribute to a set.

### Greedy pairs (1 from above)

If taking the discard card gives us a pair or better, take it.

```rust
fn greedy_pairs(hand: &Vec<Card>, candidate_card: Card) -> Action {
    match hand.contains(&candidate_card) {
        true => Action::Take,
        _ => Action::Draw,
    }
}
```

### Greedy 5 of a kind after N (2 from above)

Play the same as `greedy pairs` until we have a set of N cards. After that, we greedily try to complete the set of 5, so we only take from the discard pile if it moves us towards the set. Otherwise, we draw a card.

```rust
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
```

### Hide intentions until N of a kind

Always draw a card unless picking up a card completes a set of N. The goal is to hide what cards you're going for from opponents as discarded cards come from the player to your left.

```rust
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
```

## Initial results: Greedy pairs wins

|Policy |Num turns (median)  | Num turns (average)  |
|-------|-----------|-----------|
Greedy pairs                |10 |10.1   |
Greedy 5 of a kind after 4  |10 |10.3   |
Greedy 5 of a kind after 3  |11 |11.1   |
Hide until 4 of a kind      |14 |15.1   |
Hide until 3 of a kind      |14 |14.7   |

`Greedy pairs` seems to be the best approach. It has the lowest median and mean number of turns to win. But the main downside of this approach is that it gives information to other players for what cards you're going for. Right now, we model the discard cards as random.

Do things change if we account for other player behavior? For example, if a player knows you're going for a set of 3s they may be less likely to discard a 3 for you to pick up.

## Results with anatagonistic discard pile

To test this, we build an anatagonistic discard pile, essentially, we never let the discard pile show a card we've taken in the past. This would model the person next to you playing perfectly (a tough sell after a day of eating and drinking). With this constraint:

|Policy |Num turns (median)  | Num turns (average)  |
|-------|-----------|-----------|
Greedy pairs                |11     |11.7   |
Greedy 5 of a kind after 4  |11     |11.9   |
Greedy 5 of a kind after 3  |11     |12.3   |
Hide until 4 of a kind      |14     |15.1   |
Hide until 3 of a kind      |14     |14.7   |

As expected, there isn't much impact on the `hide until n` strategies -- the whole point is to hide what cards you're going after.

The `greedy pairs` strategy is negatviely impacted. But it's still the best strategy. And it's the simplest, if you have the discard card in your hand, take it. Turns out I was way over thinking this.

## Possible future ideas

Notes in case I want to revisit this:

* Better understand why certain strategies outperform, for example look at a graph of close-ness to goal over time. Does anything stand out between the strategies
* Evaluate distributions of results, anything unexpected?
* Improve performance of the simulation

\[0\] There is also the policy for discarding cards. For simplicity, this policy is the same for all take-policies: discard the card with the lowest number of sets in the hand.
