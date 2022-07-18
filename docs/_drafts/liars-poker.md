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
[ ] Implement counter factual regret minimization