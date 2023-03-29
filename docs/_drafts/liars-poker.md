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

[*] Swtich from strings to integers for keys, can still use strings for debugging -- can use special hashmap for this
[*] Re-tune database to work dynamically, when larger than a certain size, split key in half? and go from there?
    *Probably easier to buidl a tool to try a bunch of different cutoffs and see which ones work reasonably well
    * Then can manually tune
    *ADQSTSJSAS|9C|PPPPPPPS|3S|TSTC9SJC|ACQCKCJS|THKHQS9H|QHAHASJH|KDTDAD9D
    * 011011101001001110100001011000001111001001001011
    *01100100010101100000010111000000111100100100101100111
[ ] Move IO to a separate thread
    *Probably want to do with raw threads and maybe use rayon for computation?
    *<https://doc.rust-lang.org/book/ch16-02-message-passing.html>
[ ] Train a single deal repeatedly, seed 1? See if converges to good play?
    *May need to add way to judge performance, see thesis on possible metrics
    *results going into /tmp/seed_0
    *See Thesis, pg 139, algorithm 8
    * <http://mlanctot.info/>
[ ] Imlement other CFR algorithms, see Marc's phd and website for implementations

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

Running for 87M rounds of CFR resulted in 10M nodes stored in the database in 5.5GB. Would need ~175M rounds of the CFR for a single game. About 11 GB for nodes from a single game. Reasonable for a single hand, but would struggle to get reasonable coverage of possible deals.

## Exploring options to reduce computation

Using CFR, we only need to store each information state.

* Deal: 134k hands (24 choose 6, hand + 1 revealed card)
* Pick / pass: 5 states (everyone passes or one of each pickup)
* Suit choice: 12 states, each of 4 players could choose 3 suits (can't choose flipped suit)
* Hands (see above): 207M states

If only looking at a single game: 12.4b states -- at a byte per gamestate, this is 12TB of data

Still likely too many states to store -- could we collapse the representation of the states?

58 min for 258M states

## Disk performance testing

### sqlite multithreaded writing for data

WAL testing:
No WAL
    2023-02-07T15:44:26-07:00 - INFO - Starting self play for CFR
    2023-02-07T15:46:11-07:00 - DEBUG - cfr called 60000000 times   (0:01:45)

W/ WAL
    2023-02-07T15:47:52-07:00 - INFO - Starting self play for CFR
    2023-02-07T15:49:54-07:00 - DEBUG - cfr called 60000000 times   (0:02:02)

No index, w/ WAL
    2023-02-07T15:53:40-07:00 - INFO - Starting self play for CFR
    2023-02-07T15:55:04-07:00 - DEBUG - cfr called 60000000 times   (0:01:24)
    2023-02-07T15:57:26-07:00 - DEBUG - cfr called 100000000 times  (0:03:46)
    2023-02-07T16:01:33-07:00 - DEBUG - cfr called 200000000 times  (0:07:53)

No index, Journal mode off, synchronous Normal
    2023-02-07T18:29:25-07:00 - INFO - Starting self play for CFR
    2023-02-07T18:31:05-07:00 - DEBUG - cfr called 60000000 times   (0:01:40)
    2023-02-07T18:33:32-07:00 - DEBUG - cfr called 100000000 times  (0:04:07)

    Failed with a "database is locked"

Only getting 20M/s from iotop -- can we do better with raw writing performance

### no_op disk backed

Doesn't do any disk IO as a point of reference
    2023-02-08T10:23:59-07:00 - INFO - Starting self play for CFR
    2023-02-08T10:24:46-07:00 - DEBUG - cfr called 60000000 times (0:00:47)
    2023-02-08T10:25:30-07:00 - DEBUG - cfr called 100000000 times (0:01:31)
    2023-02-08T10:27:29-07:00 - DEBUG - cfr called 200000000 times (0:03:30)
    2023-02-08T10:37:05-07:00 - DEBUG - cfr called 713000000 times (0:13:06)

### io_uring

Built proof of concept with tokio io_uring.
to write 1M nodes:

* sqlite: 8.1325s (15-20MB/s)
* io_uring (4kb page): 2.9714s (40-60MB/s)

2.7x faster than raw writing to SQL. But this doesn't include any of the book keeping overhead sqlite incurs
But verified with perf that serialization overhead is minimal

With a 4k offset, what about 64k?
    * io_uring (64kb page): 0.67s (100-200MB/s)
    12x faster than sqlite

Dual buffer -- no noticable change, tokio is properly parallelizing

## Allocation reduction

liars_poker_bot::cfragent: 2023-03-08T16:18:55-07:00 - INFO - Starting self play for CFR
liars_poker_bot::cfragent: 2023-03-08T16:19:59-07:00 - DEBUG - cfr called 60000000 times
liars_poker_bot::cfragent: 2023-03-08T16:21:01-07:00 - DEBUG - cfr called 100000000 times

## With IStateKey and synchronous IO

liars_poker_bot::cfragent: 2023-03-29T12:18:46-05:00 - INFO - Starting self play for CFR
liars_poker_bot::cfragent: 2023-03-29T12:20:14-05:00 - DEBUG - cfr called 60000000 times
