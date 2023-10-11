
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

[ ] Play euchre against bots
    [ ] Use label "east wins" instead of the turn tracker on clearing trick
    [ ] Add something showing the game is over before going to the next one?
    [ ] Show players bid actions while waiting for other human to go
    [ ] Add click to say ready for game end, including tricks taken, running score, etc.
    [ ] Update text for dealer pickup to say "take card"
    [ ] Show all bid actions for players, e.g. if passed twice, show pass, pass
    [ ] Add icon to show which players are computers versus humans
    [*] Switch to pre-rendering pages -- seems pretty complex and not well supported -- can we get rid of the router?
        * Alternative is to move the origingal page to html -- and then just have the button say loading, button and text are replaced when we move to the different pages -- e.g. hide and show the context div on certain page loads
        * Can use the dioxus cli to convert to html
        * Make sure tailwind sees the page
    [ ] Switch to cfr as the agent -- update with latest weights
    [ ] Add gamestate key to action request
    [ ] Change server to run as a service



Improving exploitability and CFR
[ ] Do we actually need to pre-compute everything for tabular exploitability? Instead can we 'search' and compute on the fly?
    * Is there not some way to "filter" for this?
    * Confirm why the MC chance sampled version doesn't converge
[ ] What people are doing these days for approximate best response is running "DQN-BR" (use reinforcement learning as an exploiter). See, for example, the MMD paper: https://arxiv.org/pdf/2206.05825.pdf. There's an example of this in OpenSpiel:  https://github.com/deepmind/open_spiel/blob/master/open_spiel/python/examples/rl_response.py. This idea is the basis of PSRO, btw, which is a paper which may interest you: https://arxiv.org/abs/1711.00832. Anyway, there are even more sophisticated methods that add MCTS search on top of it (see our ABR paper: https://arxiv.org/abs/2004.09677) but it is a bit heavy and compute-hungry so I'd recommend starting with DQN-BR.



Misc ideas
[ ] Multithread for rollout?
[ ] Implement other rollout methods
[ ] For benchmarking -- create a policy agent. Takes policy as an input -- might be an easier way to have all the agents play
[ ] Blog post on an introduction to these search alogirithms:
    * Start with min-max
    * Ok, but no perfect information -- so instead we sample a bunch of possible worlds -- this is X
    * Ok, but have locality problem -- so ismcts
    * then improvements for alpha mu

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

