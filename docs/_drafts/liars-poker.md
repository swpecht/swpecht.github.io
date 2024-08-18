
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

Todos:
[*] Have indexer save it's hash function on commit
    * Move all config to be saved, including cards played, hash, and max size
[*] Make it so euchre cfr requires an Option<path>, lets us load the indexer if it exists -- otherwise we re-generate

[ ] After training, the agents are only using 1/4 of the theorhetical max number of infostates
    * What's causing this? Just states that don't really get explored?
    * There are a bunch of states you don't reach in practice, e.g. [Td, Jd, Qd, Kd, Ad, 9s, P, P, P, P, P, P, P, P, H, 9d]
    * Would have always called diamonds if got to this state
    * Do we add a compression step -- or could we compress on disk? -- is that possible to do on the fly?

[ ] Switch website to use the new agent -- and change to load from special file
[ ] Evaluate performance of lossy agents
[ ] Look at using CFR to estimate distribution of opponent cards -- then can use this to feed the the PIMCTS agent -- how much better is it if it has a belief about the opponent hand state?
[ ] explore using a ppo algorithm -- expose the game as a gym in python?


[ ] Play euchre against bots
    [ ] Fix go home link and add game over debug game
    [ ] Add post hand card log -- show what everyone had, feedback from Ian
    [ ] Color code the suit in the game information area: trump is X, called by Y
    [ ] When south player is discarding, add message that says choose card to discard
    [ ] Fix start new game button, make return home work
    [ ] Fix bug where shows the old suit when calling suit

[ ] Create xtask to re-train everything
[ ] Switch old isomorphic code for PIMCTS to use the new suit representation, and use masking for building rather than iteratively doing so
[ ] switch istate resampling to use constrain propogation like a sudoku solver

[ ] Reduce memory usage for CFR
    [ ] Evaluate bfloats versus f32 bot


Improving exploitability and CFR
[ ] What people are doing these days for approximate best response is running "DQN-BR" (use reinforcement learning as an exploiter). See, for example, the MMD paper: https://arxiv.org/pdf/2206.05825.pdf. There's an example of this in OpenSpiel:  https://github.com/deepmind/open_spiel/blob/master/open_spiel/python/examples/rl_response.py. This idea is the basis of PSRO, btw, which is a paper which may interest you: https://arxiv.org/abs/1711.00832. Anyway, there are even more sophisticated methods that add MCTS search on top of it (see our ABR paper: https://arxiv.org/abs/2004.09677) but it is a bit heavy and compute-hungry so I'd recommend starting with DQN-BR.


## Bot comparison

played games: 10000
cfr, 1 cards played     cfr, 3 cards played f32 0.473
cfr, 0 cards played     cfr, 0 cards played     0.4811
pimcts, 50 worlds       random                  0.9908
cfr, 0 cards played     cfr, 1 cards played     0.4576
cfr, 3 cards played f32 cfr, 0 cards played     0.5018
random                  random                  0.4915
cfr, 1 cards played     cfr, 1 cards played     0.4791
random                  cfr, 1 cards played     0.0052
cfr, 1 cards played     pimcts, 50 worlds       0.5456
random                  pimcts, 50 worlds       0.0061
cfr, 0 cards played     random                  0.9932
random                  cfr, 0 cards played     0.0064
cfr, 1 cards played     cfr, 0 cards played     0.4997
pimcts, 50 worlds       cfr, 3 cards played f32 0.4009
cfr, 0 cards played     pimcts, 50 worlds       0.526
cfr, 3 cards played f32 cfr, 1 cards played     0.4857
pimcts, 50 worlds       pimcts, 50 worlds       0.4888
cfr, 3 cards played f32 random                  0.9934
cfr, 0 cards played     cfr, 3 cards played f32 0.4614
cfr, 3 cards played f32 pimcts, 50 worlds       0.5573
pimcts, 50 worlds       cfr, 0 cards played     0.434
pimcts, 50 worlds       cfr, 1 cards played     0.4116
cfr, 3 cards played f32 cfr, 3 cards played f32 0.4797
cfr, 1 cards played     random                  0.9943
random                  cfr, 3 cards played f32 0.0063




