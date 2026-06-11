# R-NaD optimality probe: are the Euchre/Oh Hell bots near equilibrium?

Follow-up to `rnad-implementation.md`. The question: R-NaD's convergence
guarantee is two-player zero-sum only — did the fixed point it found in the
**3-player** Oh Hell game (and the 4p-team Euchre game) land anywhere near an
actual equilibrium, or did it just find a strong-but-exploitable policy?

## What "close to optimal" can mean here

Exact NashConv is intractable for both games. The operational substitute is
the **unilateral-deviation payoff**: a policy profile is an ε-equilibrium iff
no single deviator (single seat in OH; one team in Euchre, where partners act
as a unit) can gain more than ε against copies of the policy. Every concrete
deviator we test gives a *lower bound* on ε:

- **Trained NeuRD exploiter** (η=0 R-NaD learner, warm-started from the
  target): adaptive lower bound, limited by the function class + budget.
- **Static strong agents** (PIMCTS-50, CFR bid weights) seated as the
  deviator: cheap, non-neural lower bounds with totally different failure
  modes than the target's own function class.

The prior Euchre measurement (rnad-implementation entry 5) found nothing but
was flagged exploiter-bounded: target played at deployed greedy temp 0.05
only, 1000-iter budget, no positive control. This probe fixes all three.

## Probe matrix

| # | game | deviator | target | target temp | budget |
|---|---|---|---|---|---|
| 1 | OH | NeuRD single-seat exploiter | rnad_best | 0.05 (deployed) | 1500 it × 256 g |
| 2 | OH | NeuRD single-seat exploiter | rnad_best | 1.0 (policy distribution) | 1500 it |
| 3 | OH | NeuRD single-seat exploiter | **bootstrap_v2 (positive control)** | 0.05 | 1500 it |
| 4 | Euchre | NeuRD team exploiter | rnad_best | **1.0** (0.05 already measured ≈ 0) | 2000 it |
| 5 | OH | PIMCTS-50, one seat | 2 × rnad_best | 0.05 | 300 g/trick, t1–10 |

Probe 3 is the validity check for the whole method: bootstrap_v2 is known-weak
(rnad_best scores +1.26/hand against it), so the single-seat NeuRD exploiter
must find a large positive EV there. If it does, a null result on probes 1–2
is evidence of near-equilibrium rather than exploiter blindness. If it
doesn't, the NeuRD-exploiter bounds are uninformative and only probe 5 counts.

Pre-registered interpretation of the OH numbers (mean-centred payoffs,
range ±~10/hand; rnad_best beats PIMCTS by +1.19/hand for scale):
- exploit_ev ≲ +0.1: at the measurement floor — call it ε-equilibrium at the
  resolution we can see.
- +0.1 – +0.5: measurably exploitable but small relative to the field; R-NaD
  "worked in practice" with a real but bounded gap.
- ≳ +0.5: the >2-player fixed point is materially exploitable; the 2p story
  does not transfer.

## Infrastructure added

- `ExploiterSeating::{Team, SingleSeat}` on
  `collect_exploiter_games_batched_tch` — SingleSeat rotates one trainable
  seat through all positions, frozen target elsewhere; only that seat's
  decisions train.
- `eval_subject_vs_net_batched_tch` (rnad.rs): subject net at one rotating
  seat vs reference net at the rest, both greedy-LM — the deviation-payoff
  evaluator.
- `examples/oh_hell_rnad_exploit.rs`: trick counts cycled t1–10 in
  collection; exploit EV reported pooled (n=3000/eval) and at t∈{2,5,8}.
- `oh_hell_gomcts_eval` gained `OH_OPPONENT=model`: with `OH_SUBJECT=pimcts`
  this seats one PIMCTS deviator against two copies of the checkpoint
  (probe 5 — the reverse direction of the tournament evals, which only ever
  measured the model as the odd seat out).

Logs land in `oh_hell/rnad/exploit_*.log`, `oh_hell/rnad/probe_pimcts_deviator.log`,
and `gomcts/rnad/exploit_rnad_best_t100.log`.

## Results

(pending)
