# CFRES Euchre — Future Performance Improvements

Status snapshot: **15 phases shipped (8 in first batch + 7 in F-series), cumulative wall-clock on `test` profile 64.3s → ~31.5s (-51%, just over 2x).** See `cfres_optimization_progress.md` for the per-phase log. This doc lists what's next, ranked by expected value / effort ratio on the `three_card_played` workload (user's primary target).

## Already done (this session)

The first eight phases shipped in the prior commit `76ce507`. The F-series shipped in this round:

- **F1** — `get_n_highest_trump` bitmask rewrite.
- **F2** — `iso_deck` Deck::get → precomputed bitmask check.
- **F7 simple** — reuse `normalized_actions` in CFRES regret/avg-strat loops.
- **F5** — `OpenHandSolver::evaluate_player_mut` avoids per-rollout clone.
- **F8** — eliminate `Vec<Card>` collect in `evaluate_trick`.
- **F9** — `Hand::card`/`cards`/`highest` const lookup table.
- **F10** — `push_hand_as_actions` direct bit iteration in `legal_actions_play`.

The pre-shipped fix for `Deck::set` already absorbed the dealing-phase `Deck::get` callers' would-be wins, so F2's actual change ended up being just `iso_deck`. F4 (iso_deck memoization) was investigated and **declined** — analysis showed the cache would rarely hit because `transposition_table_hash` is called once per alpha_beta frame and each frame visits a unique state. F6 (`play_order` fixed array) was **declined** for now: ~20 caller updates for a sub-1% expected win after F5 eliminated the per-rollout clone.

## Methodology reminders

Every item below has a **profile signal** from `profile_data/phase12-three_card.data`. Before starting any item, re-profile (the latest shipped profile is the baseline); after shipping, profile again and confirm the targeted frame shrank. Do not ship a change that doesn't shrink its target frame. The Phase 1 reset-wipe reversal is proof that intuition is fallible here.

For each item: confidence, expected self-time recovery, estimated effort, and the concrete file/line starting point.

---

## Tier 1 — done

All Tier 1 items from the previous version of this doc are shipped (F1, F2, F7 simple). F3 (`EAction::from(action).card()` round-trip cleanup) was made obsolete by F10's direct bit iteration, which avoids the round-trip entirely in the hot caller (`legal_actions_play`).

---

## Tier 2 — done or declined

- **F4** (`iso_deck` memoization) — declined. Analysis: `transposition_table_hash` is called once per alpha_beta frame, each frame visits a unique state, so the cache would rarely hit.
- **F5** (remove `gs.clone()` in `evaluate_player`) — shipped via `evaluate_player_mut` inherent method.
- **F6** (`play_order` fixed array) — declined. ~20 caller updates for a sub-1% expected win after F5 eliminated the per-rollout clone. Reconsider only if F4's reasoning is invalidated (e.g., a future change re-introduces hot cloning).
- **F7** (`istate_key` memoization) — partial. F7 simple (the cheap hoist version) shipped. Full struct-level memoization with a mutation counter not done — would reduce `istate_key` further but adds invariants to maintain.

---

## Tier 2.5 — Bonus opportunities found this round

These came up while drilling into the F1–F10 profile data. They're below the 1% wall-clock threshold and require care.

### B1 — Specialize `process_euchre_actions` early-out

- **Where:** `crates/games/src/gamestates/euchre/processors.rs:64-72` (`process_play_actions`).
- **Signal:** `process_euchre_actions` is now ~9% self in F10 profile. Most of the cost is `evaluate_highest_trump_first` + `remove_equivlent_cards`, both of which precompute hand masks even when the action list is small.
- **Idea:** when the action list has ≤2 entries, neither helper can do meaningful pruning — early-return without doing the bitmask setup. Also: skip `evaluate_highest_trump_first` when not at start of trick.
- **Expected:** 1-2%.
- **Effort:** 1 hour.

### B2 — Inline `Hand::card` callers that already know the card

- Found while looking at F9: several `Deck::face_up()` / `Deck::played(player)` callers do `Hand::card()` to extract the single card. Since the caller often knows it expects a card to be present, the `Option<Card>` round-trip and length check are wasted work.
- **Effort:** 1 hour, sub-1% win.

### B3 — Pack `iso_deck` mask building into a single SWAR pass

- `iso_deck` (F2 version) does ~24 `loc_word` calls each branching through 6 hand checks. A SWAR (SIMD-within-a-register) approach could compute several cards in parallel via wider bitmask ops, but the data layout doesn't trivially permit it.
- **Effort:** experimental, several hours.

---

## Tier 3 — Larger rewrites (only after Tier 1/2 diminishing returns)

### F8 — AlphaBetaCache: bounded LRU instead of periodic full-wipe

- **Signal:** Phase 1 reversal proved that the full-wipe-every-100-iterations pattern is surprisingly effective because it keeps the working set L-cache-friendly. But the wipes themselves cost time (DashMap::clear walks all shards) and throw away *some* still-useful entries.
- **Fix:** swap `DashMap<_, _, FxBuildHasher>` for a bounded concurrent cache like `quick_cache` or `mini-moka` with a size cap of ~5-10M entries. Evicts cold entries automatically while keeping the hot working set resident.
- **Risks:** new dependency, more complex semantics, may or may not beat the current wipe-every-N approach. Must A/B against the current setup.
- **Expected:** uncertain — could be anywhere from 0% to ~5%. Only worth doing if a profile shows `DashMap::_get` is still dominant after Tier 1.
- **Effort:** 3-4 hours (integration + A/B testing + tuning the size cap).

### F9 — `apply_action` / `undo` restructuring for the Play phase

- **Where:** `crates/games/src/gamestates/euchre/mod.rs:apply_action` / `undo` — specifically `apply_action_play` and its `undo` counterpart.
- **Signal:** `apply_action` 10.44% + `undo` 8.35% + `Deck::set` 2.31% = **~21%** in Phase 1.2 profile. Biggest remaining subsystem.
- **Why it's hard:** these functions are inherently busy work — advancing/reversing a game state. The structural overhead is the phase-dispatch `match` (3.14% branch mispredict rate) and the various `Deck::set` calls.
- **Ideas (exploratory):**
  - Specialize the Play-phase hot path into dedicated `apply_play_card` / `undo_play_card` functions that skip the phase match and go directly to the bitmask update. Callers who know they're in the Play phase (alpha_beta's inner loop) call those directly.
  - Consolidate the multiple `Deck::set` calls in `apply_action_play` that reset played cards at trick end. Currently loops through 4 players and does `Deck::set(c, None)` per card; could be a single bulk operation that sets all four Played locations to 0 and the corresponding None bits.
  - Replace `trick_winners: [Player; 5]` bookkeeping with a compact bitfield.
- **Expected:** 3-5% but highly uncertain without experimentation.
- **Effort:** 1-2 days, risk of breaking `test_undo_is_inverse_of_apply_all_games`.

### F10 — `alpha_beta` algorithmic improvements

- **Where:** `crates/card_platypus/src/algorithms/open_hand_solver.rs:alpha_beta`.
- **Signal:** `alpha_beta` body 12.20% in Phase 1.2.
- **Ideas:**
  - **Principal variation search (PVS / NegaScout)** — after the first move, use a zero-width window to verify vs. refute, re-search only on fail-high. Typical 10-30% alpha-beta speedup in double-dummy bridge solvers.
  - **Better move ordering** — Phase 1.0 kept the existing `evaluate_highest_trump_first` heuristic, but we could also use the cached best-action from a previous alpha_beta call as the PV move.
  - **Stronger MTD-f first guess** — `mtd_search` starts with `first_guess = 0`, which means it often needs multiple tightening iterations. Seeding from the last-seen solution for similar states could converge faster.
- **Risks:** algorithmic changes need correctness testing against the existing tests (especially `test_alg_open_hand_solver_euchre` which is currently ignored due to a going-alone bug — fixing *that* bug is prerequisite work).
- **Expected:** 5-15% if done right, but high risk.
- **Effort:** 3-5 days.

---

## Not worth doing (explicitly ruled out)

- **NodeStore Mutex sharding** — original plan's Phase 3. Contention is only ~2.5% of total now. The invasive mmap-sharding work doesn't justify that return.
- **CFRES recursion-level allocation cleanup** — the ideas from the pre-profile plan (dedup istate_key, hoist normalizer clone, kill `collect_vec` in `update_regrets`). F7 absorbs the most valuable piece (hoist normalize_action); the rest is sub-1% each.
- **Replace `SeqCst` with `Relaxed` on iteration counter** — 1-3% in the original plan's estimate but under <1% in reality. Leave as-is for safety.
- **Add rustc-hash to games crate** — the inline mix in Phase 1.1 already solved the transposition hash cost. No new dep needed.

---

## Suggested next steps (post F1-F10)

The cheap wins are exhausted. Each remaining option requires either algorithmic depth or invasive struct changes:

1. **B1 (`process_euchre_actions` early-out)** — 1 hour, ~1-2% upside, if you want to keep nibbling.
2. **F8 (`alpha_beta` PVS / NegaScout)** — biggest potential algorithmic win (5-15%) but several days of work. Prerequisite: fix the currently-ignored `test_alg_open_hand_solver_euchre` going-alone test so PVS changes can be validated.
3. **F9 (`apply_action` / `undo` Play-phase specialization)** — 3-5% but high risk of breaking `test_undo_is_inverse_of_apply_all_games`.
4. **Profile a fresh `three_card_played` capture** before any further work to confirm the new top frames after F1-F10 — what was hot in earlier profiles may have shifted.

## Hard constraints to remember

- Never `cargo build` while a training run is in progress — the on-disk binary gets replaced and `perf` can't symbolicate the running process (it shows up as `(deleted)`). Found this the hard way in Phase 0.
- `pass-on-bower-cfr-train --scoring-iterations N` is the number of pre-allocated scoring worlds, NOT a "score every N iterations" cadence. Passing a huge number allocates a huge `Vec<EuchreGameState>` and OOMs. Use ≤100 and control cadence via `--num-scoring-evaluations`.
- For three_card_played profiling, copy the indexer from `~/card_platypus/infostate.three_card_played_f32/indexer` into a temp dir — don't touch the real weight file.
- WSL2 memory cap is ~30 GB on this machine; don't run two concurrent three_card_played trainings.
- User prefers to run long training commands themselves; profiling commands (≤2 min) are fine to run automatically.
