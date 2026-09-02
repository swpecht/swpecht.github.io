---
title: 'euchre-bench, take two: more time, more tools, mixed results'
date: 2026-05-25T00:00:00Z
---

# What happened

In the [previous post](/posts/llm-agents-play-euchre/), seven LLM coding agents each got a fresh sandbox, no web access, and a few hours to build a euchre bot. Nearly all of them converged on the same hand-scoring heuristic, none of them beat the `hard` bot more than 6 times in 100, and the agent that finished first got there partly by re-running the benchmark until it drew a good score.

So I took another crack at things. This time, I ran eight LLM coding agents through the euchre benchmark with a Tavily web-search tool, 12-hour budget, and prompt instructing them to iterate aggressively. (`hard` is the API's name for the strongest of the four opponents; I call it *difficult* in the prose below.)

The top score came from GPT-5.5, which took the simplest successful approach with a stateless, 447-line heuristic. Its advantage came largely from running 100-game sessions against `medium` 50 times and locking in the high draw. The benchmark grades whatever session you ran most recently, and any single session is noisy, so running it over and over and stopping after a good one is worth several wins on its own. That's a hole in my grading rule, not a euchre strategy — more on it below.

At the other end of the spectrum, Gemini-3.5-flash built the most sophisticated policy in the matrix — a real 40-world PIMCTS, or *perfect-information Monte Carlo tree search*: guess the hidden cards 40 different ways, solve each guess with an alpha-beta search as if every hand were face up, and vote on the best move — and won just 2 of its 100 matches against the difficult opponent.

Another success turned out to be sort of a happy accident. Minimax-m2.7 won 95 of its 100 matches against the random opponent, but none at all against easy, medium, or difficult. Its trump-call branch has the wrong action codes, which forces an "always order up first round" policy that random can't punish.

Auto-research itself worked — 38 Tavily calls, and two Geminis treated OpenSpiel source as reference docs — but it didn't translate into better play.

# What changed

The harness — the sandbox each agent runs in — gave every agent the following:

1. **Web search tool**: Every agent had access to a [Tavily MCP](https://tavily.com/) server with `tavily_search`, `tavily_extract`, and `tavily_research`. They could now look up euchre strategy guides, scan GitHub for action-encoding hints, and conduct deeper research.
2. **A 12-hour wall-clock budget**: This round, agents had substantially more time to test and revise their policies.
3. **An auto-research-style prompt**: Agents were given explicit instructions to keep iterating until the budget runs out, record metrics in commit messages, and use the web search tool when stuck. Each agent also worked in its own fresh git repo, so its commit history reads as a log of what it tried. See the full text in the [appendix below](#appendix-system-prompt).

# Results

Match wins against each difficulty (100 games per match):

| Model | Random | Easy | Medium | Difficult | Sum | Cost | Approach |
|---|---|---|---|---|---|---|---|
| OpenAI / GPT-5.5 | **100** | **23** | **9** | **8** | **140** | $16.45 | Three-threshold heuristic + 50x medium variance hunt |
| Google / Gemini-3.5-flash | 96 | 6 | 3 | 2 | 107 | $34.47 | Real PIMCTS: 40-World alpha-beta Solver with TT cache |
| Minimax / Minimax-m2.7 | 95 | 0 | 0 | 0 | 95 | $13 | Always-pickup; trump-call branch broken (wrong codes) |
| Google / Gemini-3-flash-preview | 88 | 2 | 1 | 2 | 93 | $7.28 | Class-based heuristic, Next-suit bonus |
| DeepSeek / DeepSeek-v4-flash | 82 | 6 | 0 | 1 | 89 | $3.12 | Always-call + offense/defense lead routing |
| Moonshotai / Kimi-K2.6 | 78 | 2 | 0 | 1 | 81 | $11.51 | Aggressive bidder (`score >= 0.5`) |
| DeepSeek / DeepSeek-v4-pro | 65 | 0 | 0 | 0 | 65 | $10.47 | Function-decomposed heuristic + 20-rollout follow |
| Qwen / Qwen3.7-max | 12 | — | 0 | 0 | 12 | $154 | Broken decoder (`min/max(legal_actions)` as proxy) |

Total spend was $251.

(Cost is token count × OpenRouter’s published per-model pricing. Most rows match what OpenCode wrote to the trajectory; Qwen3.7-max and Minimax-m2.7 don't have a discounted cache-read rate published, so cache reads on those two are billed at the full prompt rate — OpenCode’s stream had treated them as free, understating Qwen by ~5.7x and Minimax by ~2.6x.)

[Browse the trajectories.](/trajectories/)

# Auto-research worked, mostly

Every model used Tavily at least once, although usage varied substantially.

| Model | Tavily calls | What they searched |
|---|---|---|
| Gemini-3-flash-preview | 12 | OpenSpiel `euchre.cc`, `kPass` / `kPickup` / `kAlone`, action values |
| Gemini-3.5-flash | 12 | OpenSpiel `euchre.cc`, `kCallSpades`, `NumDistinctActions` |
| Kimi-K2.6 | 7 | Euchre strategy guides, Monte Carlo papers |
| DeepSeek-v4-flash | 3 | safeharborgames.net Euchre column (extract) + 2 strategy searches |
| Others | 1 each | Generic strategy queries |

The models made 38 Tavily calls in total. The two Gemini models went straight for the bench’s underlying source — OpenSpiel’s `euchre.cc`, which defines the same action encoding as the benchmark. None returned actual policy code; OpenSpiel is enum definitions and game logic, not strategies.

# How GPT-5.5 took the top spot

GPT-5.5 did the *least* research and produced the simplest policy — and still finished first. Its `euchre_bot.py` is 447 lines of stateless heuristic: there is no card counting, no rollouts, and no opponent inference. It scores hands with a three-threshold formula (`pickup=70, call=70, alone=85`, tunable per agent), plays partner-save and beater-search in trick play, and that’s it.

What it *did* do, 50 times, is run another 100-game session against a `medium` opponent. Its `notes.md:36` literally reads "Medium variance chase at 65/65/80 regressed to 2/100..." — and the distribution speaks for itself:

| Agent | 100-game sessions | Distribution | What shipped |
|---|---|---|---|
| Random | 4 | 90, 100, 99, 100 | 100 |
| Easy | 4 | 14, 11, 15, 23 | **23** (max of sample) |
| Medium | **50** | min 0, max 9, mean ~3.7 | **9** (max of sample) |
| Difficult | 15 | min 2, max 8, mean ~4.5 | **8** (= max of sample) |

The benchmark grades the newest session per agent. So if you run a 100-game match enough times and stop on a good draw, your best-ever session is the one that counts. Approximately 3 wins of standard deviation per session x 50 attempts means that the maximum is 3–4 wins above the mean by construction. GPT-5.5 found this and exploited it across two agents at once.

The next version of the benchmark will grade on the median of N sessions, which removes this exploit.

# Deep dive: Gemini-3.5-flash

Of the eight models, only Gemini-3.5-flash built proper PIMCTS rollouts in production: 40 worlds, an alpha-beta open-hand solver, a transposition-table cache (`agent.py:284`). A total of 712 lines of Python that look like real game-AI code: `sample_opponent_hands` constructs hands consistent with observed voids (line 452), `evaluate_hand_for_trump` (line 481) gives bowers 1.0/0.5/0.3 and off-aces 0.8, pickup/call thresholds 3.5/2.8 with a Next-suit bonus, alone at 4.5 with >=4 trump. Cache cleared between moves, so it doesn't grow unbounded.

It's the most sophisticated thing in the matrix, and it scored 2 wins out of 100 on difficult. It cost $34 to build.

There are three things going on:

1. **Sample-of-2.** Despite the 12-hour budget, Gemini-3.5-flash only ran 2 medium 100-game sessions and 3 difficult. With ~3 wins of standard deviation, the right tail of 2 draws is barely above the mean. GPT-5.5 ran 50 mediums. Sampling matters more than algorithm at this win count.
2. **Algorithmic ceiling is real.** PIMCTS still has the strategy-fusion problem I [wrote about back in 2023](/posts/cfr-for-euchre/) — it can pick a different “best” move per sampled world, treating its information as perfect, but in the real game has to commit to one. A total of 40 worlds and a perfect solver per world do not make this go away.
3. **Tool spend went elsewhere.** Of Gemini's 12-hour budget, 12 Tavily + 26 webfetch + 39 write + 19 edit + 51 read is a lot of research-and-edit overhead. Implementing PIMCTS chewed through hours that GPT-5.5 spent on variance hunting.

Trajectory: [google__gemini-3.5-flash_20260524](/trajectories/google__gemini-3.5-flash_20260524/) — search for "N_WORLDS" or "PIMCTS" in the event stream to find when the algorithm landed.

# Deep dive: Minimax-m2.7

Minimax scored 95 random wins and 0 against every other difficulty. The split is almost entirely an accident.

`workspace/euchre_bot_final.py` evaluates the hand and then decides in three branches: order up the face-up card, name a different suit as trump, or go alone. The first branch works — `should_pickup` (line 94) orders up whenever the bot holds the right bower plus two more trump, or three trump of any kind.

**The second branch never runs.** To name a suit, the bot has to send the benchmark an integer code for that suit, and line 150 has the wrong codes: `{'s':15, 'h':10, 'd':12, 'c':16}`, where the real ones are 6/14/22/30 (15 is "go alone"). Line 152 checks the chosen code against the list of legal moves the server sent back, and since these codes are never in that list, the branch silently falls through to `return int_actions[-1]` — usually `31`, pass.

So, in effect: Minimax orders up in the first round whenever the heuristic says “yes,” and passes in the second round always. There's a second, similar mix-up in trick play at line 185, where the bot sends the position of a card in its hand instead of the card's code. That only happens to be right for the lowest cards in some suits.

Why does this beat random 95/100? Because the random opponent can't punish over-bidding. Calling trump on a mediocre hand is normally how you get euchred: the defenders take three tricks and score two points. A random player almost never manages that, so the usual cost of bidding too often disappears. If you order up on anything reasonable and play *any* coherent trump strategy, you'll make your point most hands. Random will take 0–1 tricks per hand on its own. Minimax made 95 random wins not by playing well, but rather by playing aggressively into a nondefensive opponent.

The 0/100 across easy/medium/difficult confirms it. The instant that the opponent can punish a player who calls trump on hands they can't make, Minimax goes to zero.

The space of “policies that work against random” is wide and most of it isn’t actually euchre strategy.

Trajectory: [minimax__minimax-m2.7_20260524](/trajectories/minimax__minimax-m2.7_20260524/) — the 42 versioned `euchre_bot_v*.py` files in the workspace are worth a look. The model knew it was iterating on the trump-call branch and never noticed the codes were wrong.

# What worked and what didn't

What worked:

- Way more commits, way more versioned policy files in each workspace (Minimax: 42, Kimi: 32, Qwen: 53). The 12-hour budget let models iterate without time pressure.
- Two Geminis spent a sustained budget on reading OpenSpiel’s source as a substitute for figuring out the action encoding from probing.
- The total cost was $251, with Qwen3.7-max alone burning $154 on 50M cache-read tokens billed at the full prompt rate (see cost note above).

What didn’t:

- The winner is still a stateless heuristic. None of the algorithm work — PIMCTS, opponent inference, rollouts — made it into the top of the leaderboard.
- The grading exploit (variance-hunt under newest-counts) got *more* effective with more wall time, not less.
- No model self-discovered the strategy-fusion problem that the easy bot already demonstrates. Tavily searches found generic strategy guides and OpenSpiel source, neither of which would tell you about it. The post about it [on this blog](/posts/cfr-for-euchre/) is the only place the easy bot's weakness is laid out, and no model webfetched a fewworddotrick URL.

# What’s next

The most important change is the grading fix. Switching to median-of-N=5 for the next matrix. That removes the GPT-5.5 variance hunt.

I want to see whether Gemini-3.5-flash’s real PIMCTS holds up under median grading; my guess is yes, but probably not by much.

Tavily will stay. Two real searches per model is fine; the overhead isn’t bad.

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
