---
layout: post
title:  "CFR for euchre"
categories: project-log
---

# Context and problem


This post outlines a partial solution to the strategy fusion problem identified in the previous pass on the bower post ([Euchre wisdom: pass on the bower, lose for an hour?](/project-log/2023/05/30/pass-on-the-bower-lose-for-an-hour)). Specifically, we use counterfactual regret minimization (CFR) to find a strategy for the pre-play phases of euchre that outperforms [PIMCTS](/project-log/2023/07/15/pimcts-for-euchre).


CFR is "an iterative self-play algorithm in which the AI starts by playing completely at random but gradually improves by learning to beat earlier versions of itself" ([ref](https://www.science.org/doi/10.1126/science.aay2400#:~:text=CFR%20is%20an%20iterative%20self%2Dplay%20algorithm%20in%20which%20the%20AI%20starts%20by%20playing%20completely%20at%20random%20but%20gradually%20improves%20by%20learning%20to%20beat%20earlier%20versions%20of%20itself.)). I use CFR with external sampling based on [OpenSpiel's implementation](https://github.com/deepmind/open_spiel/blob/master/open_spiel/python/algorithms/external_sampling_mccfr.py).


CFR does not suffer from the strategy fusion problem because it stores only a single strategy for each information state. And in some cases, CFR is guaranteed to converge to optimal play.


However, we cannot naively run CFR on euchre. There are over $$10^{23}$$ information states for euchre ([details](#estimating-number-of-information-states)). We cannot store all of these.


# Traditional approach


The traditional approach to this problem is to approximate information states. From [Superhuman AI for multiplayer poker](https://www.science.org/doi/10.1126/science.aay2400):


> Decision points that are similar in terms of what information has been revealed (in poker, the player’s cards and revealed board cards) are bucketed together and treated identically. For example, a 10-high straight and a 9-high straight are distinct hands but are nevertheless strategically similar. Pluribus may bucket these hands together and treat them identically, thereby reducing the number of distinct situations for which it needs to determine a strategy.




Using this approach, Meta was able to train the "blueprint strategy" for their solution in 8 days on a 64-core server.


I did not take this approach for two reasons: 1) coding up game-specific information state abstractions takes time 2) even a training time of 8 days is too long for me to experiment with. I don't know what I'm doing and need faster feedback on whether things are working.


# What I did instead


As outlined in the [appendix](#estimating-number-of-information-states), there are a reasonable number of information states at the start of a euchre game. There are only 33k deals when the `Js` is face up. But the number of information states explodes as play goes on. This is similar to poker.


But unlike poker, the play in euchre becomes relatively simpler as play progresses. Players are often forced to play certain cards to follow suit, the right play may be obvious, or there are generally fewer cards to play.


I hypothesize that these later moves are less important and a simpler evaluation function may be good enough.


With this hypothesis, I only used CFR for the pre-play phases of euchre: telling the dealer whether to pick up or pass, calling a suit or passing, and discarding a card. Instead of using the true regrets for CFR, I use the evaluation from an [open hand solver](/project-log/2023/07/15/pimcts-for-euchre).


With this change, I only need to store 1.1m information states for games where the `Js` is the face-up card. And in practice, I store about 40% of this many as external sampling guides towards more realistic moves to explore. See [estimating the number of information states](#estimating-the-number-of-information-states) for more details.


# Training and performance versus PIMCTS


The pre-play CFR agent was trained for 20m deals with a `Js` as the face-up card. On a single thread, the training took 50 hours on a Hetzner AX41-NVMe (AMD Ryzen 5 3600 with 64 GB DDR4). The resulting weights serialize to 63MB ([json download](https://fewworddotrick.blog.nyc3.cdn.digitaloceanspaces.com/infostates.open-hand-20m.json)).

**Pre-play CFR agent score by training iteration**
```
                                    ░░░   ░░░
                                    ░░░   ░░░
                              ░░░   ░░░   ░░░
                              ░░░   ░░░   ░░░
                              ░░░   ░░░   ░░░
                        ░░░   ░░░   ░░░   ░░░
            ░░░   ░░░   ░░░   ░░░   ░░░   ░░░
            ░░░   ░░░   ░░░   ░░░   ░░░   ░░░
----------------------------------------------
Iteration:   0    10k   100k  1M    10M   20M
Score:      0.17  0.19  0.27  0.59  0.77  0.78   
```

To compare performance to the PIMCTS algorithm, I had the CFR agent and the PIMCTS agent both play as dealers against the PIMCTS agent. For the CFR agent, I used the CFR results to choose moves for all pre-play actions and then used PIMCTS for the play phase. All PIMCTS agents used 50 worlds for move evaluation.


**Average score for dealer's team, 10,000 games**
```
PIMCTS (never pass on bower) |██████ 0.67
Pre-play CFR                 |████████ 0.77
```

The pre-play CFR agent scored 16% higher than the PIMCTS agent as the dealer. We now know that the CFR agent is better than the PIMCTS agent at dealing with these situations.

# Pass on the bower results

Unlike the PIMCTS agent, the CFR pre-play agent recommends the dealer pass on the bower 31% of the time.

**Count of hands by probability CFR agent recommends dealer pick up the bower, %**
```
Always pickup (80%+)    |███████████ 22k (65%)
Usually pickup (50-80%) |█ 1.1k (3%)
Rarely pickup (20-50%)  |█ 1.0k (3%)
Never pickup (0-20%)    |████ 9.4k (28%)
```

It also looks like the "usually pickup" and "rarely pickup" buckets would disappear with enough time for the CFR training to converge:

**Portion of hands in the "Usually Pickup" and "Rarely pickup" bucket by training iterations***
```
                  ░░░       
                  ░░░       
                  ░░░       
                  ░░░       
                  ░░░       
                  ░░░    
                  ░░░   ░░░   ░░░ 
----------------------------------
Iteration:        3M    10M   20M
Portion hands:    21%   9%    7%
```

A spot check of the hands seem to make intuitive sense:

**Example hands by suggested dealer action**

| Always pickups                                                     | Never Pickup                                               |
| ------------------------------------------------------------------ | ---------------------------------------------------------- |
| `Jc9sQsKsKd`: A hand of only spades (`Kd` can be discarded)        | `Tc9s9dTdJd`: Weak, multi-suit hand, likely to get euchred |
| `9sQsKsQhQd`: Lot's of spades with a relatively high off-suit card | `9sThJhQhQd`: A single, low trump, and a red jack          |

But there are some unexpected recommendations. For example, `JcQcJhAh9d` is a “never pickup” hand. But if I were playing, I would be tempted to pick up the `Js`. The dealer would have the two highest cards (`Js` and `Jc`) and an off-suit ace. More research is needed to understand if this is an incorrect play caused by the lack of strategy fusion for the card play portion of the game or the correct strategy.

While taking the bower is usually a good idea, sometimes passing on the bower is the right choice.

# Future work

The analysis of the pre-play CFR agent still has many shortcomings:
* It's unclear why some moves are recommended when or what insights human players could draw from these findings (e.g., passing on `JcQcJhAh9d`)
* It only partially solves euchre. We still depend on PIMCTS for the play phase of the game
* We don't have a theoretically rigorous way to gauge its performance


Future work could address all of these points:
* More analysis could be done on the results from the CFR agent. For example, a decision tree could be trained on the recommended moves.
* The CFR portion of the evaluation could be extended to cover more of the game (e.g., the first trick of play or all of the game)
* An approximation for [exploitability](https://github.com/deepmind/open_spiel/blob/master/open_spiel/python/algorithms/exploitability.py) could be calculated for the agent


## Appendix


# Estimating the number of information states

There are 33k possible information states for a deal of cards if we constrain the `Js` as the face-up card.

$$\binom{23}{5}=33649$$
There are 32 possible outcomes for the bidding phase: any of the four players could pick up or pass (8), and any of the four players could call a suit (16)

Altogether, there are **1.1m pre-play information states** for euchre. This can easily be fit into memory.

Estimating information states after this point becomes more complicated. Not all cards can be legally played. But we can use a naive approach to estimate the order of magnitude. From the perspective of each player, during any trick, the other players could play any of the remaining cards in the deck, and the player could play any of the remaining cards in their hand. For example, the first trick could play out 24k ways.


| Trick                | Outcomes                       |
| -------------------- | ------------------------------ |
| 1                    | $$ 18 * 17 * 16 * 5 = 24480 $$ |
| 2                    | $$ 15 * 14 * 13 * 4 = 10920 $$ |
| 3                    | $$ 12 * 11 * 10 * 3 = 3960 $$  |
| 4                    | $$ 9 * 8 * 7 * 2 = 1008 $$     |
| 5                    | $$ 6 * 5 * 4 * 1 = 120 $$      |
| **Play phase total** | $$10^{17}$$                    |

The pre-play and play phase together have $$10^{23}$$ options just for deals where the `Js` is face up. Even if we could store every information state and the associated training data in a single byte of data (we can't), we'd need 100m petabytes to store this information.

# Convergence with PIMCTS and 50 worlds

I originally attempted to train the CFR agent by evaluating moves from a PIMCTS agent with 50 worlds rather than just the single evaluation from the open-hand solver. I hypothesized that this would make each iteration of CFR "more valuable" by returning a better approximation of the average regret.

This approach did not work. The CFR agent would repeatedly get stuck at performance slightly worse than a vanilla PIMCTS agent.