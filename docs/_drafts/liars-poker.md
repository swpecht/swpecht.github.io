
---
layout: post
title:  "Exploiting expecation maximization in imperfect information games"
categories: project-log
---

## Context

For perfect information games, can use expectation maximization to build a bot

Supposedly you can't do that with imperfect information games.

Going to use Liar's poker bots to illustrate why.

## Next email for Marc
* Interesting challenge on applying the search algorithms to euchre -- because there are hidden actions, resampling is non-trivial. What's the right policy to use for the hidden actions? -- somewhat made easier by only 1 player in euchre can take a hidden action

## TODOs

Improving exploitability and CFR
[ ] Do we actually need to pre-compute everything for tabular exploitability? Instead can we 'search' and compute on the fly?
[ ] Look at alternative sampling algorithms for CFR -- might change the read-write characteristics -- external sampling?
    * Marc mentioned this dominates chance sampling across all dimensions
[ ] What people are doing these days for approximate best response is running "DQN-BR" (use reinforcement learning as an exploiter). See, for example, the MMD paper: https://arxiv.org/pdf/2206.05825.pdf. There's an example of this in OpenSpiel:  https://github.com/deepmind/open_spiel/blob/master/open_spiel/python/examples/rl_response.py. This idea is the basis of PSRO, btw, which is a paper which may interest you: https://arxiv.org/abs/1711.00832. Anyway, there are even more sophisticated methods that add MCTS search on top of it (see our ABR paper: https://arxiv.org/abs/2004.09677) but it is a bit heavy and compute-hungry so I'd recommend starting with DQN-BR.


Evaluating other algorithms and games
[ ] Implement strategy for up the river down the river -- could constrain to the lower number of cards in hand to see what is optimal
    * https://plentifun.com/rules-to-play-up-down-river-card-game
    * re-read marc's email (from my google?) on if cfr worksw ith multiple players -- e.g. could it work for up the river down the river? with 4 players?
    * Does the exploitanility function work in that instance? -- good test bet for agents
[ ] Implement double dummy solver (open hand solver)
[ ] Extend alphamu to scoring and not just win vs loss metrics -- could see if this improve performance of the agent


Misc ideas
[ ] Multithread for rollout?
[ ] Implement other rollout methods
[ ] For benchmarking -- create a policy agent. Takes policy as an input -- might be an easier way to have all the agents play


## Tricky things about euchre

* Resampling isn't trivial -- what's the right way to handle discards -- what policy should be used?

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
| Phase   | number of options | Notes                            |
| ------- | ----------------- | -------------------------------- |
| Pick-up | 4                 | Each player can pickup or skip   |
| Call    | 81                | each player has 3 options, $3^4$ |
| Round 1 | 625               | 5 cards across 4 players, $5^4$  |
| Round 2 | 256               | $4^4$                            |
| Round 3 | 81                | $3^4$                            |
| Round 4 | 16                | $2^4$                            |
| Round 5 | 1                 | $1^4$                            |

From playing rounds alone, there are 207M options. Including pre-play rounds there are 67 billion options.


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
liars_poker_bot::cfragent: 2023-03-29T12:20:14-05:00 - DEBUG - cfr called 60000000 times (0:01:28 )

## Use hashtables to lookup pages

liars_poker_bot::cfragent: 2023-04-04T11:36:05-04:00 - INFO - Starting self play for CFR
liars_poker_bot::cfragent: 2023-04-04T11:36:41-04:00 - DEBUG - cfr called 60000000 times (0:00:36)

## Switch to using IState key for Istates used during storage and page mgmt rather than Strings 

liars_poker_bot::cfragent: 2023-04-06T14:55:06+02:00 - INFO - Starting self play for CFR
liars_poker_bot::cfragent: 2023-04-06T14:55:55+02:00 - DEBUG - cfr called 60000000 times

## Storing the first bit of IStateKey for easy retrieval

liars_poker_bot::cfragent: 2023-04-06T15:07:18+02:00 - INFO - Starting self play for CFR
liars_poker_bot::cfragent: 2023-04-06T15:07:41+02:00 - DEBUG - cfr called 60000000 times (0:00:23)



