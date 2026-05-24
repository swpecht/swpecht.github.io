# Euchre Benchmark

You're playing the Euchre benchmark at `https://euchre.fewworddotrick.com`.
Fetch `/bench/help` first — it documents the rules, the API, the istate
format, and the action encoding.

## Your goal — beat the opponent in as many matches as possible

A "match" inside a session is a to-10-points game. With `num_games=100` your
session contains 100 matches, and `challenger_match_wins + agent_match_wins`
adds up to 100. **We rank you on `challenger_match_wins` against each of the
four opponent difficulties** — i.e. your session win rate. `challenger_score`
(raw Euchre points) and per-hand outcomes are useful development signals,
but they don't decide the ranking.

## Your challenger_id — use this EXACT string for everything

```
swpecht-anthropic__claude-sonnet-4.6-20260522T194600Z
```

Every `challenger_id` you send to the bench must be this string, character
for character. Do NOT append suffixes, change the casing, or invent any
other `swpecht-*` IDs.

## What counts as a final result

Run as many sessions as you want with this challenger_id to experiment —
small `num_games`, repeated tries against the same difficulty, different
strategies side by side. They cost nothing other than your tokens, and none
of them are graded.

The grader looks at one thing only:

```
GET /bench/results?challenger_id=swpecht-anthropic__claude-sonnet-4.6-20260522T194600Z&agent_name=<agent>
```

It takes the **single newest session** for each of the four agents
(`random`, `easy`, `medium`, `hard`). For that session to count it must
have `num_games=100`. Everything else is invisible to the ranking.

So when you're done iterating: make sure the **last** session you ran
against each difficulty is a 100-game one. That's all that matters.
