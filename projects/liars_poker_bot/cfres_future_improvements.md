# CFRES Euchre — Future Performance Improvements

Status snapshot: 8 phases shipped, cumulative wall-clock on `test` profile **64.3s → ~33s (-48.7%, ~2x speedup)**. See `cfres_optimization_progress.md` for the per-phase log. This doc lists what's next, ranked by expected value / effort ratio on the `three_card_played` workload (user's primary target).

## Methodology reminders

Every item below has a **profile signal** from `profile_data/phase12-three_card.data`. Before starting any item, re-profile (the latest shipped profile is the baseline); after shipping, profile again and confirm the targeted frame shrank. Do not ship a change that doesn't shrink its target frame. The Phase 1 reset-wipe reversal is proof that intuition is fallible here.

For each item: confidence, expected self-time recovery, estimated effort, and the concrete file/line starting point.

---

## Tier 1 — Clean pattern-matches of existing wins (start here)

These are the cheapest remaining fixes. All apply the *same patterns* that worked in Phases 0.5–1.2.

### F1 — `get_n_highest_trump` bitmask rewrite

- **Where:** `crates/games/src/gamestates/euchre/processors.rs:85-118`
- **Signal:** 3.80% self-time in Phase 1.2 profile.
- **Why:** Same structural bug as the old `find_next_card_owner` (deleted in Phase 1.0). Calls `deck.get(*c)` inside a per-trump-card loop. `Deck::get` walks up to 10 card locations per call. With 6 trump cards × alpha_beta's hundreds of calls per rollout, this is pure waste.
- **Fix pattern (mirrors Phase 1.0):** Precompute per-location bitmasks (`player_hands[0..4]`, `played`, `faceup`, `none`) once at function entry, then for each trump card do a handful of bitmask ANDs to find the owner instead of calling `Deck::get`.
- **Callers to update?** No — `get_n_highest_trump` is internal to this file.
- **Expected:** ~3% wall-clock (high confidence: same class of fix that worked twice already).
- **Effort:** 30-60 min.

### F2 — `Deck::get` callers audit

- **Where:** anywhere `Deck::get` is still called in hot paths. Grep `deck.get(` to find them.
- **Signal:** `Deck::get` is inherently O(10) and shows up via its callers across the profile. Phase 0.5 fixed `Deck::set` the same way; `Deck::get` got missed.
- **Fix pattern:** For callers that only need "is card C in the player's hand", use `gs.deck.get_all(loc).contains(card)` instead (direct bitmask AND). For callers that actually need the location, consider a `Deck::locate_u8` helper that takes a bit index and scans 10 locations with a small loop — still O(10) but avoids repeated trait-call overhead if there's any.
- **Effort:** 1-2 hours (depending on how many callers).

### F3 — Collapse redundant `EAction::from`/`EAction::card` round-trips

- **Where:** `legal_actions_play` and `process_euchre_actions` call `EAction::from(action).card()` in loops. Every round-trip goes through the array lookup I added in Phase 0.7 — still fast but nonzero.
- **Signal:** `EAction::card` 1.38% + `Hand::card` 0.98% in Phase 1.2 profile.
- **Fix:** where we already have a `Card`, don't detour through `EAction`. Where we have an `Action`, consider caching the `Card` alongside if the same conversion is done multiple times per loop iteration.
- **Effort:** 1 hour.
- **Expected:** ~1-2%.

---

## Tier 2 — Algorithmic / data-structure changes (medium risk)

### F4 — `iso_deck` memoization on `EuchreGameState`

- **Where:** `crates/games/src/gamestates/euchre/isomorphic.rs:iso_deck` (called from `transposition_table_hash` at `euchre/mod.rs:1137`).
- **Signal:** 2.89% self in Phase 1.2 profile. Phase 0.8 already cut it in half by hoisting the key lookup once per alpha_beta frame; further reduction requires caching across frames.
- **Fix sketch:** Add `iso_deck_cache: Cell<Option<([u32; 4], u16)>>` (or similar) to `EuchreGameState` where `u16` is a mutation counter bumped on every `apply_action`/`undo`. `iso_deck` checks the counter, recomputes if stale. This gives a cache hit on the many consecutive alpha_beta frames that share the same trick state.
- **Risks:**
  - Grows `EuchreGameState` struct size → slower `Clone` (CFRES clones state per rayon task, and `OpenHandSolver::evaluate_player` clones the root).
  - Invalidation bugs are hard to debug. Must also invalidate on `undo`.
  - Interior mutability (`Cell`) means `transposition_table_hash(&self)` stays `&self`.
- **Expected:** 2-3%.
- **Effort:** 2-3 hours including careful invariant testing.

### F5 — Avoid `EuchreGameState::clone` in `OpenHandSolver::evaluate_player`

- **Where:** `crates/card_platypus/src/algorithms/open_hand_solver.rs:113-122` — `evaluate_player` clones the game state before passing to `mtd_search`.
- **Signal:** Not directly hot but every rollout in CFRES calls this, and `EuchreGameState::Clone` still heap-allocates `play_order: Vec<Player>` (though play_order is small).
- **Fix:** Plumb `&mut G` through `evaluate_player → mtd_search → alpha_beta` and rely on `undo()` to restore state. `alpha_beta` already uses apply/undo internally; the top-level clone is just paranoia.
- **Risks:** if `alpha_beta` ever panics mid-recursion the caller gets a corrupt state. Acceptable for a training loop that panics → aborts.
- **Expected:** 1-2%.
- **Effort:** 1 hour (trait signature update + callers).

### F6 — `play_order: Vec<Player>` → fixed-size array

- **Where:** `crates/games/src/gamestates/euchre/mod.rs:72` (`EuchreGameState` struct) and all callers of `play_order.push()` / `.pop()` / indexing.
- **Signal:** `EuchreGameState::Clone` allocates this Vec on every state clone; bounded max length is 5+4+1+2+20 = 32.
- **Fix:** Replace with `play_order: [Player; 32]` + `play_order_len: u8`, add helper methods. Struct becomes entirely `Copy`-like (the `Vec` is the only heap-allocated field, per earlier exploration).
- **Benefit:** `Clone` becomes a `memcpy` — important because F5 may not completely eliminate clones (e.g., CFRES `self.clone().iteration()` in the rayon path at `cfres.rs:244`).
- **Expected:** 1-3% (depends on how much cloning remains after F5).
- **Effort:** 2 hours.

### F7 — Memoize `istate_key` on `EuchreGameState`

- **Where:** `crates/games/src/gamestates/euchre/mod.rs:819-850` (`istate_key`).
- **Signal:** `istate_key` 2.42% + `normalize_euchre_istate` 1.17% + `norm_transform` 1.15% = **~4.7% combined** in Phase 1.2 profile.
- **Fix:** Similar to F4 — cache the last-computed istate_key per player with a mutation counter. CFRES calls `istate_key` at every decision node; many are on the same state (e.g., before and after a noop).
- **Caveat:** the original static-analysis plan called for deduping `istate_key` calls *within one `update_regrets` body* — simpler and lower-risk than full memoization. Consider that first: in `cfres.rs:307-327`, the istate key is computed once per call, then `normalize_action` re-computes it for every legal action via `normalize_euchre_istate`. Hoist the transform once per `update_regrets` frame.
- **Expected:** 2-3% (with the cheap hoist), possibly more with full memoization.
- **Effort:** 1-2 hours.

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

## Suggested order of operations

1. **F1 (`get_n_highest_trump`)** — pattern-match win, ~3% for ~1 hour, can ship today.
2. **F2 (`Deck::get` audit)** — cleanup from the same pattern, another 1-2%.
3. **F7 simple version** (hoist `normalize_action` transform in `update_regrets`) — ~2%, low risk.
4. **Profile break.** If cumulative wins in Tier 1 land us under 30s on the `test` profile (~53% vs baseline), stop and assess whether that's enough.
5. **F5 (remove `gs.clone()` in `evaluate_player`)** — ~1% for ~1 hour, low risk.
6. **F6 (`play_order` fixed array)** — small struct refactor, improves F5's ceiling.
7. **F4 (`iso_deck` memoization)** — interior-mutability gamble, 2-3% upside.
8. **Profile break.** Reassess whether further work is worth it for the user's real workload.
9. Only then consider F8/F9/F10.

## Hard constraints to remember

- Never `cargo build` while a training run is in progress — the on-disk binary gets replaced and `perf` can't symbolicate the running process (it shows up as `(deleted)`). Found this the hard way in Phase 0.
- `pass-on-bower-cfr-train --scoring-iterations N` is the number of pre-allocated scoring worlds, NOT a "score every N iterations" cadence. Passing a huge number allocates a huge `Vec<EuchreGameState>` and OOMs. Use ≤100 and control cadence via `--num-scoring-evaluations`.
- For three_card_played profiling, copy the indexer from `~/card_platypus/infostate.three_card_played_f32/indexer` into a temp dir — don't touch the real weight file.
- WSL2 memory cap is ~30 GB on this machine; don't run two concurrent three_card_played trainings.
- User prefers to run long training commands themselves; profiling commands (≤2 min) are fine to run automatically.
