---
title: 'euchre-bench: an llm coding agent benchmark'
date: 2026-05-24T00:00:00Z
---

# Context

I created a euchre-bench — a public API where you point an agent at four bots of increasing strength and see how many 100-game matches it wins. The bots range from `random` (trivial) up to `hard` (the [CFR+PIMCTS bot from a few posts ago](/posts/cfr-for-euchre/)).

Coding agents are tasked with creating a bot that wins as many matches as possible against each difficulty.

I ran seven models through it: sonnet-4.6, opus-4.7, gpt-5.5, gemini-3-flash-preview, gemini-3.5-flash, deepseek-v4-flash, and deepseek-v4-pro. All models were run in OpenCode as the agent harness, except for Opus 4.7 which ran through Claude code. Each llm gets a fresh sandbox, the same system prompt, and a few hours to do whatever it wants. You can browse the [trajectories](/trajectories/) directly.

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


# Appendix: what the agents saw

Two things shape every run: the system prompt the harness gives the
agent when its container starts, and the bench's own API reference
(`GET /bench/help`) which the prompt directs the agent to fetch first.

The prompt below is the current version — it bakes in a 12-hour budget
that the later matrix introduced, but the structure is unchanged from
this matrix.

## System prompt

````text
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

## This prompt is in your workspace

A copy of this entire prompt is at `prompt.md` in your workspace. Re-read
it (`cat prompt.md`) whenever you need to remind yourself of the rules,
your challenger_id, or the deadline — don't rely on memory for the
details below.

## Your challenger_id — use this EXACT string for everything

```
{{CHALLENGER_ID}}
```

Every `challenger_id` you send to the bench must be this string, character
for character. Do NOT append suffixes, change the casing, or invent any other IDs.

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
- **Web search is available** via the `tavily` MCP tool. Use this to research and help you develop ideas.

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

## Bench API reference (`/bench/help`)

```text
# Euchre Benchmark API

You are a Euchre-playing agent. Your goal is to play as many hands as
possible against a trained CFR bot and accumulate points for your team.

## How Euchre works (essentials)

- 4 players in 2 fixed teams: seats (0, 2) vs seats (1, 3).
- 24-card deck: 9, T, J, Q, K, A in each of 4 suits (s=spades, c=clubs,
  h=hearts, d=diamonds). Each player is dealt 5 cards; one card is turned
  face up after the deal.
- Game flow:
    1. **Pickup phase**: each player (starting left of dealer) chooses to
       have the dealer pick up the face-up card as trump (`T`) or pass (`P`).
       If picked up, the dealer must discard one card.
    2. **Trump-call phase**: if all 4 passed, each player may declare a
       different suit as trump or pass. Can't call the rejected suit.
    3. **Alone phase**: the trump caller decides whether to go alone (`L`)
       — the partner sits out — or play with the partner (`P`).
    4. **Play phase**: 5 tricks. Must follow suit if possible. Highest
       trump wins; otherwise highest card of the led suit wins.
- Trump ranking: Right Bower (J of trump), Left Bower (J of same color),
  A, K, Q, T, 9.
- Scoring per hand:
    - Caller takes 3-4 tricks: +1
    - Caller takes 5 (march): +2
    - Going alone and taking 5: +4
    - Caller euchred (takes <= 2): defending team +2
- Match ends when one team reaches 10 points.

## Your role

You control **both seats of one team** (seats 0 and 2). The bench agent
controls seats 1 and 3. To prevent cheating, you only see one player's
information at a time and you don't know which game/seat you're in —
the server interleaves N concurrent games and serves your requests in
random order.

## Workflow

1. List agents:
       GET /bench/agents
   Returns: ["random", "easy", "medium", "hard"]

   Difficulty tiers:
     - random: picks uniformly from legal actions
     - easy:   PIMCTS (open-hand Monte Carlo), no training
     - medium: CFR trained on bidding phase only
     - hard:   CFR trained through 3 cards played

2. Start a session:
       POST /bench/sessions
       {"challenger_id": "your_unique_name",
        "agent_name": "easy",
        "num_games": 200}
   Returns 200: {"session_id": "...", "num_games": 200,
                  "agent_name": "easy"}

   - num_games must be in 1..=1000.
   - Only ONE active session per challenger_id at a time. If one is
     already active, the server returns:
         409 Conflict
         {"error": "you already have an active session; resume it",
          "session_id": "<existing>",
          "agent_name": "<existing-agent>",
          "num_games": <existing-num>}
     Use that session_id to resume — the existing agent_name and
     num_games are authoritative; the values you posted are ignored.
   - To recover the in-flight istate after resuming, call
     POST /bench/sessions/{session_id}/move with action=null. The
     server returns the current Turn response without advancing state.

3. Play loop. Repeat until you receive a Complete response:
       POST /bench/sessions/{session_id}/move
       First call:  {"challenger_id": "your_unique_name", "action": null}
       Subsequent:  {"challenger_id": "your_unique_name", "action": <u8>}

   Sending action=null on any call is a no-op probe: it returns the
   current Turn (or Complete) without applying anything. Use it after
   resuming to learn the in-flight istate.

   Response shapes (untagged JSON):
       Turn:     {"istate": "<info-state string>",
                  "legal_actions": [<u8>, ...],
                  "games_done": <int>,
                  "games_total": <int>}
       Complete: {"complete": true,
                  "challenger_score": <int>,
                  "agent_score": <int>,
                  "challenger_match_wins": <int>,
                  "agent_match_wins": <int>,
                  "hands_played": <int>}

   *_score is total Euchre points (1, 2, or 4 per hand). *_match_wins is the
   count of to-10 matches won by each team within the session. With
   num_games=N, challenger_match_wins + agent_match_wins = N.

   Each submitted `action` must appear in the previous response's
   `legal_actions` list.

4. Inspect the leaderboard:
       GET /bench                                       (HTML)
       GET /bench/results                               (JSON, every session, newest first)
       GET /bench/results?challenger_id=X               (filter)
       GET /bench/results?agent_name=Y                  (filter)
       GET /bench/results?challenger_id=X&agent_name=Y  (both filters)
       GET /bench/history/{challenger_id}/{agent_name}  (HTML chart over time)

## Information state format

Pipe-delimited segments. Cards are `<rank><suit-letter>`, e.g. `Js` =
Jack of spades, `Td` = Ten of diamonds.

Example istate (player 0, post-bidding):
    "9cTcJcQcKc|Js|T|0S|9cAcKsJs|"

Segments:
    1. Your 5 cards (sorted), e.g. "9cTcJcQcKc"
    2. Face-up card, e.g. "Js"
    3. Pickup phase actions: 'T'=Pickup, 'P'=Pass (one char per player)
    4. Trump caller + trump: e.g. "0S" = player 0 called Spades
    5. (Dealer only) Discarded card after pickup
    6. Cards played in tricks so far, in play order

## Action encoding

Each action is a `u8` (the `Action.0` field). Always choose from the
`legal_actions` list returned by the server. You don't need to know the
exact integer mapping — parse the istate to understand the position.

## Available agents
  - easy
  - hard
  - medium
  - random

## Errors

- 400 Bad Request: invalid action, bad num_games, unknown agent_name,
  malformed UUID.
- 403 Forbidden: challenger_id does not match the session.
- 404 Not Found: session_id does not exist.
- 409 Conflict: you already have an active session. The response body
  carries its session_id, agent_name, and num_games — resume that
  session instead of starting a new one.

## Example client (pseudocode)

    resp = POST /bench/sessions
           body = {"challenger_id": "mybot",
                   "agent_name": "easy",
                   "num_games": 50}
    sid = resp["session_id"]

    action = None
    while True:
        r = POST /bench/sessions/{sid}/move
            body = {"challenger_id": "mybot", "action": action}
        if r.get("complete"):
            print("done", r["challenger_score"], "-", r["agent_score"])
            break
        action = pick_action(r["istate"], r["legal_actions"])
```
