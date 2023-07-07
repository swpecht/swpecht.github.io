---
layout: post
title:  "αμ Search Algorithm for Euchre"
categories: project-log
---

# Todos

[ ] Evaluate alpha mu on card play
    * If does better than PIMCTS -- then likely have implemented prolery and it's struggling with the imbalance in invalid worlds
    * If that's the case, can look at doing a combination CFR and rollout evaluation to try and ease the tranining -- what's the state space if we just need to do CFR on the bidding phase?
    * Looks like AlphaMu may do better in card play scenarios -- need more games to verify this
    [ ] Add multithreading to alpha mu to help with larger world counts
[ ] Add LRU support to the open hand solver

[ ] Figure out what alpha mu is doing using the compare helper

[*] Change mode to be the main parmeter for main, and game to be a named one. Can add an exploitability calc mode, where does exploitabiltiy for a bunch of agents and games


[ ] Make sure alpha mu us correct
    [ ] Re-check the logic for how fronts and vectors work compared to paper, with invalid worlds, starting to get errors
        * Re-read the paper, better understand what the vectors and fronts represent, may be able to figure out proper behavior
    [ ] Check the front <= code
    [ ] Why is alphamu doing worse at higher m counts?
    [ ] Make sure that actually swapping players
    [ ] Alpha mu is saying to take for the following gamestate: 9cAcJs9h9d|TcQcTsQsKs|JhKhJdQdKd|JcKc9sQhAh|Ad|P: baseline: P, test: T
        * Why is this?
[ ] Optimize alphamu
    [ ] Implement iterative deepening with transposition table, including alpha and mu cuts
        [*] Add useless world cuts -- might make the fronts smaller at min nodes? -- probably most important since need to reduce vectors
        [*] Evaluate if the hashmap storage is still needed for fronts after useless world optimization -- removed for simplicity
        [*] Parallelize getting the min of two vectors -- should be able to get a mask for a >= b -- set all values with 1s for the new one from a, and the inverse it and set from b -- should give the proper min
        [ ] Look at profiling for benchmark runs (perf record -F 99 --call-graph dwarf,16384 ./target/release/liars_poker_bot -v3 -n 10  benchmark) for optmimizations
        [ ] Add other optimizations, like deep cut comparison
        [ ] Problem: Min is being called and creating 21k sized fronts, is this correct? It's why taking so long to compare everything
        [ ] Should we add everything and then prune? Will that be fewer comparisons since we can prune?
            [ ] Can we parallelize this with rayon or gpu processing?
    [ ] Implement min function to be accelerated with GPU -- https://github.com/gfx-rs/wgpu/tree/trunk/examples/hello-compute
[*] Optimize mtd -- have player 0 always be the one to go for isomorphic deck representation
    * Didn't actually improve performance


# Content

This post show's results for my implementation of [The αμ Search Algorithm for the Game of Bridge](https://arxiv.org/abs/1911.07960) but applied to Euchre. The goal of this implementation is to solve the strategy fusion problem of evaluating euchre hands ([Euchre wisdom: pass on the bower, lose for an hour?](/project-log/2023/05/30/pass-on-the-bower-lose-for-an-hour.html)). To solve the strategy fusion problem at a high level, AlphaMu keeps a set of vectors of outcomes for each possible information state. It then chooses the single action with highest score across all possible worlds. For more details on how AlphaMu works, see the original paper: (https://arxiv.org/abs/1911.07960).

As a caveat, I was not able to find an open source implementation of AlphaMu. There could be a bug in my implementation hurting AlphaMu's performance.

# AlphaMu and improvements

I've implemented [The αμ Search Algorithm for the Game of Bridge](https://arxiv.org/abs/1911.07960) and the optmimizations from [Optimizing αμ](https://arxiv.org/abs/2101.12639). But I've made the following changes:
* Focused on performance during the bidding phase of the game rather than strictly card play
* Added support for more granular scoring rather than just wins or losses

From the AlphaMu paper: "In our experiments we fix the bids so as to concentrate on the evaluation of the card play". Since we are most interested in the bidding phase of Euchre, we evaluate both bidding and card play. Although, results for just card play evaluation can be found in the appendix (TODO).

To support the bidding phase, it is not sufficient for AlphaMu to only consider wins or losses. The downside to choosing trump in euchre is the two points your opponents can get if you don't take the majority of tricks. AlphaMu cannot properly judge this risk if it isn't aware of the score implications.


# Tuning AlphaMu

AlphaMu is parameterized on two variables: `worlds` and `m`. `worlds` is the number of worlds to use for evaluation -- this is the same as PIMCTs. `m` is the search depth for AlphaMu. More specifically, it is the number of max nodes (turns for the agent's team) to evaluate. Once AlphaMu has reached the targeted search depth, it uses an Openhand solver to play evaluate each game state. When `m=1` AlphaMu is equivalent to PIMCTs. Where applicable, each run is evaluated on an identical set of games with identical random seeds for AlphaMu.

**Average hand score when AlphaMu is dealer with a jack dealt for 1000 games**

|        | m                                 |
| worlds | 1         | 5         | 10        | 20        |
| ------ | --------- | --------- | --------- | --------- |
| 8      | ████ 0.38 | ███▌ 0.34 | ███ 0.27  | ███ 0.33  |
| 16     | ███▌ 0.35 | ████ 0.43 | ████ 0.41 | ████ 0.43 |
| 32     | ████ 0.37 | ████ 0.42 | ███▌ 0.36 | ███▌ 0.34 |



# Head to head performance for euchre

All agents play the same games, they are the all pass on the bower gamestates, defended by PIMCTS, 20 worlds

Running `target/release/liars_poker_bot --mode pass-on-bower-alpha -n 500 -v2`

liars_poker_bot::scripts::pass_on_bower_alpha: 2023-06-11T11:50:11+02:00 - INFO - starting benchmark, defended by: PIMCTS, n=100
liars_poker_bot::scripts::pass_on_bower_alpha: 2023-06-11T11:50:55+02:00 - INFO - "Random agent"	-0.6
liars_poker_bot::scripts::pass_on_bower_alpha: 2023-06-11T11:51:49+02:00 - INFO - "pimcts, 10 worlds, random"	0.394
liars_poker_bot::scripts::pass_on_bower_alpha: 2023-06-11T11:52:34+02:00 - INFO - "pimcts, 10 worlds, open hand"	0.474
liars_poker_bot::scripts::pass_on_bower_alpha: 2023-06-11T11:53:30+02:00 - INFO - "pimcts, 100 worlds, open hand"	0.598
liars_poker_bot::scripts::pass_on_bower_alpha: 2023-06-11T11:54:27+02:00 - INFO - "alphamu, open hand, m=1, 10 worlds"	0.474
liars_poker_bot::scripts::pass_on_bower_alpha: 2023-06-11T11:57:04+02:00 - INFO - "alphamu, open hand"	0.596


Card play: 
Running `target/release/liars_poker_bot -v1 benchmark card-play -n 100`
"alphamu, 32 worlds, m=20"      "pimcts, 32 worlds hand"        0.46
"pimcts, 32 worlds hand"        "alphamu, 32 worlds, m=20"      0.45



# Next post

Where things differ on pass on bower calls

## Appendix

# head to head performance for euchre card play
