---
layout: post
title:  "Speeding up CFR training for euchre"
categories: project-log
---

# Context

This post outlines how I sped up training a [CFR bot for euchre]({% post_url 2023-07-30-cfr-for-euchre %}). I used three approaches:
* Multithreading
* Normalize card suits: transforming cards so that spades is always the card dealt face up
* [Linear CFR](https://arxiv.org/pdf/1809.04040.pdf): An approach to discount regrets from early iterations of training when the agents are playing randomly


**Impact of each approach on training time to achieve PIMCTS-equivalent performance (hrs, avg of 3 runs)**
```
Baseline            |██████████████████████████████ 30
Multi-threading     |       ███████████████████████ 23
Normalized suit     |  █████ 5
Linear CFR          | | 0
All optimizations   |█ 2
```

All results are on a Hetzner AX41-NVME (AMD Ryzen 5 3600 with 64GB of DDR4 RAM).

# Multithreading

For multi-threading that main change was storing the CFR weights in a [DashMap](https://docs.rs/dashmap/latest/dashmap/) rather than Rust's standard `HashMap`. DashMap is a concurrent HashMap implementation.

Once everything could be shared across threads, I used [Rayon](https://github.com/rayon-rs/rayon) to parallelize the work.

Multithreading took the convergence time from 30 hrs to 7 hrs.

This is significantly slower than we would see with perfect parallelization across the 12 threads on the AMD Ryzen 5. Further work is needed to investigate why we don't get the theoretical gains.

# Normalizing card suit

In euchre, there are 9.7m infostates to store before the play phase of the game:

$$
I = D * (B + D')\\
\begin{align}
&\text{where:}\\
&I=\text{count of infostates}\\
&D=\text{count of deals}\\
&B=\text{count of bids}\\
&D'=\text{count of discard states}\\
\end{align}
$$

For the deal states, there are 24 total cards. The player gets five and one of the reamaining 19 is dealt face up:

$$
D=\binom{24}{5}*19
$$

There are 8 possible bid states:
```
"" -- no one has acted yet
"P" -- player 0 passes telling the dealer to pickup
"PP"
"PPP"
"PPPP"
"PPPPP" -- player 0 passes calling suit
"PPPPPP"
"PPPPPPP"
```

And 4 possible discard states:
```
"PPPT|Dis|"
"PPT|Dis|"
"PT|Dis|"
"T|Dis|"
```

All together this gives us the 9.7m infostates.

$$
\binom{24}{5}*19 * (4+8) = 9.7\text{m}
$$

We can do better. Similar to the past post on [speeding up PIMCTS]({% post_url 2023-07-15-pimcts-for-euchre %}), we can take advantage of a symmetry between suits.

For example, having a hand of `9dTdJdQdKd` with the `Ad` as the face up card is effectively the same hand as `9sTsJsQsKs` with `As` as the face up suit.

Using this information we can normalize the infostate so that the face up card is always a spade.

This reduces the number of deals we could get. As there are six options for the face up card and 5 of the only 23 remaining cards for the hand:

$$
D=6*\binom{23}{5}
$$

$$
6*\binom{23}{5} * (4+8) = 2.4\text{m}
$$

This reduces the number of infostates by 1/4 to 2.4m.

This change takes us from 7 hrs to 2 hrs -- almost achieving a linear speed up with respect to the reduction in the number of info states.

# Linear CFR

Linear CFR (LCFR) is well described in [Solving Imperfect-Information Games
via Discounted Regret Minimization](https://arxiv.org/pdf/1809.04040.pdf). The main challenge was making it performant in a multi-threaded environment. The naive approach to Linear CFR is to "multiply the accumulated regret by $\frac{t}{t + 1}$ where $t$ is the current iteration."

This approach creates significant read and write contention on our hashmap as every thread tries to update all elements of the hashmap at each iteration. Instead I store the last iteration a node was updated with each infostate.

And whenever a node is updated, I apply all of the discounting the node would have received in vanilla LCFR up to that point.

```rust
fn add_regret(infostate: &mut InfoState, action: Action, amount: f64, iteration: usize) {
    if feature::is_enabled(feature::LinearCFR)
        && infostate.last_iteration > 0
    {
        let lcfr_factor: f64 = (infostate.last_iteration..iteration)
            .map(|i| i as f64 / (i as f64 + 1.0))
            .product();
        infostate.regrets.iter_mut().for_each(|r| *r *= lcfr_factor);
    }

    infostate.last_iteration = iteration;

    infostate.regrets[action] += amount;
}
```

In theory, a later iteration could finish before an earlier iteration and touch the same node, but this is rare as each iteration touches a small fraction of the 2m istates. In practice, this effect did not negatively impact convergence times.

Linear CFR had no impact on training time to achieve parity with PIMCTS.

# Conclusion

Rust delivered on its promise to make it easy to convert my single-threaded code to parallel code. Combined with the normalization improvements, I can now train a CFR agent in 1-2 hours rather than a couple of days.

Linear CFR didn't improve the time to PIMCTS parity, but it may help with long-term convergence. In a future post, I'll compare the performance of bots trained with and without LCFR.
