---
layout: post
title: "Perfect information monte carlo tree search (PIMCTS) for euchre"
categories: project-log
---


# Context


In my previous post [Euchre wisdom: pass on the bower, lose for an hour?](/project-log/2023/05/30/pass-on-the-bower-lose-for-an-hour), I used perfect information monte carlo tree search (PIMCTS) to evaluate euchre hands. This post outlines the optimizations from evaluating 3 hands/second to 44/s.




Much of this work is adapted from [Bo Haglund's double dummy solver for bridge](http://privat.bahnhof.se/wb758135/).




PIMCTS evaluates several euchre hands, assuming that each player can see the other's cards and play optimally. It then suggests the move with the highest chance of winning across all games evaluated.


For example, imagine we are dealt the following hand: `Qc9sTs9dAd|Qs` where `Qc9sTs9dAd` are our cards and `Qs` is the face-up card. This is our information state- all of the information we know about the game. We don’t know the actual game state, i.e., the cards the other players are dealt. To overcome this, we generate several games that have the same information state and solve each of those games independently, such as:


```
Qc9sTs9dAd|9cKsThQhTd|KcAsJhKhQd|AcJs9hAhJd|Qs
Qc9sTs9dAd|JcKcKsAsKd|9cTc9hJhKh|JsThQhTdJd|Qs
Qc9sTs9dAd|9cJcAc9hKh|KsAsQhAhJd|TcKcJsQdKd|Qs
Qc9sTs9dAd|KcAcJsAhQd|JcKsQhTdJd|9c9hThJhKd|Qs
Qc9sTs9dAd|9cTcThJhTd|JcKcKsAs9h|JsQhAhJdKd|Qs
^           ^ Other player's hands vary
Our hand is always the same
```


In Rust code, it would look something like this:


```rust
/// Return the expected score of the passed game state for the `maximizing_player`
fn evaluate_player(&mut self, gs: &EuchreGameState, maximizing_player: Player) -> f64 {
let num_rollouts = 50; // number of generated worlds to evaluate
let mut worlds = Vec::with_capacity(n);
for _ in 0..n {
worlds.push(gs.resample_from_istate(gs.cur_player(), rng));
}
// This could be any solver. Here we use one that can see all cards in the
// generated worlds and assumes all players play optimally
let solver = OpenHandSolver::new();
let sum: f64 = worlds
.into_par_iter() // parallelize the search across worlds with Rayon
.map(|w|
solver.clone().mtd_search(gs.clone(), maximizing_player))
.sum();


sum / num_rollouts as f64
}
```


The optimizations are focused on improving the `mtd_search` function. See below for an overview of the optimizations.


**Information states evaluated per second, for 50 game PIMCTS, 2000 samples**
```
1) Vanilla MTD              |█▌3
2) Transposition table      | ██ +4
3) Isometric representation | ███████████████ +31
4) Euchre-specific rules    | ███ +7
Optimized                   |██████████████████████ 44 (22ms per game)
```


All benchmarks were done on a ThinkPad laptop with a 10th gen Core i7 Pro and 16GB of RAM.


# 1) Vanilla MTD


I use the MTD search algorithm from [Aske Plaat's post](http://people.csail.mit.edu/plaat/mtdf.html).


In pseudocode:
```
function MTDF(root : node_type; f : integer; d : integer) : integer;
    g := f;
    upperbound := +INFINITY;
    lowerbound := -INFINITY;
    repeat
        if g == lowerbound then beta := g + 1 else beta := g;
        g := AlphaBetaWithMemory(root, beta - 1, beta, d);
        if g < beta then upperbound := g else lowerbound := g;
    until lowerbound >= upperbound;
    return g;
```


To start, our `AlphaBetaWithMemory` has no memory, and we must fully re-run the search each time. We max out at evaluating 3 games / s.


# 2) Transposition table


The first optimization is to give some memory to the `AlphaBetaWithMemory` function. We use the [dashmap crate](https://docs.rs/dashmap/latest/dashmap/) as a performant, thread-safe hashmap for this.


Initially, we use the full game state as the key when storing and retrieving values, for example, `Qc9sTs9dAd|9cKsThQhTd|KcAsJhKhQd|AcJs9hAhJd|Qs`.


Because a zero-window alpha-beta search (what we use with MTD) returns bounds for each search and not exact values, we store the lower bound, upper bound, and the best action in each entry. For more details on how this works, see [Aske's post](http://people.csail.mit.edu/plaat/mtdf.html#abmem).


For memory reasons, we only store results during the bidding phase and at the start of new tricks for euchre. For example:


```
Qc9sTs9dAd|9cKsThQhTd|KcAsJhKhQd|AcJs9hAhJd|Qs|PT: stored, we're still in the bidding phase
Qc9sTs9dAd|9cKsThQhTd|KcAsJhKhQd|AcJs9hAhJd|Qs|PT|AdTdQdJd: stored, we're in the play phase and at the start of a new trick
Qc9sTs9dAd|9cKsThQhTd|KcAsJhKhQd|AcJs9hAhJd|Qs|PT|AdTdQd: not stored, we're in the middle of a trick.
```


Adding in the transposition table means we don't need to recalculate everything on future searches to `AlphaBetaWithMemory` for the same game state. We can evaluate 7 games / s — more than twice as many as without the transposition table.


# 3) Isometric representation


While we can now re-use results between calls to `AlphaBetaWithMemory` for the same game state, we don't get any benefits from similar game states. We need an isometric representation for game states where those with the same value look the same.


For example, imagine we stored the cards in a table, where each entry is the player holding the card. If the entry is empty, the card is out of play, i.e., it has been played or was never dealt. `JL` is the left jack, e.g., `Jc` when spades are trump. And `JR` is the right jack. An x means the card isn't valid, e.g., the `Jc` becomes `JL.`


| Suit           | 9   | 10  | J   | Q   | K   | A   | JL  | JR  |
| -------------- | --- | --- | --- | --- | --- | --- | --- | --- |
| Clubs          | 2   | 1   | x   |     | 2   |     | x   | x   |
| Spades (trump) | 1   |     | x   |     | 3   | 4   |     |     |
| Hearts         |     |     | 3   |     |     | 4   | x   |     |
| Diamonds       |     |     |     |     |     |     |     | x   |


This would correspond to the following hands:
* Player 1: 9s 10c
* Player 2: 9c Kc
* Player 3: Jh Ks
* Player 4: As Ah


In euchre, the rank of a card only matters for evaluating the card against other cards of the same suit. For example, if spades is trump, a `9s` beats an `Ah` "as much" as a `Js` does. And if the lead card is `9d`, no heart card will ever be able to beat it [0].


Knowing this, we can shift all of the cards in our table without changing the relative value of the cards, e.g., Player 3's `Ks` has the same values as the `10s` would -- it's the second highest spade. With this change, the new table looks like this:




| Suit           | 9   | 10  | J   | Q   | K   | A   | JL  | JR  |
| -------------- | --- | --- | --- | --- | --- | --- | --- | --- |
| Clubs          | 2   | 1   | x   | 2   |     |     | x   | x   |
| Spades (trump) | 1   | 3   | x   | 4   |     |     |     |     |
| Hearts         | 3   | 4   |     |     |     |     | x   |     |
| Diamonds       |     |     |     |     |     |     |     | x   |


Now we can match our cards against a much wider set of hands. We're only storing the relative value of the cards rather than the cards themselves.


Next, we know suits cause some symmetry. For example, an `Ah` when spades are trump is the highest possible heart in the same way the `As` when hearts are trump is the highest. We can change the order of the rows in our table to make it indifferent to the card's suit. Specifically, we store trump in the first row, then sort the rows by the number of cards. Our table becomes:


| Suit           | 9   | 10  | J   | Q   | K   | A   | JL  | JR  |
| -------------- | --- | --- | --- | --- | --- | --- | --- | --- |
| Spades (trump) | 1   | 3   | x   | 4   |     |     |     |     |
| Clubs          | 2   | 1   | x   | 2   |     |     | x   | x   |
| Hearts         | 3   | 4   |     |     |     |     | x   |     |
| Diamonds       |     |     |     |     |     |     |     | x   |


With this change, we're agnostic to the suit and specific value of cards, only caring about their relative ordering.


If we store as a hash of this table the current score of tricks, the calling team for the current trick, and the current player, we can get many more hits in our transposition table -- allowing us to avoid calculating the score for many game states.


With this change, we can evaluate 38 games / s — 5.4x with the naive transposition table.


# 4) Euchre-specific rules


The final optimization is to add some euchre-specific rules on when games are over and what moves to evaluate.


For determining if a game is over, we know that if a player has the highest trump card, they are guaranteed to get at least one more trick, similarly for the second highest trump card, etc. We can use this fact to determine if games are over early and stop evaluation.


For choosing what moves to evaluate, we do two optimizations. First, we remove equivalent cards from our hand. Similarly, the `9c` is the same as the `10c` if a player holds both. We only need to evaluate one of those plays. It is also usually beneficial to play the highest trump card to start a new trick if you have it. So we evaluate that move first.


With these changes, we can evaluate up to 44 games / s — Sa 16% improvement.


# Conclusion


Being able to evaluate 44 games / s will significantly reduce the amount of waiting I need to do when evaluating more advanced euchre bots. I was also surprised to see how large of an impact the isometric representation had, especially compared to the naive transposition table.




[0] Some care needs to be taken during the bidding phase because the relative value of jacks has yet to be established.

