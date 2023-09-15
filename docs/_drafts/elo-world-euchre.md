
[ ] Implement the mem map store
    [ ] Create phf for different lengths of euchre game
    [ ] Do an estimate on the data we would need
    [ ] Switch over diskstore to use the mem map -- use an anon map for no file access

[ ] Optimize infostate representation

[ ] Shard training on the face up card -- all the istates should be independent
    * Estimate max size we can hold, -- may need to use array tree again to avoid allocation behavior where it doubles

[ ] Add benchmark configuration to the TOML file
[ ] Add agent configuration to the TOML file

[ ] Make training continue from previous iterations
[ ] Re-run training with the new infostate tree

[ ] Compare performance between different number of played cards trained on
[*] Figure out data usage from CFRES nodes
[*] See if can call `reserve` explicitly to avoid the doubling behavior of hashmaps -- or allocate everything up front with a large reserve call?


[ ] Don't actually lose that much speed using a single reader writer (mutex hashmap) -- simplify to single reader / writer setup?

# Data size analysis

Infostate: 80 bytes


f64 is 8 bytes each
NormalizedAction is u8 -- 1 byte

**Breakdown of `InfoState` memory usage, bytes (percent of total)**
```
actions         |███ 29 (12%)
regrets         |   ██████ 64 (27%)
avg_strategy    |         ██████ 64 (27%)
last_iteration  |               █ 8 (3%)
InfoState       |████████████████ 165
IStateKey       |                ███████ 72 (30%)
Total per entry |████████████████████████ 237 (100%)
actions_mask    |                      ██ -25, switch from Vec<Action> to u32 mask
New total       |██████████████████████ 212 (89%)

```

Estimated data usage:

$$
9.7\text{m infostates} * 245 \text{bytes} = 2.3 \text{GB}
$$

Could reduce the actions component from 29 bytes to 4 bytes if instead store a u32 mask of the actions -- and assert we can never have fewer actions

Could get a 27% reduction if switch to f32 instead of f64 -- does this create instability?

~30% reduction if we don't need to store the full key with each entry? -- essentiall reduce the 72 for the IstateKey to 1? since only need to store the actual action of the leaf node? -- not quite right with the deals not being stored, but might be close-ish

Compare memory layout of different data structures?

# Constraints of loading to and from disk

$$\frac{\text{1 entry}}{\text{212 B}} * \frac{\text{30 MB}}{\text{s}}*\frac{\text{1,000,000 B}}{\text{1MB}}*\frac{\text{3600 s}}{\text{1 hr}}=\frac{\text{509m entries}}{\text{hr}}$$

The current implementation touches about 800m nodes to train. If we had no other work to do, it would take <2 hrs to fully train an agent -- this is faster than what it takes today. Paging too and from disk isn't too much of a burden on training times

This behavior only exists because we do a fair amount of computation on each iteration to evaluate the game using PIMCTS. As we extend CFR deeper into the game, the PIMCTS will take less time, and this will become a bigger bottleneck.

# Other

Include a euchre bot that goes deeper into the play phase


have bot play each other
calculate elo

create a struct called the botarena

one iteration is each bot playing the others

randomly decide who deals

serialize results

Details of expanding euchre to more cards
* Performance versus weight size as we expand? Predictions for further expansion?
* May need to switch to HAMT (hash array tries) for the better memory usage of the huge maps, tbd






# Agent performance

Compare agents by number of cards played