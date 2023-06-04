---
layout: post
title:  "Euchre wisdom: pass on the bower, lose for an hour?"
date:   2023-05-30 02:00:43 +0000
categories: project-log
---

# Problem
A common saying in [Euchre](https://en.wikipedia.org/wiki/Euchre) is "pass on the bower, lose for an hour". It represents the idea that the dealer should always choose to pickup a jack if they have the opportunity to -- it would be the highest card in the game. The downside, is if they don't win the majority of the tricks, they'll be "euchred" and the other team gets 2 points.

In this post, I explore if this is true for euchre[0] using an open hand solver. All of the code is available [on Github](https://github.com/swpecht/swpecht.github.io/tree/master/projects/liars_poker_bot).


# Approach
I use an open hand solver -- a Perfect Information Monte Carlo sampling (PIMC) bot. Given a deal of Euchre it can see, it generates possible cards for the other players (different worlds) and solves them using Alpha-Beta search. It effectively assumes that all players can see each others cards and play perfectly.

**Example of what the bot can see from a deal of Euchre**
```
Game:     QhTsAdTc9s|9d|PPPT
Meaning: |--- D ----|F | P/T

* D: Dealers hand, e.g. Qh = Queen of Hearts
* F: Face up card
* P/T: Whether each player in turn Passed or told the dealer to Take the card
```

Since it's not possible to reasonably calculate this for all possible deals for a given hand, we do this for 100 different random worlds and take the average score across all worlds to evaluate a deal state. For more information on why 100 worlds, see [appendix 1](#appendix-1).

For example, our solver will output the following: 
```
# Game                 Valuation at 10k iterations
1 JcTsAsAhTd|JS|PPPT|  1.93
2 9hTh9dTdQd|JS|PPPT|  -0.96
```

For 1, the dealer's team is expected to get almost 2 points (1.93) from this game state. They are very likely to win and have a good chance of taking all 5 tricks. The dealer has a very strong hand with the 3 highest cards in the game (Js, Jc, As) and an offsuit ace (Ah). Conversely in 2,  the dealer is likely to lose the game. They'll have the highest card, but also no other trump and low offsuit cards.


# Findings: always pickup the jack?
There are 33k possible deals ($\binom{23}{5}$) for the dealer if we only look at times when the Jack of Spades is face up. We can evaluate all of these possible deals to determine when it makes sense to pass. While we're only looking at deals where JS is face up, these results translate to the mirrored suits.

After running all 33k possible deals the dealer could receive through the open hand solver, I compared the expected value of picking up the jack to the expected value of passing it. A positive value means picking up the card is better than passing it. The distribution values is below:

**Distribution of hands by expected value of (Pickup - Pass)**
```
<0      |1
0-0.5   |12
0.5-1.5 |███3091 (9.2%)
1.5-2.5 |███████████11733 (35%)
2.5-3.5 |█████████████████17633 (52%)
3.5-4.0 |█1178 (3.5%)
4.0+    |1
```


There is only a single deal where it is better to pass on the jack rather than pick it up:
```
9cTcQcKcAc|Js|PPP
```

This is one of the worst possible hands to get in this situation. It's single suited with a guarantee of no trump unless the dealer picks up the card. And it's extremely unlikely for another player to call clubs as trump since we have all of the trump cards.

But this analysis is wrong.


# Why it's wrong
The open hand solver doesn't represent actual play. We can see this if we look at the expected value distributions for Pickup states (time when the dealer has chosen to pickup the Jack) and Pass states.

**Distribution of hands by expected value and action**
```
Pickup                        
<-1  |103                          
-1-0 |██████6k (18%)               
0-1  |██████████████14k (43%)      
1-2  |█████████████13k (39%)       
2    |91

Pass
<-1  |█████████████████████████████████33k
-1-0 |197
0-1  |0
1-2  |0
2    |0
```

Whenever we pass, we have an expected value of <-1 -- indicating the other team is often taking all 5 tricks and scoring 2 points against us.

This is the strategy fusion problem of Perfect Information Monte Carlo sampling (PIMC) ([arXiv:1911.07960](https://arxiv.org/abs/1911.07960)). The open hand solver isn't constrained to have a consistent strategy between each world it evaluates. It plays each hand as if had perfect information. And the ability to see all cards and arbitrarily choose trump is too strong of an advantage. With this advantage, the player after the dealer is able to choose a trump suit to almost always get all 5 tricks.

The next step would be to find a way to evaluate euchre hands that doesn't suffer from the strategy fusion problem.

## Appendix

# Appendix 1

It's not possible to run the open hand solver on all possible 618M worlds for a given deal.
$$
\binom{18}{5} * \binom{13}{5} * \binom{8}{5} = 618M
$$

Instead, I estimated convergence of the open hand solver by scoring 500 different deals for a variety of rollouts to see when we get close enough.

**Difference in game value for dealer by number of open hand solver iterations (n=500)**
```
Iterations    Max difference vs 10k iterations 
1             |██████ 3.11 		
10            |██▌1.43
100           |▌0.40
1,000         |▌0.15
10,000        |0
```

The maximum difference between 100 iterations and 10k is 0.13. If there were any gamestates where the difference in value between Passing and Picking up is lower than this value, I would have re-evaluated them with a higher number of iterations.

# Footnotes
[0] For simplicity, I've ignored "going alone".