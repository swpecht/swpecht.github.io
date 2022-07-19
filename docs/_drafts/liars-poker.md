---
layout: post
title:  "Exploiting expecation maximization in imperfect information games"
categories: project-log
---

## Context

For perfect information games, can use expectation maximization to build a bot

Supposedly you can't do that with imperfect information games.

Going to use Liar's poker bots to illustrate why.

## Results

[x] Need to switch the minimax agent to use the game tree implementation.
    Easier to debug and inspect and can work on optimizations later
[ ] Implement scoring and pruning as part of the tree building process
[X] Make minimax algo work from P2 position
[ ] Implement bot that can exploit pure minimax
    * Tried with Incorporate bet agent -- where it assumes the other players bets are never bluffs. But it's about equal to slightly worse than minimax
[ ] Implement counter factual regret minimization


Results as of July 18 for 10k games:
Random wins: 5039,  Random wins: 4961
Random wins: 1631,  Minimax wins: 8369
Random wins: 4052,  OwnDiceAgent wins: 5948
Random wins: 1753,  IncorporateBetAgent wins: 8247
Minimax wins: 8519,  Random wins: 1481
Minimax wins: 4944,  Minimax wins: 5056
Minimax wins: 7416,  OwnDiceAgent wins: 2584
Minimax wins: 5018,  IncorporateBetAgent wins: 4982
OwnDiceAgent wins: 6414,  Random wins: 3586
OwnDiceAgent wins: 2447,  Minimax wins: 7553
OwnDiceAgent wins: 3738,  OwnDiceAgent wins: 6262
OwnDiceAgent wins: 0,  IncorporateBetAgent wins: 10000
IncorporateBetAgent wins: 8498,  Random wins: 1502
IncorporateBetAgent wins: 5014,  Minimax wins: 4986
IncorporateBetAgent wins: 10000,  OwnDiceAgent wins: 0
IncorporateBetAgent wins: 5706,  IncorporateBetAgent wins: 4294