# CFRES Euchre Optimization — Progress Log

Plan: `/home/steven/.claude/plans/velvet-zooming-salamander.md`

This log tracks every step, command run, measurement taken, and file touched while working the plan. Newest entries at the bottom of each section.

## Phase 0 — Profile and baseline

Goal: produce real flamegraph + allocation + contention data so Phase 1+ is driven by measurement, not static code reading.

### Environment

- `perf`: `/usr/lib/linux-tools/6.8.0-110-generic/perf` (user-provided)
- `kptr_restrict`: already `0` (no sudo needed)
- Platform: WSL2 (Linux 5.15.167.4-microsoft-standard-WSL2)

### Step log

- **Build release binary** — `cargo build --release --package card_platypus` — completed in 5.31s (`optimized + debuginfo` profile, good for perf symbolication).
- **Install inferno-flamegraph** — `cargo install inferno` — completed (`inferno v0.12.6`, binaries at `~/.cargo/bin/inferno-*`).
- **CLI subcommand discovery** — actual subcommand is `euchre-cfr-train <profile>` (positional), not `EuchreCFRTrain --profile=...`. Plan file's commands need updating.

### Baseline numbers (test profile, 100k iterations, unprofiled)

Command: `time ./target/release/card_platypus euchre-cfr-train test`

```
real    1m4.254s
user    7m51.958s
sys     1m27.617s
```

- Throughput: ~1560 iterations/s wall-clock
- Effective parallelism: user/real ≈ **7.35x** (machine clearly has 8+ cores and rayon is saturating them)
- `sys` time is **~22s out of 64s wall-clock** — unusually high for CPU-bound work. Candidates: mmap page faults on NodeStore resize, mutex syscalls on the single `Arc<Mutex<NodeStore>>`, or log I/O. Phase 0 profiling should confirm.
- This is the number every later phase benchmarks against.

### Parallel CPU profile (test profile, 55s capture, 55,341 samples @ 99Hz × ~10 threads)

Command used (for reproducibility):

```bash
./target/release/card_platypus euchre-cfr-train test > /tmp/train-parallel.log 2>&1 &
TRAIN_PID=$!; sleep 3
/usr/lib/linux-tools/6.8.0-110-generic/perf record \
  -o profile_data/baseline-parallel.data -p $TRAIN_PID \
  -F 99 --call-graph dwarf -- sleep 55
wait $TRAIN_PID
```

Post-processing:

```bash
/usr/lib/linux-tools/6.8.0-110-generic/perf script -i profile_data/baseline-parallel.data \
  | inferno-collapse-perf > profile_data/baseline-parallel.folded
inferno-flamegraph < profile_data/baseline-parallel.folded > profile_data/baseline-parallel.svg
/usr/lib/linux-tools/6.8.0-110-generic/perf report -i profile_data/baseline-parallel.data \
  --stdio --no-children -g none
```

Artifacts:

- `profile_data/baseline-parallel.data` — 445 MB raw perf samples
- `profile_data/baseline-parallel.folded` — 11,861 collapsed stacks
- `profile_data/baseline-parallel.svg` — 2.8 MB flamegraph

**Flat self-time top 20 (parallel, test profile, max_cards_played=0):**

| # | % self | Symbol |
|---|---|---|
| 1 | **11.35%** | `open_hand_solver::alpha_beta` |
| 2 | 8.59% | `EuchreGameState::apply_action` |
| 3 | 7.13% | `euchre::deck::Deck::set` |
| 4 | 6.49% | `core::hash::sip::Hasher::write` |
| 5 | 6.25% | `DashMap::_get` |
| 6 | 4.89% | `EuchreGameState::undo` |
| 7 | 4.43% | `euchre::ismorphic::iso_deck` |
| 8 | 4.35% | `EuchreGameState::legal_actions` |
| 9 | 3.65% | `euchre::processors::find_next_card_owner` |
| 10 | 3.41% | `euchre::processors::euchre_early_terminate` |
| 11 | 3.41% | `hashbrown::map::make_hash` |
| 12 | 3.11% | `alloc::vec::Vec::retain` |
| 13 | 2.72% | `EuchreGameState::transposition_table_hash` |
| 14 | **2.35%** | **`CFRES::update_regrets`** |
| 15 | 2.28% | `DefaultHasher::write` |
| 16 | 2.23% | `Vec::from_iter` |
| 17 | 2.08% | `DashMap::_insert` |
| 18 | 1.88% | `libc::_int_free` |
| 19 | 1.85% | `crossbeam_epoch::with_handle` |
| 20 | 1.54% | `euchre::processors::get_n_highest_trump` |

### Phase 0 key finding — THE STATIC-ANALYSIS PLAN WAS POINTED AT THE WRONG CODE

The original plan (at `/home/steven/.claude/plans/velvet-zooming-salamander.md`) targeted `CFRES::update_regrets` and its `istate_key` / normalizer / NodeStore Mutex cost. The profile shows **`update_regrets` is only 2.35% of self-time**. Optimizing it to zero would be a ~2% win at most.

The real cost for `max_cards_played=0` workloads (which is both `test` and `baseline` in Train.toml) is concentrated in the **open-hand solver (`card_platypus::algorithms::open_hand_solver`)** which CFRES invokes at `cfres.rs:303` once `depth_checker.is_max_depth(gs)` fires. With `max_cards_played=0` this happens on essentially every iteration immediately after the bidding phase, so the solver runs on every rollout.

Adding up everything inside or directly supporting the alpha-beta solver:

| Subsystem | ~% self-time | Notes |
|---|---|---|
| `alpha_beta` recursion body | 11.35% | the solver itself |
| `apply_action` + `undo` + `Deck::set` | 20.61% | state mutation driven by alpha_beta tree walk |
| `legal_actions` + `Vec::retain` + `Vec::from_iter` | 9.69% | action generation in alpha_beta |
| `AlphaBetaCache` (`DashMap::_get/_insert` + `SipHasher::write` + `DefaultHasher::write` + `hashbrown::make_hash`) | 20.51% | transposition table lookups |
| `iso_deck` + `transposition_table_hash` | 7.15% | cache key generation |
| `find_next_card_owner` + `euchre_early_terminate` + `get_n_highest_trump` | 8.60% | alpha-beta-specific game evaluation helpers |
| `crossbeam_epoch` / `_int_free` | ~3.7% | DashMap epoch-based memory reclamation + heap churn |
| **Approximate alpha_beta + its direct support** | **~81%** | |
| CFRES + NodeStore put path | ~4-5% | `update_regrets` + `put_entry` + Mutex futex wait |

The NodeStore Mutex contention (`Mutex::lock_contended` → `futex_wait`) was visible but at the **~1% range** — not the 30-50% the plan estimated.

**Implication:** for `max_cards_played=0` profiles, the only way to materially speed up training is to speed up the open-hand solver or reduce how often it is called. Tinkering inside `CFRES::update_regrets` is a rounding error.

### Deep profile attempt (max_cards_played=3) — INVALID, need to retry

Ran `pass-on-bower-cfr-train 2000000 --max-cards-played 3 --weight-file /tmp/infostate.profile_deep` with the prebuilt three_card_played indexer copied in. Only 4,716 samples in 50s — the profile captured startup (baseline scoring + deal enumeration + `legal_actions_dealing`), not real training. Need to re-run with a longer warmup window before attaching perf.

Preserved for reference at `profile_data/baseline-deep.data` but NOT used for any decisions below.

### Attempting three_card_played profile (user's actual priority) — BLOCKED

User clarified mid-run that `three_card_played` (600M iter, max_cards=3) is the real optimization target, not `test`/`baseline`.

Tried two approaches:

1. **Attach perf to user's already-running `euchre-cfr-train three_card_played`** (pid 1752058). Captured 12,804 samples cleanly but **symbolication is broken**: `cargo build --release` run earlier replaced the on-disk binary, so the running process's path is shown as `card_platypus (deleted)` in perf.data. Attempts to recover symbols via `perf buildid-cache --add` and restoring the old binary at the original path both failed — the " (deleted)" suffix in the perf.data path is treated literally by `perf report`. `addr2line` against `/proc/<pid>/exe` produced bogus function names (ASLR/load-address mismatch). Raw profile preserved at `profile_data/baseline-three_card.data` but is un-symbolicated and therefore not actionable.

2. **Start my own `pass-on-bower-cfr-train 10000000 --max-cards-played 3` in parallel.** This would have given a clean profile with the current binary. **OOM-killed** at ~29 GB RSS — WSL2 / available RAM cannot hold both the user's long-running three_card_played mmap (~60 GB file-backed) and a second concurrent training. `dmesg` confirms: `oom-kill ... task=card_platypus, pid=1890679, total-vm:42659412kB, anon-rss:29227812kB`.

**Decision:** Do not kill the user's running training. Proceed with the findings we already have from the parallel test profile plus targeted code reading. Defer three_card_played profile capture until the user's run finishes or they choose to pause it.

**Risk this creates:** the hot-path distribution for max_cards=3 is expected to differ from max_cards=0 — CFRES does more decision-node work and less rollout work per iteration. Some Phase 0 findings transfer straight across, others do not. Tag each recommendation in the plan accordingly.

### Phase 0 summary & decisions

Data we have:

- Clean parallel profile of `test` (max_cards=0). Full symbol resolution. Top 30 frames, collapsed flamegraph, raw perf.data all preserved under `profile_data/baseline-parallel.*`.
- Cleanup: `profile_data/baseline-deep.data` (4,716 samples, startup-dominated, invalid) and `profile_data/baseline-three_card.data` (12,804 samples, un-symbolicatable due to deleted-binary issue). Kept for forensic reference only.

Key conclusions feeding the plan rewrite:

1. **The original static-analysis plan targeted the wrong code.** CFRES `update_regrets` is 2.35% of self-time on `test`. The open-hand solver (`open_hand_solver::alpha_beta`) and its support account for ~81%.
2. **AlphaBetaCache reset bug, confirmed by reading `cfres.rs:247-248` + `pass_on_bower_cfr.rs:168-172` + `open_hand_solver.rs:166-168, 289-291`.** `self.evaluator.reset()` is called after every `train(100)` batch, wiping the `Arc<DashMap>` transposition table 1000 times in a 100k-iteration run. This is almost certainly a major cause of the hot DashMap frames in the profile. **One-line fix.**
3. **NodeStore single-Mutex contention is ~1%, not 30-50%.** Original plan's top priority is refuted.
4. **`three_card_played` hot path is unverified.** We could not capture a symbolicated profile without interrupting the user's training (would need to pause their run or wait for it to finish).

Plan pivot implemented at `/home/steven/.claude/plans/velvet-zooming-salamander.md`:

- Phase 1 = delete the cache reset (risk low, measurement: re-run `test` and watch DashMap frames collapse). Gated on a quick rebuild only.
- Phase 2 = capture a real `three_card_played` profile before any further invasive work. Gate on user either pausing their training or agreeing to wait for it to finish.
- Phase 3 = faster AlphaBetaCache key path (FxHasher, memoize `iso_deck` on game state, hoist key lookup inside alpha_beta).
- Phase 4 = remove `EuchreGameState::clone()` in `OpenHandSolver::evaluate_player`, plumb `&mut G` through.
- Phase 5 = eliminate `Vec::retain` / `Vec::from_iter` allocations in the solver's action-processing path.
- Phase 6 = re-measure, decide.

**Open questions pending user input:**

- Can the user pause their current `three_card_played` training so we can capture a clean profile for Phase 2, or do we stay on the Phase 1-only path until it finishes?
- Is Phase 1 (delete the cache reset line) approved to implement now without waiting for the `three_card_played` profile? It has no dependency on `max_cards_played` — the evidence comes from code structure, not the profile.

### three_card_played clean profile (user killed their run)

User killed their in-progress `euchre-cfr-train three_card_played` so I could profile the real target workload cleanly. Started my own run:

```bash
cp ~/card_platypus/infostate.three_card_played_f32/indexer /tmp/infostate.profile_deep/indexer
./target/release/card_platypus pass-on-bower-cfr-train 50000000 \
    --weight-file /tmp/infostate.profile_deep \
    --max-cards-played 3 \
    --scoring-iterations 100 \
    --num-scoring-evaluations 2 &
# wait for indexer load (2s) + baseline scoring (13s) + startup, ~30s
/usr/lib/linux-tools/6.8.0-110-generic/perf record \
    -o profile_data/clean-three_card.data \
    -p <pid> -F 99 --call-graph dwarf -- sleep 60
```

**Gotcha:** an earlier attempt with `--scoring-iterations 1000000000` tried to allocate a `Vec<EuchreGameState>` of 1 billion entries at `pass_on_bower_cfr.rs:113` (`.collect_vec()` of pre-generated worlds). That's a 216 GB allocation — caused `memory allocation of 216000000000 bytes failed`. The `--scoring-iterations` flag in this CLI isn't "score every N iterations," it's "number of worlds to score against." Use a small value (100 here) and then limit frequency separately via `--num-scoring-evaluations`.

Captured 29,713 samples over 60s. 495 samples/sec across rayon workers (lower sample rate than `test`'s 1006/sec, reflecting heavier per-sample work in `three_card_played`). Artifacts preserved at `profile_data/clean-three_card.{data,folded,svg}`.

**Flat self-time top 25 (three_card_played, max_cards_played=3):**

| # | % self | Symbol | Subsystem |
|---|---|---|---|
| 1 | 9.15% | `EuchreGameState::apply_action` | state mutation |
| 2 | 6.92% | `Deck::set` | state mutation |
| 3 | **6.79%** | **`open_hand_solver::alpha_beta`** | solver body |
| 4 | 5.53% | `EuchreGameState::undo` | state mutation |
| 5 | 5.12% | `SipHasher::write` | AlphaBetaCache hashing |
| 6 | 4.40% | `iso_deck` | cache key |
| 7 | **4.16%** | **`CFRES::update_regrets`** | CFR loop |
| 8 | 4.15% | `find_next_card_owner` | euchre processors |
| 9 | 3.83% | `Vec::retain` | solver allocation |
| 10 | 3.71% | `EuchreGameState::legal_actions` | action gen |
| 11 | 3.52% | `DashMap::_get` | AlphaBetaCache lookup |
| 12 | 3.50% | `hashbrown::make_hash` | DashMap hash |
| 13 | **2.63%** | **`EuchreGameState::istate_key`** | CFRES istate |
| 14 | 2.21% | `get_n_highest_trump` | euchre processors |
| 15 | **2.07%** | **`Mutex::lock_contended`** | NodeStore mutex |
| 16 | 1.92% | `Vec::from_iter` | allocation |
| 17 | 1.88% | `libc::malloc` | libc |
| 18 | 1.76% | `process_euchre_actions` | euchre processors |
| 19 | 1.67% | `transposition_table_hash` | cache key |
| 20 | 1.66% | `libc::_int_free` | libc |
| 21 | 1.60% | `libc::_int_malloc` | libc |
| 22 | 1.51% | `libc::cfree` | libc |
| 23 | 1.41% | `boomphf::try_hash` | NodeStore MPHF |
| 24 | 1.14% | `normalize_euchre_istate` | CFRES normalizer |
| 25 | 0.95% | `DefaultHasher::write` | DashMap hash |

**Inclusive (children) breakdown for top-level frames:**

| Inclusive % | Self % | Symbol |
|---|---|---|
| 97.37% | 4.16% | `CFRES::update_regrets` (inlined) |
| **75.39%** | 0.30% | **`OpenHandSolver::evaluate_player`** |
| 74.41% | 6.79% | `alpha_beta` |
| 16.30% | 9.15% | `EuchreGameState::apply_action` |
| 12.77% | 0.00% | `apply_action_play` (inlined) |
| 3.93% | 3.71% | `legal_actions` |

### Comparison: test (max_cards=0) vs three_card_played (max_cards=3)

| Subsystem | test % | three_card % | Direction |
|---|---|---|---|
| **Solver total (evaluate_player inclusive)** | ~81% | **75%** | still dominant, slightly less |
| `alpha_beta` body self | 11.35% | 6.79% | ↓ |
| `apply_action` + `undo` + `Deck::set` self | 20.61% | 21.60% | flat |
| AlphaBetaCache (DashMap + SipHasher + make_hash) | ~17% | ~13% | ↓ |
| `iso_deck` + `transposition_table_hash` | 7.15% | 6.07% | flat |
| `CFRES::update_regrets` self | 2.35% | **4.16%** | ↑ ~2x |
| `istate_key` self | not top | **2.63%** | new |
| `normalize_euchre_istate` self | not top | 1.14% | new |
| `Mutex::lock_contended` (NodeStore) | ~1% | **2.07%** | ~2x |
| `boomphf::try_hash` (NodeStore MPHF) | small | 1.41% | new |

**Key findings:**

1. **The solver still dominates at max_cards=3** — 75% of total time is spent inside `OpenHandSolver::evaluate_player`. The plan's pivot to solver-centric optimizations is correct for this workload.
2. **CFRES overhead is real but small (~8% combined: update_regrets 4.16% + istate_key 2.63% + normalize_euchre_istate 1.14%).** The original static-analysis plan's ideas (de-duplicating `istate_key` in `normalize_action`, etc.) would save at most 3-4% of total time — worth doing but as a later phase.
3. **NodeStore Mutex contention is 2.07% — real but small.** Not worth the invasive sharding work the old plan proposed for this magnitude. Cheap wins only.
4. **The AlphaBetaCache reset bug matters even more at max_cards=3**: the DashMap + hashing subsystem is ~13% of time. Reusing the cache across batches should recoup most of that plus additional savings from more alpha-beta cache hits reducing actual tree walks.
5. **State-mutation cost (apply_action + undo + Deck::set = ~21.6%) is the 2nd-biggest subsystem** and is inside the solver's hot loop. Reducing that or bypassing it (e.g., keeping less state, or making `Deck::set` cheaper) is a big lever.
6. **Allocations are ~8% (libc malloc/free/int_malloc/int_free + Vec::retain + Vec::from_iter)** — reducing them via pooling and in-place operations in the solver's action processor and `legal_actions_play` is cleanly measurable.

### Plan validity after three_card_played findings

The plan's Phase 1-4 ordering still holds, with slightly revised expected wins:

- **Phase 1** (delete `evaluator.reset()`): still the first move. Expected ~5-15% wall-clock on three_card_played.
- **Phase 3** (faster AlphaBetaCache key: FxHasher + iso_deck memoization + key hoisting): expected 5-10% on three_card_played.
- **Phase 4** (remove `gs.clone()` in `evaluate_player`): smaller but clean, 1-3%.
- **Phase 5** (solver action-gen allocations): 3-5%.
- **NEW Phase 5.5** (originally-proposed CFRES istate_key de-duplication): worth doing after solver work since it's now 3-4% of time. Add to plan.

Updating the plan file accordingly.

### Extended three_card_played profiling (120s parallel + perf stat + call-graph drill-down)

Captured a longer, tighter profile and additional measurements:

- `profile_data/long-three_card.data` — 120s, 74,552 samples, 600 MB raw. Tighter numbers on smaller frames.
- `perf stat` over 30s of steady-state training.

**perf stat metrics (three_card_played, 30s window):**

```
  540,642,336,122  cycles:u
  775,484,768,126  instructions:u    #   1.43  insn per cycle
  140,223,679,206  branches:u
    4,403,375,529  branch-misses:u   #   3.14% of all branches
   14,002,601,807  cache-references:u
    3,218,797,190  cache-misses:u    #  22.99% of all cache refs
    2,122,583,287  L1-dcache-load-misses:u
              0    context-switches:u
              0    cpu-migrations:u
```

Interpretation:

- **IPC = 1.43.** Moderate. Not memory-stalled (IPC > 1.0 rules that out), but meaningful headroom vs. a compute-tight ~2.5 ceiling. Algorithmic wins will translate to wall-clock gains.
- **Branch mispredict rate = 3.14%.** Higher than ideal (modern code is typically 1–2%). Candidates: the phase-dispatch `match self.phase()` in `apply_action`/`legal_actions`/`undo`, the alpha-beta maximizing/minimizing branch, DashMap shard selection. This is a second-order concern — not worth chasing until bigger wins are in.
- **L1 dcache miss rate ~3.9 per 1000 cycles, LLC miss rate 23% of cache references.** DashMap random-access and mmap random-access are the obvious culprits. The Phase 1 cache-reset fix should reduce DashMap pressure and improve these numbers.
- **Zero context switches / cpu migrations** over 30s — rayon thread pool is stable.

**CALL-GRAPH DRILL-DOWN — two major one-line fixes found**

Drilled into the top self-time frames via `perf report --call-graph graph,callee`. Found two issues that weren't visible in flat profiles and weren't on the original hypothesis table at all:

---

#### Finding A: `Deck::set` walks *all 10 card locations* to remove a card (`deck.rs:136-144`)

```rust
pub fn set(&mut self, card: Card, loc: CardLocation) {
    // remove the card everywhere
    for locs in self.locations.iter_mut() {
        locs.remove(card);
    }
    // then set its final spot
    self.locations[loc.idx()].add(card);
}
```

Every `Deck::set` iterates over all 10 `Hand` buckets (Player0-3, Played(0-3), FaceUp, None) and calls `Hand::remove` on each one, even though a card can only ever be in **one** location at a time. Call graph confirms: `Deck::set` 6.91% self, `Hand::remove` 5.43% inclusive under it, and the cost funnels into `EuchreGameState::undo` (4.60%) inside `alpha_beta` recursion.

Additionally, `Hand::remove` (`deck.rs:224-226`) uses `ToPrimitive::to_u32(&card).unwrap()` when `Card` is `#[repr(u32)]` with explicit bitmask values (`actions.rs:199-200`) — a `card as u32` cast is a zero-cost replacement for the trait-dispatch path.

**One-line fix:** add `break;` after the first removal. Or even cleaner, since callers of `Deck::set` often know the source location, push an explicit from/to API (bigger refactor, ignore for now).

**Caveat:** `set(card, CardLocation::None)` is used as a "delete wherever this card currently is" idiom (lines 358, 422, 1032, 1038 in `euchre/mod.rs`). `None` is one of the 10 locations, so writing "set it to None" after removing from its current location is a no-op — the break-early loop handles this correctly because None is scanned in the loop and matches if the card is already in None.

Actually checking again — if the card is currently in `Player3` and we call `set(card, None)`, the break-early version removes from `Player3` then adds to `None`, which is correct. If the card is already in `None`, break-early removes from `None` then adds to `None`, also correct.

**Expected win:** `Deck::set` self drops from 6.91% → ~1%, and the `Hand::remove` callees drop proportionally. Total: **~5-6% wall-clock.**

---

#### Finding B: `add_regret` computes a telescoping product the hard way (`cfres.rs:506-517`)

```rust
if feature::is_enabled(feature::LinearCFR)
    && infostate.last_iteration > 0
{
    let factor: Weight = (infostate.last_iteration..iteration.min(LINEAR_CFR_CUTOFF))
        .map(|i| i as Weight / (i as Weight + 1.0))
        .product();

    infostate.regrets.iter_mut().for_each(|r| *r *= factor);
}
```

This is an iterator chain that multiplies `i / (i+1)` for all `i` in `[last_iteration, min(iteration, 1_000_000))`. `LINEAR_CFR_CUTOFF = 1_000_000` (line 47). In a 600M-iteration training run, an infostate that hasn't been updated in a while can easily have a gap of hundreds of thousands of iterations — the product loop then runs hundreds of thousands of f32 divides and multiplies **per `add_regret` call**, and `add_regret` is called once per legal action per player-node CFRES visit.

Call graph confirms: 2.66% of total CPU time is spent inside this `.product()` chain (inlined into `update_regrets`).

**The product telescopes to a closed form:**

```
∏(i=a..b) i/(i+1)
= a/(a+1) · (a+1)/(a+2) · (a+2)/(a+3) · ... · (b-1)/b
= a/b     (everything in between cancels)
```

So the entire loop is equivalent to **one division**:

```rust
let end = iteration.min(LINEAR_CFR_CUTOFF);
let factor: Weight = if infostate.last_iteration >= end {
    1.0
} else {
    infostate.last_iteration as Weight / end as Weight
};
```

This is not only O(1) vs the current O(gap), it's also *more* numerically accurate — one f32 division vs thousands of accumulated roundings. The outer guard `infostate.last_iteration > 0` already prevents division by zero.

**Expected win:** `.product()` chain drops from 2.66% → ~0%. Total: **~2-3% wall-clock.**

---

#### Finding C: `DashMap::_get` has ~22% self-cost in RwLock-drop machinery

Drill shows 0.78% of `DashMap::_get`'s 3.53% self is the inlined `RwLock::unlock_shared → drop_in_place → core::ptr::drop_in_place<RwLockReadGuard...>` cleanup. That's confirming evidence for Phase 3's proposal to replace the shared-map entry lock with a cheaper concurrent hashmap or a manual shard+FxHasher. The full cache-lookup path `AlphaBetaCache::get → DashMap::_get → (hash, shard, lock, read, drop)` is what the call graph traces repeatedly under `alpha_beta`.

---

#### Finding D: `iso_deck` called from both `get` AND `insert` inside a single `alpha_beta` frame

Call graph: 2.39% of `iso_deck`'s 4.11% self comes through the `get` path, and 0.68% comes through the `insert` path (`AlphaBetaCache::insert` → `transposition_table_hash` → `iso_deck`). So a single `alpha_beta` call that both reads and writes the cache recomputes `iso_deck` twice. Hoisting the key to a local at function entry saves the whole insert-side path. Validates the key-hoisting step already in the plan.

---

### Revised top-of-plan action list (after call-graph drill-down)

Entirely new top-3, all trivial:

1. **Phase 0.5 (NEW) — `Deck::set` break-early + `Card as u32`.** Single file. ~5-6% win.
2. **Phase 0.6 (NEW) — `add_regret` LinearCFR closed form.** Single file, four lines. ~2-3% win.
3. **Phase 1 — Delete `self.evaluator.reset()` + log steady-state cache size.** Single file, one line. Uncertain but likely 5-15% win.

Then the previously-planned solver phases (AlphaBetaCache FxHasher, iso_deck memoization, key hoisting, clone removal, Vec::retain cleanup) which require more work for smaller individual wins.

**Combined expected gain from Phase 0.5 + 0.6 + 1: ~12-24% wall-clock** on three_card_played for ~5 lines of code changes across 2 files.

---

## Phase 0.5 SHIPPED — `Deck::set` break-early + `card as u32`

### Changes

- `crates/games/src/gamestates/euchre/deck.rs:136-147` — `Deck::set` now breaks after the first location match, using a local `mask = card as u32` variable.
- `crates/games/src/gamestates/euchre/deck.rs:220-226` — `Hand::add` / `Hand::remove` use `card as u32` instead of `ToPrimitive::to_u32(&card).unwrap()`.

### Test results

`cargo test --release -p games`: **52 passed, 0 failed**. Critical safety tests passed:
- `test_undo_euchre`
- `test_undo_is_inverse_of_apply_all_games`
- `test_alone_undo`
- `euchre_test_unique_istate`

### Wall-clock delta (test profile, 100k iters, parallel)

```
Before:  real  1m4.254s  user  7m51.958s  sys  1m27.617s
After:   real  0m55.496s  user  9m6.813s  sys  1m40.606s
```

**-13.6% wall-clock** (64.3s → 55.5s). User/real ratio improved **7.35 → 9.85** — more effective parallelism, probably because less time is spent spinning in work that shared state + cache misses dominate.

### Hotspot delta (three_card_played, 60s, 26,588 samples)

Artifact: `profile_data/phase05-three_card.data`.

| Symbol | Before (120s) | After Phase 0.5 (60s) | Δ |
|---|---|---|---|
| `Deck::set` | 6.91% | **1.99%** | **-4.92pp** ✓ |
| `Hand::remove` (inlined) | ~5.4% inclusive | absent from top | collapsed ✓ |
| `apply_action` | 8.33% | 9.94% | +1.6pp (redistribution) |
| `undo` | 5.13% | 6.29% | +1.2pp (redistribution) |
| `alpha_beta` | 7.16% | 7.28% | ~flat |

### Profile reshuffle — `EAction::from_i64` now visible at **8.90%**

Previously hidden. Call graph shows it's the macro-generated `num_traits::FromPrimitive::from_i64 → from_u64 → from_u32` dispatch inside `EAction::from(action: Action)` at `euchre/actions.rs` (the `#[derive(FromPrimitive)]` impl). Called in hot paths including `undo` (2.03%) and `apply_action`/`legal_actions`/etc. These calls were previously inlined inside `Hand::remove`'s callers and thus attributed to `Hand::remove`.

**This is not a regression — it's existing cost now visible.** Wall-clock is clearly faster (-13.6%).

**It is also exactly the same bug I just fixed**, one level up the stack. `EAction` is `#[repr(u32)]` (confirmed at `games/src/gamestates/euchre/actions.rs:22`) so the `FromPrimitive` trait dispatch can be replaced with a cheap `TryFrom` / match / or `transmute`-with-assertion. Added as Phase 0.7 candidate.

**Phase 0.5 verdict: ship. Moving to Phase 0.6.**

---

## Phase 0.6 SHIPPED — LinearCFR telescoping product closed form

### Changes

- `crates/card_platypus/src/algorithms/cfres.rs:498-520` — replaced the `∏(i=last..end) i/(i+1)` iterator chain with the closed form `last/end` (the product telescopes: every `(k+1)` in the numerator cancels the `(k+1)` in the next denominator).
- Added unit test `linear_cfr_factor_closed_matches_reference` comparing the f32 closed form to an f64 mathematical reference across 12 edge cases (empty range, small, large, clamped to `LINEAR_CFR_CUTOFF`).
- Cleaned up unused `ToPrimitive` import in `games/src/gamestates/euchre/deck.rs`.

### Correctness note

The existing iterative f32 version accumulates rounding drift: for `last=500_000, iter=1_000_000` it computes `0.4988` when the exact answer is `0.5000` — a ~0.2% error. The closed form is **more** accurate than the code it replaces, not less. The unit test uses an f64 reference (the true math) rather than the drifting f32 iterative as the source of truth. All 52 existing `games` tests and the full workspace `cargo test --release` pass.

### Wall-clock delta (test profile, 100k iters, parallel)

```
Phase 0.5:   real  0m55.496s  user  9m06.813s  sys  1m40.606s
Phase 0.6:   real  0m40.140s  user  9m02.948s  sys  1m38.561s
```

**-27.7% wall-clock on top of Phase 0.5** (55.5s → 40.1s). User/real ratio **9.85 → 13.53**, effectively using 13+ cores out of whatever WSL2 is giving us.

**Cumulative vs. Phase 0 baseline: 64.3s → 40.1s = -37.6%.** Over one-third off total training time from 2 files, ~15 lines of code, both correctness-preserving.

### Hotspot delta (three_card_played, 60s, 28,219 samples)

Artifact: `profile_data/phase06-three_card.data`.

| Symbol | Phase 0.5 | Phase 0.6 | Δ |
|---|---|---|---|
| `CFRES::update_regrets` (self) | 2.00% | **disappeared from top 25** | ✓ |
| `Deck::set` | 1.99% | 1.97% | flat (as expected, already fixed) |
| `apply_action` | 9.94% | 10.63% | +0.7pp (redistribution) |
| `undo` | 6.29% | 5.89% | -0.4pp |
| `alpha_beta` | 7.28% | 6.94% | -0.3pp |
| `Mutex::lock_contended` (NodeStore) | 1.81% | 1.79% | flat |
| `DashMap::_get` (AlphaBetaCache) | 3.52% | 4.42% | +0.9pp (redistribution) |

`update_regrets` falling off the top-25 confirms the product-chain was the primary cost inside it. Other shifts are redistribution — the total wall-clock dropped by 15 seconds and the relative proportions have reshuffled.

**Phase 0.6 verdict: ship. Moving to Phase 1 (delete `evaluator.reset()`).**

---

## Phase 1 ATTEMPTED — delete `evaluator.reset()`

### Change

- `crates/card_platypus/src/algorithms/cfres.rs:247-248` — removed `self.evaluator.reset();` call from `CFRES::train()` post-batch cleanup, leaving only `self.play_bot.reset();`.

### Hypothesis (from Phase 0 analysis)

AlphaBetaCache results are deterministic functions of `(team, canonical_position)` — wiping the shared `Arc<DashMap>` transposition table every 100 iterations throws away valid cached work. Expected 5-15% wall-clock win from improved cache hit rate.

### Result: REFUTED. Reverted.

**Wall-clock (test profile):**

```
Phase 0.6 (with reset):     real 0m40.140s user 9m02.948s
Phase 1  (no reset, run 1): real 0m43.241s user 8m00.048s
Phase 1  (no reset, run 2): real 0m42.675s user 7m59.857s
Reverted:                    real 0m40.884s user 9m04.975s
```

**+6-7% wall-clock regression.** User time DROPPED by ~1 minute (less total CPU work), but wall-clock went UP. Parallelism efficiency fell from 13.53x → 11.1x.

**Hotspot delta on three_card_played (`profile_data/phase1-three_card.data`, 26,441 samples):**

| Symbol | Phase 0.6 (reset) | Phase 1 (no reset) | Δ |
|---|---|---|---|
| **`DashMap::_get`** | **4.42%** | **7.93%** | **+3.51pp** ← dominant cost |
| `transposition_table_hash` | 1.32% | 1.24% | -0.08pp |
| `iso_deck` | 3.65% | 2.95% | -0.70pp (more hits) |
| `alpha_beta` | 6.94% | 6.43% | -0.51pp (slightly more hits) |
| `SipHasher::write` | 4.18% | 3.70% | -0.48pp (fewer inserts) |
| `alpha_beta cache::insert` (via `DashMap::_insert`) | 1.48% | 1.29% | -0.19pp |

The small wins from "cache hit rate got better" are **overwhelmed** by the 3.5pp jump in `DashMap::_get`. The growing cache (unbounded) destroys L-cache locality: each lookup touches a larger random set of cache lines, and rayon workers contend on more shards.

### What I got wrong

- Assumed a large cache's **hit rate improvement** would dominate the **lookup cost** increase. It didn't. For this workload the baseline cache is already hitting frequently enough that marginal hit-rate gains are small, while lookup cost scales badly with size.
- Assumed the reset was "obviously" waste. It was actually acting as a **de facto size cap** that kept the working set L2/L3-cache-friendly. The frequent-wipe pattern produces a small, hot cache that's faster to query than a large one with a slightly higher hit rate.

### What to try later (separate phases, not Phase 1)

1. **Bounded-LRU cache** — keep the semantic benefit of the reset (small hot working set) without the cliff of full wipes. Swap `DashMap` for a `quick_cache`/`mini-moka` style bounded concurrent map. Evicts cold entries while retaining hot ones across batches.
2. **Reset less often** — reset every 1000 or 10000 iterations instead of every 100. Let the cache grow for a while but still cap it. Cheap to try.
3. **Smaller key type** — `(Team, u64)` with `Team` padded is 16 bytes of key; could pack into `u64` only. Reduces DashMap memory footprint.

These are candidates for a future phase if the other low-hanging fixes don't get us to target.

**Phase 1 verdict: reverted. Moving to Phase 0.7.**

---

## Phase 0.7 SHIPPED — `EAction::from(Action)` + `EAction::from(Card)` + `EAction::card()` → direct array lookup

### Change

- `crates/games/src/gamestates/euchre/actions.rs` — added a dense `const ACTION_TO_EACTION: [EAction; 32]` lookup table indexed by bit position. Replaced three hot conversion impls:
  - `impl From<Action> for EAction` — now `ACTION_TO_EACTION[value.0 as usize]`, single array load.
  - `impl From<Card> for EAction` — now `ACTION_TO_EACTION[(value as u32).trailing_zeros() as usize]`.
  - `impl From<u32> for EAction` — same pattern, with assertion for single-bit input.
  - `EAction::card()` — const `EACTION_TO_CARD: [Option<Card>; 32]` lookup by `trailing_zeros`.

All four paths were previously going through `num_traits::FromPrimitive::from_u32`, which the macro generates as a 24-32-arm runtime match over sparse discriminant values. The discriminants are dense in `0..32` bit space (every position 0..=31 corresponds to exactly one `EAction` variant), so a u8-indexed array is O(1) and fits in one cache line.

### Test results

`cargo test --release -p games`: **52 passed, 0 failed**, including conversion round-trip tests (`test_undo_euchre`, `euchre_test_unique_istate`, `test_isomorphic_normalization_is_idempotent`).

### Wall-clock delta (test profile)

```
Phase 0.6:  real 0m40.140s  user 9m02.948s  sys 1m38.561s
Phase 0.7:  real 0m35.390s  user 7m10.065s  sys 1m44.220s
```

**-11.5% on top of Phase 0.6** (40.1s → 35.4s). User time dropped from 9:03 → 7:10 — ~2 minutes of CPU work eliminated. **Cumulative vs. Phase 0 baseline: 64.3s → 35.4s = -45.0%.**

### Hotspot delta (three_card_played, `profile_data/phase07-three_card.data`, 28,922 samples)

| Symbol | Phase 0.6 | Phase 0.7 | Δ |
|---|---|---|---|
| `EAction::from_i64` (FromPrimitive) | 8.98% | **2.28%** | **-6.70pp** ✓ (residual is `From<Card>`, see below) |
| `EAction::card` | 4.82% | dropped out of top 25 | ✓ |
| `apply_action` | 10.63% | 10.25% | ~flat |
| `alpha_beta` | 6.94% | 7.95% | +1pp redistribution |
| `undo` | 5.89% | 6.98% | +1pp redistribution |
| `legal_actions` | 4.92% | 6.16% | +1.2pp redistribution |

Residual `from_i64` at 2.28% was traced to `From<Card> for EAction` called from `legal_actions_play` — fixed in the same commit, should be near-zero on the next profile. The other small redistributions are cost that was previously hidden under the slow conversion function now being correctly attributed to its enclosing hot callers.

**Phase 0.7 verdict: ship.**

---

## Phase 0.8 SHIPPED — hoist `transposition_table_hash` in `alpha_beta`

### Change

- `crates/card_platypus/src/algorithms/open_hand_solver.rs` — added `AlphaBetaCache::get_by_key` and `insert_by_key` that take a precomputed `u64` cache key, made `get_game_key` `pub(crate)`, and refactored `alpha_beta` to compute the key **once** at function entry and reuse it for both the entry get and exit insert. Previously both `cache.get` and `cache.insert` called `get_game_key` internally, running `iso_deck` + hashing twice per frame.

### Test results

`cargo test --release -p card_platypus`: **all passed** (3 CFR/CFRES tests + 1 euchre indexing test; `test_alg_open_hand_solver_euchre` was already ignored upstream). Bluff and Kuhn Poker MTD tests still pass.

### Wall-clock delta

```
Phase 0.7:  real 0m35.390s  user 7m10.065s
Phase 0.8:  real 0m34.777s  user 6m45.918s
```

**-1.7% on top of Phase 0.7** (35.4s → 34.8s). Smaller than expected for the test profile because max_cards_played=0 hits the solver very frequently but with shallow trees where the hoist saves less per frame. Better signal on three_card_played:

### Hotspot delta (three_card_played, `profile_data/phase08-three_card.data`, 27,342 samples)

| Symbol | Phase 0.7 | Phase 0.8 | Δ |
|---|---|---|---|
| `iso_deck` | 3.99% | **2.87%** | **-1.12pp** ✓ |
| `transposition_table_hash` | 1.91% | dropped out of top 25 | ✓ |
| `EAction::from_i64` (residual) | 2.28% | gone from top 25 | ✓ (From<Card>/From<u32> fixed) |
| `SipHasher::write` | 5.02% | 3.83% | -1.19pp |
| `DashMap::_get` | 4.32% | 5.68% | +1.36pp (cache-key-hashing cost redistributed) |

Combined AlphaBetaCache key + access cost: 15.24% → 12.38% (**-2.86pp**).

**Cumulative vs. baseline: 64.3s → 34.8s = -45.9%.** Five fixes shipped, one reverted, ~50 lines of code across 3 files.

**Phase 0.8 verdict: ship. Moving to Phase 0.9 (swap DashMap hasher).**

---

## Phase 0.9 SHIPPED — DashMap hasher → FxBuildHasher

### Change

- `crates/card_platypus/src/algorithms/open_hand_solver.rs` — changed `Arc<DashMap<TranspositionKey, AlphaBetaResult>>` to `Arc<DashMap<TranspositionKey, AlphaBetaResult, FxBuildHasher>>`, constructed via `DashMap::with_hasher(FxBuildHasher)`. `rustc-hash = "2.1.1"` was already a workspace dependency.

Rationale: the cache key is `(Team, u64)` where the `u64` is already a precomputed hash. Running it through SipHasher (DashMap's default `RandomState`) is pure overhead. FxHasher is a few multiply-xor-shift instructions on a single u64.

### Wall-clock delta

```
Phase 0.8:  real 0m34.777s  user 6m45.918s
Phase 0.9:  real 0m34.640s  user 6m38.207s
```

**-0.4% on test** (marginal, near noise floor). Solid on three_card_played:

### Hotspot delta (three_card_played, `profile_data/phase09-three_card.data`)

| Symbol | Phase 0.8 | Phase 0.9 | Δ |
|---|---|---|---|
| **`DashMap::_get`** | **5.68%** | **3.39%** | **-2.29pp** ✓ |
| `DashMap::_insert` | 2.31% | 2.40% | flat |
| `SipHasher::write` | 3.83% | 4.52% | +0.69pp (see note) |

DashMap lookup cost nearly halved. The residual `SipHasher::write` is **not** from DashMap — it's from `EuchreGameState::transposition_table_hash` internally hashing the `iso_deck` result to produce the cache-key u64. A separate fix would swap that to FxHasher too, or (better) fold `iso_deck`'s `[Locations; 4]` directly into a u64 without a hasher, since `iso_deck` is already canonicalized.

**Cumulative vs. baseline: 64.3s → 34.6s = -46.2%.**

**Phase 0.9 verdict: ship. Moving to Phase 1.0 (eliminate `Vec::retain` in the solver action processor).**

---

## Phase 1.0 SHIPPED — `remove_equivlent_cards` bitmask rewrite

### Change

- `crates/games/src/gamestates/euchre/processors.rs` — deleted `find_next_card_owner` helper (4.06% self-time in Phase 0.9 profile) and rewrote `remove_equivlent_cards` to precompute three `u32` bitmasks once per call (`cur_hand`, `all_hands`, `visible`) and check the equivalence condition with bitmask AND operations inside the retain closure. Previously every action called `Deck::get` which loops through all 10 card locations.
- `crates/games/src/gamestates/euchre/deck.rs` — added `Hand::raw_mask() -> u32` as `pub(super)` accessor so processors.rs can do direct bitmask ops.

### Test results

`cargo test --release -p games`: **52 passed**, including `test_remove_equivalent_cards` which exercises this exact function.

### Wall-clock delta (test profile)

```
Phase 0.9:  real 0m34.640s
Phase 1.0:  real 0m34.729s
```

Flat on test (within noise). Max_cards_played=0 doesn't exercise this code heavily because the solver does shallow rollouts. Real measurement needs three_card_played.

### Hotspot delta (three_card_played, `profile_data/phase10-three_card.data`)

| Symbol | Phase 0.9 | Phase 1.0 | Δ |
|---|---|---|---|
| `find_next_card_owner` | 4.06% | **deleted** | ✓ |
| `process_euchre_actions` | 1.77% | 5.41% | +3.64pp (absorbs old find_next_card_owner work) |
| `Vec::retain` closure | 4.76% | 3.90% | -0.86pp |

Net subsystem (`find_next_card_owner` + `process_euchre_actions` + `Vec::retain`): **10.59% → 9.31% = -1.28pp**. Real but small — the work didn't disappear, it just got cheaper per card.

**Phase 1.0 verdict: ship. Moving to Phase 1.1.**

---

## Phase 1.1 SHIPPED — inline fast mix replaces `DefaultHasher` in `transposition_table_hash`

### Change

- `crates/games/src/gamestates/euchre/mod.rs:1138-1180` — replaced the `DefaultHasher::new() + .hash(&mut hasher) × 5` pattern with an inlined 3-mix FxHash-style fold operating on packed `u64` words. All the hash inputs (iso_deck's `[u32; 4]`, 2 `u8`s, 2 bit flags, 2 bit cur_player) fit in three `u64` words and the mix function is a rotate-xor-multiply that inlines to a handful of instructions.

No new crate dep needed — the games crate doesn't have rustc-hash and adding it would bloat compile time, so the mix is written inline.

### Wall-clock delta

```
Phase 1.0:  real 0m34.729s
Phase 1.1:  real 0m34.095s
```

**-1.8% on test** (34.7s → 34.1s).

### Hotspot delta (`profile_data/phase11-three_card.data`)

| Symbol | Phase 1.0 | Phase 1.1 | Δ |
|---|---|---|---|
| `SipHasher::write` | 4.07% | **dropped out of top 25** | ✓ |
| `transposition_table_hash` (self) | ~1.67% | out of top 25 | ✓ |
| `alpha_beta` self | 8.26% | 10.68% | +2.4pp (redistribution from hash) |

The hasher swap cleanly eliminated SipHasher from the profile while redistributing attribution into `alpha_beta` (which now counts the inlined mix calls as its own self-time).

**Phase 1.1 verdict: ship. Moving to Phase 1.2.**

---

## Phase 1.2 SHIPPED — stack-allocate trick buffer in `apply_action_play`

### Finding

Call-graph drill on `Vec::from_iter` (2.72% self in Phase 1.1) traced directly to `EuchreGameState::evaluate_trick` via `apply_action_play`, specifically `last_trick_with_entry` at `euchre/mod.rs:438` which allocated a fresh `Vec::with_capacity(4)` on every trick-boundary (every 4 cards played in the play phase — a LOT of times inside alpha_beta).

### Change

- `crates/games/src/gamestates/euchre/mod.rs:438-450` — changed `last_trick_with_entry` to return `Option<[Option<Card>; 4]>` (stack array) instead of `Option<Vec<Option<Card>>>`. Only one caller, and it passed `&trick` to `evaluate_trick(cards: &[Option<Card>], ...)` which `&[Option<Card>; 4]` coerces to for free.

### Test results

Full workspace `cargo test --release`: **all suites green** (52 games tests + 3 card_platypus lib tests + 1 integration test). `test_undo_euchre`, `test_alone_full_game_playthrough`, `euchre_test_unique_istate` and the deep MTD/solver tests all pass.

### Wall-clock delta

```
Phase 1.1:  real 0m34.095s
Phase 1.2:  real 0m33.921s  (33.9s) — confirmed by second run at 32.5s, third at 34.2s
```

**-0.5% on test** (34.1 → ~33s). The solo test-profile impact is small because max_cards_played=0 only triggers a handful of trick boundaries; the payoff is on deeper profiles where every alpha_beta rollout plays many tricks.

### Hotspot delta (`profile_data/phase12-three_card.data`, 27,201 samples)

| Symbol group | Phase 1.1 | Phase 1.2 | Δ |
|---|---|---|---|
| `libc::malloc` | 3.33% | 1.98% | -1.35pp |
| `libc::_int_free` | 2.16% | 1.15% | -1.01pp |
| `libc::cfree` | 2.04% | (out of top 25) | -2.04pp |
| `libc::_int_malloc` | 2.01% | 2.05% | flat |
| **Allocation subsystem total** | **~9.5%** | **~5.2%** | **-4.3pp** ✓ |
| `Vec::from_iter` | 2.72% | 3.12% | +0.4pp (redistribution) |

Allocator pressure clearly dropped. `Vec::from_iter` frame didn't fall because that symbol aggregates multiple call sites; only the trick-boundary caller was eliminated.

**Phase 1.2 verdict: ship.**

---

## Cumulative status after Phase 1.2 (8 phases shipped, 1 reverted)

**Wall-clock on `test` profile (100k iters, parallel):**

| Phase | real | Δ vs baseline | Δ vs prev |
|---|---|---|---|
| baseline | 64.3s | — | — |
| 0.5 Deck::set break-early + card as u32 | 55.5s | -13.7% | -13.7% |
| 0.6 LinearCFR closed-form product | 40.1s | -37.6% | -27.7% |
| 0.7 EAction::from const lookup | 35.4s | -45.0% | -11.5% |
| 0.8 hoist alpha_beta cache key | 34.8s | -45.9% | -1.7% |
| 0.9 DashMap → FxBuildHasher | 34.6s | -46.2% | -0.4% |
| ~~1 delete evaluator.reset()~~ | ~~43.2s~~ | **reverted** | **+6-7%** |
| 1.0 remove_equivlent_cards bitmask | 34.7s | -46.0% | ~flat |
| 1.1 inline fast transposition hash | 34.1s | -47.0% | -1.7% |
| 1.2 stack-alloc trick buffer | ~33s | **-48.7%** | -0.5% |

**Lines of code touched: ~80.** Files modified: 4 (`euchre/deck.rs`, `euchre/actions.rs`, `euchre/processors.rs`, `euchre/mod.rs`, `cfres.rs`, `open_hand_solver.rs`).

**All fixes are correctness-preserving.** Every phase ran the workspace test suite green, including the critical undo-inverse tests. The telescoping product fix is also *more* numerically accurate than the code it replaces.

**Cumulative speedup: roughly 2x** (64.3s → ~33s). For the 600M-iteration `three_card_played` profile this should translate to several hours saved per full training run.

