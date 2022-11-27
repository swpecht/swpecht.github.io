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
    *Tried with Incorporate bet agent -- where it assumes the other players bets are never bluffs. But it's about equal to slightly worse than minimax
    * Tried a meta minimax agent, but still not doing better than minimax. Probably need a way to troubleshoot why a bot is losing to fix, e.g. other player made perfect bet to start
    *Alternatively, could just minimax agent play out the entire game to see which lines and then outcomes are most likely
[X] Implement weighted rock paper scisors or similar game where nash equilibrium varies between minimax and CFR
    * <https://arxiv.org/pdf/2007.13544.pdf>
[X] Implement minimax for RPS -- show that this converges to the random agent
    *TODO: add a way for gamestate to return all the possible hidden states, that with eval should enable a generic score function for GameTree
[X] Implement counter factual regret minimization
[X] Add state filtering to GameState rather than as part of the game
[ ] Implement CFR for liars poker -- need to extend to multiple turns
    * Look up how ReBel does this -- is it just normal CFR applied multiple times w/ bottom up search?
    *<https://justinsermeno.com/posts/cfr/>
    * <https://xyzml.medium.com/learn-ai-game-playing-algorithm-part-iii-counterfactual-regret-minimization-b182a7ec85fb#ede8>
[ ] Redo agents to be based on game tree evaluation? -- could all agents be done this way? -- then have a "scorer" be the thing that changes?
    *Removes the complexity of storing state, because the "score" can be kept in the game tree
    * To do this, going to remove GameState action as first class citizen -- too confusing, instead the gamestate will just say which children states are reachable. Actions are still concepts, but they aren't associated types for GameStates
    *Removing CFR agent for now, to make it simpler to refactor things
    * Need to finish clean-up and refactor the agents to just be scorers for the game tree? Or do they take in certain functionality from for the tree as well, e.g. which algo to propogate scores up the tree? Could always be added later?
        * Seems better as would be composing the functionality rather than inheriting it

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
