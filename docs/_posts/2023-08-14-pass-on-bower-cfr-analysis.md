---
layout: post
title:  "Pass on a bower lose for an hour, part 2"
categories: project-log
---

# Context
This post is a followup to [Euchre wisdom: pass on the bower, lose for an hour?](/project-log/2023/05/30/pass-on-the-bower-lose-for-an-hour.html) and [CFR for euchre](/project-log/2023/07/30/cfr-for-euchre.html). In the counterfactual regret minimization (CFR) for euchre post, we saw that CFR for just the pre-play phase of euchre could result in a stronger agent than just perfect information monte carlo tree search (PIMCTS). We also saw that the pre-play CFR bot did not recommend always picking up the jack as the dealer -- in fact it recommended passing ~30% of the time.

In this post, I summarize the bot's strategy.

All of these results are based on using CFR for the pre-play phases of euchre and PIMCTS with 50 worlds for the play phase. The CFR was trained over 20m iterations using CFR with external sampling.

# Analyzing high-level strategy

We can plot a heatmap of the bots policy based on what cards are in a hand. Each cell represents all hands containing the matching card from the cell's row and column. The color of the cell represents the probability of the bot picking up the jack of spades. The blue colors mean to pickup the jack more than 50% of the time. While red colors are for <50% of the time. The cards are sorted from highest likelihood of pickup to lowest.

As an example, the top left area of the heatmap represents when the agent has multiple spade cards (would be trump). This area is dark blue -- as expected, the bot is likely to pickup the jack of spades if it already has many spades in it's hand.

{: style="text-align:center"}
![heatmap of when to pickup by cards in hand](/assets/pass-on-bower-2-heatmap.png)

I identified 5 major regions of the policy:

{: style="text-align:center"}
![heatmap of when to pickup by cards in hand](/assets/pass-on-bower-2-heatmap-annotated.png)

| Area                                 | Recommended action                                                                                 |
| ------------------------------------ | -------------------------------------------------------------------------------------------------- |
| A) At least one spade in hand        | Likely pickup, would have many trump cards                                                         |
| B) At least one off-suit ace in hand | Usually pickup, high non-trump cards                                                               |
| C) Other cards                       | Neutral, these cards don't seem to have a large impact on the recommended action                   |
| D) Low clubs                         | Lean towards not picking up                                                                        |
| E) Red jack                          | Rarely pickup, would have highest or second highest card if someone else calls a red suit as trump |


## Translating to a decision tree

The heatmap gives a highlevel overview of how the bots value cards, but it can only represent information for 2 cards at a time. To get a deeper understanding of the strategy, we can use the heatmap regions as additional inputs to train a decision tree model.

Using [Minimal Cost-Complexity Pruning](https://scikit-learn.org/stable/modules/tree.html#minimal-cost-complexity-pruning), I generated a simplified, 3-level, decision tree, that achieves 94% accuracy for predicting the action a bot would take.

**Decision tree for passing or picking up the jack based on cards in hand**

<pre>
|--- 0 spades
|   |--- <b>Pass</b>
|--- 1 spade
|   |--- 1+ red jacks
|   |   |--- 0 offsuit aces
|   |   |   |--- <b>Pass</b>
|   |   |--- 1+ offsuit ace
|   |       |--- <b>Pickup</b>
|   |--- 0 red jacks
|       |--- <b>Pickup</b>
|--- 2+ spades
|    |--- <b>Pickup</b>
</pre>

As expected, having more spades or offsuit aces is a strong indicator for picking up the jack of spades -- these are strong cards that can win tricks. 

Of note, the bots will rarely pickup a jack if they have no other spades. This only happens for 11 hands [0]. 

## Passing to make room for euchring the opponent

As with the heatmap, having a red jack causes the bots to be more likely to pass. And in the last post, I noted that `JcQcJhAh9d|Js|PPP` -- a hand with a single red jack -- is always a pass for the bots.

To better understand what's going on I had the bots play 50k games where the dealer is dealt this hand. Of those games, 52% (26k) made it to the dealer being able to choose to pass or pickup the jack.

When the dealer picks up the jack, their team wins 62% of games. But the average score is 0.0002 -- the dealer's team wins as many points as they lose by often getting ecuhred (losing the majority of tricks when calling trump).

**Distribution of scores `JcQcJhAh9d|Js|PPPT`**
```
Score   
-2    |████ 38%
-1    | 0 (can't happen since dealer's team called trump)
 1    |██████ 57%
 2    |█ 5%
```

Things look remarkably different if the dealer passes. Their team wins 74% of games. But the average score is only 0.0003. An extra point every ten thousand games.

**Distribution of scores `JcQcJhAh9d|Js|PPPP`**
```
Score   
-2    |2%
-1    |██ 23%
 1    |██ 18%
 2    |█████ 54%
```

The bot is able to get two points most of the time. This could be due to either euchering the opponent or by taking all 5 tricks. We can look at a breakdown of the score by who calls trump to better understand what is happening:

**Heatmap of dealer team's score by who called trump**

{: style="text-align:center"}
![heatmap of score by who called trump](/assets/pass-on-bower-2-JcQcJhAh9d-heatmap.png)

Player 0 is the player left of the dealer and player 3 is the dealer. The dealer is getting most of the wins by euchering the other team. Player zero is calling clubs as trump 32% of the time. And with the `Jc`, `Qc`, and an offsuit ace (`Ah`), the dealer is in a good position to euchre.

The propensity for player 0 to call clubs explains area D from the heatmap at the start of the post. The bot is more likely to pass if it has clubs in its hand because it values those cards for euchering the other team when they call Clubs as trump.

# Future work and limitations

This post outlines simplified rules for when to pass on the bower. But these results are not based on fully solving the game of euchre. And there is not guarantee that the combined CFR + PIMCTS agent will converge to an optimal strategy.

Next steps are to:
* Baseline performance against human players
* Get a theoretical measure for exploitability
* Extend CFR into the play phase of the game


[0] Hands where the bot will pickup with no other spades:
* `AcKhAhKdAd`   
* `AcAhQdKdAd`   
* `AcThAhTdAd`   
* `Ac9hThAhAd`   
* `AcQhAhQdAd`   
* `AcQhAh9dAd`   
* `Ac9hAh9dAd`   
* `AcQhKhAhAd`   
* `AcAh9dKdAd`   
* `AcThQhAhAd`   
* `AcAhTdQdAd`