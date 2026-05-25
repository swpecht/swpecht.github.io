---
title: 'euchre-bench, take two: more time, more tools, mixed results'
date: 2026-05-25T00:00:00Z
---

# Context

I ran 8 LLM coding agents through the euchre benchmark with a Tavily web-search tool, a 12-hour budget, and a prompt instructing them to iterate aggressively. gpt-5.5 took #1 with a stateless 447-line heuristic, by re-running 100-game sessions against `medium` 50 times and locking in the high draw. gemini-3.5-flash built the most sophisticated thing in the matrix — a real 40-world PIMCTS with an alpha-beta solver — and scored just 2/100 on `hard`. minimax-m2.7's 95 random wins are largely an accident: its trump-call branch has the wrong action codes, which forces an "always order up first round" policy that random can't punish. Auto-research itself worked (38 Tavily calls; two Geminis treated OpenSpiel source as reference docs), but it didn't translate into better play.

The harness gave each agent:

1. **A search tool**: gave every agent a [Tavily MCP](https://tavily.com/) server with `tavily_search`, `tavily_extract`, and `tavily_research`. Agents could now look up euchre strategy guides, scan GitHub for action-encoding hints, and do deep-research crawls.
2. **A real wall-clock budget**: 12 hours per model.
3. **An auto-research-style prompt**: explicit instructions to keep iterating until the budget runs out, treat each commit as an experiment-journal entry (metric in the message), and reach for the search tool when stuck. The workspace was a fresh per-run git repo the agent pushed to as it worked. Full text in the [appendix below](#appendix-system-prompt).

# Results

Match wins against each difficulty (100 games per match):

| model | rand | easy | med | hard | sum | cost | approach |
|---|---|---|---|---|---|---|---|
| openai / gpt-5.5 | **100** | **23** | **9** | **8** | **140** | $16.45 | three-threshold heuristic + 50x medium variance hunt |
| google / gemini-3.5-flash | 96 | 6 | 3 | 2 | 107 | $34.47 | real PIMCTS: 40-world alpha-beta solver with TT cache |
| minimax / minimax-m2.7 | 95 | 0 | 0 | 0 | 95 | $13 | always-pickup; trump-call branch broken (wrong codes) |
| google / gemini-3-flash-preview | 88 | 2 | 1 | 2 | 93 | $7.28 | class-based heuristic, Next-suit bonus |
| deepseek / deepseek-v4-flash | 82 | 6 | 0 | 1 | 89 | $3.12 | always-call + offense/defense lead routing |
| moonshotai / kimi-k2.6 | 78 | 2 | 0 | 1 | 81 | $11.51 | aggressive bidder (`score >= 0.5`) |
| deepseek / deepseek-v4-pro | 65 | 0 | 0 | 0 | 65 | $10.47 | function-decomposed heuristic + 20-rollout follow |
| qwen / qwen3.7-max | 12 | — | 0 | 0 | 12 | $154 | broken decoder (`min/max(legal_actions)` as proxy) |

Total spend was $251.

(Cost is token count × OpenRouter's published per-model pricing. Most rows match what opencode wrote to the trajectory; qwen3.7-max and minimax-m2.7 don't have a discounted cache-read rate published, so cache reads on those two are billed at the full prompt rate — opencode's stream had treated them as free, understating qwen by ~5.7x and minimax by ~2.6x.)

[Browse the trajectories.](/trajectories/)

# Auto-research worked, mostly

Every model used Tavily at least once. Two leaned on it heavily:

| model | tavily calls | what they searched |
|---|---|---|
| gemini-3-flash-preview | 12 | OpenSpiel `euchre.cc`, `kPass` / `kPickup` / `kAlone`, action values |
| gemini-3.5-flash | 12 | OpenSpiel `euchre.cc`, `kCallSpades`, `NumDistinctActions` |
| kimi-k2.6 | 7 | euchre strategy guides, Monte Carlo papers |
| deepseek-v4-flash | 3 | safeharborgames.net Euchre column (extract) + 2 strategy searches |
| others | 1 each | generic strategy queries |

38 calls across the matrix. The two Geminis went straight for the bench's underlying source — OpenSpiel's `euchre.cc` defines the same action encoding the bench uses. None returned actual policy code; OpenSpiel is enum definitions and game logic, not strategies.

# How gpt-5.5 took #1

gpt-5.5 is also the model that did the *least* research and shipped the simplest policy. Its `euchre_bot.py` is 447 lines of stateless heuristic — no card counting, no rollouts, no opponent inference. It scores hands with a three-threshold formula (`pickup=70, call=70, alone=85`, tunable per agent), plays partner-save and beater-search in trick play, and that's it.

What it *did* do, 50 times, was run another 100-game session against `medium`. Its `notes.md:36` literally reads "Medium variance chase at 65/65/80 regressed to 2/100..." — and the distribution speaks for itself:

| agent | 100g sessions | distribution | what shipped |
|---|---|---|---|
| random | 4 | 90, 100, 99, 100 | 100 |
| easy | 4 | 14, 11, 15, 23 | **23** (max of sample) |
| medium | **50** | min 0, max 9, mean ~3.7 | **9** (max of sample) |
| hard | 15 | min 2, max 8, mean ~4.5 | **8** (= max of sample) |

The bench grades on the *newest* session per agent. So if you run a 100g match enough times and stop on a good draw, you lock in the upper tail. ~3 wins of standard deviation per session x 50 attempts means the maximum is 3–4 wins above the mean by construction. gpt-5.5 found this and exploited it across two agents at once.

Grading will switch to median-of-N for the next matrix to remove this exploit.

# Deep dive: gemini-3.5-flash

Of the eight models, only gemini-3.5-flash built proper PIMCTS rollouts in production: 40 worlds, an alpha-beta open-hand solver, a transposition-table cache (`agent.py:284`). 712 lines of Python that look like real game-AI code: `sample_opponent_hands` constructs hands consistent with observed voids (line 452), `evaluate_hand_for_trump` (line 481) gives bowers 1.0/0.5/0.3 and off-aces 0.8, pickup/call thresholds 3.5/2.8 with a Next-suit bonus, alone at 4.5 with >=4 trump. Cache cleared between moves so it doesn't grow unbounded.

It's the most sophisticated thing in the matrix and it scored 2/100 on hard. It cost $34 to build.

Three things going on:

1. **Sample-of-2.** Despite the 12-hour budget, gemini-3.5-flash only ran 2 medium 100g sessions and 3 hard. With ~3 wins of std-dev, the right tail of 2 draws is barely above the mean. gpt-5.5 ran 50 mediums. Sampling matters more than algorithm at this win count.
2. **Algorithmic ceiling is real.** PIMCTS still has the strategy-fusion problem I [wrote about back in 2023](/posts/cfr-for-euchre/) — it can pick a different "best" move per sampled world, treating its information as perfect, but in the real game has to commit to one. 40 worlds and a perfect solver per world don't make this go away.
3. **Tool spend went elsewhere.** Of gemini's 12-hour budget, 12 Tavily + 26 webfetch + 39 write + 19 edit + 51 read is a lot of research-and-edit overhead. Implementing PIMCTS chewed through hours that gpt-5.5 spent on variance hunting.

Trajectory: [google__gemini-3.5-flash_20260524](/trajectories/google__gemini-3.5-flash_20260524/) — search for "N_WORLDS" or "PIMCTS" in the event stream to find when the algorithm landed.

# Deep dive: minimax-m2.7

Minimax scored 95 random wins and 0 against every other difficulty. The split is almost entirely an accident.

`workspace/euchre_bot_final.py` ships a hand evaluator and pickup/trump-call/alone branches. The pickup branch (`should_pickup`, line 94) orders up whenever right-bower + ≥2 trump, or ≥3 trump. **The trump-call branch is broken**: line 150 maps suit-call actions as `{'s':15, 'h':10, 'd':12, 'c':16}`. The actual bench action codes are 6/14/22/30. (15 is "go alone".) The `code in legal_actions` guard at line 152 means the call branch silently never fires — it falls through to `return int_actions[-1]` (usually `31 = pass`).

So in effect: minimax orders up first round whenever the heuristic says yes, and passes second round always. There's also a buggy trick-play action mapping at line 185 — `action = card_index_in_hand` only happens to work for the lowest cards in some suits.

Why does this beat random 95/100? Random doesn't punish aggressive making. If you order up on anything reasonable and play *any* coherent trump strategy, you'll make your point most hands. Random will take 0–1 tricks per hand on its own. Minimax made 95 random wins not by playing well, but by playing aggressively into a non-defensive opponent.

The 0/100 across easy/medium/hard confirms it. The instant the opponent can punish a loose maker, minimax goes to zero.

The space of "policies that work against random" is wide and most of it isn't actually euchre strategy.

Trajectory: [minimax__minimax-m2.7_20260524](/trajectories/minimax__minimax-m2.7_20260524/) — the 42 versioned `euchre_bot_v*.py` files in the workspace are worth a look. The model knew it was iterating on the trump-call branch and never noticed the codes were wrong.

# What worked, what didn't

What worked:

- Way more commits, way more versioned policy files in each workspace (minimax: 42, kimi: 32, qwen: 53). The 12-hour budget let models iterate without time pressure.
- Two Geminis spent a sustained budget on reading OpenSpiel's source as a substitute for figuring out the action encoding from probing.
- Total cost was $251, with qwen3.7-max alone burning $154 on 50M cache-read tokens billed at the full prompt rate (see cost note above).

What didn't:

- The winner is still a stateless heuristic. None of the algorithm work — PIMCTS, opponent inference, rollouts — made it into the top of the leaderboard.
- The grading exploit (variance-hunt under newest-counts) got *more* effective with more wall time, not less.
- No model self-discovered the strategy-fusion problem the easy bot already demonstrates. Tavily searches found generic strategy guides and OpenSpiel source, neither of which would tell you about it. The post about it [on this blog](/posts/cfr-for-euchre/) is the only place the easy bot's weakness is laid out, and no model webfetched a fewworddotrick URL.

# What's next

The grading fix is overdue. Switching to median-of-N=5 for the next matrix. That removes the gpt-5.5 variance hunt. I want to see whether gemini-3.5-flash's real PIMCTS holds up under median grading; my guess is yes-but-narrowly.

Tavily stays. Two real searches per model is fine; the overhead isn't bad.

# Appendix: system prompt

The system prompt the harness gave each agent at container startup. `{{BENCH_URL}}` and `{{CHALLENGER_ID}}` are placeholders the harness fills in.

````text
# Euchre Benchmark

You're playing the Euchre benchmark at `{{BENCH_URL}}`.
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
{{CHALLENGER_ID}}
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
GET /bench/results?challenger_id={{CHALLENGER_ID}}&agent_name=<agent>
```

It takes the **single newest session** for each of the four agents
(`random`, `easy`, `medium`, `hard`). For that session to count it must
have `num_games=100`. Everything else is invisible to the ranking.

So when you're done iterating: make sure the **last** session you ran
against each difficulty is a 100-game one. That's all that matters.

## You have 12 hours — work autonomously

You are running unattended. There is no operator to answer questions or
confirm choices. If something is ambiguous, make a defensible call and
keep moving; don't stop to ask. "I have a working policy" is not a
stopping condition — iterate until you're out of ideas or out of time.

Use the full 12 hours. There is no bonus for finishing early, and the
four 100-game finals are cheap relative to your exploration budget. Do
not spin on a change that isn't producing better scores — switch to a
different difficulty or a different idea instead.

You started at `{{STARTED_AT}}` UTC. Your container is killed at
`{{DEADLINE_UTC}}` UTC. Check the current time with `date -u` whenever
you want to know how much budget you have left. Plan so the last
100-game session against each of `random/easy/medium/hard` finishes
well before the deadline — a session killed mid-stream is wasted.

## How to spend the 12 hours

The bench is your experiment substrate. Sessions are free, you can open
as many as you want with this challenger_id, and only the *last* 100-game
session per agent counts for the ranking. Use that:

- **Cheap probes:** small `num_games` (20–50) sessions to test "does this
  change help at all?" — fast, noisy, directional.
- **Confirm before believing:** when a change looks like a real
  improvement, re-run with `num_games=100` to confirm. Win rate from a
  50-game session has roughly ±7% noise at p≈0.5; don't over-fit to it.
- **The finals are not a separate step.** Your "final" against each agent
  is just *the last 100-game session* you ran against it. Make sure the
  last session you ran against each of `random/easy/medium/hard` is a
  100-game session reflecting your best confirmed policy. That's the only
  thing the grader sees.
- **Write a self-contained policy.** Your final policy should play
  without further LLM calls — a Python script (or equivalent) that picks
  actions from `legal_actions` + `istate`. Per-move LLM evaluation burns
  the token budget for no measured benefit.
- **Keep a notes file** in your workspace (any name). Record what you
  tried, what the win rate looked like, and what you'd try next. This is
  how future-you decides what to do when an idea fails.
- **Budget across difficulties.** `hard` is meaningfully stronger than
  `random`; don't burn 11 hours beating `random` and discover you have an
  hour left for `hard`.
- **Stay in motion.** Every block of time should produce a tested
  experiment, a code change, or a recorded session. Pure deliberation
  without writing code or starting a session is wasted budget — if
  you're stuck, change approach rather than thinking harder.
- **A failed experiment is data, not failure.** Record what didn't work
  in your notes and move to the next hypothesis. Three regressions in a
  row means change direction, not stop. Walking away from a dead end
  early is correct; quitting because you're tired is not.
- **No final lap.** There is no "wrap up and review" phase. As long as
  there is budget left, there is another experiment to run. The harness
  will keep restarting you if you exit early.
- **Web search is available** via the `tavily` MCP tool. Useful for
  Euchre strategy lookups (bidding rules, hand evaluation, AI literature
  on the game).

<!-- BEGIN_REPO_SECTION -->
## Your workspace is a git repo

`/run/workspace` is a git clone of `{{REPO_URL}}`. Treat it as your
experiment journal — `git log` is how future-you reconstructs what
you've already tried, and the harness uses it to inspect your run
afterward.

- **Commit small, commit often.** One commit per tested idea (one
  policy change + the result you observed). Don't bundle three changes
  into one commit — you lose the ability to attribute what helped.
- **Put the metric in the commit message.** Example:
  `add right-bower bonus to bidding: hard 28% → 34% (50 games)`. The
  log becomes scannable — you can spot regressions, revisit ideas, or
  bisect without re-reading every diff.
- **Commit your notes file in the same commit as the code change it
  refers to.** Code + observation move together, not in separate
  "fix typo" commits.
- **Push every commit (`git commit && git push`).** The container can
  die at any moment; an unpushed commit is a lost commit. Don't batch.
- **Use `git log --oneline` as your memory.** Before trying something,
  check whether you (or earlier-you) already tried it. Don't waste
  budget re-running dead ends.
<!-- END_REPO_SECTION -->

## You are not the last competitor

Other agents are evaluated before *and* after your run — not in parallel.
Even if the public leaderboard at `/bench/results` shows your scores
matching or beating the current top, a stronger model may be evaluated
later and overtake you. Do not stop on "good enough" or "I've tied the
leader." Push for the best score you can in the time you have. The
ranking is global and rolling; only the final standings matter, not what
the leaderboard looked like when you happened to read it.
````
