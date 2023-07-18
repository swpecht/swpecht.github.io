---
layout: post
title:  "CFR for Euchre"
categories: project-log
---


[ ] Set up CFR with PIMCTS to evaluate hands for a single deal of euchre -- choose one of the pass on the bower hands where PIMCTs says to take
       * JcQcKcAc9s|TsQsKsAsJh|KhAh9dTdJd|**9cTc9hThQh|Js|PPP**
 [*] Get initial one up and running with chance sampling

[ ] Implement external sampling to speed up convergence -- continue from the openspiel implementation
      [*] Confirm convergence for KP
      [*] Confirm convergence for Bluff1,1
      [ ] Run 1 million iterations on remote machine to see if converges
            * `scp root@165.232.135.220:~/swpecht.github.io/projects/liars_poker_bot/infostates infostates``
      [ ] Check that we're doing the simple average update properly
 [ ] Significantly reduce the memory footprint -- not as necessary if we only look at the bidding phase
      [*] Do a sizing on memory needed if only store the bid phase in CFR, and do everything else in Openhand solver
      [*] Set up max depth for CFRES, train it on just the bid phase for euchre games -- see how performs versus PIMCTs
      [ ] Change the number of iteration for PIMCTs for the non-bidding play to something comparable to the opponent
      [ ] Add the one legal move optimization
[ ] Set up script to measure convergence by playing against PIMCTs agent -- tbd if this works since the cfr agent will effectively know our players cards -- does it still result in better performance against the PIMCTs agent?

[ ] Change how node store works to return references to things rather than cloning everything and re-looking things up

# Memory needs

Going to do CFR for just the Bidding phase of euchre, open hand solver seems to work well for card play

Istates: 
      Hands: $$\binom{23}{5}=33649$$ -- if have each card individually 
      Bid P/T: 4 (_, P, PP, PPP)
      Bid suit: 4 (PPPP, PPPPP, PPPPPP, PPPPPPP)
      = 538,384 istates -- if we stop play after the bidding phase

If we constrain to a known deal for the dealer, even fewer hands available $$\binom{18}{5}=8568$$
      ~137088 total istates

Should be able to easily fit this in memory


# Difference with old method

# External sampling

Add in tests that it converges


# Compare CFR sampling algorithms?

Compare converge speed across for different games

# Maybe add in up the river down the river to compare performance