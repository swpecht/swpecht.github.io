---
layout: post
title:  "Exploiting expecation maximization in imperfect information games"
categories: project-log
---

## Context

For perfect information games, can use expectation maximization to build a bot

Supposedly you can't do that with imperfect information games.

Going to use Liar's poker bots to illustrate why.

## TODOs

[*] Implement Kuhn poker
    *<https://github.com/deepmind/open_spiel/blob/master/open_spiel/spiel.h> -- see how gamestate managed by deepmind

[*] Implement CFR for Kuhn poker
    *How to figure out the policy from the weights? What's the right way to update things for convergence?
    * Do we actually just need to implement CFR?
    *Implement a full CFR bot: <https://towardsdatascience.com/counterfactual-regret-minimization-ff4204bf4205>
    * Implement an MCCFR bot: <https://xyzml.medium.com/learn-ai-game-playing-algorithm-part-iii-counterfactual-regret-minimization-b182a7ec85fb#6cf7>
[*] Fix the problem with types so that can test multiple games and agents at the same time
[*] Attempt to use the CFR implementation for euchre
[*] Implement sqlite caching to reduce memory usage, and see the cached reference below for perf improvements
    *Need to implement a get_policy version of the sql reading
[*] Implement benchmarks for kuhn poker CRF training?
    *Currently getting killed for too much memory usage
    *Add wrapper functions and then look to use SQLite as the backend to materialize to disk?
    * <https://docs.rs/cached/latest/cached/> for the cache on the function?
[*] Implement performance improvements
    [*] Batch writing of nodes
    [*] Batch reading of nodes
    [*] Implement summary stats for how often pages are added and removed -- can see if need new caching algorithm
    [ ] Benchmark -- see if too many pages are slowing down execution -- most time is spent in page contains
        *About 1k pages total -- need to reduce the time for this
    [*] Implement shortcut if only 1 viable move
    [ ] Switch to heap allocations: https://nnethercote.github.io/perf-book/heap-allocations.html?highlight=borrow
    [ ] Could get all 27M states to fit into memory?
    [ ] Implement multi sized pages, e.g. shorter depth at start, and get longer, to try and better balance out the page sizes (fewer options at the end)
        * Need to do this for the root page
    [ ] Reduce memory footprint for each node -- can we get all 27M into memory?
        *Cannot reduce the action space easily
        * Could protentially revmoe CFRNode::Strategy and just recompute each time it is needed
    [ ] Avoid storing some nodes
        * if only a single action
        * ???
[ ] Add estimates for how many nodes remain
    [ ] Figure out how many states we'd actually need to cover. Is there a way to collapse states? See notes below
[*] build support for named database
[ ] Switch to non-recursive CFR for better debugability? Or switch straight to MCCFR?
[ ] Implement MCCFR for kuhn poker

<https://arxiv.org/pdf/2206.15378.pdf>

## Design

* score leaf node
* propogate score

Algorithm | score_leaf_node | propogate_score |
|---------|-----------------|-----------------|
| Random    | random    | minimax |
| Minimax   | rollout expected value    | minimax   |
| CFR   | CFR / regret matching | ???? Minimax? |
| Owndice agent |

## Results

Results as of July 19 for 1k games:
Random wins: 475,  Random wins: 525
Random wins: 154,  MinimaxAgent wins: 846
Random wins: 166,  MetaMiniMaxAgent wins: 834
Random wins: 426,  OwnDiceAgent wins: 574
Random wins: 171,  IncorporateBetAgent wins: 829
MinimaxAgent wins: 852,  Random wins: 148
MinimaxAgent wins: 491,  MinimaxAgent wins: 509
MinimaxAgent wins: 502,  MetaMiniMaxAgent wins: 498
MinimaxAgent wins: 741,  OwnDiceAgent wins: 259
MinimaxAgent wins: 529,  IncorporateBetAgent wins: 471
MetaMiniMaxAgent wins: 806,  Random wins: 194
MetaMiniMaxAgent wins: 507,  MinimaxAgent wins: 493
MetaMiniMaxAgent wins: 513,  MetaMiniMaxAgent wins: 487
MetaMiniMaxAgent wins: 754,  OwnDiceAgent wins: 246
MetaMiniMaxAgent wins: 511,  IncorporateBetAgent wins: 489
OwnDiceAgent wins: 630,  Random wins: 370
OwnDiceAgent wins: 256,  MinimaxAgent wins: 744
OwnDiceAgent wins: 263,  MetaMiniMaxAgent wins: 737
OwnDiceAgent wins: 396,  OwnDiceAgent wins: 604
OwnDiceAgent wins: 0,  IncorporateBetAgent wins: 1000
IncorporateBetAgent wins: 844,  Random wins: 156
IncorporateBetAgent wins: 511,  MinimaxAgent wins: 489
IncorporateBetAgent wins: 513,  MetaMiniMaxAgent wins: 487
IncorporateBetAgent wins: 1000,  OwnDiceAgent wins: 0
IncorporateBetAgent wins: 558,  IncorporateBetAgent wins: 442

Seems like a key difference between Minimax and CFR -- minimax assumes that each hidden state is equally likely. This works well for things like dice, but doesn't work as well when the hidden state is influenced by the player -- what's hidden depends on their strategy.

Random agent: Baseline
Always first agent: Dominates the always random agent
CFR agent (at equilibrium): has an expected value of 0 against the other agents

## Estimating Euchre game tree

### Starting states

There are 600 million possible states given a a deal of euchre.
24 cards in a deck, with 6 revealed (5 in hand + 1 shown to dealer)

    `18 choose 5 * 14 choose 5 * 8 choose 5 = 618m`

### Estimating the size of the game tree

For the first layer of the gametree, the minimum memory required is:

    `600m * 24 cards * 2 bits / 8 bits per byte / 1000 kB per byte / 1000 MB per kB = 3,600 MB`
The 2 bits represent the 4 locations that unknowns cards can be in: 3 hands + discard
Each of these possible states could play out in a variety of ways.
| Phase     | number of options | Notes |
|-------    |-------------------|-------|
| Pick-up   | 4                 | Each player can pickup or skip      |
| Call      |   81              | each player has 3 options, $3^4$|
| Round 1   | 625               | 5 cards across 4 players, $5^4$   |
| Round 2   | 256               | $4^4$ |
| Round 3   | 81                | $3^4$ |
| Round 4   | 16                | $2^4$ |
| Round 5   | 1                 | $1^4$ |

From playing rounds alone, there are 207M options. Including pre-play rounds there are 67 billion options.

### Combined

Taking these two together leads to 41 trillion game state leaf nodes: $O(10^12)$ nodes. Even at 1 bit per gamestate this would be 125 GB of data for the leaf nodes alone.

It is not possible for us to evaluate the game in this way.

## Verifying on actual game rules

run with `--mode analyze` to see summary stats about games

For a given deal, ~27M possible rollouts. One order of magnitude less than the naive approach.

How many total game nodes is this? -- need to count across the tree
~175M nodes --many to keep in memory?

22 total rounds

Less nodes, since can ignore last layer and layer before

This is possible with the current set up I have so far. But there are 16 quadrillion deal states:
    `26 choose 5 * 21 choose 5 * 16 choose 5 * 11 choose 5 * 6 choose 1`
    `65780 * 20349 * 4368 * 462 * 6`
    `1.6*10^16 (16 quadrillion)`
This is far too many to evaluate repeatedly (necessary to get convergence using CFR).

How can this be reduced? e.g. with a better representation? Symmetry along the suits?

Can we break the problem apart into before and after trump is called? And then simplify the gamestate?

## Exploring options to reduce computation

Using CFR, we only need to store each information state.

* Deal: 134k hands (24 choose 6, hand + 1 revealed card)
* Pick / pass: 5 states (everyone passes or one of each pickup)
* Suit choice: 12 states, each of 4 players could choose 3 suits (can't choose flipped suit)
* Hands (see above): 207M states

If only looking at a single game: 12.4b states -- at a byte per gamestate, this is 12TB of data

Still likely too many states to store -- could we collapse the representation of the states?
