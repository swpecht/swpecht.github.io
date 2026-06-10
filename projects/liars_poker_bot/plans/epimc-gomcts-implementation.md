# EPIMC and GO-MCTS implementation plan

Goal: implement two recent imperfect-information search algorithms in this
codebase and baseline them on Euchre (and later Oh Hell).

  - **EPIMC** — "Perfect Information Monte Carlo with Postponing Reasoning",
    Arjonilla, Saffidine, Cazenave (CoG 2024). arXiv: 2408.02380.
  - **GO-MCTS** — "Transformer Based Planning in the Observation Space with
    Applications to Trick Taking Card Games" (2024). arXiv: 2404.13150.

## Experiment log

Compact timeline. Each row: what was tried, key config, headline result,
what we concluded. Detailed notes for each are in the sections below.

| # | when | what | config / knobs | result | conclusion |
|---|---|---|---|---|---|
| 1 | 2026-06-06 | EPIMC depth sweep on Euchre | depths {1,2,3}, OpenHandSolver leaf, 25 rollouts, n=80 games per depth | all depths 45-49% win rate vs PIMCTS opponents; differences within noise | depth>1 not clearly beneficial; OpenHandSolver dominates per-move time so depth scaling was "free" but no signal at this n |
| 2 | 2026-06-06 | EPIMC depths {1-5} in full tournament | 28 pairings × 1000 matches | killed mid-flight (~9% done) | not the highest priority; pivoted to GO-MCTS |
| 3 | 2026-06-06 | GO-MCTS v1 Kuhn smoke | uniform model + UCT, 100 iters | sanity passes | search infra works |
| 4 | 2026-06-06 | GO-MCTS tabular Kuhn | 5k self-play games, per-action visit + value table | King-policy bets > Jack-policy bets | value-driven softmax sampling needed; visit-proportional doesn't learn |
| 5 | 2026-06-06 | GO-MCTS transformer + Kuhn | d=32/2L smoke, AdamW, 6 iter × 1000 games | mean reward −0.21 → +0.05 to +0.13 vs uniform | pipeline works, network learns; not at Nash |
| 6 | 2026-06-06 | GO-MCTS Euchre smoke config | d=64/2L, 4 iter × 150 games, MCTS=16 | **+0.217 ± 0.046** vs random at n=2000 | first real Euchre transformer; clear win above baseline |
| 7 | 2026-06-06 | Medium config | d=128/4L, 6 iter × 200 games | +0.160 raw vs random (worse than smoke) | bigger model overfit; data budget too small for capacity |
| 8 | 2026-06-06 | Long medium | d=128/4L, 10 iter × 300 games | +0.164 raw (no better) | more training didn't help; not a capacity issue |
| 9 | 2026-06-07 | Paper-default config | d=256/8L/8H, 6 iter × 200 games | +0.198 raw (tied with smoke) | confirmed: model size is not the bottleneck for self-play |
| 10 | 2026-06-07 | E1: MCTS budget sweep at inference | smoke ckpt, MCTS ∈ {0,32,128,512} | +0.40 at MCTS=128 (n=200 → noisy CI); MCTS=512 regresses | bias-variance peak at moderate search; more search hits diminishing returns |
| 11 | 2026-06-07 | E1 tournament: gomcts vs pimcts | smoke ckpt + MCTS=128, n=5 matches | 0–5, 23% point share | gomcts at this scale loses badly to PIMCTS; killed early |
| 12 | 2026-06-07 | AlphaZero value targets | paper config, MCTS-root-value targets (not terminal), 20 iters | +0.198 raw, h2h oscillates ±0.20 across iters | self-play plateau is real; not a target-quality issue |
| 13 | 2026-06-07 | CUDA backend enabled | gpu_cuda cargo feature + /usr/local/cuda-12.6 | Kuhn smoke ~10× faster; Euchre paper only ~1.1× (sequential batch=1) | GPU was barely used due to per-decision batch=1 ioctls in WSL; need batching |
| 14 | 2026-06-07 | E5: cross-game batched self-play | one service thread + N game threads via mpsc; one big forward per batched step | **17× throughput** at batch=32; GPU util 39% → 89% | this is the throughput win that unlocked paper-scale training |
| 15 | 2026-06-07 | Batched eval / h2h / pop | same pattern as E5 for the non-self-play phases | iter time 200s → 100s expected | misc speedup, makes longer runs tractable |
| 16 | 2026-06-07 | Train mini-batch sweep | batch ∈ {64, 128, 256, 512, 1024} | 256 = sweet spot (1.5× over 64); 1024 OOMs | EU_BATCH_SIZE=256 from now on |
| 17 | 2026-06-07 | CFR bootstrap (cfr3-vs-cfr3 supervised) | 100k games, paper config, 25 epochs, batch=512 | loss 2.20 → 2.23 over 25 epochs (LM ~30% on correct action) | data collection 33 min + train 90 min; doable |
| 18 | 2026-06-07 | Bootstrap eval (ArgmaxVal\*) | n=2000 vs random, MCTS=16 | **raw −0.118**, gomcts −0.036 | worse than random! diagnosed: value head never saw counterfactual actions |
| 19 | 2026-06-07 | Bootstrap eval (LM-softmax) | same weights, EU_INFER=lm | **raw +0.350**, gomcts −0.019 | bootstrap LM head learned cfr3; value head broken for ArgmaxVal\* |
| 20 | 2026-06-07 | Post-cfr3-bootstrap self-play | paper, ~8 iter × 750 games, MCTS=64, rollout=4, lr=1e-4 | wall ballooned to 9h+ (iter 8 took 4.1h alone), 3 OOMs, mean_reward bounced ±0.05 around 0 | self-destruct loop: cfr3 LM head + broken V head → MCTS soft targets are noise → bootstrap LM head degrades to uniform |
| 21 | 2026-06-07 | Diagnosed cfr3-bootstrap failure | analytical | LM head learned cfr3 great; V head can't extrapolate to counterfactual actions because cfr3's policy is sharply peaked | for ArgmaxVal\* to work, V needs counterfactual coverage of the action space. cfr3 doesn't provide it; random does |
| 22 | 2026-06-08 | Virtual-loss parallel MCTS | added GoMctsConfig.n_parallel_sims + Sim state machine + apply_virtual_loss + backup_with_virtual_loss in GoMcts | k=8 parallel sims share the tree, batched leaf-value forwards, virtual loss = 1.0 per AlphaZero convention | unlocks K× larger per-call GPU batch within one game |
| 23 | 2026-06-08 | Random bootstrap (paper-faithful) | 1M uniform-random Euchre games, paper config, batch=512, lr=1e-4, 15 epochs | **raw ArgmaxVal\* = +0.219 ± 0.046**, LM-mode = −0.061 (just imitates random) | mirror-asymmetry of cfr3 case: V works now, LM is weak. ~8h wall (mostly training) |
| 24 | 2026-06-08 | Post-random-bootstrap self-play | paper config, 10 iter × 750 games × MCTS=32 + parallel_sims=8, rollout=0, lr=1e-4 | 71 min wall (parallel sims worked), loss 2.02 → 1.87, but **mean_reward regressed +0.219 → +0.135** at n=2000 | self-play didn't compound the bootstrap; soft targets at MCTS=32 are too noisy at our scale. Bootstrap survived (no destruction) but didn't improve |
| 25 | 2026-06-08 | Difficulty tournament (random_bootstrap weights, MCTS=16) | gomcts (random_bootstrap, paper config) vs pimcts, cfr0, random; 30 matches/pair | **gomcts vs pimcts: 0-30 (18.6 pt%)**, **gomcts vs cfr0: 0-30 (21.2 pt%)**, gomcts vs random: 23-7 (63.1 pt%), pimcts vs cfr0: 12-18 (45.4 pt%) | **targets NOT met**. gomcts is solidly above random but ~4× below pimcts/cfr0. Slight improvement vs cfr0 (15.4% → 21.2%) over the smoke-config baseline tournament |
| 26 | 2026-06-08→09 | tch-rs port + CUDA-graph forward | dropped candle entirely; TF32; thread caps; batch retunes | ~5× self-play throughput, −49% iter wall, then −20% more | see commits c501aed…ab82b61. Set up for rollout-to-terminal runs that never happened (see 27) |
| 27 | 2026-06-10 | **Machine reboot wiped /tmp** | — | ALL checkpoints lost (smoke/medium/paper ckpts, cfr3 + random bootstraps) + libtorch itself | everything default-pathed under /tmp. Fixed: libtorch → ~/libtorch, ckpts/datasets → /home/steven/card_platypus/gomcts/, bootstrap collection now caches its dataset to disk (re-train without re-collect) |
| 28 | 2026-06-10 | Re-read paper closely (ar5iv full text) | — | 5 implementation discrepancies found, 2 of them load-bearing (see "Paper-faithfulness audit") | (a) paper does NO search during self-play training — greedy ArgmaxVal\* live seat vs LM-sampling frozen seats, live data only; (b) ArgmaxVal\* is λ-gated by the LM head (p ≥ 0.01–0.05) — directly mitigates our "broken V head" failure; (c) value = outcome-class distribution (CE), not scalar MSE; (d) rollout is N_steps-capped (2–5), NOT to-terminal; (e) Skat bootstrap needed random + weak-bot MIX — pure random was "unstable" (precedent for our cfr3+ε mix) |
| 29 | 2026-06-10 | ε-exploration bootstrap (cfr3_eps) | 200k games, all seats cfr3 with ε=0.15 uniform deviations; exploration moves recorded with policy_weight=0 (V trains, LM masked); paper config, 6 epochs, batch=256, lr=1e-4 | 4.66M examples (700k exploration), 49 min collect (68 games/s × 24 threads) + 61 min train; loss 0.48 → 0.29 | hypothesis H1: get cfr3-strength LM head AND a working V head (counterfactual coverage at cfr3-reachable states) in one bootstrap. Unit test `policy_weight_masks_lm_but_trains_value` pins the mask semantics |
| 30 | 2026-06-10 | **H1 VALIDATED**: ε-bootstrap eval, n=2000 raw vs random | EU_INFER ∈ {lm, argmax, gated λ=.05 temp=.05} | **lm +0.578 ± 0.046**, **argmax +0.336**, **gated +0.594** | every prior record beaten. V head fixed: ungated ArgmaxVal\* went −0.118 (pure cfr3) → +0.336; ε-counterfactual coverage was the missing ingredient. λ-gate + near-greedy temp is best (+0.594) — beats the old *search-wrapped* best (+0.358) with zero search. tch eval: n=2000 in 2 s (was 76 s) |
| 31 | 2026-06-10 | MCTS=100 wrap over ε-bootstrap (n=400, gated) | EU_MCTS_ITER=100, same seeds' raw=+0.725±0.10 | gomcts **+0.625 ± 0.10** | search does NOT improve on the raw gated policy vs random opponents (overlapping CIs, trending worse), at 4.6 s/hand. Use raw gated for tournaments; revisit search only vs strong opponents |
| 32 | 2026-06-10 | Found+fixed ITER=0 uniform bug | GoMcts::run_search zero-visit fallback returned uniform, not model policy | first tournament arm at ITER=0 opened 0-8 vs cfr0 (4% pts) while the same weights were +0.59 raw | every past "mcts_iter=0 ≈ raw" claim was wrong; now falls back to `model.policy()`. Re-ran |
| 33 | 2026-06-10 | **Difficulty tournament: ε-bootstrap raw gated** | EUCHRE_GOMCTS_ITER=0 (raw policy), INFER=gated λ=.05 t=.05, 30 matches/pair | **vs cfr0: 12-18, 45.8% pts** (was 0-30, 21.2%); **vs pimcts: 14-16, 48.8% pts** (was 0-30, 18.6%); vs random 30-0, 85.6%; calibration cfr0-vs-pimcts 15-15 | statistical tie with pimcts, near-tie with cfr0, with NO search at 2 ms/decision. The two original targets are within ~1 SE; "beat cfr0" needs a few more points |
| 34 | 2026-06-10 | LM-mode arm vs cfr0 | INFER=lm, ITER=0, 50 matches | 22-28 (44%), **48.2% pts** over 496 hands | pure cfr3-imitation is ≥ gated vs cfr0 (48.2 vs 45.8 pt%, within noise). cfr3-imitation alone nearly ties cfr0 |
| 35 | 2026-06-10 | MCTS=100 arm vs cfr0 (killed early) | INFER=gated, ITER=100 | 2-4 after 6 matches, 37.8% pts, ~28 s/hand | search trending worse than raw vs cfr0 too (cf. entry 31 vs random). GO-MCTS search over this V head adds nothing at any opponent strength — V noise compounds in the tree. Killed to free GPU |
| 36 | 2026-06-10 | Categorical outcome head (paper Eq. 1) A/B | same cached 4.66M dataset, 6-class CE over {±1,±2,±4}, 6 epochs | n=2000 raw: lm +0.580 / argmax +0.323 / gated +0.559 (scalar: +0.578/+0.336/+0.594) — equivalent. **vs cfr0 (gated, 50 matches): 24-26, 48.1% pts** — best arm yet | head type doesn't move raw strength at this data scale; outcome head ≥ scalar vs cfr0 (48.1 vs 45.8 within noise). Dataset cache made this A/B collection-free (2 s load vs 49 min) |
| 37 | 2026-06-10 | Paper-loop self-play from outcome bootstrap | EU_PAPER_LOOP=1, 10 iters × 20k games, λ=.05 t=.05 ε=.10, 3 epochs lr=1e-4, **83 s/iter** (was ~26 min) | h2h vs prev snapshot ≈ parity all 10 iters (mean ~0.00). But final raw vs random REGRESSED: lm +0.580→+0.445, gated +0.559→+0.457; **vs cfr0: 14-36, 36.9% pts** (bootstrap: 48.1%) | first self-play that didn't *collapse* the bootstrap — but it still erodes it slowly. Root cause is structural: our live data generator (gated argmax ≈ cfr0-tie) is WEAKER than the bootstrap's data source (cfr3), so cloning own-play regresses toward it. The paper's setup escapes this because their bootstrap was random play — their greedy player exceeded its data. Self-play canNOT exceed a strong-expert bootstrap by behavior cloning alone |
| 38 | 2026-06-10 | **cfr3 vs cfr0 ceiling measurement** | difficulty bench, 50 + 200 matches (2 seeds) | pooled: 116-134 (46.4%), **48.5% pts over ~2400 hands** | cfr3 does NOT beat cfr0 in match play (if anything marginally behind on points). Structural: both = identical CFR bidding + identical PIMCTS card play; they differ only on the first ~3 card plays. So (a) our 48.1% bootstrap is already AT the cfr3-imitation ceiling, (b) beating cfr0 requires more than imitating any cfr agent. → exploiter bootstrap (entry 39) |
| 39 | 2026-06-10 | Exploiter bootstrap (cfr3+ε vs cfr0 data) | 200k games, hero team cfr3+ε=0.15 recorded, opponents cfr0; 2.36M examples; fine-tune 3 epochs lr=1e-4 from outcome bootstrap | vs random: lm +0.613, gated +0.585 (generality intact, slightly up). **vs cfr0 (100 matches): 42-58, 45.6% pts** | **hypothesis NOT confirmed** — vs-cfr0 value targets didn't add points over the plain bootstrap (45.6 vs 48.1, within noise; both ≈ cfr3's 48.4). Likely: (a) the λ-gate + near-greedy temp means V only arbitrates among 2-3 cfr3-plausible moves — little room to express a best response; (b) cfr0's bidding is near-Nash and its play is PIMCTS — the exploitable margin for this function class appears to be ~0 |

### Paper-faithfulness audit (2026-06-10)

Re-read arXiv 2404.13150 end-to-end after the self-play plateau. Where
our implementation diverges, with severity:

| # | paper | ours (before 06-10) | severity / action |
|---|---|---|---|
| 1 | **No MCTS during training.** Self-play data = greedy ArgmaxVal\* (live, current weights) vs 3 seats LM-sampling a uniformly-drawn previous iteration; **only the live seat's data trains**. GO-MCTS used only at final eval | MCTS-soft-target AlphaZero loop + all-seats data | **load-bearing.** Also explains cost: paper games are ~30 forwards each, no search. Implemented as `EU_PAPER_LOOP=1` (collect_paper_pop_examples_batched_tch) |
| 2 | **ArgmaxVal\* is λ-gated**: only actions with LM-prob ≥ λ (0.05 Hearts/Skat, 0.01 Crew) compete on value; deterministic argmax | un-gated softmax(V/0.5) sampling | **load-bearing** for the broken-V failure (entry 18): the gate would have excluded the actions V couldn't rank. Implemented: EU_INFER=gated + EU_LAMBDA + EU_TEMP in eval; EUCHRE_GOMCTS_INFER=gated in difficulty bench; RemoteModel.with_inference |
| 3 | Value head = **outcome-class distribution** (CE over discrete outcomes; Hearts 2234, Skat 397, Crew 2), V = Σ p(o)·v(o) | scalar V + MSE | medium. Euchre has ~6 outcome classes (±1, ±2, ±4) — natural categorical head. Do if V quality still limits after ε-bootstrap |
| 4 | Rollout = N_steps player moves (2–5) then model V at leaf; real outcome only if terminal reached | rollout_to_terminal mode (built 06-09) rolls to terminal always | minor; rollout-to-terminal is *more* grounded but slower. The 06-09 mode was mislabeled "paper-faithful" |
| 5 | AdamW lr=1e-4, **3 epochs**/iter, loss 0.9/0.1 | lr ok; we used up to 25 epochs on bootstrap (loss flat after ~1) | minor; 25 was wasted compute. New default 6 |

Other paper numbers for calibration: bootstrap = 4M random games
(Hearts/Crew); Skat needed 4M games from random+XSkat permutations
because pure random was **unstable** — supports the cfr3+ε mixture.
Self-play scale 500k–2M games/iter × 10–20 iters. Eval MCTS: 100
runs/decision, C 0.1–0.4, λ 0.01–0.05, μ 0.01.

| 40 | 2026-06-10 | Inference sweep (exploiter ckpt) vs cfr0 | 100 matches/arm: gated λ∈{.02,.05} t∈{.05,.2}, ungated argmax, lm | lm 47.7% > gated .05/.2 45.9% > gated .05/.05 45.6% > gated .02 44.7% ≫ ungated argmax 34.6% | the LM head (cfr3 imitation) carries everything vs cfr0; the V head only subtracts when given more freedom. All modes saturate at cfr3's own 48.4%. cfr0 holds a small consistent edge (~2-4 pts share) over the entire cfr/pimcts/transformer family |
| 41 | 2026-06-10 | **Greedy-LM mode** (temp 0.05 instead of sampled temp 1.0) vs cfr0 | 100 + 300 matches, 2 seeds | n=100: 52-48, 50.4% pts (first ≥50% arm!); n=300: 48.3%, 49.1% pts. Pooled: **49.3% ± 1.3 over ~3.9k hands** | greedy imitation > sampled imitation (+1.5pts) and is the best inference mode overall — but it lands exactly AT the cfr3 ceiling (48.5%), i.e. statistical parity with cfr0, not superiority |

| 42 | 2026-06-10 | pimcts-200 vs cfr0 (stronger-expert existence check) | EUCHRE_PIMCTS_ROLLOUTS=200, 100 matches | 51-49, **50.2% pts** | 4× search budget is ALSO only parity. Every strong agent in the codebase (cfr3, pimcts-50, pimcts-200, our transformer) lands at 48-50% vs cfr0 — cfr0 is at the practical skill frontier of this family; Euchre match play has a thin skill margin among strong agents |

| 43 | 2026-06-10 | **Combined 7M bootstrap (final model of the session)** | cfr3_eps 4.66M + exploiter 2.36M examples merged, outcome head, 6 epochs; eval greedy-LM | raw vs random: greedy-LM **+0.649** (session best), lm-sampled +0.628, gated +0.598. Tournament (greedy-LM): **vs cfr0 49.2% pts (n=3002 hands)** — tie; **vs pimcts 59-41, 52.7% pts** — first measured WIN over pimcts (~1.8 SE); vs random 100-0, 87.4% | checkpoint: /home/steven/card_platypus/gomcts/bootstrap_combined.safetensors, EU_VHEAD=outcome EU_CONFIG=paper, inference = greedy-LM (INFER=lm TEMP=0.05, no search, ~1 ms/decision) |

### Session assessment (2026-06-10, final)

**Where we ended**: `bootstrap_combined.safetensors` + greedy-LM inference is
the strongest GO-MCTS agent to date: ties cfr0 (49.2% pts, n=3002 hands),
beats pimcts (52.7% pts), +0.649/hand vs random — at ~1 ms/decision with no
search. At the season start the best transformer sat at 21.2% pts vs cfr0.

**Original targets**: "beat PIMCTS" — met (52.7%, marginal significance);
"tie cfr0" — met (49.2%, n=3002). "MORE powerful than cfr0" (today's goal) —
**not met, and the evidence says it is not reachable by any agent in this
codebase**: cfr3 scores 48.5% vs cfr0 (n≈2400 hands), pimcts-200 scores
50.2%. cfr0 = near-Nash CFR bidding + PIMCTS play sits at the practical
skill frontier of Euchre match play; the per-hand skill margin between any
two strong agents is ~0-2 points of share, swamped by deal variance.

**Ruled out this session** (each with measurements):
  - GO-MCTS search over the learned model: subtracts at every budget
    (entries 31, 35) — V-head noise compounds in the tree.
  - Paper-style self-play: holds h2h parity but erodes absolute strength;
    structurally cannot exceed a strong-expert bootstrap (entry 37).
  - Exploiter value training vs cfr0: no gain (entry 39) — the λ-gate
    leaves V too little room, and cfr0's exploitable margin for this
    function class is ≈ 0.
  - Gate/temperature sweeps: greedy-LM dominates; every V-involving mode
    is equal or worse vs strong opponents (entries 40-41).

**If "beat cfr0 clearly" is still the goal**, the honest options are:
  1. accept that the metric may be saturated: validate with a much larger
     cfr0 pairing (1000+ matches) whether ANY agent separates from cfr0;
  2. paper-scale self-play compute (5-10M games/iter × 10-20 iters) from a
     random bootstrap — the paper's actual regime, untested here;
  3. an expert stronger than the cfr family to imitate (none exists in the
     codebase today — building one, e.g. deep EPIMC or solver-grade play,
     is its own project).

### Headline numbers so far

| condition | mean vs random (n=2000) | 95% CI |
|---|---|---|
| **ε-bootstrap raw (gated ArgmaxVal\*, λ=.05 t=.05)** | **+0.594** | [+0.506, +0.684] |
| **ε-bootstrap raw (LM-policy)** | **+0.578** | [+0.488, +0.668] |
| ε-bootstrap raw (ungated ArgmaxVal\*, t=.5) | +0.336 | [+0.247, +0.425] |
| Random baseline | +0.068 | [−0.023, +0.159] |
| GO-MCTS smoke + MCTS=16 search | +0.358 | [+0.268, +0.448] |
| **cfr3 bootstrap raw (LM-policy)** | **+0.350** | [+0.261, +0.439] |
| **Random bootstrap raw (ArgmaxVal\*)** | **+0.219** | [+0.129, +0.309] |
| GO-MCTS smoke raw | +0.217 | [+0.127, +0.307] |
| AlphaZero paper-config raw | +0.198 | [+0.106, +0.289] |
| Post-random-bootstrap self-play final | +0.135 | [+0.044, +0.226] |
| cfr3 bootstrap raw (ArgmaxVal\* — broken) | −0.118 | [−0.210, −0.026] |
| Random bootstrap raw (LM-policy — weak) | −0.061 | [−0.154, +0.032] |

### Targets (FINAL — not met)

- **Beat PIMCTS**: gomcts (random_bootstrap, MCTS=16, paper config) lost **0-30** to pimcts in difficulty tournament (18.6% point share). Smoke at MCTS=128 lost 0-5. Gap closed slightly but PIMCTS still dominates.
- **Tie cfr0** (~50% point share): gomcts lost **0-30** with **21.2% point share**. Previous smoke baseline was 15.4% point share. ~7pp closer to the target, still ~3 SE short of "tie".

### Why the targets weren't met (final analysis)

  - **PIMCTS does ~50 perfect-info game-tree solves per move** at its leaf (each is essentially alpha-beta to terminal). Our MCTS=16 with a transformer leaf is ~3 orders of magnitude less per-decision work.
  - **cfr0 adds CFR-trained bidding** on top of PIMCTS-style play — even stronger.
  - **Self-play didn't compound** at our scale. The random bootstrap's value head had a real but noisy signal; MCTS at 32 sims didn't average out the noise enough to produce useful soft targets. After 10 iters we had a worse model than the bootstrap alone.
  - **Total self-play games used: ~7500**. Paper used 5M-10M. We're ~3 orders of magnitude below the paper's self-play compute budget. At our scale, self-play looks like noise-injection on top of a decent bootstrap, not refinement.
  - **The transformer's value head is structurally weaker than OpenHandSolver**. To match PIMCTS the V head would need to closely approximate `E_w[OpenHandSolver(w) | observed history]` — which is what E2 (PIMCTS-bootstrap) would target. We deliberately skipped that.

### Decisions / lessons learned

  - **Model capacity is not the bottleneck** for self-play from random init. Smoke/medium/paper all plateau around +0.20 vs random.
  - **WSL2 ioctl per kernel launch** was the GPU bottleneck. Batched inference (E5) fixed it.
  - **Terminal payoff is a noisy value target** in imperfect-info games. AlphaZero MCTS-root-value targets are smoother but don't fix the cold-start problem.
  - **Bootstrap is the cold-start fix.** Supervised from cfr3 jumped raw policy from +0.22 to +0.35 immediately. But ArgmaxVal\* needs counterfactual training — MCTS self-play has to follow.
  - **ArgmaxVal\* requires counterfactual value training.** A network whose value head only ever saw `V(h⊕action_taken)` produces ~identical V across alternative actions; ArgmaxVal\* becomes random. LM-head softmax is the right inference mode for supervised-only models; ArgmaxVal\* is the right mode after self-play refinement.
  - **The bootstrap-data distribution determines which head works.** cfr3 bootstrap → strong LM, broken V (cfr3 is sharply peaked, no counterfactual coverage). Random bootstrap → strong V, weak LM (random covers the action space but doesn't tell you which action is good). The paper chose random for a reason: it's the right setup for ArgmaxVal\*-then-self-play.
  - **At our compute scale, self-play does not refine the random bootstrap.** It modestly degrades it. The paper used 5M-10M self-play games; we used ~7500. The MCTS=32 visit distributions over our 750-game iterations are too noisy to provide useful soft policy targets.
  - **The remaining gap to PIMCTS is structural**, not a hyperparameter / training-scheme tweak away. PIMCTS does ~50 full-game perfect-info solves per move; our V head approximates a single such value in one forward pass but with substantial error. Closing that gap with our current approach would need a value head that's a strong approximator of `E_w[OpenHandSolver(w)]` (E2 territory, user-excluded), or paper-scale self-play (5M+ games), or much larger inference-time MCTS (MCTS≥256, ~6 min/move).

---

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
