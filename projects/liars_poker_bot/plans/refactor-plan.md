# Codebase Cleanup & Refactor Plan

This plan is the result of a comprehensive audit of the entire workspace. Each task is
self-contained and can be picked up independently. Tasks are ordered within each phase
so that earlier tasks reduce risk or unblock later ones.

Run tests after every task: `cargo test --release`

---

## Phase 1: Bug Fixes & Correctness [COMPLETE]

These are correctness issues that should be fixed before any refactoring.

### 1.1 Fix operator precedence bug in deck.rs [DONE]

Changed `!played_hand.len() == 1` to `played_hand.len() != 1` in deck.rs.

### 1.2 Fix typo in xtask systemd restart command [DONE]

Renamed `eucher-server.service` to `euchre-server.service` (all occurrences).

### 1.3 Remove `static mut` counter pattern (UB risk) [DONE]

Fixed in commit d077648.

---

## Phase 2: Dead Code & Unused Dependencies [COMPLETE]

### 2.1 Remove dead code paths in main.rs [DONE]

Removed `Run`, `Analyze`, `Play` commands and their handlers (`run()`, `run_analyze()`, `run_play()`).
Also removed `PassOnBowerCFRParseWeights` command (called broken `get_infostates()`).

### 2.2 Remove dead code in cfres.rs [DONE]

Removed `get_infostates()`, `full_update_average()`, `AverageType` enum, `parse_weights()` function,
and associated dead imports (`DashMap`, `IStateKey`, `Serialize`, `fs`, `Deref`).

### 2.3 Remove unused dependencies [DONE]

Removed 10 unused dependencies across 5 crates using `cargo machete`:
- `games`: removed `log`
- `card_platypus`: removed `half`
- `euchre-app`: removed `anyhow`, `form_urlencoded`, `futures`, `js-sys`, `serde_json` (kept `getrandom` for WASM)
- `euchre_server`: removed `serde`, `serde_json`
- `xtask`: removed `serde`, `toml`

### 2.4 Remove commented-out code [DONE]

Removed commented-out `rmp_serde::to_vec` line from database/mod.rs.

### 2.5 Remove unused UpDownRiver game stub [DONE]

Deleted `updownriver.rs` and its `mod` declaration.

---

## Phase 3: Naming & Spelling Fixes [COMPLETE]

### 3.1 Rename `ismorphic` to `isomorphic` [DONE]

Renamed file `ismorphic.rs` -> `isomorphic.rs`, updated mod declaration and all imports
across the workspace (cfres.rs, iterator.rs, pass_on_bower_cfr.rs, processors.rs, mod.rs).

### 3.2 Fix typos in comments and strings [DONE]

Fixed: "underfined"->"undefined", "debuging"->"debugging", "unwraping"->"unwrapping",
"sending actiond"->"sending action".

---

## Phase 4: Unsafe Code Audit & Reduction [COMPLETE]

### 4.1 Add SAFETY comments to all unsafe blocks [DONE]

Added `// SAFETY:` comments to:
- `lib.rs`: Pod/Zeroable impls for Action
- `istate.rs`: Pod/Zeroable impls for IStateKey
- `actions.rs`: EAction card() method conversion

### 4.2 Replace transmutes with safe alternatives [DONE]

- `actions.rs:card()`: Replaced transmute with `Card::from_u32().expect()` using `FromPrimitive`
- `cards.rs:From<[u16;4]>`: Replaced transmute with `bytemuck::cast()`
- `deck.rs`: Previously replaced transmute with `from_ne_bytes` (commit d077648)

### 4.3 Pod/Zeroable SAFETY documentation [DONE]

Added detailed SAFETY comments explaining why the unsafe impls are sound for Action
and IStateKey. The types use `#[repr(transparent)]` and `#[repr(C)]` respectively,
making derive macros infeasible due to complex field layouts. SAFETY comments document
the invariants instead.

---

## Phase 5: Workspace Configuration Cleanup [COMPLETE]

### 5.1 Consolidate shared dependencies in workspace Cargo.toml [DONE]

Added `[workspace.dependencies]` section with 8 shared deps (serde, rand, log, anyhow,
itertools, serde_json, clap, uuid). Updated 27 dependency declarations across 6 crates
to use `workspace = true`.

### 5.2 Fix inconsistent log version [DONE]

`euchre-app/Cargo.toml` now uses `log = { workspace = true }` (resolves to "0.4").

### 5.3 Deduplicate uuid dependency [DONE]

Moved uuid to workspace-level declaration. Both `client-server-messages` and
`euchre_server` now reference `uuid = { workspace = true }`.

---

## Phase 6: Error Handling Improvements [COMPLETE]

### 6.1 Replace panics in database layer with Result [DONE]

Replaced `panic!()` with `bail!()` in `new_kp` and `new_bluff_11`.
Changed `file.metadata().unwrap()` to `.context("failed to read file metadata")?`.

### 6.2 Replace panics in euchre game logic with Result [SKIPPED]

Too invasive — changing `GameState::apply_action` to return `Result` ripples
through the entire codebase. The panics are internal invariant checks, not user input.

### 6.3 Replace panics in euchre_server with proper HTTP errors [DONE]

Replaced `.expect()` on player lookups with match/if-let returning proper HTTP errors.
Replaced `.parse().unwrap()` with infallible `PathBuf::from()`.
Documented mutex `.lock().unwrap()` as intentional (poisoned = corrupt state).

---

## Phase 7: Architecture Improvements [COMPLETE]

### 7.1 Split euchre_server/main.rs into modules [SKIPPED]

Low value — file is manageable at ~430 lines. Pure reorganization.

### 7.2 Extract hardcoded values into constants/config [DONE]

Added 7 constants: `DEFAULT_WEIGHTS_PATH`, `MAX_CARDS_PLAYED`, `SERVER_HOST`,
`SERVER_PORT`, `WIN_SCORE`, `INDEX_FILE`, `LOG_FILE`. Updated all usage sites.

### 7.3 Replace sync Mutex with async-friendly locking [SKIPPED]

Actix uses a single-threaded runtime by default. Sync Mutex is appropriate
and avoids adding async locking overhead.

### 7.4 Refactor CFRES constructors to reduce duplication [DONE]

Created generic `new_simple()` constructor with `GameState + ResampleFromInfoState`
bounds. Simplified `new_kp()` and `new_bluff_11()` to one-liners.

---

## Phase 8: Performance Improvements [COMPLETE]

### 8.1 Audit CFRES clone in parallel training loop [SKIPPED]

Deep performance investigation — the clone is needed for thread-local RNG and
object pools. The shared state (NodeStore, iteration counter) is already behind
Arc/Mutex.

### 8.2 Optimize ActionVec linear search [SKIPPED]

Risky change to core data structure. Actions are typically 2-6 elements,
so linear search is likely optimal due to cache locality.

### 8.3 Optimize NodeStore.len() from O(n) to O(1) [DONE]

Added `populated_count: AtomicUsize` field. Counts existing entries at load time,
increments on new insertions. Changed `len()` to atomic load.

### 8.4 Enable thin LTO for release builds [DONE]

Enabled `lto = "thin"` in release profile. Benchmarks show 4-16% improvement
across deterministic benchmarks.

---

## Phase 9: Test Coverage [COMPLETE]

### 9.1 Add tests for euchre_server [SKIPPED]

Would require setting up actix test infrastructure. Lower priority.

### 9.2 Add regression test for deck.rs multi-card play bug [DONE]

Added `test_play_rejects_multiple_cards` in deck.rs tests.

### 9.3 Add property-based tests for game invariants [DONE]

Added to `crates/games/src/lib.rs` tests module:
- `test_undo_is_inverse_of_apply_all_games` — tests all games (Euchre, KP, Bluff variants)
- `test_terminal_states_have_no_legal_actions` — tests KP and Bluff variants
- `test_isomorphic_normalization_is_idempotent` — tests Euchre normalization

---

## Benchmark Cleanup

Removed `node_storage_benchmark` (tested unused storage approaches).
Removed `shift rotate`/`shift bitshift` micro-benchmarks (not testing production code).
Fixed deprecated `criterion::black_box` usage.
