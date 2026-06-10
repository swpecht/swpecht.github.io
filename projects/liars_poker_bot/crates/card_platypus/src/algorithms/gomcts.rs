//! GO-MCTS: MCTS in the observation space, with a generative model
//! providing opponent action samples and value estimates.
//!
//! Reference: "Transformer Based Planning in the Observation Space with
//! Applications to Trick Taking Card Games", arXiv:2404.13150.
//!
//! See `plans/epimc-gomcts-implementation.md` § "GO-MCTS implementation
//! plan" for the design rationale and the deliberate "hybrid" simplification
//! we make versus the paper (we maintain a determinised true state during
//! each simulation so that game rules supply `legal_actions` /
//! `cur_player` / `is_terminal` / `evaluate`, instead of asking the
//! generative model to learn those). This lets us ship a working algorithm
//! without a transformer ML stack — when we eventually plug in a trained
//! model, the `GenerativeModel` impl is the only thing that changes.
//!
//! v1 scope:
//!   * Search algorithm: AlphaZero-style (no rollout phase; leaf value
//!     comes from `model.value()` directly). Penalty μ on detected illegal
//!     trajectories.
//!   * Models: `UniformRandomModel` (no learning) and
//!     `TabularGenerativeModel` (per-IStateKey policy/value table, trained
//!     via self-play).
//!   * Tested on Kuhn Poker (small enough for tabular). Euchre is wired up
//!     as a smoke test only — quantitative Euchre numbers wait for a real
//!     transformer model.

use std::{collections::HashMap, marker::PhantomData};

use games::{
    actions, istate::IStateKey, resample::ResampleFromInfoState, Action, GameState, Player,
};
use rand::{
    rngs::StdRng,
    seq::{IndexedRandom, SliceRandom},
    RngExt, SeedableRng,
};
use rustc_hash::FxHashMap;

use crate::{agents::Agent, collections::actionvec::ActionVec, policy::Policy};

/// The model side of GO-MCTS. Sees observation histories from the search
/// player's POV and returns action samples, values, and policy priors.
///
/// In the paper this is a transformer; here we ship a uniform-random
/// fallback and a tabular learner. The trait is intentionally narrow so a
/// future transformer impl drops in cleanly.
pub trait GenerativeModel<G: GameState>: Send {
    /// Sample one of `legal` actions, weighted by the model's policy over
    /// this observation history. v1 fallback is uniform-over-legal.
    fn sample(&mut self, history: &IStateKey, legal: &[Action], rng: &mut StdRng) -> Action;

    /// Value estimate at this history, from the search player's POV.
    /// Used at leaf expansion. Return 0 if unsure.
    fn value(&mut self, history: &IStateKey) -> f64;

    /// Probability mass over `legal`. Returned vector aligns with `legal`.
    /// Used as a UCT prior; defaults to uniform.
    fn policy(&mut self, history: &IStateKey, legal: &[Action]) -> Vec<f64> {
        vec![1.0 / legal.len() as f64; legal.len()]
    }

    /// Batched leaf-value query. Default impl just loops `value()`; the
    /// real win comes from overriding this in `RemoteModel` /
    /// `TransformerGenerativeModel` to make ONE forward over the entire
    /// `histories` slice. Used by `run_search_parallel` to merge K
    /// concurrent sims' leaf evaluations into a single GPU call.
    fn batch_value(&mut self, histories: &[IStateKey]) -> Vec<f64> {
        histories.iter().map(|h| self.value(h)).collect()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct GoMctsConfig {
    /// UCT exploration constant. Paper uses 0.1 (Crew), 0.3 (Skat), 0.4 (Hearts).
    pub uct_c: f64,
    /// Per-decision search budget.
    pub n_iterations: usize,
    /// Illegality penalty applied to children whose simulation produced an
    /// illegal sampled trajectory. Paper uses 0.01.
    pub mu: f64,
    /// Number of model-sampled moves to roll out from a newly-expanded
    /// leaf before falling back to `model.value()`. Paper Algorithm 1
    /// uses ~4-10. 0 = AlphaZero-style (no rollout, value head at leaf).
    /// Provides extra search depth essentially for free since each
    /// rollout step is one model.sample call instead of an alternative
    /// node expansion.
    pub n_rollout_steps: usize,
    /// When true, rollouts ignore `n_rollout_steps` and continue
    /// sampling until the game reaches a terminal state, then return
    /// the real game payoff. Paper-faithful: V learns from actual game
    /// rewards instead of from V's own estimate at a fixed-length
    /// horizon, breaking the AlphaZero-style fixed-point trap where
    /// V(h) = mean(V(h⊕a*)) has many valid solutions including
    /// degenerate ones.
    pub rollout_to_terminal: bool,
    /// AlphaZero-style parallel-simulation width: run this many sims
    /// concurrently within one game's MCTS so their leaf-value forwards
    /// can be batched into one `batch_value` call. 1 = sequential
    /// (default; unchanged from previous behaviour). Higher values give
    /// the GPU larger per-call batches without needing more games in
    /// parallel.
    pub n_parallel_sims: usize,
}

impl Default for GoMctsConfig {
    fn default() -> Self {
        Self {
            uct_c: 0.4,
            n_iterations: 256,
            mu: 0.01,
            n_rollout_steps: 0,
            rollout_to_terminal: false,
            n_parallel_sims: 1,
        }
    }
}

/// Virtual-loss magnitude applied while a parallel sim is descending
/// through a path. Adds `visits +=1, value_sum -= VIRTUAL_LOSS` per
/// (history, action) in the in-flight trajectory so UCT pushes other
/// concurrent sims toward different branches. Reverted in
/// `backup_with_virtual_loss` when the real leaf value arrives.
/// AlphaZero uses 1.0; we match.
const VIRTUAL_LOSS: f64 = 1.0;

/// Tree node stats for one observation history.
#[derive(Default, Debug, Clone)]
struct Node {
    /// total visit count across all children (also = sum(children visits))
    total_visits: u32,
    /// per-action stats. Populated lazily at first visit.
    children: HashMap<Action, ChildStats>,
}

#[derive(Default, Debug, Clone)]
struct ChildStats {
    visits: u32,
    value_sum: f64,
    /// Cached prior from the generative model at expansion time.
    prior: f64,
}

impl ChildStats {
    fn mean_value(&self) -> f64 {
        if self.visits == 0 {
            0.0
        } else {
            self.value_sum / self.visits as f64
        }
    }
}

/// GO-MCTS search bot. `M` is the generative model.
/// Per-simulation pause state for `run_search_parallel`. Each `Sim`
/// holds its own determinised world + trajectory; the outer scheduler
/// drives them in lockstep so leaf-value calls can be batched.
#[derive(Clone)]
enum SimPhase {
    /// Sim is mid-descent. Call `advance_sim_until_pause` to step.
    InProgress,
    /// At a newly-expanded leaf. The history is what the model.value
    /// (or batch_value) call should be made against.
    NeedLeafValue { history: IStateKey },
    /// Opponent's turn. Need a model.sample(history, legal) action.
    NeedOpponentSample { history: IStateKey, legal: Vec<Action> },
    /// Sim finished; `value` is the search-player payoff. `illegal`
    /// flags the (currently unreachable) μ-penalty path.
    Done { value: f64, illegal: bool },
    /// Sim slot exhausted (no more iterations to launch). Skipped by
    /// the scheduler.
    Inactive,
}

struct Sim<G> {
    w: G,
    trajectory: Vec<(IStateKey, Action)>,
    saw_illegal: bool,
    phase: SimPhase,
}

impl<G: GameState> Sim<G> {
    fn new(w: G) -> Self {
        Self { w, trajectory: Vec::new(), saw_illegal: false, phase: SimPhase::InProgress }
    }
    fn is_done(&self) -> bool {
        matches!(self.phase, SimPhase::Done { .. } | SimPhase::Inactive)
    }
    fn resume_leaf_value(&mut self, value: f64) {
        self.phase = SimPhase::Done { value, illegal: self.saw_illegal };
    }
    fn resume_opponent_sample(&mut self, action: Action, legal: &[Action]) {
        if !legal.contains(&action) {
            self.saw_illegal = true;
            self.phase = SimPhase::Done { value: 0.0, illegal: true };
            return;
        }
        self.w.apply_action(action);
        self.phase = SimPhase::InProgress;
    }
}

pub struct GoMcts<G, M> {
    config: GoMctsConfig,
    model: M,
    nodes: FxHashMap<IStateKey, Node>,
    rng: StdRng,
    _phantom: PhantomData<G>,
}

impl<G: GameState + ResampleFromInfoState, M: GenerativeModel<G>> GoMcts<G, M> {
    pub fn new(config: GoMctsConfig, model: M, rng: StdRng) -> Self {
        Self { config, model, nodes: FxHashMap::default(), rng, _phantom: PhantomData }
    }

    pub fn model(&self) -> &M {
        &self.model
    }
    pub fn model_mut(&mut self) -> &mut M {
        &mut self.model
    }

    /// Clear the search tree. Per-decision search starts fresh.
    pub fn reset(&mut self) {
        self.nodes.clear();
    }

    /// Search-aggregated value at a history key, from the perspective of
    /// the search player at that node. Returns `None` if the history was
    /// never expanded (e.g. called before any `run_search`). This is the
    /// AlphaZero-style value target — strictly stronger than the leaf
    /// alone because it's the leaf compounded through `n_iterations`
    /// simulations of MCTS.
    pub fn root_value(&self, history: &IStateKey) -> Option<f64> {
        let node = self.nodes.get(history)?;
        if node.total_visits == 0 {
            return None;
        }
        let value_sum: f64 = node.children.values().map(|c| c.value_sum).sum();
        Some(value_sum / node.total_visits as f64)
    }

    /// Run `n_iterations` simulations from `gs` and return the root node's
    /// visit distribution (renormalised to a probability vector aligned
    /// with the game's legal action ordering).
    fn run_search(&mut self, gs: &G) -> ActionVec<f64> {
        self.reset();
        let search_player = gs.cur_player();
        let root_key = gs.istate_key(search_player);

        if self.config.n_parallel_sims > 1 {
            self.run_search_parallel(gs, search_player);
        } else {
            for _ in 0..self.config.n_iterations {
                let w = gs.resample_from_istate(search_player, &mut self.rng);
                let _ = self.simulate(w, search_player);
            }
        }

        let actions = actions!(gs);
        let mut probs = ActionVec::new(&actions);
        let total = self
            .nodes
            .get(&root_key)
            .map(|n| n.total_visits as f64)
            .unwrap_or(0.0);
        if total == 0.0 {
            // No simulation produced data — fall back to uniform. (Should be
            // unreachable in practice unless n_iterations == 0.)
            let p = 1.0 / actions.len() as f64;
            for a in &actions {
                probs[*a] = p;
            }
            return probs;
        }
        for a in &actions {
            let v = self
                .nodes
                .get(&root_key)
                .and_then(|n| n.children.get(a))
                .map(|c| c.visits as f64 / total)
                .unwrap_or(0.0);
            probs[*a] = v;
        }
        probs
    }

    /// One simulation: descend the tree, expand, evaluate, back up.
    /// Returns the simulation's terminal/leaf value for the search player.
    /// Per-rollout trajectory and the "any-illegal-opponent-sample" flag
    /// govern backup behaviour.
    fn simulate(&mut self, mut w: G, search_player: Player) -> f64 {
        // (history_at_decision, action_taken). Only search-player nodes
        // get backed up — opponent samples don't have UCT stats to update.
        let mut trajectory: Vec<(IStateKey, Action)> = Vec::new();
        let mut saw_illegal_opponent = false;
        let mut leaf_value: Option<f64> = None;

        let mut buf: Vec<Action> = Vec::new();

        loop {
            if w.is_terminal() {
                leaf_value = Some(w.evaluate(search_player));
                break;
            }
            if w.is_chance_node() {
                // Determinisation should have resolved deal-time chance
                // nodes. Mid-game chance (none in Kuhn/Euchre after the
                // deal) is handled with a uniform draw.
                buf.clear();
                w.legal_actions(&mut buf);
                let a = *buf.choose(&mut self.rng).expect("non-empty chance");
                w.apply_action(a);
                continue;
            }

            let cur = w.cur_player();
            let history = w.istate_key(search_player);
            buf.clear();
            w.legal_actions(&mut buf);

            if cur == search_player {
                // Search-player node.
                if !self.nodes.contains_key(&history) {
                    // Leaf — expand, then either eval at the leaf (when
                    // n_rollout_steps == 0) or roll out for N_steps more
                    // moves before evaluating. The rollout follows the
                    // paper's Algorithm 1; extra search depth essentially
                    // for free since each step is just one model.sample
                    // call we'd be making anyway in deeper iterations.
                    self.expand(history, &buf);
                    leaf_value = Some(self.rollout_value(&mut w, search_player));
                    break;
                }
                let a = self.select_uct(&history, &buf);
                trajectory.push((history, a));
                w.apply_action(a);
            } else {
                // Opponent node — sample from model. The model returns one
                // of the legal actions we pass in, so it is legal by
                // construction in v1. We keep the illegality machinery
                // wired up so that future trained models that produce out-
                // of-legal samples can be penalised cleanly.
                let a = self.model.sample(&history, &buf, &mut self.rng);
                if !buf.contains(&a) {
                    saw_illegal_opponent = true;
                    // Treat this trajectory as having value 0 and let
                    // backup apply the μ penalty.
                    leaf_value = Some(0.0);
                    break;
                }
                w.apply_action(a);
            }
        }

        let value = leaf_value.unwrap_or(0.0);
        self.backup(&trajectory, value, saw_illegal_opponent);
        value
    }

    /// Parallel-sim variant. Runs `n_parallel_sims` simulations
    /// concurrently as a state machine, batching their leaf-value
    /// forwards into a single `batch_value` call per "tick" of the
    /// outer loop. Opponent samples still go through `model.sample`
    /// per-sim (they're cheap individually; the dominant per-call
    /// cost is the leaf forward, which is what we batch).
    ///
    /// No virtual loss in v1: sims diverge naturally after the first
    /// opponent move because each one starts with an independent
    /// determinization. The wave of K parallel sims explores at most
    /// K root actions in the worst case (typically fewer); subsequent
    /// waves cover the rest as UCT's log-N exploration term grows.
    fn run_search_parallel(&mut self, gs: &G, search_player: Player) {
        let k = self.config.n_parallel_sims.max(1);
        let n_target = self.config.n_iterations;
        if n_target == 0 {
            return;
        }
        let active = k.min(n_target);
        let mut sims: Vec<Sim<G>> = (0..active)
            .map(|_| {
                let w = gs.resample_from_istate(search_player, &mut self.rng);
                Sim::new(w)
            })
            .collect();
        let mut launched: usize = active;
        let mut completed: usize = 0;
        // Scratch buffers reused per tick.
        let mut leaf_indices: Vec<usize> = Vec::with_capacity(active);
        let mut leaf_histories: Vec<IStateKey> = Vec::with_capacity(active);

        while completed < n_target {
            // Phase 1: advance every still-active sim to its next pause point.
            for (i, sim) in sims.iter_mut().enumerate() {
                if sim.is_done() {
                    continue;
                }
                self.advance_sim_until_pause(sim, search_player);
                let _ = i;
            }
            // Phase 2: batch the leaf-value requests (the expensive ones).
            leaf_indices.clear();
            leaf_histories.clear();
            for (i, sim) in sims.iter().enumerate() {
                if let SimPhase::NeedLeafValue { history } = &sim.phase {
                    leaf_indices.push(i);
                    leaf_histories.push(*history);
                }
            }
            if !leaf_histories.is_empty() {
                let values = self.model.batch_value(&leaf_histories);
                for (idx, v) in leaf_indices.iter().zip(values.into_iter()) {
                    sims[*idx].resume_leaf_value(v);
                }
            }
            // Phase 3: process opponent samples one-by-one (small batch
            // already happens inside `model.sample` via |legal|-way
            // ArgmaxVal\*). Cross-sim batching of these would require
            // changing the trait; not done in v1.
            for sim in sims.iter_mut() {
                if let SimPhase::NeedOpponentSample { history, legal } = sim.phase.clone() {
                    let a = self.model.sample(&history, &legal, &mut self.rng);
                    sim.resume_opponent_sample(a, &legal);
                }
            }
            // Phase 4: drain any sim that finished this tick, account
            // for it in the visit/value stats, and either start a fresh
            // one (still within budget) or mark the slot inactive.
            for sim in sims.iter_mut() {
                if let SimPhase::Done { value, illegal } = sim.phase {
                    let trajectory = std::mem::take(&mut sim.trajectory);
                    self.backup_with_virtual_loss(&trajectory, value, illegal);
                    completed += 1;
                    if launched < n_target {
                        let w = gs.resample_from_istate(search_player, &mut self.rng);
                        *sim = Sim::new(w);
                        launched += 1;
                    } else {
                        sim.phase = SimPhase::Inactive;
                    }
                }
            }
        }
    }

    /// Apply virtual loss at `(history, action)` — visits +=1,
    /// value_sum -= VIRTUAL_LOSS. Called when a parallel sim descends
    /// through this node so concurrent sims' UCT scoring will see this
    /// edge as temporarily worse and pick different paths.
    fn apply_virtual_loss(&mut self, history: &IStateKey, action: Action) {
        let node = self.nodes.get_mut(history).expect("node must exist before VL");
        let child = node.children.get_mut(&action).expect("child must exist before VL");
        child.visits += 1;
        child.value_sum -= VIRTUAL_LOSS;
        node.total_visits += 1;
    }

    /// Back up the trajectory after a parallel sim's leaf value arrives.
    /// Undoes the virtual loss applied during descent, then folds in the
    /// real value. Net effect on each child is identical to the
    /// sequential `backup()` (visits +1, value_sum += value) — the
    /// virtual loss is only ever observed by OTHER concurrent sims
    /// during their UCT selection.
    fn backup_with_virtual_loss(
        &mut self,
        trajectory: &[(IStateKey, Action)],
        value: f64,
        illegal: bool,
    ) {
        for (history, action) in trajectory {
            let node = self.nodes.get_mut(history).expect("must exist");
            let child = node.children.get_mut(action).expect("child must exist");
            // Step 1: revert the virtual loss.
            child.value_sum += VIRTUAL_LOSS;
            // Step 2: apply the real backup.
            if illegal {
                // Sequential `backup()` treats illegal as "visits
                // unchanged, value_sum -= μ". The VL already
                // incremented visits + total_visits, so we undo those
                // here so the net is "visits unchanged".
                child.visits -= 1;
                node.total_visits -= 1;
                child.value_sum -= self.config.mu;
            } else {
                // VL already added visits + total_visits; we just add
                // the value.
                child.value_sum += value;
            }
        }
    }

    /// Advance one `Sim` until it either finishes or needs a model call.
    /// Mirrors the body of `simulate()` but uses the Sim's externalised
    /// state so we can pause/resume across the batched forwards.
    fn advance_sim_until_pause(&mut self, sim: &mut Sim<G>, search_player: Player) {
        let mut buf: Vec<Action> = Vec::new();
        loop {
            match sim.phase {
                SimPhase::Done { .. } | SimPhase::Inactive | SimPhase::NeedLeafValue { .. } | SimPhase::NeedOpponentSample { .. } => {
                    return;
                }
                SimPhase::InProgress => {}
            }
            if sim.w.is_terminal() {
                sim.phase = SimPhase::Done { value: sim.w.evaluate(search_player), illegal: sim.saw_illegal };
                return;
            }
            if sim.w.is_chance_node() {
                buf.clear();
                sim.w.legal_actions(&mut buf);
                if let Some(a) = buf.choose(&mut self.rng) {
                    sim.w.apply_action(*a);
                }
                continue;
            }
            let cur = sim.w.cur_player();
            let history = sim.w.istate_key(search_player);
            buf.clear();
            sim.w.legal_actions(&mut buf);
            if cur == search_player {
                if !self.nodes.contains_key(&history) {
                    self.expand(history, &buf);
                    // Rollout phase (n_rollout_steps moves from the new
                    // leaf) before requesting the value. For parallel-
                    // sim mode we run the rollout synchronously here —
                    // the rollout's own `model.sample` calls go through
                    // the standard path and aren't cross-sim-batched.
                    // Then we yield a single `NeedLeafValue` request.
                    if self.config.n_rollout_steps > 0 || self.config.rollout_to_terminal {
                        let leaf_value = self.rollout_value(&mut sim.w, search_player);
                        sim.phase = SimPhase::Done { value: leaf_value, illegal: sim.saw_illegal };
                    } else {
                        sim.phase = SimPhase::NeedLeafValue { history: sim.w.istate_key(search_player) };
                    }
                    return;
                }
                let a = self.select_uct(&history, &buf);
                // Apply virtual loss: this in-flight sim provisionally
                // "spends" a visit at (history, a) with a negative
                // value. Other concurrent sims see this and avoid the
                // path — encouraging diverse search across K sims.
                // Reverted in `backup_with_virtual_loss` when the
                // leaf returns.
                self.apply_virtual_loss(&history, a);
                sim.trajectory.push((history, a));
                sim.w.apply_action(a);
            } else {
                // Opponent move: yield to the outer loop so it can
                // dispatch the sample. We pass the legal-actions buffer
                // we already populated.
                sim.phase = SimPhase::NeedOpponentSample { history, legal: buf.clone() };
                return;
            }
        }
    }

    /// Roll out model-sampled moves from `w`, advancing the
    /// determinised game state. Two modes:
    ///   * `rollout_to_terminal=true`: keep rolling until terminal,
    ///     return the real game payoff. Paper-faithful — V learns
    ///     from real reward.
    ///   * `rollout_to_terminal=false`: cap at `n_rollout_steps`, then
    ///     fall back to `model.value(...)` at the rollout's final
    ///     position. With both 0 this is just `model.value(...)` at
    ///     the leaf — preserving the original AlphaZero default.
    fn rollout_value(&mut self, w: &mut G, search_player: Player) -> f64 {
        let mut buf = Vec::new();
        // usize::MAX in the terminal-mode lets the loop run to game
        // end. Euchre / Kuhn / Bluff are all finite-horizon so this
        // terminates; if it didn't, the caller would have to add its
        // own safety cap.
        let cap = if self.config.rollout_to_terminal {
            usize::MAX
        } else {
            self.config.n_rollout_steps
        };
        for _ in 0..cap {
            if w.is_terminal() {
                return w.evaluate(search_player);
            }
            if w.is_chance_node() {
                buf.clear();
                w.legal_actions(&mut buf);
                if let Some(a) = buf.choose(&mut self.rng) {
                    w.apply_action(*a);
                }
                continue;
            }
            let history = w.istate_key(search_player);
            buf.clear();
            w.legal_actions(&mut buf);
            if buf.is_empty() {
                break;
            }
            let a = self.model.sample(&history, &buf, &mut self.rng);
            if !buf.contains(&a) {
                // Illegal sample mid-rollout; bail out with neutral
                // value. (The caller's outer simulate() doesn't see
                // this as an illegality penalty — the rollout is
                // auxiliary search, not a tree path that gets
                // stat-credited.) Paper-faithful mode shouldn't hit
                // this since the model.sample legality check happens
                // upstream, but defensively return 0.
                return 0.0;
            }
            w.apply_action(a);
        }
        if w.is_terminal() {
            w.evaluate(search_player)
        } else {
            // Only reachable in capped mode when the cap is exhausted
            // before terminal. Paper-faithful mode never lands here.
            self.model.value(&w.istate_key(search_player))
        }
    }

    /// Create a new tree node for `history`, pre-populating each legal
    /// child's prior from the model.
    fn expand(&mut self, history: IStateKey, legal: &[Action]) {
        let prior = self.model.policy(&history, legal);
        let mut children = HashMap::with_capacity(legal.len());
        for (i, a) in legal.iter().enumerate() {
            children.insert(*a, ChildStats { visits: 0, value_sum: 0.0, prior: prior[i] });
        }
        self.nodes.insert(history, Node { total_visits: 0, children });
    }

    /// UCT selection at a search-player node. Returns the chosen action.
    /// First-visit children (visits=0) are sampled in random legal order
    /// before UCT kicks in, matching how OpenSpiel's ISMCTS handles unseen
    /// arms.
    fn select_uct(&mut self, history: &IStateKey, legal: &[Action]) -> Action {
        let node = self.nodes.get(history).expect("node must exist");
        // First-play urgency: any zero-visit child wins outright.
        let mut unvisited: Vec<Action> = legal
            .iter()
            .filter(|a| node.children.get(*a).map(|c| c.visits == 0).unwrap_or(true))
            .copied()
            .collect();
        if !unvisited.is_empty() {
            unvisited.shuffle(&mut self.rng);
            return unvisited[0];
        }

        // UCT: argmax over legal actions of mean + C·sqrt(ln(N) / n_a)
        let log_n = (node.total_visits.max(1) as f64).ln();
        let mut best_score = f64::NEG_INFINITY;
        let mut best_action = legal[0];
        for a in legal {
            let c = node.children.get(a).expect("child must exist after expand");
            let score = c.mean_value() + self.config.uct_c * (log_n / c.visits as f64).sqrt();
            if score > best_score {
                best_score = score;
                best_action = *a;
            }
        }
        best_action
    }

    /// Back up `value` (search player POV) along the trajectory. If
    /// `illegal` is set, apply the μ penalty *instead of* accumulating
    /// value (paper: "(val − μ, visits unchanged)") so the search learns
    /// to avoid generating these trajectories.
    fn backup(&mut self, trajectory: &[(IStateKey, Action)], value: f64, illegal: bool) {
        for (history, action) in trajectory {
            let node = self.nodes.get_mut(history).expect("node must exist");
            let child = node.children.get_mut(action).expect("child must exist");
            if illegal {
                child.value_sum -= self.config.mu;
                // Per paper: visits unchanged on illegal backup.
            } else {
                child.value_sum += value;
                child.visits += 1;
                node.total_visits += 1;
            }
        }
    }
}

impl<G: GameState + ResampleFromInfoState, M: GenerativeModel<G>> Policy<G> for GoMcts<G, M> {
    fn action_probabilities(&mut self, gs: &G) -> ActionVec<f64> {
        self.run_search(gs)
    }
}

impl<G: GameState + ResampleFromInfoState, M: GenerativeModel<G>> Agent<G> for GoMcts<G, M> {
    fn step(&mut self, s: &G) -> Action {
        let probs = self.run_search(s).to_vec();
        probs.choose_weighted(&mut self.rng, |(_, p)| *p).unwrap().0
    }
}

// =====================================================================
// Model implementations
// =====================================================================

/// Uniform-over-legal model. No learning. Useful for isolating the search
/// algorithm: if GO-MCTS with this model can't find sensible play with
/// enough iterations, the bug is in the search.
#[derive(Clone, Default)]
pub struct UniformRandomModel;

impl<G: GameState> GenerativeModel<G> for UniformRandomModel {
    fn sample(&mut self, _: &IStateKey, legal: &[Action], rng: &mut StdRng) -> Action {
        *legal.choose(rng).expect("non-empty legal actions")
    }
    fn value(&mut self, _: &IStateKey) -> f64 {
        0.0
    }
}

/// Tabular generative model. Per-history visit counts (→ policy via
/// normalised visits) and a running mean of observed terminal values.
/// Feasible for Kuhn Poker, infeasible for Euchre.
#[derive(Default, Clone)]
pub struct TabularGenerativeModel {
    table: FxHashMap<IStateKey, HistoryStats>,
}

#[derive(Default, Clone, Debug)]
struct HistoryStats {
    /// Per-action stats. Lazily populated as actions are taken.
    actions: HashMap<Action, ActionStats>,
    /// Total visits at this history = Σ actions[a].visits.
    total_visits: u32,
    /// Sum of search-player values observed at terminals reached from this
    /// history. Used by `value()` lookups.
    value_sum: f64,
}

#[derive(Default, Clone, Debug)]
struct ActionStats {
    visits: u32,
    value_sum: f64,
}

impl ActionStats {
    fn mean_value(&self) -> f64 {
        if self.visits == 0 { 0.0 } else { self.value_sum / self.visits as f64 }
    }
}

/// Softmax temperature used by the tabular model's sampling. Smaller →
/// more greedy; larger → more uniform. 0.5 lands roughly between argmax
/// and uniform for Kuhn-scale payoffs (±1, ±2).
const TABULAR_SOFTMAX_TEMP: f64 = 0.5;

impl TabularGenerativeModel {
    pub fn new() -> Self {
        Self { table: FxHashMap::default() }
    }

    /// Record that `action` was chosen at observation history `history`,
    /// and the trajectory's eventual value (from the search player's POV)
    /// was `value`.
    pub fn record(&mut self, history: IStateKey, action: Action, value: f64) {
        let entry = self.table.entry(history).or_default();
        let action_stats = entry.actions.entry(action).or_default();
        action_stats.visits += 1;
        action_stats.value_sum += value;
        entry.total_visits += 1;
        entry.value_sum += value;
    }

    pub fn len(&self) -> usize {
        self.table.len()
    }

    pub fn is_empty(&self) -> bool {
        self.table.is_empty()
    }
}

/// Softmax over `scores` with the given temperature. Returns probabilities
/// aligned with `scores`. Subtracts the max first for numerical stability.
fn softmax(scores: &[f64], temperature: f64) -> Vec<f64> {
    let max = scores.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let exps: Vec<f64> = scores.iter().map(|s| ((s - max) / temperature).exp()).collect();
    let total: f64 = exps.iter().sum();
    exps.into_iter().map(|e| e / total).collect()
}

impl<G: GameState> GenerativeModel<G> for TabularGenerativeModel {
    fn sample(&mut self, history: &IStateKey, legal: &[Action], rng: &mut StdRng) -> Action {
        let probs = <Self as GenerativeModel<G>>::policy(self, history, legal);
        let mut r: f64 = rng.random::<f64>();
        for (i, p) in probs.iter().enumerate() {
            r -= *p;
            if r <= 0.0 {
                return legal[i];
            }
        }
        legal[legal.len() - 1]
    }

    fn value(&mut self, history: &IStateKey) -> f64 {
        self.table
            .get(history)
            .filter(|s| s.total_visits > 0)
            .map(|s| s.value_sum / s.total_visits as f64)
            .unwrap_or(0.0)
    }

    /// Value-driven softmax policy. Unseen actions get the per-history
    /// mean value (default 0) so they're not penalised for absence of
    /// data; once tried, they're updated toward their observed mean.
    /// This is the change that turns the trainer from a positive-feedback
    /// loop into an actual learner.
    fn policy(&mut self, history: &IStateKey, legal: &[Action]) -> Vec<f64> {
        let default_value =
            self.table.get(history).map(|s| {
                if s.total_visits > 0 { s.value_sum / s.total_visits as f64 } else { 0.0 }
            }).unwrap_or(0.0);
        let scores: Vec<f64> = legal
            .iter()
            .map(|a| {
                self.table
                    .get(history)
                    .and_then(|s| s.actions.get(a))
                    .filter(|s| s.visits > 0)
                    .map(|s| s.mean_value())
                    .unwrap_or(default_value)
            })
            .collect();
        softmax(&scores, TABULAR_SOFTMAX_TEMP)
    }
}

// =====================================================================
// Self-play training (Kuhn-scale tabular)
// =====================================================================

/// Train a `TabularGenerativeModel` via self-play. On each game, all
/// players sample from the current model; we record (history, action,
/// final_value) tuples and feed them back into the table. This is a
/// reduced version of the paper's population-based self-play, sufficient
/// for the Kuhn-scale validation. The starting game is provided as a
/// fresh-state factory `make_state`.
pub fn self_play_train<G: GameState + ResampleFromInfoState, F: Fn() -> G>(
    model: &mut TabularGenerativeModel,
    make_state: F,
    n_games: usize,
    seed: u64,
) {
    let mut rng: StdRng = SeedableRng::seed_from_u64(seed);
    let mut buf = Vec::new();
    for _ in 0..n_games {
        let mut gs = make_state();
        // Resolve chance nodes (dealing).
        while gs.is_chance_node() {
            buf.clear();
            gs.legal_actions(&mut buf);
            let a = *buf.choose(&mut rng).expect("non-empty chance");
            gs.apply_action(a);
        }
        // Record per-player trajectories.
        let n_players = gs.num_players();
        let mut traj: Vec<Vec<(IStateKey, Action)>> = vec![Vec::new(); n_players];
        while !gs.is_terminal() {
            let p = gs.cur_player();
            let history = gs.istate_key(p);
            buf.clear();
            gs.legal_actions(&mut buf);
            let a = <TabularGenerativeModel as GenerativeModel<G>>::sample(
                model, &history, &buf, &mut rng,
            );
            traj[p].push((history, a));
            gs.apply_action(a);
        }
        // Credit each player's history-action pairs with that player's
        // final value.
        for p in 0..n_players {
            let v = gs.evaluate(p);
            for (h, a) in traj[p].drain(..) {
                model.record(h, a, v);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use games::{
        gamestates::{
            euchre::{Euchre, EuchreGameState},
            kuhn_poker::{KPAction, KPGameState, KuhnPoker},
        },
        GameState,
    };
    use rand::{rngs::StdRng, seq::IndexedRandom, SeedableRng};

    /// Parallel-sim mode with k=4 produces a non-degenerate policy.
    /// Identical structure to `gomcts_uniform_model_kuhn_smoke`; pins
    /// that the state-machine refactor doesn't crash and gives a valid
    /// distribution.
    #[test]
    fn gomcts_parallel_sims_kuhn_smoke() {
        let mut bot: GoMcts<_, UniformRandomModel> = GoMcts::new(
            GoMctsConfig {
                uct_c: 0.4,
                n_iterations: 512,
                mu: 0.01,
                n_rollout_steps: 0,
                rollout_to_terminal: false,
                n_parallel_sims: 4,
            },
            UniformRandomModel,
            SeedableRng::seed_from_u64(7),
        );
        let gs = KuhnPoker::from_actions(&[KPAction::Jack, KPAction::Queen]);
        let probs = bot.action_probabilities(&gs);
        let mass: f64 = probs.to_vec().iter().map(|(_, p)| *p).sum();
        assert!((mass - 1.0).abs() < 1e-6, "probs should sum to 1, got {}", mass);
        assert!(
            probs.to_vec().iter().any(|(_, p)| *p > 0.0),
            "expected at least one action visited"
        );
    }

    /// GO-MCTS + UniformRandomModel on Kuhn should find that the worse
    /// hand loses on average. Sanity check for the search wiring with no
    /// learning involved.
    #[test]
    fn gomcts_uniform_model_kuhn_smoke() {
        let mut bot: GoMcts<_, UniformRandomModel> = GoMcts::new(
            GoMctsConfig { uct_c: 0.4, n_iterations: 512, mu: 0.01, n_rollout_steps: 0, rollout_to_terminal: false, n_parallel_sims: 1 },
            UniformRandomModel,
            SeedableRng::seed_from_u64(7),
        );
        let gs = KuhnPoker::from_actions(&[KPAction::Jack, KPAction::Queen]);
        let probs = bot.action_probabilities(&gs);
        // Jack opener vs Queen — searching with a uniform opponent model
        // should put most of the visit mass on whichever action GO-MCTS
        // judges best. We just sanity-check that the policy normalises and
        // is non-degenerate (some action has > 0 mass).
        let mass: f64 = probs.to_vec().iter().map(|(_, p)| *p).sum();
        assert!((mass - 1.0).abs() < 1e-6, "probs should sum to 1, got {}", mass);
        let any_nonzero = probs.to_vec().iter().any(|(_, p)| *p > 0.0);
        assert!(any_nonzero, "expected at least one action visited");
    }

    /// Self-play training on Kuhn should populate the model table. We
    /// don't claim Nash convergence — just that the training loop runs and
    /// records data.
    #[test]
    fn tabular_self_play_populates_table() {
        let mut model = TabularGenerativeModel::new();
        self_play_train(&mut model, || KuhnPoker::new_state(), 200, 42);
        assert!(model.len() > 0, "training should populate the istate table");
    }

    /// After self-play, a King-holding opener should bet more often than
    /// pass — this is the sanity check that the tabular trainer's
    /// signal-to-noise ratio is high enough on Kuhn. We don't insist on
    /// equilibrium frequencies; just on monotone direction.
    #[test]
    fn tabular_self_play_learns_king_bets() {
        let mut model = TabularGenerativeModel::new();
        // Many games: Kuhn has only 12 player-istates so we can saturate
        // the table cheaply.
        self_play_train(&mut model, || KuhnPoker::new_state(), 5_000, 11);

        // The opening decision after being dealt a King: istate = [King].
        let mut king_istate = IStateKey::default();
        king_istate.push(KPAction::King.into());
        let mut jack_istate = IStateKey::default();
        jack_istate.push(KPAction::Jack.into());

        let bet: Action = KPAction::Bet.into();
        let pass: Action = KPAction::Pass.into();
        let legal = [bet, pass];

        let king_pi = <TabularGenerativeModel as GenerativeModel<KPGameState>>::policy(
            &mut model, &king_istate, &legal,
        );
        let jack_pi = <TabularGenerativeModel as GenerativeModel<KPGameState>>::policy(
            &mut model, &jack_istate, &legal,
        );
        // bet is index 0 in `legal`.
        let king_bet = king_pi[0];
        let jack_bet = jack_pi[0];
        assert!(
            king_bet > jack_bet,
            "King should bet more often than Jack after training: king_bet={}, jack_bet={}",
            king_bet,
            jack_bet,
        );
    }

    /// Smoke test: GO-MCTS runs end-to-end on Euchre without crashing.
    /// Uses UniformRandomModel — no training, no quality claims.
    #[test]
    fn gomcts_euchre_smoke_full_game() {
        let mut rng: StdRng = SeedableRng::seed_from_u64(3);
        let mut gs: EuchreGameState = Euchre::new_state();
        let mut acts = Vec::new();
        while gs.is_chance_node() {
            gs.legal_actions(&mut acts);
            let a = *acts.choose(&mut rng).expect("non-empty chance");
            gs.apply_action(a);
            acts.clear();
        }
        let mut bot: GoMcts<_, UniformRandomModel> = GoMcts::new(
            GoMctsConfig { uct_c: 0.4, n_iterations: 16, mu: 0.01, n_rollout_steps: 0, rollout_to_terminal: false, n_parallel_sims: 1 },
            UniformRandomModel,
            SeedableRng::seed_from_u64(3),
        );
        // Play one full hand. The other 3 seats also draw via this same
        // bot for simplicity (still cheap at 16 iters).
        while !gs.is_terminal() {
            let a = bot.step(&gs);
            gs.apply_action(a);
        }
        assert!(gs.is_terminal());
    }
}

