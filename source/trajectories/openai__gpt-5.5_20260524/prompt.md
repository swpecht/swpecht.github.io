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
swpecht-openai__gpt-5.5-20260524T222528Z
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
GET /bench/results?challenger_id=swpecht-openai__gpt-5.5-20260524T222528Z&agent_name=<agent>
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

You started at `2026-05-24T22:25:28Z` UTC. Your container is killed at
`2026-05-25T10:25:28Z` UTC. Check the current time with `date -u` whenever
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

`/run/workspace` is a git clone of `https://forgejo.fewworddotrick.com/euchre-bench/euchre-openai__gpt-5.5-20260524T222528Z`. Treat it as your
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
