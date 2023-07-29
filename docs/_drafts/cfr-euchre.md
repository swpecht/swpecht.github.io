---
layout: post
title:  "CFR for Euchre"
categories: project-log
---

# Context

This post outlines a partial solution to the strategy fusion problem identified in the previous pass on the bower post ([Euchre wisdom: pass on the bower, lose for an hour?](/project-log/2023/05/30/pass-on-the-bower-lose-for-an-hour)). Specifically, we use counterfactual regret minimization (CFR) to find a strategy for the pre-play phases of euchre that outperforms [PIMCTS](/project-log/2023/07/15/pimcts-for-euchre). CFR is "an iterative self-play algorithm in which the AI starts by playing completely at random but gradually improves by learning to beat earlier versions of itself" ([ref](https://www.science.org/doi/10.1126/science.aay2400#:~:text=CFR%20is%20an%20iterative%20self%2Dplay%20algorithm%20in%20which%20the%20AI%20starts%20by%20playing%20completely%20at%20random%20but%20gradually%20improves%20by%20learning%20to%20beat%20earlier%20versions%20of%20itself.)) 

Unlike PIMCTS which can use a different strategy for every "world" it evaluates, CFR is constrained to have a single strategy for any given information state that is sees. For example, imagine a CFR agent is evaluating the information state `Qc9sTs9dAd|Qs` (hand of `Qc9sTs9dAd` with a `Qs` as the face up card). It will look up the average strategy from all regrets whenever it encountered the info state `Qc9sTs9dAd|Qs` in the past. There is only a single strategy for this information state and the children information states -- all strategies are "fused".

I use CFR with external sampling based on [Open Spiel's implementation](https://github.com/deepmind/open_spiel/blob/master/open_spiel/python/algorithms/external_sampling_mccfr.py). External sampling has faster convergence that other CFR algorithms, but there are 2 challenges to overcome before applying CFR to euchre:
* Reducing the number of information states we need to store
* Adapting CFR to a co-operative imperfect information game

# Reducing the number of information states to store

CFR has been applied to games larger than euchre in the past -- Meta used CFR as part of it's [Superhuman AI for multiplayer poker](https://www.science.org/doi/10.1126/science.aay2400). While the "blueprint strategy" for their solution was computed in 8 days on a 64-core server, I needed a way to iterate faster given I have little idea what I'm doing.

To achieve this, I only used CFR for the pre-play phases of euchre: telling the dealer whether to pickup or pass, calling a suit or passing, and discarding a card. For the play phase, I used a rollout from the [open hand solver](/project-log/2023/07/15/pimcts-for-euchre) I developed for PIMCTS.

Instead of using the true regrets for CFR, I use the evaluation from then open hand solver.

With this change, I only need to store 538k information states for games where the `Js` is the face up card. **X% less than if I needed to store all Yb infostates from the full game (todo to update this)**. See **link to appendix for more details**.

Another approach to reduce the number of information states is through information abstraction:

> Decision points that are similar in terms of what information has been revealed (in poker, the player’s cards and revealed board cards) are bucketed together and treated identically. For example, a 10-high straight and a 9-high straight are distinct hands but are nevertheless strategically similar. Pluribus may bucket these hands together and treat them identically, thereby reducing the number of distinct situations for which it needs to determine a strategy.
[Superhuman AI for multiplayer poker](https://www.science.org/doi/10.1126/science.aay2400)

I may pursue this in the future, but have not yet implemented it to save development time. 

# Adapting CFR to a co-operative imperfect information game



* Change the CFR algorithm to work on teams for min and max
* Change simple averaging to work on teams
simple averaging change, any team based changes



# Training and performance versus PIMCTS

CFR was trained for 20m iterations on a **comp specs** single thread. The resulting weights can be serialized to 63MB.

Run benchmark of jack of spades games


# Pass on the bower results
Note: this is different than the pure PIMCTS approach for two reasons:
* Solve the strategy fusion problem
* We're conditioning on the other players not telling the dealer to pickup, 


Move most of this to future post, just have some of the ones called out in the last post reviewed here.

Always take:
* `JcTsKsAhTd|JS|PPP` 
* [example with lots of trump]
* [example with no other trump]
* ...

Sometimes take:
* [example with lots of trump]
* [example with no other trump]
* ...

Never take:
* `ThAh9dJdKd`
* `TcJc9hQh9d|JS|PPP` -- surprising, this would give the first and second highest card, why losing here? -- search for this game?


| hand       | Old rating | New rating |
| ---------- | ---------- | ---------- |
| TcJc9hQh9d |            |            |
|            |            |            |

How to comapre the previous results with these ones?


# Future work

Deeper analysis into where it makes sense to pickup vs pass and why


## Appendix

# Infostate sizing

      Deals: $$\binom{23}{5}=33649$$
      * (8 outcomes for bidding + 8 outcomes for


Istates: 
      Hands + face: $$\binom{23}{5}=33649$$ -- if have each card individually 
      
Bid (8)
{hand + face}
{hand + face}P
{hand + face}PP
{hand + face}PPP
{hand + face}PPPP
{hand + face}PPPPP
{hand + face}PPPPPP
{hand + face}PPPPPPP

+ Discard (8)
{hand + face}T
{hand + face}PT
{hand + face}PPT
{hand + face}PPPT
{hand + face}PPPPT
{hand + face}PPPPPT
{hand + face}PPPPPPT
{hand + face}PPPPPPPT

= 538k infostates

What if include the first trick:
 Player 0: 1M discard istates
 Player 1: $$binom{18}{1}$$
 Player 2: * $$binom{17}{1}$$ 
 Player 3: * $$binom{16}{1}$$
 = ~5b istates?


# how does convergence compare for different number of PIMCTs runs for the evaluation phase
Need to be a single openhand run, not the 50 PIMCTs runs