---
layout: post
title:  "Evaluating Euchre sayings -- pass on the bower, lose for an hour?"
categories: project-log
---


## Todos:
[ ] Speed up open hand solver, takeing ~30hrs to evaluate all moves -- now at a bit over an hour
  [ ] Add test to verify transposition table implementation
  [ ] Make deck indifferent to whose move it is -- have player0 always be whoever is going to go next
  [*] Switch to DashMap

[ ] Evaluate what was found from previous run, for example, why 9CJCQCKCAC|9STSKSAS9H|THKHAHTDJD|TCQSJHQH9D|JS| considered a take?
  * problem may be caused by caching incorrect values for open hand solver -- implement the verification code before continuing
  [*] Verify working for Kunh Poker
  [*] Verify working with entire state hashed as part of isomorphic key
  [*] Verify each element of making the deck isomorphic -- just return the deck doesn't make the results match
  [ ] See what is failing with deck isomorphic call
    * Re-ordering suits works as expected
    * Down shifting cards seems to fail
    * Re-factor the isomorphism call to be part of the euchre game, and not tracked in the deck? Or at least remove the trump tracking
    * See notes in failing test -- downshifting a face up card causes the error, but only when the card doesn't end up in play
  [ ] change pre-play hash calls -- to be more isomorphic
  [ ] Re-organize deck to take cur_player and trump as parameters to isomorphic call

## Content
There are 10^15 possible deals for Euchre. 10^13 if set the face up card to be one of the jacks.

https://docs.google.com/spreadsheets/d/1naRU_pnwoS7RmBhVK0ruyavqhnlRiQSeN7LTpQbdWXM/edit#gid=0 -- calculate an isomorphism


https://docs.google.com/spreadsheets/d/1naRU_pnwoS7RmBhVK0ruyavqhnlRiQSeN7LTpQbdWXM/edit#gid=0


Do an initial post with just the open hand evaluator -- see when it recommends doing something else

* Deal myself random hands with Jack of spade as the face up card
* Run the evaluator on each


Problems with open hand solver:
* If p0 has a very strong hand to take the face up, but partner has an even stronger hand for a different suit. Won't call take, will let it come around and call their suit -- since know every hand
  * Example: `QDJDKHQCKD|TDJH9HACJC|KSJSKCQSAS|QHTSADTC9S|9D|PPPP|S|KH9HKCQH|JS9SQCJC|JDTDQSAD|QDACKSTC|KDJHAS`: 3: value: 0, action: TS


CFR approach:
* Likely not enough compute for iterations, 10^14 30s iterations to see each possible game state once
* Challenge calculating exploitability -- high memory requirements for tabular exploitability calcs

Search approach:
* Create an OpenHand solver to use for rollouts in search algorithms
* Fix AlphaMu to work for changing valid world states
* Need to figure out how to handle re-sampling of euchre states since there is a discard action that is hidden
  * Maybe do an analysis of how random discards would be a negative here?

## How many iterations are needed for the open hand solver?

No difference in answer from 20 iterations and 100 iterations from open and solver -- use 20 iterations

Computation estimate:
* Number of hands = 23 choose 5 = 34k hands
* Estimated number of solver worlds for convergence -- 20 iterations (0.8s)

see txt file for convergence data

https://docs.google.com/spreadsheets/d/1jSnrLpAOYBPiV-qoYRqIv6wxFkVrDlFsdcSSF2k41wk/edit
4 out of 50 changed when looking at 20 iterations vs 100


About 8 hrs of computation at the current rates

## Find games to evaluate -- naive approach

Naive approach -- evaluate all games -- from that state, see when should take it


## Solver playing solver, what can we estimate about oponents hands? What game states would get us to the decisions we currently have?
More complicated -- look at games where the open hand solver would get us into this state -- what's different?

TBD if this is world doing -- maybe just save the question for more complicated agents
