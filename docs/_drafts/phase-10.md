---
layout: post
title:  "How to play Phase 10"
date:   2021-11-29 00:00:00 +0000
categories: project-log
---

## Context

Phase 10 is a rummy-like cardgame where the player needs to accomplish various goals (or phases) to progress. For example, the player may need to get 2 sets of 3 cards or a 7 card straight. Wikipedia gives a good overview of [Phase 10 rules](https://en.wikipedia.org/wiki/Phase_10).

## Problem

While playing over Thanksgiving break, I was unsure on the best approach to accomplishing phases 9 and 10 of the game. These phases require the player to get 1 set of 5 and 1 set of 2 (or 3). Specifically, I was trying to decide between:

1. Greedily taking cards from the discard pile to start pairs, hurting my near term chances of getting to the set of 5, but giving me more options in the future.
2. Only taking cards from the discard pile when they helped me get to sets of 3 or 4 and instead taking my chances with the draw pile.

To simplify things, I ignore the actions of other players. The top card on the discard pile is the one discarded by the player to your right. For this post I assume that the top discard pile card is random. This could have some implications on the final result as picking a card from the discard pile reveals information to other players. But the game isn't that competetive and it makes things much easier.

What is the best policy for determining when to take the face up card or to draw a new card? Where best lets us complete Phase 9 and Phase 10 in the smallest number of turns.

## Method

I built a simulation for Phase 10 in Rust ([code on Github](https://github.com/swpecht/swpecht.github.io/tree/master/projects/phase-10)). I chose Rust because I wanted to learn it.

The main area of interest in the 'take-policy' -- the set or rules that determine when to take the face-up discarded card or to draw an unknown card\[0\].

For each take-policy, I run 100k simulations and compare the average and the distribution of turns to win. See below for an overview of the policies.

### Baseline policy

Rules that apply no matter what we're doing:

* Always take wild cards
* Always draw if a skip card is on the discard pile

### Greedy pairs (1 from above)

If taking the discard card gives us a pair or better, take it.

```rust
fn take_if_pair(hand: &Vec<Card>, candidate_card: Card) -> Action {
    match hand.contains(&candidate_card) || candidate_card == Card::Wild {
        true => Action::Take,
        _ => Action::Draw,
    }
}
```

### Greedy 5 of a kind after N (2 from above)

Play the same as `greedy pairs` until we have a set of N cards. After that, we greedily try to complete the set of 5, so we only take from the discard pile if it moves us towards the set. Otherwise, we draw a card.

```rust
fn take_if_no_n_of_kind(hand: &Vec<Card>, candidate_card: Card) -> Action {
    let counts = get_counts(&hand);
    let (mcard, mcount) = counts[counts.len() - 1]; // end of list has highest count

    if mcount < 3 {
        return take_if_pair(hand, candidate_card);
    }

    // If it's part of the max set, take it
    return match candidate_card {
        x if x == mcard => Action::Take,
        _ => Action::Draw,
    };
}
```

## Results

|Policy |Num turns  | Sim time  |
|-------|-----------|-----------|
Greedy pairs |10.1   |39s|
Greedy 5 of a kind after 4|10.2  |56s    |
Greedy 5 of kind after 3|11.4  |60s    |

[todo: insert graph of close-ness to goal over time, to show why this works]


[evaluate distributions of results, does one have a higher variance?]


[how often do I pick up more than 1 of the same card -- can tell us how wrong I are about ignoring other players]


## Performance improvements

\[0\] There is also the policy for discarding cards. For simplicity, this policy is the same for all take-policies: discard the card with the lowest number of sets in the hand.
