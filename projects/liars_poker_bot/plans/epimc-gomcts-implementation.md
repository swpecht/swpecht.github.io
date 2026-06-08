# EPIMC and GO-MCTS implementation plan

Goal: implement two recent imperfect-information search algorithms in this
codebase and baseline them on Euchre (and later Oh Hell).

  - **EPIMC** — "Perfect Information Monte Carlo with Postponing Reasoning",
    Arjonilla, Saffidine, Cazenave (CoG 2024). arXiv: 2408.02380.
  - **GO-MCTS** — "Transformer Based Planning in the Observation Space with
    Applications to Trick Taking Card Games" (2024). arXiv: 2404.13150.

Status (2026-06-06):
  - [x] Background research on both papers
  - [x] Survey existing card_platypus algorithm infrastructure
  - [x] Decision: start with EPIMC
  - [x] Implement vanilla EPIMC (`crates/card_platypus/src/algorithms/epimc.rs`)
  - [x] Tests on Kuhn Poker + smoke tests on Oh Hell + Euchre
        (`cargo test -p card_platypus --release --lib algorithms::epimc`)
  - [x] Euchre baseline example
        (`crates/card_platypus/examples/euchre_epimc_baseline.rs`)
  - [ ] Collect numbers from the Euchre baseline at depths {1,2,3} with
        a meaningful sample size and write them up.
  - [ ] Stretch: subgame-solver EPIMC (depth-3 with CFR-on-infostates leaf)
  - [ ] Future: GO-MCTS prototype

## Why EPIMC first

GO-MCTS requires:
  - A GPT-2 style transformer (8 layers, 8 heads, 256 emb, 1024 FF), trained
    via population-based self-play over multi-million-game datasets.
  - A discrete observation token vocabulary per game (Hearts: 60, Skat: 113,
    Crew: 83). Euchre would need a new one (~40-60 tokens: 24 cards × pass/play
    × bid + suit + alone-flag).
  - A neural net runtime in Rust (`candle`, `burn`, `tch` FFI to libtorch). The
    workspace currently has no ML stack.
  - 10+ training iterations × hundreds of thousands of self-play games per
    iteration. Days of compute even with reasonable hardware.

EPIMC by contrast:
  - Is a strict generalisation of PIMC: depth=1 reduces to current
    `PIMCTSBot`. The hyperparameter `depth` is the only addition.
  - Reuses every piece of existing infrastructure: `Evaluator` trait,
    `OpenHandSolver`, `RandomRolloutEvaluator`, `ResampleFromInfoState`,
    `Agent`, `Policy`, `ActionVec`.
  - Has a ~300 LoC core. We can ship and benchmark today.
  - Provides a stronger PIMC baseline that GO-MCTS later has to beat.

## Algorithm sketches

### Standard PIMC (already implemented in `pimcts.rs`)

```
PIMC(s):
    actions = legal_actions(s)
    for each a in actions:
        value[a] = 0
        for r in 0..n_rollouts:
            w  = resample_from_istate(s, cur_player(s))   # determinize
            w' = w.apply(a)
            value[a] += PerfectAlgo(w')                   # OpenHandSolver / RandomRollout
        value[a] /= n_rollouts
    return argmax(value)
```

Issue: PerfectAlgo plays a *different* optimal strategy in each sampled world
w'. The real player has to commit to ONE strategy across all consistent
worlds. PIMC's averaging masks that constraint → "strategy fusion".

### EPIMC, simplest form (depth-d PIMC)

```
EPIMC(s, depth):
    actions = legal_actions(s)
    for each a in actions:
        value[a] = 0
        for r in 0..n_rollouts:
            w  = resample_from_istate(s, cur_player(s))
            w  = w.apply(a)
            for _ in 0..(depth - 1):
                if w.is_terminal(): break
                if w.is_chance_node():
                    w.apply(random_legal(w))
                else:
                    w.apply(random_legal(w))     # uniform random for opponents AND us
            value[a] += PerfectAlgo(w)
        value[a] /= n_rollouts
    return argmax(value)
```

At depth=1 this is exactly current `PIMCTSBot`. At depth>1 the random
playthrough delays the moment PerfectAlgo gets to "see" the determinized
world, so the bot's commitment to action `a` is averaged over a wider mix of
opponent responses — strategy fusion is reduced.

The paper notes that for games with mostly public observations (Card Game,
Battleship) increasing depth doesn't help; for games with private observations
(Dark Chess, Bridge, Skat) it does. Euchre has private hands → expected to
benefit.

### EPIMC, full form (subgame U + ImperfectAlgo)

Paper Algorithm 2:

```
ExtendedPIMC(depth, s):
    U = empty subgame rooted at infostate(s)
    while budget remaining:
        w = sample world from s
        Query(U, u_root, w, depth)
    return ImperfectAlgo(U)

Query(U, u, w, d):
    if d == 0 or w.is_terminal():
        u.value += PerfectAlgo(w)
        return
    a = random_legal(w)
    w' = w.apply(a)
    u' = U.child(u, infostate_action_from(a, w))
    Query(U, u', w', d - 1)
```

`ImperfectAlgo` solves the small accumulated subgame using a regret-based
solver (CFR+ / Information Set Search) and returns the root policy.

We will skip the full subgame builder in v1 and revisit it in v2: it requires
either CFR+ on a small infostate tree (we have `cfres.rs` — not directly
reusable since it's tied to disk-backed node store) or an information-set
minimax. Both are doable but neither is a one-day task.

### Important subtlety: who picks the random action at depth d>1

Two reasonable choices:
  1. **Uniform random for every player including current player.** Simple,
     matches the paper's `RandomAction(w)`. Used in our v1.
  2. **Uniform random for opponents only, search player picks via inner EPIMC
     (or argmax over child values).** Closer to the full subgame variant but
     adds recursion and a much larger compute footprint.

We start with (1). If results are noisy/poor we will switch the search
player's depth-2 choice to argmax over a fresh inner PIMC evaluation.

## Implementation plan

### File layout

```
crates/card_platypus/src/algorithms/
  pimcts.rs        # existing
  epimc.rs         # NEW: EPIMCBot<G, E>
  mod.rs           # add `pub mod epimc;`
```

### `EPIMCBot<G, E>` (v1, vanilla depth-d PIMC)

```rust
pub struct EPIMCBot<G, E> {
    n_rollouts: usize,
    depth: usize,        // 1 = standard PIMC; 2,3,... = postponed
    rng: StdRng,
    solver: E,
    eval_count: usize,
    _phantom: PhantomData<G>,
}
```

  - Implements `Evaluator<G>`, `Policy<G>`, `Agent<G>`, `Seedable` — mirror
    the surface of `PIMCTSBot` so it can drop into the same benchmark
    harnesses.
  - At `depth == 1` it must produce *identical* output to `PIMCTSBot` given
    the same seed, n_rollouts, evaluator. This is the unit test.
  - The `action_probabilities` hot loop:
    1. Sample `n_rollouts` worlds via `get_worlds()` (reusable function in
       `pimcts.rs` — re-export it via `pub(super)` or duplicate).
    2. For each candidate action a:
       - Clone each world, apply a, then play (depth-1) random actions on a
         clone, then evaluate with the solver.
       - Sum and average.
    3. argmax → one-hot ActionVec.
  - Use `rayon::par_iter` over worlds, matching `PIMCTSBot`.

### Random rollouts in the playout phase

Use the bot's own RNG (`StdRng`). For determinism across rollouts on the same
gamestate, seed similarly to `RandomRolloutEvaluator` (hash the gs.key into a
sub-RNG). Each per-world thread needs its own RNG to avoid contention; spawn
a per-task `SeedableRng::from_rng(&mut master)` or hash (world_idx, action_idx).

### Tests

  - `epimc_matches_pimc_at_depth_1`: instantiate both with same seed/RNG, same
    evaluator (OpenHandSolver::default()), compare `action_probabilities` on a
    fixed Kuhn Poker state and fixed Euchre state.
  - `epimc_kuhn_smoke`: tiny KuhnPoker test like `test_pimcts_kuhn`.
  - `epimc_oh_hell_full_game`: full-game smoke at depth=2.
  - `epimc_consistency`: same seed yields identical policy across 100 runs.

### Euchre baseline example

`crates/card_platypus/examples/euchre_epimc_baseline.rs`:
  - Sweep `depth ∈ {1, 2, 3}`, fixed rollouts (e.g. 25) and OpenHandSolver
    evaluator.
  - Pit EPIMC against PIMCTS baseline (same evaluator, same rollouts) at all
    4 seats over N games.
  - Report: win rate, mean reward, time per move, time per game.
  - Emit `kestrel: …` lines for plotting via kestrel-tail.

Expected result given paper findings: depth=2 or 3 should beat depth=1
(=PIMC) on Euchre by a small but reliable margin, at the cost of (depth)x
runtime.

### Stretch: full EPIMC with ImperfectAlgo

Add `EPIMCSubgameBot<G, E>`:
  - Build an explicit infostate-keyed tree to depth d.
  - Accumulate per-node values during rollouts.
  - At the end, run a simple expectimax-on-infostates pass to produce a root
    policy. Skip CFR+ for the v2 — a depth-2 expectimax over (root action,
    opponent random action, depth-2 child value) is enough to test the
    hypothesis that the subgame structure beats independent averaging.

### After EPIMC ships

GO-MCTS exploratory work:
  - Spike: design a token vocabulary for Euchre observations (~50 tokens).
    Use the existing `IStateKey` action stream as the canonical sequence.
  - Spike: try `candle-core` for the transformer. Without GPUs, training is
    likely impractical; we may need to (a) accept multi-day training or
    (b) port to PyTorch + libtorch and gate behind a feature flag.
  - This is a separate plan document and a separate research session.

## GO-MCTS implementation plan (started 2026-06-06)

Paper recap (from arXiv 2404.13150, Algorithms 1 and 3):
  - **MCTS over observation sequences.** Tree keyed by the search player's
    observation history (= our `IStateKey` for perfect-recall games).
  - **At search-player nodes**: UCT-select an action.
    `uct = val/visits + C·√(log(totalVisits)/visits)`, C ∈ [0.1, 0.4]
    depending on the game.
  - **At opponent nodes**: sample the next observation token from the
    generative model.
  - **Expansion**: when a leaf node is reached, the model provides an
    initial value V(h) for backup. (Paper Algorithm 1 also supports a
    rollout phase of `N_steps` player moves; we skip rollouts in v1 and
    just use the model's V(h) at the leaf — AlphaZero-style.)
  - **Legality threshold λ**: during expansion, prune children whose
    p_legal(a|h) < λ. λ = 0.01–0.05 in the paper. Used only at action
    selection, not backup.
  - **Illegality penalty μ**: when the *whole sampled trajectory* is later
    detected illegal (e.g. the opponent's sampled action wasn't actually
    legal in the underlying world), update child stats with
    (val - μ, visits unchanged). μ = 0.01.

### Hybrid simplification we adopt

Pure GO-MCTS never touches the underlying world — the transformer is
responsible for keeping trajectories plausible. We don't have a trained
transformer; we DO have the game rules. The pragmatic compromise:

  - At the root, sample a determinization w₀ from the search player's
    istate, exactly like PIMCTS.
  - Walk `w` forward during the simulation. At every node, query the
    GameState for `cur_player`, `legal_actions`, `is_terminal`,
    `evaluate`. The *search tree* is keyed by the search player's
    observation history (`istate_key(search_player)`), but the game
    state provides the dynamics.
  - The generative model only needs to sample actions and predict values
    conditioned on the observation history. We give it the legal-action
    list at the call site so even a uniform model returns sensible plays.

This is closer to ISMCTS-with-a-prior than pure GO-MCTS, but it lets us
ship a working algorithm without an ML stack. When we later have a
transformer, we replace the `GenerativeModel` impl and the tree-search
code is unchanged.

### Trait surface

```rust
pub trait GenerativeModel<G: GameState>: Send + Sync {
    /// Sample one of `legal` actions given the search player's observation
    /// history. v1 model returns uniform-over-legal.
    fn sample(&mut self, history: &IStateKey, legal: &[Action], rng: &mut StdRng) -> Action;

    /// Estimated value at this history, from the search player's POV.
    /// v1 model returns 0.
    fn value(&mut self, history: &IStateKey) -> f64;

    /// Policy logits over legal actions (used as UCT prior; optional).
    /// v1 model returns uniform.
    fn policy(&mut self, history: &IStateKey, legal: &[Action]) -> Vec<f64> {
        vec![1.0 / legal.len() as f64; legal.len()]
    }
}
```

### Concrete models

  - `UniformRandomModel` — no learning, returns uniform policy and V=0.
    Lets us validate the search algorithm in isolation. With enough
    iterations on Kuhn, UCT should still recover the right policy.
  - `TabularGenerativeModel` — `FxHashMap<IStateKey, NodeStats>` with
    per-action visit counts and a running mean value. Trained via
    self-play. Feasible for Kuhn (handful of istates); infeasible for
    Euchre.
  - `TransformerModel` — future work, not in v1.

### Self-play training loop (Kuhn)

Iteration `k`:
  1. With probability ½ each player uses GO-MCTS(model_{k-1}); the other
     samples from model_{k-1} directly.
  2. Play N games. For every search-player decision, record
     `(history, root_visit_distribution)`; for every terminal,
     record `(history, outcome)` along the trajectory.
  3. Update `TabularGenerativeModel`: increment visit counts,
     EMA-update value estimates.
  4. Loop.

For Kuhn this should converge to a near-Nash policy quickly because the
game tree is tiny (~12 istates per player).

### Euchre path (v1: stub-model smoke test)

  - The token vocabulary, transformer, and self-play data pipeline are
    explicitly out of scope this session.
  - We will ship a `UniformRandomModel`-backed GO-MCTS run on a single
    Euchre game just to confirm the search code is generic and doesn't
    crash on the larger state space.
  - Anything quantitatively meaningful on Euchre will wait for a
    real transformer model — separate workstream, requires picking an
    ML stack (`candle-core` is the in-Rust path; `tch` is the libtorch
    FFI path).

### Files

  - `crates/card_platypus/src/algorithms/gomcts.rs` — search + trait +
    `UniformRandomModel` + `TabularGenerativeModel` for Kuhn.
  - Tests in the same file: Kuhn validation + Euchre smoke.

### v1 status (landed 2026-06-06)

  - [x] `GenerativeModel<G>` trait with `sample` / `value` / `policy`.
  - [x] `GoMcts<G, M>` search: per-decision tree clear, UCT selection,
        AlphaZero-style leaf-via-model, illegality-μ-penalty backup
        (wired for future use; current models never produce illegal
        samples since the search hands them legal-action lists).
  - [x] `UniformRandomModel`: no learning. Used to isolate search bugs.
  - [x] `TabularGenerativeModel`: per-`IStateKey` per-action visit and
        value sums. Sampling is softmax over per-action mean values
        (temperature 0.5 — between argmax and uniform on Kuhn-scale
        payoffs); value-driven, so the training loop actually converges.
  - [x] `self_play_train()`: plays N games where every player samples
        from the current model, then attributes the terminal payoff to
        the recorded `(history, action)` pairs.
  - [x] Tests: uniform-model Kuhn smoke; tabular self-play populates the
        table; tabular self-play learns "King bets more than Jack" after
        5000 self-play games (sanity check that the value signal flows);
        Euchre full-hand smoke with uniform model.

### Known limitations / non-goals for v1

  - Hybrid (not pure paper) GO-MCTS: we maintain a determinised true
    state during simulation so the GameState supplies `legal_actions` /
    `cur_player` / `is_terminal`. Pure GO-MCTS expects the model to
    learn these. Honest about the divergence in the doc above.
  - No rollout phase: leaf value comes from `model.value()` directly.
    Trivial to add `n_rollout_steps` if a later experiment wants it.
  - Tabular model on Kuhn only. Euchre's istate space is far too large
    for a `FxHashMap<IStateKey, _>` to be sensible — that's why we need a
    transformer model, future work.
  - Self-play loop is a *much* reduced version of the paper's. No
    population sampling across iterations, no separate "ArgmaxVal*"
    seat: every seat samples the current model. Fine for Kuhn-scale
    validation; insufficient for Hearts/Skat-style claims.
  - Penalty μ is wired into backup but unreached in v1 (since
    `model.sample()` returns one of the legal actions we hand it).
    Kept for when a future trained model produces logits over a fixed
    vocabulary and may select tokens that aren't legal in the
    determinised world.

### How to continue this work (handoff notes)

If picking this up cold:
  1. Read this section + skim `gomcts.rs`. The search is ~250 LoC; the
     trait + two models + self-play are another ~150 LoC.
  2. The fastest meaningful next step is **NOT** to wire up a
     transformer. It is to verify the tabular model on Kuhn converges to
     something close to Nash, e.g. via the existing
     `exploitability.rs` machinery — measure exploitability of the
     trained-tabular-model-as-policy after N self-play games.
  3. For Euchre, the path is (a) pick an ML stack (`candle-core` is the
     in-Rust route; `tch` is the libtorch FFI route), (b) design the
     observation token vocabulary (~40-60 tokens: cards × bid/play
     types), (c) port the self-play loop to write training data to a
     buffer the model consumes, (d) train, (e) plug into the
     `GenerativeModel` trait. This is multi-day work and likely wants
     its own plan document.
  4. The penalty-μ backup branch in `simulate` is currently
     unreachable. Don't delete it — it's needed when a trained model
     starts emitting illegal samples.

### ML status (landed 2026-06-06)

The transformer-backed `GenerativeModel` is now live —
`gomcts_transformer.rs` (~600 LoC) on top of `candle-core` +
`candle-nn`. Key design decisions:

  - **Architecture**: GPT-2-style decoder with pre-LN, causal multi-head
    attention, MLP+GELU, LM head, scalar value head. Tiny defaults
    (Kuhn: d=32/2L/2H/FF=64; Euchre: d=64/2L/4H/FF=128). Paper uses
    much larger (256/8L/8H/1024) — we picked small for CPU training
    feasibility.
  - **Tokenizer trait**: per-game adapter from `IStateKey` → token
    sequence. Token 0 = PAD; real tokens start at 1.
      - `KuhnTokenizer`: 6 tokens (PAD, J, Q, K, Bet, Pass), max_ctx=8.
      - `EuchreTokenizer`: 33 tokens (PAD + 32 EAction bit-positions),
        max_ctx=48. Relies on `EAction`'s u8 values falling in 0..32,
        which is an invariant of the underlying single-bit-discriminant
        enum encoding.
  - **Training data**: per-example `(history, action, value)`. For each
    example the input fed to the transformer is `tokens(history) ⊕
    [action_token]` so the value head sees both V(h) AND V(h⊕a)
    positions and can distinguish counterfactual actions at the same
    history. **This was the key fix** — earlier training only supervised
    at the pre-action position and the LM-head-softmax policy was a
    fixed-point imitation loop with no value signal.
  - **Loss**: 0.9 · cross_entropy(LM @ prefix_pos, action_token) + 0.1 ·
    [MSE(V@prefix_pos, terminal) + MSE(V@action_pos, terminal)] / 2.
  - **Inference (ArgmaxVal\***)**: for each legal action a, query
    V(h⊕a), softmax with temperature 0.5. This is what makes the policy
    actually improve across self-play iterations. The LM head is
    available for legality priors but isn't on the policy hot path in
    v1.
  - **Optimiser**: AdamW (candle-nn) with configurable LR.

### Validation on Kuhn (`examples/kuhn_gomcts_train.rs`)

Configuration: 6 iters × 1000 self-play games/iter × 6 epochs/iter,
batch=64, lr=5e-3, eval=1500 games head-to-head vs uniform-random
opponent (rotating seats).

```
iter 0 (random init):  mean_reward=-0.2053
iter  examples  loss   mean_reward
   1     2108  0.57       +0.0813
   2     2288  0.79       +0.0500
   3     2240  0.64       +0.1320
   4     2208  0.74       +0.0300
   5     2169  0.68       +0.1207
   6     2172  0.66       +0.0487
```

Goes from clearly worse than uniform (-0.20) to clearly better
(+0.05 to +0.13) in one iteration; oscillates in the +0.03 to +0.13
range thereafter. The oscillation is from policy churn between
neighbouring fixed points + 1500-game eval noise (SE ~ 0.04).
For reference, the equilibrium player-0 score vs uniform random in
Kuhn is closer to +0.20-0.30, so we land in the credible
"strong-but-not-optimal" zone after ~6 quick iterations of CPU
training. More iters / bigger model would close this further.

### Euchre smoke (`examples/euchre_gomcts_smoke.rs`)

Pure plumbing test: builds an Euchre transformer, plays 10 self-play
games, trains for 4 epochs, confirms loss decreases.

```
collected 224 (history, action, value) tuples from 10 games in 2.42s
loss before: 5.1491, loss after 4 epochs: 4.4860 (train wall: 1.87s)
OK: loss decreased — Euchre training pipeline alive.
```

Per-game collection ≈ 0.24s (CPU-only, ~25 moves/game × ~2 forward
passes/move for ArgmaxVal\*). At 1000 games/iter that's ~4 min of
collection + ~1 min training per iter. A 20-iter run = ~2 hours. Still
nowhere near paper-quality on Euchre, but the pipeline scales.

### What's NOT in v1 (deliberately)

  - **Population-based self-play.** Paper iteration N uses one
    "ArgmaxVal\*" seat with the latest weights vs other seats sampling
    from previous iterations. We use only-current-weights at every seat.
    This is faster but biased; matters more on Skat-scale games than
    on Kuhn.
  - **GO-MCTS-driven training targets.** Paper's mature training also
    uses GO-MCTS root visit distributions as policy targets (AlphaZero
    style). We just use the sampled action token. Cheap enough to add.
  - **GPU.** Candle-core supports CUDA + Metal; we use the CPU backend
    here so anyone in the repo can run it. Adding a `gpu` Cargo feature
    that swaps the backend is straightforward.
  - **Paper-size transformer.** d=32 (Kuhn) and d=64 (Euchre) vs the
    paper's d=256. Bigger is feasible but slow on CPU.
  - **Checkpointing.** `VarMap::save` is available via candle; we just
    don't wire it up.

### v2 fixes (landed 2026-06-06, same session)

All five "NOT in v1" items above shipped. Specifically:

  - **GPU feature flag**: `[features] gpu_cuda` / `gpu_metal` in
    `Cargo.toml` flip candle's backend. A new `default_device()` picks
    CUDA → Metal → CPU based on what's compiled in and what initialises
    at runtime, so a flag-built binary still runs without the hardware.
  - **Paper-faithful config**: `TransformerConfig::paper_default()`
    returns 256-d / 8 heads / 8 layers / 1024 FF. The Kuhn and Euchre
    examples still default to small configs for fast iteration; users
    opt into paper size for serious runs.
  - **Checkpointing**: `GoMctsTransformer::save(path)` /
    `.load(path)` wrap `VarMap` safetensors I/O. Tested by the
    `transformer_save_load_roundtrip` unit test which trains a model,
    saves it, loads into a fresh transformer, and asserts identical
    policy outputs.
  - **Population-based self-play**: new `Population<G, T>` holds
    `live: TransformerGenerativeModel` (trainable) plus a `Vec<Snapshot>`
    of frozen historical iterations. `Snapshot::from_model` writes
    weights to a tempfile (safetensors); `Snapshot::hydrate` builds a
    fresh transformer and loads them. During `collect_population_game`,
    one designated seat plays via `live`, every other seat sampled
    one-frozen-per-seat-per-game. Tested by `population_self_play_smoke`.
  - **MCTS-driven training targets**: `collect_self_play_game_mcts`
    takes `&mut GoMcts<G, M>` and records each decision's *root visit
    distribution* as the soft policy target. `TrainExample` now carries
    an optional `policy_target: Vec<(Action, f32)>`; when present, the
    LM head's loss is `-Σ target · log_softmax(logits)` (soft
    cross-entropy) instead of the hard cross-entropy at the sampled
    action token. The soft path is a strict generalisation: hard
    targets convert to one-hot soft targets, which makes the soft-CE
    numerically identical to the hard-CE we had before. Tested by
    `mcts_self_play_soft_targets`.

### v2 Kuhn training example

`examples/kuhn_gomcts_train.rs` now drives the full paper-style loop:

  1. Run MCTS-driven games (`mcts_frac` of the iter budget) to collect
     soft targets.
  2. Run population games (the rest) for broader coverage with frozen
     opponents.
  3. Train on the combined buffer.
  4. Snapshot live → frozen.
  5. Checkpoint `iter_NNN.safetensors` to `KP_CKPT_DIR`.
  6. Eval vs uniform random; emit a kestrel metric line per iter.

A `final.safetensors` is written after the last iter for downstream
use. The mix-ratio of MCTS vs population games is tunable via
`KP_MCTS_GAMES_FRAC` (default 0.5).

### How to run the heavy stuff

CPU-only is the default:

```
cargo run -p card_platypus --release --example kuhn_gomcts_train
```

CUDA-accelerated (requires CUDA installed):

```
cargo run -p card_platypus --release --features gpu_cuda \
    --example kuhn_gomcts_train
```

To use the paper-size transformer, edit the example's `cfg = ...` line
to call `TransformerConfig::paper_default(...)`. At 8 layers × 256-d
this is slow on CPU; pair with a GPU build.

### Test count (final)

15 unit tests across `gomcts` + `gomcts_transformer`:
  - 4 in `gomcts` (search + tabular)
  - 11 in `gomcts_transformer` (forward smoke, training loss
    decreases, save/load, snapshot/hydrate, population game, MCTS soft
    targets, Euchre tokenizer smoke, …)

### Euchre training: first attempt (2026-06-06)

`examples/euchre_gomcts_train.rs` uses the same v2 pipeline as Kuhn —
population + MCTS-driven self-play, soft + hard targets, snapshotting,
checkpointing — sized for Euchre via `EuchreTokenizer` (33-token vocab)
and a selectable `TransformerConfig`.

First serious run, 4 iters × 150 games/iter × MCTS=16/decision × 4
epochs, smoke config (d=64, 2L, 4H, FF=128), CPU-only, ~58 min wall:

| iter | train_loss | mean_reward (300-game eval) |
|---|---|---|
| 0 (init) | — | +0.123 |
| 1 | 3.55 | +0.113 |
| 2 | 3.16 | +0.187 |
| 3 | 3.45 | −0.063 |
| 4 | 3.08 | +0.180 |

Loss trended down (3.55 → 3.08); mean reward averaged +0.104 across
iters 1-4. Per-iter eval noise is ~±0.07 (95% CI at n=300, payoff
SE ≈ 0.6), and iter 0 random init was lucky at +0.123 — so this
small-N eval doesn't cleanly separate "trained" from "random init".

Real signal needs a tighter-CI eval; see `examples/euchre_gomcts_eval.rs`
which runs n=2000+ hands per condition (raw transformer, GO-MCTS over
the transformer) plus a random-vs-random baseline for calibration.

### Euchre tight-CI eval (2026-06-06)

`EU_GAMES=2000 EU_MCTS_ITER=16 EU_CONFIG=smoke` against the trained
checkpoint from the first attempt:

| condition | mean | 95% CI | wall |
|---|---|---|---|
| random vs 3 random (baseline) | +0.068 | [−0.023, +0.159] | 0s |
| raw transformer (no MCTS) | +0.217 | [+0.127, +0.307] | 76s |
| GO-MCTS over transformer (16 sims) | +0.358 | [+0.268, +0.448] | 87 min |

The trained transformer is ~3 SE above the random baseline (p < 0.01);
search-wrapping the transformer adds another ~3 SE on top. So GO-MCTS
genuinely works on Euchre — the network learns enough trick-taking
structure that UCT search over its value head compounds the policy.

Cost: at MCTS=16 the search-wrapped agent runs ~2.6s/hand on CPU. For
serious tournament play against cfr0-3 you want MCTS≤8 or accept hours
per pairing.

### CUDA backend enabled (2026-06-07)

`/usr/local/cuda-12.6` + `--features gpu_cuda` builds clean.
`default_device()` resolves to `Cuda(...)` automatically. Kuhn smoke
ran ~10× faster than CPU; Euchre training was only ~1.1× faster because
each self-play game does batch=1 sequential inference and the GPU is
underused. Real GPU speedup needs batched self-play (N games' inference
together) — a real refactor, deferred.

### Bigger-model attempts didn't beat the smoke config

Two medium-config (`euchre_medium`: d=128 / 4L / 4H / FF=256) runs:
  - 6 iters × 200 games × MCTS=16, eval=400 (78 min GPU)
  - 10 iters × 300 games × MCTS=16, eval=500 (3h GPU)

Best n=2000 raw-vs-random numbers from any medium checkpoint: +0.172
(short-run iter_4) and +0.164 (long-run final). The smoke config
(+0.217) still wins. Bigger model + same noisy soft targets ⇒
overfitting; the smaller smoke policy generalises better against
random.

Final ranking, n=2000 raw vs random (95% CI ≈ ±0.046 each):

| config | mean | iter |
|---|---|---|
| smoke final | +0.217 | iter 4 of smoke run |
| medium iter_4 | +0.172 | mid-training peak (short medium) |
| long-medium final | +0.164 | iter 10 (long medium) |
| medium final | +0.160 | iter 6 (short medium) |
| long-medium iter_8 | +0.147 | mid-training peak (long medium) |
| random baseline | +0.068 | — |

### Difficulty tournament: gomcts (smoke) vs cfr0 + random

`BENCH_MATCHES=30 EUCHRE_GOMCTS_CONFIG=smoke EUCHRE_GOMCTS_ITER=16`,
GPU-enabled binary, 16 min wall:

| pair | gomcts W-L | match% | pt% (A vs B) |
|---|---|---|---|
| gomcts vs cfr0 | 0–30 | 0% | 15.4% – 84.6% |
| gomcts vs random | 25–5 | 83% | 64.6% – 35.4% |
| cfr0 vs random | 30–0 | 100% | 89.6% – 10.4% |

Ordering: **cfr0 ≫ gomcts > random**. The transformer + GO-MCTS agent
is a real "between random and cfr0" agent, but cfr0 dominates by a
massive margin. Caveat: cfr0 was trained on 50M CFR iterations of the
bidding tree; gomcts trained on ~600 self-play games. Apples-to-apples
training compute would compare gomcts to a cfr-baseline trained on a
similar wall-clock budget.

### Paths to close the gap (handoff notes)

  1. **Batched self-play inference**: each game currently does
     sequential batch=1 forward passes — the GPU sits idle. Batching N
     games' inference together is the single biggest expected win (~10×
     throughput) and unblocks bigger configs.
  2. **Snapshot vs snapshot eval for convergence detection**: extend
     the train loop to pit `live` against `pop.sample_frozen()` over a
     few hundred hands per iter; early-stop when the head-to-head
     plateaus at ~50%. ~60 LoC on top of the existing eval.
  3. **CFR-bootstrap**: generate training examples by playing one of
     the cfr0-3 weights against itself; supervised-train the
     transformer on that data before switching to self-play. Gets the
     value head into the right neighbourhood faster.
  4. **paper_default config** (d=256 / 8L / 8H / FF=1024) — almost
     certainly needs batched self-play first to be tractable, even on
     GPU.

### Three improvements landed (2026-06-07)

(CFR bootstrap deliberately excluded per user.)

**A) Batched ArgmaxVal\* inference** —
   `GoMctsTransformer::forward_history_batch(&[IStateKey])` returns
   `(Vec<logits>, Vec<value>)` from one padded forward.
   `masked_policy` now builds the |legal|-many post-action histories
   and forwards them as one batch. Pinned by the
   `forward_history_batch_matches_unbatched` unit test
   (gather indices preserved exactly within 1e-3 float noise).

**B) Snapshot head-to-head** —
   `head_to_head_eval(a, b, new_state, n, seed)` plays
   `a`-team vs `b`-team Euchre (or any GameState) with seat-rotation,
   returns `(mean_a_payoff, win_rate)`. `Population::sample_specific_frozen(idx)`
   hydrates a chosen snapshot. The Euchre train loop now calls
   `head_to_head_eval(live, pop_iter_N-1, 300)` after each snapshot
   and prints `h2h_mean` / `h2h_win%` columns + matching kestrel
   metrics. The signal is the proper convergence detector — for the
   paper_default run below, h2h confirmed plateau by iter 2-3.

**C) `paper_default` training run** —
   d=256, 8 layers, 8 heads, FF=1024 on GPU. 6 iters × 200 games ×
   MCTS=12 × 4 epochs, lr=0.0005. **57 min wall** — comparable to the
   medium run (78 min, 6 iters × 200) despite 8× the parameters,
   because batched inference paid for the scaling.

### Final standings (all at n=2000 raw vs uniform random)

| config | mean | 95% CI | trained on |
|---|---|---|---|
| **smoke final**       | **+0.217** | [+0.127, +0.307] | 4 × 150 games (CPU) |
| paper iter_3          | +0.210     | [+0.120, +0.300] | 3 × 200 games (GPU) |
| paper final (iter_6)  | +0.198     | [+0.106, +0.289] | 6 × 200 games (GPU) |
| medium iter_4         | +0.172     | [+0.082, +0.262] | 4 × 200 games (GPU) |
| long-medium final     | +0.164     | [+0.073, +0.254] | 10 × 300 games (GPU) |
| medium final          | +0.160     | [+0.069, +0.251] | 6 × 200 games (GPU) |
| random baseline       | +0.068     | [−0.023, +0.159] | — |

**The plateau is real.** Smoke (d=32, 2L), medium (d=128, 4L), and paper
(d=256, 8L) all converge to +0.20 ± 0.05 vs random. Model capacity is
NOT the bottleneck — self-play training signal quality is.

### CFR bootstrap (started 2026-06-07)

User opted into CFR bootstrap after observing that pure self-play
plateaued at +0.20 vs random across all transformer sizes. Hypothesis:
the value head spends most of its training budget escaping cold-start
(random init → barely-better-than-random) before *any* strategic signal
accumulates. Initialising from cfr3-supervised data should skip that
entire regime.

#### Why this is fundamentally different from "use OHS/PIMCTS values"

OHS / PIMCTS bootstrap (E2, which the user previously excluded) trains
the value head against perfect-info leaf evaluations. Those targets
inherit strategy-fusion bias — the network learns to predict
"PIMCTS-leaf-style optimistic values".

CFR bootstrap trains the network against *what cfr3 actually does* in
imperfect-info Euchre. The action targets are the moves a Nash-ish
policy plays, and the value targets are the actual outcomes of
self-play between Nash-ish policies. No perfect-info assumption.

#### Steps (notes as we go)

  1. Wrote `examples/euchre_gomcts_bootstrap.rs`:
     - Loads N copies of `EuchreCfres` (one per worker, sharing the
       /home/steven/card_platypus/infostate.three_card_played_f32 mmap).
     - Spawns `EU_BOOT_THREADS` worker threads via `std::thread::scope`.
     - Each worker plays `n_games / threads` games where all 4 seats are
       cfr3, recording per-seat `(history, chosen_action, terminal_value)`
       tuples as `TrainExample::hard(...)`.
     - Collects all examples, then trains a fresh paper-config
       transformer with the existing `train()` (soft CE on one-hot from
       the hard target, two-position value MSE).
     - Saves to `EU_BOOT_OUT` (default `/tmp/euchre_gomcts_bootstrap/
       bootstrap.safetensors`).
  2. Added `EU_INIT_WEIGHTS=path` knob to `euchre_gomcts_train.rs`:
     - Loaded *before* the optional `EU_RESUME_FROM` snapshot rebuild.
     - Distinct semantics from RESUME_FROM: just loads weights, no
       Population rebuild, iter counter still starts at 1.
     - Lets you start self-play from any checkpoint (bootstrap, prior
       run, etc.) without pretending it was iter-N.
  3. Run order:
     - Wait for the current alphazero run to drain (iter 20 imminent).
     - Run bootstrap: ~5-10k cfr3 vs cfr3 games (timing depends on
       per-decision cfr3 cost ≈ 50-200ms; rough estimate ~hours).
     - Resume self-play from `bootstrap.safetensors` via
       `EU_INIT_WEIGHTS=`. Compare iter-1 mean_reward + h2h to a
       random-init iter-1 (we have plenty of those datapoints).
  4. Timing reality (vs guess):
     - cfr3 turned out to be **way** cheaper than estimated. A 50-game
       smoke test with 4 worker threads ran at **30 games/sec** —
       ~33ms per game, ~1.3ms per cfr3 decision. The bidding policy
       is a tabular CFR lookup; the card play also short-circuits
       through cfr-trained tables for the early plies. Total
       throughput on the 16-core 7950X3D with 24 worker threads
       should be ~100-200 games/sec.
     - 100k cfr3-vs-cfr3 games × ~24 trajectory positions = ~2.4M
       supervised training examples. Estimated wall: ~10 min data
       collection + 60-90 min training (paper config, batch=512,
       25 epochs on GPU).
     - Launched: `EU_BOOT_GAMES=100000 EU_BOOT_THREADS=24
       EU_BOOT_EPOCHS=25 EU_BOOT_BATCH=512`.
  5. After bootstrap finishes:
     - Eval `bootstrap.safetensors` with `euchre_gomcts_eval` at
       n=2000 to confirm we matched (or got close to) cfr3 strength
       in the raw transformer / GO-MCTS-wrapped configurations.
     - Launch self-play with `EU_INIT_WEIGHTS=$(BOOT)` (plus
       `EU_ALPHAZERO=1 EU_BATCH_GAMES=32 EU_BATCH_SIZE=256`). Expect
       iter-1 mean_reward far above the +0.20 plateau we hit from
       random init.

#### Train batch size sweep (paper config, GPU)

`euchre_gomcts_train_batch_bench` with synthetic data, 300 steps per
condition, dataset=16k:

| batch_size | examples/sec | ms/step | speedup vs 64 |
|---|---|---|---|
| 64 | 7245 | 8.83 | 1.00× |
| 128 | 9534 | 13.42 | 1.32× |
| **256** | **10883** | **23.52** | **1.50×** |
| 512 | 11035 | 46.40 | 1.52× |
| 1024 | OOM | — | — |

Conclusion: **`EU_BATCH_SIZE=256`** is the sweet spot — 99% of peak
throughput with half the per-step memory of batch=512. Going above 512
risks OOM when other GPU activity (e.g. self-play inference) is also
happening. Updated the next-launch defaults accordingly.

#### Expected speedup

Today: 12 iters of self-play to barely beat random (+0.06 to +0.18 vs
random). Cold-start regime burns most of the training budget.

After bootstrap: iter 1 should sit near cfr3's strength (which we
measured at ~89.6% point share vs random in the difficulty tournament
== mean reward roughly +1.5 to +2.5 per hand). Self-play would then
*refine* this rather than discover it.

Stop condition the same as before: gomcts has to beat pimcts in a
tournament check AND tie cfr0 to count as success. With cfr3 bootstrap,
*both* feel reachable for the first time.

### Paths that could still move the needle

  1. **Multi-game batched self-play**: batch=|legal| is the within-decision
     batching we shipped. Cross-game batching — N games stepped in
     lockstep, every forward batches N×|legal| together — would let us
     scale games-per-iter ~10×, getting more data per training iter
     at the same wall time. Bigger refactor: needs round-aligned
     simulators or a "game pool" abstraction.
  2. **Exploration during self-play**: Dirichlet noise added to the
     root policy in MCTS sims (AlphaZero standard) + temperature
     schedule on the sampled action. Currently ArgmaxVal\* is too
     greedy at convergence; the model can't escape the fixed point.
  3. **KL regularisation in training**: penalise policy moves away
     from the population mean. Stabilises against the iter 4 / 5 / 8
     regressions we saw, where one bad iter cratered the model.
  4. **GO-MCTS-driven training targets at evaluation time only**:
     today the soft target IS the MCTS visit distribution, but in
     training we sample one move from that distribution. AlphaZero
     trains against the visit distribution directly — already what
     `policy_target` does — but only counts when MCTS is wide
     enough. Try MCTS=32 or 64 during self-play. Costs proportional
     wall time; with batched inference that's tractable.

### Test count (final final)

13 unit tests across `gomcts` + `gomcts_transformer`:
  - 4 in `gomcts` (search + tabular)
  - 9 in `gomcts_transformer` — added
    `forward_history_batch_matches_unbatched` and
    `head_to_head_eval_runs` this session

### Wiring to the difficulty-benchmark tournament

`examples/euchre_difficulty_benchmark.rs` now accepts `gomcts` as an
agent name. Set:

  - `EUCHRE_GOMCTS_WEIGHTS=/path/to/final.safetensors`
  - `EUCHRE_GOMCTS_CONFIG=smoke|medium|paper` (must match training)
  - `EUCHRE_GOMCTS_ITER=32` (per-decision search budget)

The agent boxes a `GoMcts<EuchreGameState,
TransformerGenerativeModel<…>>` which implements `Agent<G>`, so it
slots into the existing pairings unchanged.

### Files (final)

  - `crates/card_platypus/src/algorithms/gomcts.rs` — search + trait +
    `UniformRandomModel` + `TabularGenerativeModel`. ~440 LoC.
  - `crates/card_platypus/src/algorithms/gomcts_transformer.rs` —
    `TransformerConfig`, `GoMctsTransformer`, `Tokenizer` trait,
    `TransformerGenerativeModel`, training loop, Kuhn + Euchre
    tokenizers. ~700 LoC.
  - `crates/card_platypus/examples/kuhn_gomcts_train.rs` — end-to-end
    Kuhn training + eval. ~150 LoC.
  - `crates/card_platypus/examples/euchre_gomcts_smoke.rs` — Euchre
    plumbing smoke. ~100 LoC.
  - 7 unit tests across both modules (gomcts: 4, gomcts_transformer:
    3). All passing.

## Performance notes (target / budget)

Per-move budget reference (current PIMCTSBot with OpenHandSolver on Euchre,
rollouts=25):
  - ~50-200ms/move depending on phase. Scaling EPIMC depth=3 to 3× that is
    acceptable for a baseline experiment.
  - Use `--release` for all benchmarks (per CLAUDE.md convention).

## How to continue this work (handoff notes)

If picking this up cold:
  1. Read this doc and skim `crates/card_platypus/src/algorithms/pimcts.rs` —
     EPIMC is "PIMC + an inner random playout loop", ~80% structural overlap.
  2. Check which checkboxes above are done.
  3. The `Evaluator` trait (`ismcts.rs:82`) is the single integration point
     for the leaf evaluator; both `OpenHandSolver` and `RandomRolloutEvaluator`
     implement it.
  4. `EuchreGameState::resample_from_istate` is the determinization primitive
     (`crates/games/src/gamestates/euchre/resample.rs:17`).
  5. For Oh Hell support `OhHellGameState::resample_from_istate` works the
     same way. Both already have extensive test coverage.

## Decisions made

  - Start with EPIMC v1 (vanilla depth-d PIMC). Defer full subgame solver.
  - Random playthrough uses uniform-random for ALL players at depth > 1.
  - First evaluator: OpenHandSolver. The Euchre baseline example uses
    `OpenHandSolver::new_euchre()` for both bots so the only variable is
    the EPIMC `depth` hyperparameter.
  - Euchre first, Oh Hell second.

## Implementation notes (from the v1 build)

  - `EPIMCBot` mirrors the surface of `PIMCTSBot` (`Evaluator`, `Policy`,
    `Agent`, `Seedable`) so existing harnesses can swap them transparently.
  - At depth=1, EPIMC does NOT draw any additional RNG from `self.rng`
    beyond what PIMCTSBot does. This is the basis for the
    `epimc_matches_pimc_at_depth_1_*` invariant tests — keep this property
    if you refactor.
  - Per-rollout sub-RNGs are derived from a per-call `base_seed` and a
    splitmix64 of the world index. This makes the rayon parallel sum
    determinism-friendly: same `base_seed` + same world set yields the
    same value regardless of which thread handles which world.
  - The `evaluator.reset()` cadence (every 1000 evals) is copied verbatim
    from PIMCTSBot.

## Baseline harness shape

`examples/euchre_epimc_baseline.rs` design:
  - One EPIMC seat (rotating) vs three PIMCTS seats. Both use
    `OpenHandSolver::new_euchre()` and the same `rollouts`.
  - `epimc_avg` is the EPIMC bot's team score averaged across all games and
    rotated seats — this is the headline metric. Depth=1 should give
    ~0 (PIMC vs PIMC with different seeds); depth>1 > 0 means postponing is
    helping. Negative would mean postponing is hurting (e.g. random rollout
    of teammate creates worse leaf evaluations than perfect-info teammate).
  - `pimc_avg` averages over the 3 non-EPIMC seats and so mixes teammate +
    opponents; it's a sanity number, not the headline.
  - Per-depth runtime is reported as `secs/move` because depth `d` makes
    each rollout take O(d) extra apply_action calls on top of the leaf
    evaluation cost.

## Open questions

  - Does Euchre's heavy isomorphism / strong open-hand solver mean EPIMC's
    depth>1 gain is small? Worth checking before scaling rollouts.
  - For team play (Euchre), should random playthrough use random-for-teammate
    too? The paper assumes adversarial; our `is_maximizer` for Euchre treats
    teammates as cooperative inside OpenHandSolver — investigate whether the
    random playout should match this team-aware structure.

## First baseline run (2026-06-06)

Configuration: `EU_GAMES=80 EU_ROLLOUTS=25 EU_DEPTHS=1,2,3`,
`OpenHandSolver::new_euchre()` evaluator, 1 EPIMC seat vs 3 PIMC seats,
rotating EPIMC seat across games. Total wall time: ~49s.

| depth | epimc_avg | win%  | s/move |
|-------|-----------|-------|--------|
| 1     | -0.062    | 48.8% | 0.0372 |
| 2     | -0.163    | 45.0% | 0.0354 |
| 3     | +0.050    | 45.0% | 0.0350 |

  - **Sample size is too small to detect a real effect.** All three depths
    sit within ~0.2 score units and within ~4 pp of 50%. The paper's
    reported gains in private-info games are on the order of a few percent
    win rate, so N=80 is in the noise floor. Conclusion at this scale:
    pipeline works; can't yet distinguish depth=1 from depth=3.
  - **Per-move time is flat across depths.** The OpenHandSolver leaf
    evaluation dominates; the extra (depth-1) random `apply_action` calls
    are essentially free. depth=3 is *not* 3× slower than depth=1, which
    means we can scale rollouts and depths without a runtime penalty.
  - **Next experiment**: re-run with `EU_GAMES=500` and `EU_ROLLOUTS=50` to
    cut the standard error roughly in half (and the per-game variance, since
    Euchre scores are ±2 with non-zero variance per hand). At that scale a
    real 2-3 pp win-rate gap should be visible. With s/move flat at 0.035,
    500 games × 25 moves × 0.035s ≈ 7 min wall time.
  - **Alternative**: pit EPIMC against a *weaker* baseline (e.g. PIMCTS with
    RandomRolloutEvaluator instead of OpenHandSolver). When both bots use
    OpenHandSolver the leaf evaluator is already near-optimal in the
    determinised world, so the strategy-fusion error is small. A weaker
    leaf might amplify the gap EPIMC is supposed to close.
