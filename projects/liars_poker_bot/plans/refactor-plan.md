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

## Phase 6: Error Handling Improvements

### 6.1 Replace panics in database layer with Result

**File:** `crates/card_platypus/src/database/mod.rs`

- Lines 50, 65: `panic!("serialization not supported")` -> return `Err(...)`
- Lines 81, 107: `unwrap_or_else(|| panic!("failed to index"))` -> return `Result`
- Line 180: `unwrap()` on file metadata -> propagate error

### 6.2 Replace panics in euchre game logic with Result

**File:** `crates/games/src/gamestates/euchre/mod.rs`

Priority locations (called in hot paths):
- Line 194, 241, 251, 278, 293: Validation panics in `apply_action` variants
- Line 395: `panic!("tried to get leading card at invalid time")`

**Approach:** Change `GameState::apply_action` return type to `Result<(), GameError>`
or use debug_assert for invariants that are only violated by internal bugs (not user
input). This is a significant API change — plan carefully.

### 6.3 Replace panics in euchre_server with proper HTTP errors

**File:** `crates/euchre_server/src/main.rs`

- Lines 84, 101, 103, 108, 126: Mutex `.unwrap()` -> return 500 error
- Line 192: `.unwrap()` on position search -> return 404
- Line 225, 274: `.expect()` on missing players -> return 400

**Pattern:**
```rust
let games = data.games.lock().map_err(|_| actix_web::error::ErrorInternalServerError("lock failed"))?;
```

---

## Phase 7: Architecture Improvements

### 7.1 Split euchre_server/main.rs into modules

Current: 429 lines in a single file mixing HTTP handlers, game logic, state
management, and configuration.

**Target structure:**
```
euchre_server/src/
  main.rs          — startup, config, logging
  handlers.rs      — HTTP route handlers
  game_manager.rs  — game state management, progression logic
  bot.rs           — AI opponent logic
  error.rs         — error types and conversions
```

### 7.2 Extract hardcoded values into configuration

| Value | Location | Action |
|---|---|---|
| `/var/lib/card_platypus/infostate.three_card_played` | euchre_server:46 | Env var or config file |
| `localhost:4000` | euchre_server:385 | Env var with default |
| `5 second` polling interval | euchre-app/in_game.rs:109 | Constant at top of file |
| Remote server address | xtask:12 | Env var |

### 7.3 Replace sync Mutex with async-friendly locking in server

**File:** `crates/euchre_server/src/main.rs:36-37`

```rust
// Current: blocks async runtime
games: Mutex<HashMap<Uuid, GameData>>,
bot: Mutex<CFRES>,
```

**Options:**
- Use `tokio::sync::RwLock` for read-heavy game state access
- Use `tokio::sync::Mutex` at minimum to avoid blocking the executor
- Consider a channel-based game manager actor for the bot

### 7.4 Refactor CFRES constructors to reduce duplication

**File:** `crates/card_platypus/src/algorithms/cfres.rs`

Three nearly identical constructors: `new_euchre()`, `new_kp()`, `new_bluff_11()`.

**Action:** Create a generic `CFRES::new<G: GameState>(config: CFRESConfig)` that
takes a configuration struct instead of duplicating setup logic.

---

## Phase 8: Performance Improvements

### 8.1 Audit CFRES clone in parallel training loop

**File:** `crates/card_platypus/src/algorithms/cfres.rs:248`

```rust
(0..n).into_par_iter().for_each(|_| self.clone().iteration())
```

This clones the entire CFRES struct per iteration. Investigate what fields actually
need to be thread-local vs shared. The `Arc<Mutex<NodeStore>>` and `DashMap` are
already thread-safe — the clone may be unnecessary overhead.

### 8.2 Optimize ActionVec linear search

**File:** `crates/card_platypus/src/collections/actionvec.rs:34-44`

`get_index()` does O(n) linear search on every access. This is called millions of
times during training.

**Options:**
- Maintain a sorted order + binary search
- Use a small fixed-size hash map
- Since Action is a u8, use a 256-entry lookup table

### 8.3 Optimize NodeStore.len() from O(n) to O(1)

**File:** `crates/card_platypus/src/database/mod.rs`

`len()` iterates the entire index. Add an atomic counter that tracks insertions.

### 8.4 Consider enabling thin LTO for release builds

**File:** root `Cargo.toml`

```toml
[profile.release]
lto = "thin"  # Better codegen with moderate compile time increase
```

Benchmark before/after on the training loop to verify improvement.

---

## Phase 9: Test Coverage

### 9.1 Add tests for euchre_server

**File:** `crates/euchre_server/src/main.rs:429` — empty test module

Priority tests:
- Game creation and joining
- Action submission and state progression
- Error responses for invalid actions
- Bot play integration

### 9.2 Add test for the deck.rs bug fix (Phase 1.1)

After fixing the operator precedence bug, add a regression test that verifies
multi-card play is properly rejected.

### 9.3 Add property-based tests for game invariants

Using `proptest` or `quickcheck`, verify:
- `undo()` is the inverse of `apply_action()` for all games
- `is_terminal()` states have no legal actions
- Game tree is finite (no infinite loops)
- Isomorphic normalization is idempotent

---

## Task Dependency Graph

```
Phase 1 (Bugs)           -- no dependencies, do first
Phase 2 (Dead code)      -- no dependencies
Phase 3 (Naming)         -- no dependencies
Phase 4 (Unsafe)         -- no dependencies
Phase 5 (Workspace)      -- do before Phase 6 (cleaner deps)
Phase 6 (Error handling) -- do after Phase 1 (bugs fixed first)
Phase 7 (Architecture)   -- do after Phase 6 (error types needed)
Phase 8 (Performance)    -- do after Phase 7 (cleaner code to optimize)
Phase 9 (Tests)          -- do alongside or after each phase
```

Phases 1-5 are independent and safe to parallelize. Phases 6-8 build on each other.
Phase 9 should be woven into every other phase.
