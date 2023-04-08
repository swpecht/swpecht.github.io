---
layout: post
title:  "Reducing the number of game states for Euchre"
categories: project-log
---

https://docs.google.com/spreadsheets/d/1naRU_pnwoS7RmBhVK0ruyavqhnlRiQSeN7LTpQbdWXM/edit#gid=0

See isomorphism poker paper for more details 

134k starting hand states

There are 4^5 suit configurations for an infoset = 1,024


Can instead represent as counts of suits (hand, face up)

(5, 1)
(5,0) (0, 1) // 2
(4, 1) (1,0)
(4, 0) (1,1)
(4, 0) (1, 0) (0, 1) // 3
(3, 1) (2, 0)
(3, 1) (1, 0) (1, 0)
(3, 0) (2, 1)
(3, 0) (2, 0) (0, 1)
(3, 0) (1, 1) (1, 0)
(3, 0) (1, 0) (1, 0) (0, 1) // 6

(2, 1) (3, 0) -- repeat 
(2, 0) (3,1) -- repeat
(2, 1) (2, 0) (1, 0)
(2, 0) (2, 0) ( 1, 1)
(2, 0) (2, 0) (1, 0) (0, 1)
(2, 0) (1, 1) (1, 0) (1, 0) // 4

15 encodings for the suit possible. 96M / 1024 * 15 = 1.4M starting infosets (1/68) of starting amount.


For cards encode 7 options (six cards + on and off color jack). and 0-2 for which suit set the card belongs to. Total hands =
15 suit encodings * 7^6 * 4^6 = 1.2b

1.8M for suit + face. How to figure out suit aignment

For actual play -- could we encode the suits in a similar way? Could reduce memory pressure. Probably doesn't reduce compute -- could converge faster


Could encode the jack of same color of turn as a separate card -- keeps the suit encoding simple 

## Verifying on actual game rules

run with `--mode analyze` to see summary stats about games

For a given deal, ~27M possible rollouts. One order of magnitude less than the naive approach.

How many total game nodes is this? -- need to count across the tree
~175M nodes --many to keep in memory?

CFRNode is ~100bytes ignoring the key

```Rust
pub struct CFRNode {
    pub info_set: String, // ignore for now
    pub actions: [u8; 5] // 5 bytes,
    pub num_actions: u8, // 1 byte
    pub regret_sum: [f32; 5], // 20 bytes // since we know this is 0-1 range, could we use a smaller size? f32 uses 3 bytes for storing the decimal. get to 15 bytes?
    pub strategy: [f32; 5], // 20 bytes
    pub strategy_sum: [f32; 5], // 20 bytes
}
```
66 bytes for the CFRNode excl. key is 32 bytes -- 98 bytes total
~180M nodes

180M * 98 bytes / 1000 (kB) / 1000 (MB) / 1000 (GB) = ~18GB to store

22 total rounds

Less nodes, since can ignore last layer and layer before

## Starting game states
https://docs.google.com/spreadsheets/d/1naRU_pnwoS7RmBhVK0ruyavqhnlRiQSeN7LTpQbdWXM/edit#gid=0


5*10^14 possible starting game states --

How can this be reduced? e.g. with a better representation? Symmetry along the suits?

Can we break the problem apart into before and after trump is called? And then simplify the gamestate?

Running for 87M rounds of CFR resulted in 10M nodes stored in the database in 5.5GB. Would need ~175M rounds of the CFR for a single game. About 11 GB for nodes from a single game. Reasonable for a single hand, but would struggle to get reasonable coverage of possible deals.

## Exploring options to reduce computation

Using CFR, we only need to store each information state.

* Deal: 134k hands (24 choose 6, hand + 1 revealed card)
* Pick / pass: 5 states (everyone passes or one of each pickup)
* Suit choice: 12 states, each of 4 players could choose 3 suits (can't choose flipped suit)
* Hands (see above): 207M states

If only looking at a single game: 12.4b states -- at a byte per gamestate, this is 12TB of data

Still likely too many states to store -- could we collapse the representation of the states?

58 min for 258M states
