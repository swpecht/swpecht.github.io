---
layout: post
title:  "Euchre wisdom: pass on the bower, lose for an hour?"
date:   2023-05-30 02:00:43 +0000
categories: project-log
---

# Problem
A common saying in the card game [euchre](https://en.wikipedia.org/wiki/Euchre) is "pass on the bower, lose for an hour." In other words, the dealer should always pick up a jack — known as a “bower” — if they have the chance; it’s the highest card in the game. The downside is that if they don't win the majority of the tricks, they'll be "euchred" and the other team gets 2 points.

In this post, I explore if this is true for euchre[0] using an open-hand solver. All of the code is available [on GitHub](https://github.com/swpecht/swpecht.github.io/tree/master/projects/liars_poker_bot).


# Approach
I use an open-hand solver — a Perfect Information Monte Carlo sampling (PIMC) bot. Given a deal of euchre the bot can see, it generates possible cards for the other players (different worlds) and solves them using Alpha-Beta search. It assumes that all players can see each other’s cards and play perfectly.

**Example of what the bot can see from a deal of euchre**
```
Game:     QhTsAdTc9s|9d|PPPT
Meaning: |--- D ----|F | P/T

* D: Dealer’s hand (e.g., Qh = queen of hearts)
* F: Face-up card
* P/T: Whether each player, in turn, chose to Pass or told the dealer to Take the card
```

Since it's not possible to reasonably calculate this for all possible deals for a given hand, we calculate it for 100 different random worlds and take the average score across all worlds to evaluate a deal state. For more information on why we’ve used 100 worlds, see [appendix 1](#appendix-1).

For example, our solver will output the following: 
```
# Game                 Valuation at 10k iterations
1 JcTsAsAhTd|JS|PPPT|  1.93
2 9hTh9dTdQd|JS|PPPT|  -0.96
```

For 1, the dealer's team is expected to get almost 2 points (1.93) from this game state. The team is very likely to win and has a good chance of taking all five tricks. The dealer has a very strong hand, with the three highest cards in the game (Js, Jc, As) and an off-suit ace (Ah). Conversely, in 2, the dealer’s team is likely to lose the game. The team will have the highest card, but also no other trump and low off-suit cards.


# Findings: Always pick up the jack?
There are 33k possible deals ($\binom{23}{5}$) for the dealer if we only look at instances when the jack of spades is face-up. We can evaluate all of these possible deals to determine when it makes sense to pass. While we're only looking at deals where the Js is face-up, these results can be extended to other suits. For example, a deal of `TsQsKsAsAh|Js` is effectively the same as `ThQhKhAhAs|Jh`. Even if the suits differ, the value of the hand is the same because we have the same distribution of trump and non-trump cards.

After running all 33k possible deals the dealer could receive through the open-hand solver, I compared the expected value of picking up the jack to the expected value of passing it. A positive value means picking up the card is better than passing it. The distribution values are below:

**Distribution of hands by expected value of (Pickup – Pass)**
```
<0      |1
0–0.5   |12
0.5–1.5 |███3091 (9.2%)
1.5–2.5 |███████████11733 (35%)
2.5–3.5 |█████████████████17633 (52%)
3.5–4.0 |█1178 (3.5%)
4.0+    |1
```


According to these results, of the 33k deals, there is only a single possible deal where it is better to pass on the jack than to pick it up:
```
9cTcQcKcAc|Js|PPP
```

This is one of the worst possible hands to get. Why? It's single-suited, with a guarantee of no trump unless the dealer picks up the card. And it's extremely unlikely for another player to call clubs as trump, since we have all of the trump cards.

On the surface, it seems to make sense. But this analysis is wrong.


# Why it's wrong
The open-hand solver doesn't represent actual play. We can see this if we look at the expected value distributions for Pickup states (instances when the dealer has chosen to pick up the jack) and Pass states.

**Distribution of hands by expected value and action**
```
Pickup                        
<-1  |103                          
-1–0 |██████6k (18%)               
0–1  |██████████████14k (43%)      
1–2  |█████████████13k (39%)       
2    |91

Pass
<-1  |█████████████████████████████████33k
-1–0 |197
0–1  |0
1–2  |0
2    |0
```

Whenever we pass, we have an expected value of <-1, indicating that the other team is often taking all five tricks and scoring 2 points against us.

This is the strategy fusion problem of Perfect Information Monte Carlo sampling (PIMC) ([arXiv:1911.07960](https://arxiv.org/abs/1911.07960)). The open-hand solver isn't constrained to have a consistent strategy between each world it evaluates. It’s working from a simplified model based on two key assumptions: that it can see all cards, and that it can choose different actions for each set of cards it sees. Because of this,, when it sees the opportunity to wait and then choose trump later with perfect information, it will always make that decision. “Believing” its view of the game is correct,it plays each hand as if it’s working from perfect information. 

The next step would be to find a way to evaluate euchre hands that avoids the strategy fusion problem.

## Appendix

# Appendix 1

It's not possible to run the open-hand solver on all possible 618M worlds for a given deal.
$$
\binom{18}{5} * \binom{13}{5} * \binom{8}{5} = 618M
$$

The open-hand solver can evaluate a single position in 50ms. Evaluating all hands would take over 8,000 years.

Instead, I estimated the open-hand solver’s convergence by scoring 500 different deals for a variety of rollouts to see when we get close enough.

**Difference in game value for dealer by number of open-hand solver iterations (n=500)**
```
Iterations    Max difference vs. 10k iterations 
1             |██████ 3.11 		
10            |██▌1.43
100           |▌0.40
1,000         |▌0.15
10,000        |0
```

The maximum difference between 100 iterations and 10k is 0.13. If there were any game states where the difference in value between Pass and Pickup was lower than this value, I would have re-evaluated them with a higher number of iterations.

# Footnotes
[0] For simplicity, I've ignored "going alone."
