# R-NaD (Regularized Nash Dynamics) on the GO-MCTS transformer

Goal: implement DeepNash-style R-NaD (Perolat et al., Science 2022) and use it
to push the Euchre transformer past the behavior-cloning ceiling identified in
`epimc-gomcts-implementation.md` entry 40 (self-play at small scale degrades the
bootstrap; BC alone can't exceed the teacher).

Why R-NaD over the alternatives considered (ReBeL / Student of Games):
- Belief-state methods are sound only for 2-player zero-sum; the machinery
  (public belief states + CFR subgame solving) breaks on 4-player partnership
  Euchre, and the engineering cost is high.
- R-NaD is model-free, search-free at inference (one forward pass — exactly the
  regime the experiment log already found strongest: greedy-LM beat MCTS-100),
  and extends mechanically to team play.
- The regularization toward π_reg gives self-play a convergence force that the
  paper-loop population self-play lacked (entry 33–40: plateau/decay).

## Algorithm

Three repeating steps:
1. **Reward transform**: rewards augmented with −η·log(π(a|h)/π_reg(a|h)) for
   the acting player's team and +η·(same) for the opposing team. Keeps the
   transformed game team-zero-sum. `team = player % 2` (works for both 2p
   zero-sum and Euchre's 0&2-vs-1&3).
2. **Dynamics**: NeuRD policy updates + MC value regression on the transformed
   returns until the policy nears the regularized fixed point.
3. **Update**: π_reg ← π every `reg_update_every` learner steps.

In 2p zero-sum the fixed-point sequence converges to Nash (validated on Kuhn,
below). For 4p teams no guarantee carries over — pragmatic application.

## Implementation (`crates/card_platypus/src/algorithms/rnad.rs`)

- Policy = LM head masked to legal-action tokens, softmax temp 1.0.
  Value = V head at the prefix position (outcome-head expectation works
  unchanged). Backbone, tokenizers, batching service, snapshot machinery all
  reused from `gomcts_transformer.rs`.
- **Collection**: `collect_rnad_games_batched_tch` — scoped-thread +
  batching-service architecture; all seats sample the live policy (LmSoftmax,
  temp 1.0). Records (player, istate, action, legal, behavior_prob) per step.
- **Learner** (`RnadTrainer::learner_step`), three phases:
  - A (no-grad): batch-evaluate log πθ(a), log π_reg(a), V(h), centered legal
    logit, entropy, KL(πθ‖π_reg) for every step.
  - B (plain Rust): reverse-scan per trajectory → transformed returns G_t
    (terminal payoff + η log-ratio stream routed by team), advantages
    A_t = G_t − V(h_t), NeuRD coefficients
    coef = clip(1/π_b(a), is_clip) · A_t, gated by the NeuRD logit threshold β
    (no update pushing a centered logit beyond ±β).
  - C (grad): shuffled minibatches; loss = −coef·(centered legal logit of a)
    + value_weight·MSE(V, G). One persistent AdamW (built off the new
    `GoMctsTransformerTch::var_store()` accessor), grad-norm clip.
- Sampled-action NeuRD with 1/π importance correction matches the all-actions
  replicator update in expectation; the gradient is on the *centered legal
  logit*, not log π — that's the NeuRD distinction that keeps updates alive at
  near-deterministic policies.
- MC returns instead of v-trace: episodes are ≤ ~30 decisions and consumed
  on-policy immediately after collection.
- `RnadNetPolicy` adapts a net to the `Policy` trait for the exact
  exploitability oracle.

## Validation: Kuhn poker (entry 1)

`examples/kuhn_rnad_train.rs`, defaults (η=0.2, lr=5e-4, reg_every=50,
256 games/iter, kuhn_small net, from scratch):

- NashConv: 0.917 (uniform init) → ~0.08 by iter 300 (range 0.03–0.16,
  oscillating — the expected damped R-NaD oscillation around equilibrium).
- 0.08–0.12 s/iter on the 4080; 300 iters ≈ 30 s.
- Unit test `rnad_kuhn_smoke` covers NaN-freeness + sanity at 20 iters.

Confirms the reward transform + NeuRD dynamics find Nash in a game where we
can measure it exactly.

## Euchre run 1 (entry 2)

`examples/euchre_rnad_train.rs`: warm start from
`bootstrap_combined.safetensors` (paper config + categorical outcome head; the
strongest BC checkpoint: tie vs cfr0, 52.7% vs PIMCTS). π_reg starts as the BC
policy, so early dynamics are KL-tethered to the teacher.

Settings: 2000 iters × 256 games/iter = 512k self-play games (~70× the old
paper-loop volume), lr=3e-5, η=0.2, reg refresh every 200 iters (10 fixed-point
steps), eval every 50 iters (500 games vs random + 500 h2h vs the frozen init,
both greedy-LM temp 0.05), checkpoints every 200 iters under
`/home/steven/card_platypus/gomcts/rnad/`.

Key metrics to watch:
- `eval_vs_init` mean/win-rate: did R-NaD exceed the BC ceiling?
- `kl_reg` between refreshes: inner-loop convergence.
- `entropy`: BC init is sharp (H≈0.21); R-NaD should keep it stochastic where
  the equilibrium is mixed rather than collapsing.

Results (log: `/home/steven/card_platypus/gomcts/rnad/train_run1.log`, kestrel
`euchre-rnad-1`, 78 min wall on the 4080):

- **Entropy**: 0.21 (BC init) → ~0.40 within 250 iters and stable there. R-NaD
  re-opened mixed strategies the BC policy had collapsed; it did NOT collapse
  back — the η-regularization works as designed.
- **KL(reg)**: ~0.09 during the first fixed-point iteration, then 0.005–0.016
  after each refresh — inner dynamics converge between refreshes.
- **vs_init** (500-game h2h vs the BC bootstrap, greedy-LM both sides):
  climbed from −0.04 to ~+0.11–0.15 (peaks at iters ~350 and 1000–1150,
  six consecutive evals ≥ +0.10 in the 950–1150 window), then declined to
  −0.05…−0.13 over iters 1200–2000. Classic overshoot past the best fixed
  point; per-200-iter checkpoints make this recoverable by selection.
- **vs_random** improved overall: +0.49 (init, n=500 — consistent with the
  entry-30 +0.59 at n=2000 given SE) → frequent +0.75–0.89 evals late.
  Notably vs_random kept improving in the same window where vs_init declined
  — the late policy got stronger against weak opposition while drifting away
  from (and slightly below) the bootstrap head-to-head.

### Checkpoint selection (entry 3)

`examples/euchre_rnad_select.rs`: every per-200-iter checkpoint vs the BC
bootstrap, 2000 hands each, greedy-LM temp 0.05 both sides
(log: `/home/steven/card_platypus/gomcts/rnad/select_sweep.log`).

At n=2000 every checkpoint lands within ±0.08 of the bootstrap (best:
iter_01800 +0.081, wr 0.505; worst: iter_00600 −0.024). **The in-training
n=500 evals' apparent trends (peak +0.15, late −0.13) were mostly noise** —
SE on the mean payoff at n=500 is ~0.09.

Re-run of the top 3 at n=10000 (SE ≈ 0.024):
  rnad_final      +0.037  wr 0.494
  rnad_iter_01400 +0.042  wr 0.496
  rnad_iter_01800 +0.026  wr 0.491

Read: a small (~+0.03–0.04, ≈1.5σ) but consistent positive edge over the
bootstrap, roughly flat across the back half of the run — R-NaD nudged past
the BC ceiling rather than smashing it. Win-rate ≤ 0.5 with positive mean ⇒
the R-NaD policy loses slightly more hands but wins bigger ones (more
2/4-point swings). Selected `rnad_iter_01400` (pooled n=12k best) for the
tournament evals. Each 2000-hand h2h costs ~2.4 s on the 4080 — the batched
greedy-LM eval path is extremely cheap; future sweeps should default to
n≥10000.

### Tournament results (entry 4)

`euchre_difficulty_benchmark`, greedy-LM no search (INFER=lm TEMP=0.05
ITER=0), 300 matches per pairing — directly comparable to the entry-43
bootstrap numbers. Logs: `rnad1400_vs_{cfr0,pimcts}_300.log`.

| pairing | matches | match win% | point share | hands |
|---|---|---|---|---|
| rnad1400 vs cfr0   | 153–147 | **51.0%** | **50.0%** | 2967 |
| rnad1400 vs pimcts | 171–129 | **57.0%** | **53.8%** | 2859 |

Reference points (existing logs):
- bootstrap_combined vs cfr0: 46.3% matches, 49.2% pts (n=3002)
- bootstrap_combined vs pimcts: 59.0% matches (n=100), 52.7% pts (n=962)
- cfr3 vs cfr0: 46.5% matches, 48.4% pts → cfr0 is the strongest CFR tier
- pimcts vs cfr0: 51.0% matches, 50.2% pts → parity

**Conclusion: `rnad_iter_01400` (copied to `rnad_best.safetensors`) is now
the strongest player in the repo, by a hair**: exact point parity + positive
match record vs cfr0 (the previous champion), the largest measured margin
over pimcts (53.8% pts at n=2859, ≈4σ above 50%), and a small direct edge
over the BC bootstrap (+0.03–0.04/hand at n=12k). Still ~1 ms/decision,
no search.

### Lessons / next levers

1. **R-NaD's qualitative promises held**: entropy re-opened (0.21→0.40) and
   stayed open; inner dynamics converged between π_reg refreshes (KL
   stabilizing ~0.01); self-play did NOT collapse or degrade the bootstrap
   the way the entry-33–40 population loop did. The convergence force works.
2. **The quantitative gain was modest (+0.8pp vs cfr0, +1.1pp vs pimcts)**.
   At 512k self-play games we're still ~3 orders of magnitude below DeepNash
   scale; the surprise is that any measurable gain survived selection at all.
3. In-training evals at n=500 are decoration, not signal (SE≈0.09). Trust
   only the n≥10k sweeps; they cost seconds.
4. Late-run divergence (vs_random ↑ while vs_init ↓) suggests the regularized
   fixed-point sequence drifts somewhere genuinely different from the BC
   policy rather than strictly dominating it — consistent with R-NaD
   converging toward its own equilibrium, not toward "beat the teacher".
5. Untried levers, in expected-value order: (a) longer runs with slower
   π_reg refresh (more games per fixed point — DeepNash's actual regime);
   (b) η/lr schedule annealing; (c) v-trace + larger trajectory buffers for
   better advantage estimates; (d) categorical (outcome-head CE) value loss
   instead of MSE-on-expectation; (e) PIMCTS-opponent mixing in self-play to
   anchor the distribution near strong play.
