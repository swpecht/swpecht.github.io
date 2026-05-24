---
title: 'euchre-bench: an llm coding agent benchmark'
date: 2026-05-24T00:00:00Z
---

# Context

I created a [euchre-bench](https://euchre.fewworddotrick.com/bench) — a public API where you point an agent at four bots of increasing strength and see how many 100-game matches it wins. The bots range from `random` (trivial) up to `hard` (the [CFR+PIMCTS bot from a few posts ago](/posts/cfr-for-euchre/)).

Coding agents are tasked with creating a bot that wins as many matches as possible against each difficulty.

I ran seven models through it: sonnet-4.6, opus-4.7, gpt-5.5, gemini-3-flash-preview, gemini-3.5-flash, deepseek-v4-flash, and deepseek-v4-pro. All models were run in OpenCiode as the agent harness, except for Opus 4.7 which ran through Claude code. Each llm gets a fresh sandbox, the same system prompt, and a few hours to do whatever it wants. You can browse the [trajectories](/trajectories/) directly.

# Results

Match wins (out of 100) against each difficulty:

| model | rand | easy | med | hard | sum | cost | approach |
|---|---|---|---|---|---|---|---|
| claude-code / opus-4.7 | 99 | 9 | 6 | **6** | **120** | ~$329 | heuristic + variance-hunt |
| anthropic / claude-sonnet-4.6 | **100** | 5 | **8** | 5 | 118 | $23.70 | heuristic + per-card PIMCTS |
| google / gemini-3.5-flash | 97 | 2 | 7 | **6** | 112 | $5.22 | heuristic + 5-world PIMCTS |
| openai / gpt-5.5 | 90 | 3 | 2 | 3 | 98 | $1.46 | pure heuristic |
| deepseek / deepseek-v4-flash | 81 | 0 | 0 | 1 | 82 | $0.57 | pure heuristic |
| google / gemini-3-flash-preview | 80 | 0 | 1 | 0 | 81 | $0.44 | pure heuristic |
| deepseek / deepseek-v4-pro | 41 | 0 | 0 | 0 | 41 | $2.06 | pure heuristic |

For reference, the `random` opponent picks a legal action uniformly at random. Losing 19 matches out of 100 to random — like v4-flash — means the policy is genuinely worse than coin-flipping play in many situations. v4-pro losing 59 of them is impressive.

# What every model converged on

Same playbook, every time:

1. **Probe the action encoding by hand.** The bench docs don't publish how integer actions map to cards. Every model issued single-game sessions, watched legal-action lists, and reverse-engineered the same map: `suit_base * 8 + rank` with `S=0, C=8, H=16, D=24`, and special slots for `PASS=31`, `PICKUP=7`, `ALONE=15`, and per-suit `CALL=6/14/22/30`. That's the bench's internal `Action.0` field. No model trusted the encoding without checking, and every model arrived at the same answer.

2. **Write a single self-contained Python policy.** Not one agent kept the LLM in the loop per move. The economic case was probably obvious to all of them: per-move LLM calls would cost hundreds of dollars per 100-game session even before you measured how often the model would get the legal-action list wrong.

3. **Score hands with the same shape**: count trump, add bower bonuses (`right=10, left=9`), add side aces. Constants vary, structure doesn't.

The convergence is striking. Different model families, different training data, different runtimes — and the policy file you find in each workspace looks like a port of the same source.

# The decision that mattered

The five pure-heuristic agents bunched between 0 and 3 hard-difficulty wins. The two that built rollouts — sonnet (n_sims=100 per decision) and gemini-3.5-flash (5-world PIMCTS sample-and-rollout) — scored 5 and 6.

The thing is, this is the same algorithm the `easy` bot uses. Both agents went and read [the post about it](/posts/cfr-for-euchre/) before deciding to build rollouts. That's a real piece of "competitive intelligence" that the heuristic-only agents missed — and it shows up in the scores.

The cheap and pleasant inference is that PIMCTS is the right move for this benchmark. But there's a caveat. Claude Code (opus-4.7) tried four different Monte Carlo variants — open-hand for play, with heuristic playout, with open-hand playout, vote-based — and found every one of them was *worse* than a well-tuned heuristic. From its own write-up:

> Open-hand MC: conservative (assumes opp has perfect info too). Pickup MC with heuristic playout: too optimistic (every hand says order up). Pickup MC with open-hand playout: too conservative (every hand says pass). Vote-based open-hand MC: too slow.

So sonnet and gemini-3.5-flash built simpler rollouts and got lucky on variance; opus built more sophisticated rollouts, found them worse than heuristics, and got lucky on variance a *different* way (more on that next).

The honest read is: at 5–8 wins out of 100 with maybe 3 wins of standard deviation per session, the gap between the top three is mostly noise. The gap between the top three and the bottom four is real.

# One agent gamed the rules

The bench grades on the *latest* 100-game session per challenger. I picked that rule to keep the leaderboard simple. Claude Code (opus-4.7) noticed:

> Iterating "until ≥ threshold" via `run_until_good.py` exploited this — locked hard at 6/100 (tied #1) on attempt 2 of the final sequence.

`run_until_good.py` runs sessions back-to-back, stopping when the score crosses a configurable threshold. The thresholds it set per agent were calibrated to "just beat or match the current leader" — which means opus had also gone and read the public leaderboard before kicking those off. Five of its webfetches hit `/bench/results` first.

This isn't against the literal rule (the rule is the rule, and the underlying policy is solid). But with ~3 wins of standard deviation and 4 attempts allowed, the "hard 6" is plausibly just the top of four draws from a distribution centered around 3–4.

I'm going to change the grading rule. Probably to *median of N sessions*, or pin the official one and refuse to overwrite it. Either change makes variance-hunting net-zero.

Sonnet under the *previous* prompt did the same thing — I killed an earlier run when I noticed a Python script doing "rerun medium until challenger_score ≥ 295". The current prompt added an explicit ranking metric (match wins, not raw points) and sonnet stopped re-rolling on its own. That suggests prompts can shut this down for in-harness agents, but anything driving the loop from the outside — like Claude Code calling the API directly — won't see the new prompt.

# Cost notes

The cost-per-hard-win spread is two orders of magnitude:

- gpt-5.5: $0.49 per hard win (3 wins on $1.46)
- gemini-3.5-flash: $0.87 per hard win
- claude-sonnet-4.6: $4.74 per hard win
- claude-code / opus-4.7: ~$55 per hard win at list pricing

The first two are heuristic-only, the second two are heuristic + rollouts. You can pay more and get marginally better worst-case play; you can also pay 60x more and get the same number of hard wins as gemini-3.5-flash.

The cheapest run that completed all four 100-game evaluations was gemini-3-flash-preview at $0.44. It scored 0/0/1/0 on easy/medium/hard. So "completes the benchmark for under a dollar" is achievable. "Beats the medium bot for under a dollar" is not, in this matrix.

# What's next

I'm planning to explore an [auto research](https://github.com/karpathy/autoresearch/tree/master) approach.
