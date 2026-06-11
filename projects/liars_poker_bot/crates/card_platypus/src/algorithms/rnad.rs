//! Regularized Nash Dynamics (R-NaD) — the DeepNash training scheme
//! (Perolat et al., "Mastering the game of Stratego with model-free
//! multiagent reinforcement learning", Science 2022) on top of the
//! GO-MCTS transformer backbone.
//!
//! The algorithm iterates three steps:
//!   1. Reward transformation: play the game with rewards augmented by
//!      −η·log(π(a|h)/π_reg(a|h)) for the acting player's team (and
//!      +η·… for the opposing team), where π_reg is a frozen
//!      "regularization policy".
//!   2. Dynamics: run policy-gradient style updates (NeuRD) until the
//!      policy approaches the fixed point of the transformed game.
//!   3. Update: set π_reg to the current policy and repeat.
//!
//! In two-player zero-sum games the sequence of fixed points converges
//! to a Nash equilibrium. We treat 4-player partnership games (Euchre)
//! as two-team zero-sum with `team = player % 2` — no formal guarantee
//! carries over, but the same dynamics apply mechanically.
//!
//! Implementation notes:
//!   * The policy is the LM head masked to legal-action tokens
//!     (softmax, temperature 1.0); the value head provides V(h).
//!   * NeuRD update on the sampled action only, importance-corrected
//!     by 1/π_behavior(a|h) (clipped) so it matches the all-actions
//!     replicator update in expectation. The gradient is taken on the
//!     *centered legal logit* (logit minus mean legal logit), not
//!     log π — that is the NeuRD/replicator-dynamics distinction that
//!     avoids vanishing updates at deterministic policies.
//!   * The NeuRD logit threshold β gates updates that would push an
//!     already-saturated logit further out.
//!   * Value targets are Monte-Carlo returns of the *transformed*
//!     rewards (terminal payoff + the η log-ratio stream), so the
//!     critic tracks the regularized game the policy is solving.
//!   * Games here are short episodes (≤ ~30 decisions), trajectories
//!     are consumed on-policy right after collection — plain MC
//!     returns stand in for v-trace.

use anyhow::{anyhow, Result};
use rand::{rngs::StdRng, seq::IndexedRandom, RngExt, SeedableRng};
use tch::{nn, nn::OptimizerConfig, Kind, Tensor};

use games::{actions, istate::IStateKey, Action, GameState};

use super::gomcts::GenerativeModel;
use super::gomcts_transformer::{
    forward_histories_batch_tch, pad_to, serve_batched_tch, ActionTokenFn, GoMctsTransformerTch,
    InferenceMode, RemoteModel, ServiceRequest, SnapshotTch, Tokenizer,
};
use crate::{collections::actionvec::ActionVec, policy::Policy};

// =====================================================================
// Config / data types
// =====================================================================

#[derive(Clone, Copy, Debug)]
pub struct RnadConfig {
    /// Regularization strength η on the log(π/π_reg) reward term.
    pub eta: f64,
    /// AdamW learning rate.
    pub lr: f64,
    /// Weight on the value MSE relative to the NeuRD policy loss.
    pub value_weight: f64,
    /// NeuRD logit threshold β: no update that pushes a centered legal
    /// logit beyond ±β.
    pub neurd_clip: f64,
    /// Cap on the 1/π_behavior importance weight.
    pub is_clip: f64,
    /// Cap on |log(π/π_reg)| inside the reward transform (NaN guard
    /// for actions π_reg has nearly abandoned).
    pub log_ratio_clip: f64,
    /// Global grad-norm clip per optimizer step.
    pub grad_clip: f64,
    /// Learner steps between π_reg ← π refreshes (the R-NaD outer loop).
    pub reg_update_every: usize,
    /// Step rows per forward/optimizer step inside one learner step.
    pub minibatch_steps: usize,
    /// Reward-transform routing. `false` (default): two-team zero-sum —
    /// the actor's team pays −η·logratio, the opposing team receives
    /// +η·… (correct for 2p games and 4p partnerships, keeps the
    /// transformed game zero-sum). `true`: the actor pays −η·logratio
    /// and every OTHER player receives +η·logratio/(n−1) — preserves
    /// zero-sum for n individual players (3-player Oh Hell, whose
    /// `evaluate` is mean-centred) and reduces exactly to the 2p
    /// formula at n=2. Don't use for partnership games: the actor's
    /// partner would wrongly receive a share of the bonus.
    pub spread_penalty: bool,
}

impl Default for RnadConfig {
    fn default() -> Self {
        Self {
            eta: 0.2,
            lr: 5e-5,
            value_weight: 1.0,
            neurd_clip: 2.0,
            is_clip: 10.0,
            log_ratio_clip: 10.0,
            grad_clip: 5.0,
            reg_update_every: 100,
            minibatch_steps: 1024,
            spread_penalty: false,
        }
    }
}

/// One decision point recorded during self-play.
#[derive(Clone)]
pub struct RnadStep {
    pub player: usize,
    pub history: IStateKey,
    pub action: Action,
    pub legal: Vec<Action>,
    /// π_behavior(a|h) at sample time, for the importance correction.
    pub behavior_prob: f32,
}

/// One self-play game: every player's decisions plus final payoffs.
pub struct RnadTrajectory {
    pub steps: Vec<RnadStep>,
    /// Terminal payoff per player (Euchre: team-symmetric).
    pub payoffs: Vec<f64>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RnadStats {
    pub policy_loss: f64,
    pub value_loss: f64,
    pub mean_abs_adv: f64,
    pub mean_entropy: f64,
    /// Mean KL(π‖π_reg) — convergence of the inner dynamics shows up
    /// as this stabilizing between π_reg refreshes.
    pub mean_kl_reg: f64,
    /// Outer-loop convergence metric, populated only on learner steps
    /// that performed a π_reg refresh: KL(π_now ‖ π_reg_outgoing) over a
    /// sample of recent decision states — i.e. how far this fixed-point
    /// iteration moved the policy. The R-NaD fixed-point sequence has
    /// converged when this →0 across successive refreshes.
    pub kl_outer: Option<f64>,
    pub n_steps: usize,
}

fn team_of(player: usize) -> usize {
    // 2p zero-sum: identity. 4p partnership (Euchre 0&2 vs 1&3): p % 2.
    player % 2
}

// =====================================================================
// Self-play collection (batched-service architecture, all seats sample
// the live policy's LM head at temperature 1.0)
// =====================================================================

fn play_one_hand_rnad<G: GameState>(
    actor: &mut RemoteModel,
    mut gs: G,
    rng: &mut StdRng,
) -> RnadTrajectory {
    let mut buf = Vec::new();
    let mut steps = Vec::new();
    while !gs.is_terminal() {
        buf.clear();
        gs.legal_actions(&mut buf);
        if gs.is_chance_node() {
            let a = *buf.choose(rng).expect("non-empty chance");
            gs.apply_action(a);
            continue;
        }
        let p = gs.cur_player();
        let h = gs.istate_key(p);
        let probs = <RemoteModel as GenerativeModel<G>>::policy(actor, &h, &buf);
        let mut r: f64 = rng.random::<f64>();
        let mut idx = buf.len() - 1;
        for (i, pr) in probs.iter().enumerate() {
            r -= *pr;
            if r <= 0.0 {
                idx = i;
                break;
            }
        }
        steps.push(RnadStep {
            player: p,
            history: h,
            action: buf[idx],
            legal: buf.clone(),
            behavior_prob: probs[idx].max(1e-9) as f32,
        });
        gs.apply_action(buf[idx]);
    }
    let n = gs.num_players();
    RnadTrajectory { steps, payoffs: (0..n).map(|p| gs.evaluate(p)).collect() }
}

/// Collect `n_games` self-play trajectories with every seat sampling
/// the live policy. Same scoped-thread + batching-service architecture
/// as the GO-MCTS self-play paths.
///
/// `new_state` receives the game index, so callers can vary game
/// parameters across the batch (e.g. cycling Oh Hell trick counts).
#[allow(clippy::too_many_arguments)]
pub fn collect_rnad_games_batched_tch<G, T, FNS>(
    net: &GoMctsTransformerTch,
    tokenizer: &T,
    new_state: FNS,
    n_games: usize,
    base_seed: u64,
    action_token_fn: ActionTokenFn,
    use_graph: bool,
    graph_batch_size: i64,
) -> Vec<RnadTrajectory>
where
    G: GameState + Send,
    T: Tokenizer<G> + Send + Sync,
    FNS: Fn(usize) -> G + Send + Sync + Copy,
{
    use std::sync::mpsc;
    if n_games == 0 {
        return Vec::new();
    }
    // LmSoftmax requests are one history each; the service coalesces up
    // to max_batch of them per forward.
    let max_batch = n_games.clamp(32, 512);
    let (request_tx, request_rx) = mpsc::channel::<ServiceRequest>();
    std::thread::scope(|s| {
        let svc = s.spawn(move || {
            serve_batched_tch(net, tokenizer, request_rx, max_batch, use_graph, graph_batch_size)
        });
        let mut handles = Vec::with_capacity(n_games);
        for game_idx in 0..n_games {
            let req_tx = request_tx.clone();
            let atf = action_token_fn.clone();
            let seed = base_seed.wrapping_add(game_idx as u64);
            handles.push(s.spawn(move || {
                let mut actor = RemoteModel::new(req_tx)
                    .with_inference(InferenceMode::LmSoftmax, 0.0, Some(atf))
                    .with_temp(1.0);
                let mut rng: StdRng = SeedableRng::seed_from_u64(seed);
                play_one_hand_rnad(&mut actor, new_state(game_idx), &mut rng)
            }));
        }
        drop(request_tx);
        let out: Vec<RnadTrajectory> =
            handles.into_iter().map(|h| h.join().expect("game thread panicked")).collect();
        svc.join().expect("service thread panicked");
        out
    })
}

// =====================================================================
// Exploiter collection (approximate-exploitability harness)
// =====================================================================

/// Which seats the exploiter controls in best-response training.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExploiterSeating {
    /// The exploiter plays one TEAM (`team_of`, i.e. seats of equal
    /// parity), alternating teams by game index. Right for 2p games
    /// and 4p partnerships (Euchre).
    Team,
    /// The exploiter plays exactly ONE seat, rotating through all
    /// seats by game index — the literal unilateral-deviation test
    /// for n-player games (3p Oh Hell): can a single defector profit
    /// against n−1 copies of the frozen policy?
    SingleSeat,
}

fn play_one_hand_exploiter<G: GameState>(
    live: &mut RemoteModel,
    frozen: &mut RemoteModel,
    seating: ExploiterSeating,
    game_idx: usize,
    mut gs: G,
    rng: &mut StdRng,
) -> RnadTrajectory {
    let n_players = gs.num_players();
    let is_live = |p: usize| match seating {
        ExploiterSeating::Team => team_of(p) == game_idx % 2,
        ExploiterSeating::SingleSeat => p == game_idx % n_players,
    };
    let mut buf = Vec::new();
    let mut steps = Vec::new();
    while !gs.is_terminal() {
        buf.clear();
        gs.legal_actions(&mut buf);
        if gs.is_chance_node() {
            let a = *buf.choose(rng).expect("non-empty chance");
            gs.apply_action(a);
            continue;
        }
        let p = gs.cur_player();
        let h = gs.istate_key(p);
        let a = if is_live(p) {
            let probs = <RemoteModel as GenerativeModel<G>>::policy(live, &h, &buf);
            let mut r: f64 = rng.random::<f64>();
            let mut idx = buf.len() - 1;
            for (i, pr) in probs.iter().enumerate() {
                r -= *pr;
                if r <= 0.0 {
                    idx = i;
                    break;
                }
            }
            steps.push(RnadStep {
                player: p,
                history: h,
                action: buf[idx],
                legal: buf.clone(),
                behavior_prob: probs[idx].max(1e-9) as f32,
            });
            buf[idx]
        } else {
            <RemoteModel as GenerativeModel<G>>::sample(frozen, &h, &buf, rng)
        };
        gs.apply_action(a);
    }
    let n = gs.num_players();
    RnadTrajectory { steps, payoffs: (0..n).map(|p| gs.evaluate(p)).collect() }
}

/// Collect best-response training games: the live (exploiter) net plays
/// the `seating`-selected seats — rotating by game index so every seat
/// parity/position is covered — against a FROZEN target net at the
/// other seats. Only the exploiter's decisions are recorded, so
/// `learner_step` trains a best response. Run the trainer with
/// `eta = 0` (no reward transform); the exploiter's head-to-head EV
/// against the target is then a lower bound on the target's
/// exploitability.
///
/// `frozen_temp` sets the target's sampling temperature: 0.05 ≈ exploit
/// the deployed greedy-LM agent; 1.0 = exploit the policy distribution
/// the equilibrium argument is actually about.
#[allow(clippy::too_many_arguments)]
pub fn collect_exploiter_games_batched_tch<G, T, FNS>(
    live: &GoMctsTransformerTch,
    frozen: &GoMctsTransformerTch,
    tokenizer: &T,
    new_state: FNS,
    n_games: usize,
    base_seed: u64,
    action_token_fn: ActionTokenFn,
    frozen_temp: f64,
    seating: ExploiterSeating,
) -> Vec<RnadTrajectory>
where
    G: GameState + Send,
    T: Tokenizer<G> + Send + Sync,
    FNS: Fn(usize) -> G + Send + Sync + Copy,
{
    use std::sync::mpsc;
    if n_games == 0 {
        return Vec::new();
    }
    let max_batch = n_games.clamp(32, 512);
    let (req_live_tx, req_live_rx) = mpsc::channel::<ServiceRequest>();
    let (req_frozen_tx, req_frozen_rx) = mpsc::channel::<ServiceRequest>();
    std::thread::scope(|s| {
        let svc_live = s.spawn(move || {
            serve_batched_tch(live, tokenizer, req_live_rx, max_batch, false, 1)
        });
        let svc_frozen = s.spawn(move || {
            serve_batched_tch(frozen, tokenizer, req_frozen_rx, max_batch, false, 1)
        });
        let mut handles = Vec::with_capacity(n_games);
        for game_idx in 0..n_games {
            let req_live = req_live_tx.clone();
            let req_frozen = req_frozen_tx.clone();
            let atf = action_token_fn.clone();
            let seed = base_seed.wrapping_add(game_idx as u64);
            handles.push(s.spawn(move || {
                let mut live_actor = RemoteModel::new(req_live)
                    .with_inference(InferenceMode::LmSoftmax, 0.0, Some(atf.clone()))
                    .with_temp(1.0);
                let mut frozen_actor = RemoteModel::new(req_frozen)
                    .with_inference(InferenceMode::LmSoftmax, 0.0, Some(atf))
                    .with_temp(frozen_temp);
                let mut rng: StdRng = SeedableRng::seed_from_u64(seed);
                play_one_hand_exploiter(
                    &mut live_actor,
                    &mut frozen_actor,
                    seating,
                    game_idx,
                    new_state(game_idx),
                    &mut rng,
                )
            }));
        }
        drop(req_live_tx);
        drop(req_frozen_tx);
        let out: Vec<RnadTrajectory> =
            handles.into_iter().map(|h| h.join().expect("game thread panicked")).collect();
        svc_live.join().expect("live service panicked");
        svc_frozen.join().expect("frozen service panicked");
        out
    })
}

/// Evaluate `subject` occupying ONE seat (rotating by game index)
/// against `reference` at every other seat, both greedy-LM at their own
/// temperatures. Returns (mean subject payoff, SEM). This is the
/// unilateral-deviation payoff — for a policy at equilibrium it is ≤ 0
/// up to noise no matter what `subject` is.
#[allow(clippy::too_many_arguments)]
pub fn eval_subject_vs_net_batched_tch<G, T, FNS>(
    subject: &GoMctsTransformerTch,
    reference: &GoMctsTransformerTch,
    tokenizer: &T,
    new_state: FNS,
    n_games: usize,
    base_seed: u64,
    action_token_fn: ActionTokenFn,
    subject_temp: f64,
    reference_temp: f64,
) -> (f64, f64)
where
    G: GameState + Send,
    T: Tokenizer<G> + Send + Sync,
    FNS: Fn(usize) -> G + Send + Sync + Copy,
{
    use std::sync::mpsc;
    if n_games == 0 {
        return (0.0, 0.0);
    }
    let max_batch = n_games.clamp(32, 512);
    let (req_s_tx, req_s_rx) = mpsc::channel::<ServiceRequest>();
    let (req_r_tx, req_r_rx) = mpsc::channel::<ServiceRequest>();
    let scores: Vec<f64> = std::thread::scope(|s| {
        let svc_s = s.spawn(move || {
            serve_batched_tch(subject, tokenizer, req_s_rx, max_batch, false, 1)
        });
        let svc_r = s.spawn(move || {
            serve_batched_tch(reference, tokenizer, req_r_rx, max_batch, false, 1)
        });
        let mut handles = Vec::with_capacity(n_games);
        for game_idx in 0..n_games {
            let req_s = req_s_tx.clone();
            let req_r = req_r_tx.clone();
            let atf = action_token_fn.clone();
            let seed = base_seed.wrapping_add(game_idx as u64);
            handles.push(s.spawn(move || {
                let mut subj = RemoteModel::new(req_s)
                    .with_inference(InferenceMode::LmSoftmax, 0.0, Some(atf.clone()))
                    .with_temp(subject_temp);
                let mut refr = RemoteModel::new(req_r)
                    .with_inference(InferenceMode::LmSoftmax, 0.0, Some(atf))
                    .with_temp(reference_temp);
                let mut rng: StdRng = SeedableRng::seed_from_u64(seed);
                let mut gs = new_state(game_idx);
                let mut buf = Vec::new();
                let n_players = gs.num_players();
                let subject_seat = game_idx % n_players;
                while !gs.is_terminal() {
                    buf.clear();
                    gs.legal_actions(&mut buf);
                    if gs.is_chance_node() {
                        let a = *buf.choose(&mut rng).expect("non-empty chance");
                        gs.apply_action(a);
                        continue;
                    }
                    let p = gs.cur_player();
                    let h = gs.istate_key(p);
                    let a = if p == subject_seat {
                        <RemoteModel as GenerativeModel<G>>::sample(&mut subj, &h, &buf, &mut rng)
                    } else {
                        <RemoteModel as GenerativeModel<G>>::sample(&mut refr, &h, &buf, &mut rng)
                    };
                    gs.apply_action(a);
                }
                gs.evaluate(subject_seat)
            }));
        }
        drop(req_s_tx);
        drop(req_r_tx);
        let scores: Vec<f64> =
            handles.into_iter().map(|h| h.join().expect("game thread panicked")).collect();
        svc_s.join().expect("subject service panicked");
        svc_r.join().expect("reference service panicked");
        scores
    });
    super::gomcts_transformer::finish_mean_se(&scores)
}

// =====================================================================
// Learner
// =====================================================================

/// Tokenized minibatch of decision points, resident on the net's device.
struct RowBatch {
    input: Tensor,      // (B, ctx) i64
    prefix: Tensor,     // (B,) i64 — last real token index of `history`
    action_tok: Tensor, // (B,) i64
    legal_add: Tensor,  // (B, V) f32: 0 on legal tokens, -1e9 elsewhere
    legal_bool: Tensor, // (B, V) f32: 1 on legal tokens
    n_rows: i64,
}

fn build_rows<G: GameState, T: Tokenizer<G>>(
    net: &GoMctsTransformerTch,
    tokenizer: &T,
    steps: &[&RnadStep],
) -> RowBatch {
    let cfg = net.config();
    let device = net.device();
    let (ctx, vocab) = (cfg.max_context, cfg.vocab_size);
    let pad = tokenizer.pad_token();
    let b = steps.len();
    let mut tokens: Vec<i64> = Vec::with_capacity(b * ctx);
    let mut prefix: Vec<i64> = Vec::with_capacity(b);
    let mut action_tok: Vec<i64> = Vec::with_capacity(b);
    let mut legal_add: Vec<f32> = vec![-1e9; b * vocab];
    let mut legal_bool: Vec<f32> = vec![0.0; b * vocab];
    for (i, st) in steps.iter().enumerate() {
        let enc = tokenizer.encode(&st.history);
        assert!(!enc.is_empty(), "R-NaD requires non-empty observation histories");
        let (padded, real_len) = pad_to(&enc, ctx, pad);
        tokens.extend(padded.iter().map(|&u| u as i64));
        prefix.push((real_len - 1) as i64);
        action_tok.push(tokenizer.action_token(st.action) as i64);
        for &a in &st.legal {
            let t = tokenizer.action_token(a) as usize;
            legal_add[i * vocab + t] = 0.0;
            legal_bool[i * vocab + t] = 1.0;
        }
    }
    RowBatch {
        input: Tensor::from_slice(&tokens).reshape([b as i64, ctx as i64]).to_device(device),
        prefix: Tensor::from_slice(&prefix).to_device(device),
        action_tok: Tensor::from_slice(&action_tok).to_device(device),
        legal_add: Tensor::from_slice(&legal_add)
            .reshape([b as i64, vocab as i64])
            .to_device(device),
        legal_bool: Tensor::from_slice(&legal_bool)
            .reshape([b as i64, vocab as i64])
            .to_device(device),
        n_rows: b as i64,
    }
}

/// Per-row quantities from one forward pass: LM logits at the prefix
/// position (B, V) and V at the prefix position (B,).
fn forward_rows(net: &GoMctsTransformerTch, rows: &RowBatch) -> (Tensor, Tensor) {
    let vocab = net.config().vocab_size as i64;
    let (lm, val) = net.forward(&rows.input);
    let lm_idx = rows
        .prefix
        .unsqueeze(-1)
        .unsqueeze(-1)
        .expand([rows.n_rows, 1, vocab], false);
    let lm_at_prefix = lm.gather(1, &lm_idx, false).squeeze_dim(1);
    let val_at_prefix = val.gather(1, &rows.prefix.unsqueeze(-1), false).squeeze_dim(1);
    (lm_at_prefix, val_at_prefix)
}

/// Centered legal logit of the sampled action: logit(a) − mean legal
/// logit. The quantity NeuRD updates.
fn centered_action_logit(lm: &Tensor, rows: &RowBatch) -> Tensor {
    let legal_count = rows.legal_bool.sum_dim_intlist([-1i64].as_ref(), false, Kind::Float);
    let legal_mean = (lm * &rows.legal_bool).sum_dim_intlist([-1i64].as_ref(), false, Kind::Float)
        / legal_count.clamp_min(1.0);
    let logit_a = lm.gather(1, &rows.action_tok.unsqueeze(-1), false).squeeze_dim(1);
    logit_a - legal_mean
}

/// Detached per-step quantities from phase A of the learner.
struct StepEval {
    logp: Vec<f32>,      // log πθ(a|h)
    logp_reg: Vec<f32>,  // log π_reg(a|h)
    value: Vec<f32>,     // V(h)
    c_logit: Vec<f32>,   // centered legal logit of a (for the NeuRD gate)
    entropy: Vec<f32>,   // H(πθ(·|h))
    kl_reg: Vec<f32>,    // KL(πθ ‖ π_reg) over legal actions
}

pub struct RnadTrainer<G, T> {
    pub net: GoMctsTransformerTch,
    reg: GoMctsTransformerTch,
    opt: nn::Optimizer,
    pub cfg: RnadConfig,
    tokenizer: T,
    learner_iters: usize,
    /// Sample of the most recent batch's decision states, retained so a
    /// π_reg refresh can measure `kl_outer` on-distribution.
    recent_steps: Vec<RnadStep>,
    _g: std::marker::PhantomData<G>,
}

impl<G: GameState, T: Tokenizer<G>> RnadTrainer<G, T> {
    pub fn new(net: GoMctsTransformerTch, tokenizer: T, cfg: RnadConfig) -> Result<Self> {
        let reg = SnapshotTch::from_model(&net)?.hydrate(net.device())?;
        let opt = nn::AdamW::default()
            .build(net.var_store(), cfg.lr)
            .map_err(|e| anyhow!("build AdamW: {e}"))?;
        Ok(Self {
            net,
            reg,
            opt,
            cfg,
            tokenizer,
            learner_iters: 0,
            recent_steps: Vec::new(),
            _g: std::marker::PhantomData,
        })
    }

    pub fn learner_iters(&self) -> usize {
        self.learner_iters
    }

    /// Update the optimizer learning rate (lr-annealing schedules: damp
    /// the late-run orbit around the fixed point).
    pub fn set_lr(&mut self, lr: f64) {
        self.opt.set_lr(lr);
    }

    /// Update η mid-run (η-annealing: shrink the smoothing gap between
    /// the regularized fixed point and the unregularized equilibrium as
    /// the outer loop settles). `cfg` is public so this is sugar, but it
    /// keeps schedule code symmetric with `set_lr`.
    pub fn set_eta(&mut self, eta: f64) {
        self.cfg.eta = eta;
    }

    /// π_reg ← current live weights (the R-NaD "update" step). Returns
    /// `kl_outer`: KL(π_now ‖ π_reg_outgoing) over the retained recent
    /// states, measured BEFORE the swap — the distance this fixed-point
    /// iteration travelled. `None` when there is nothing meaningful to
    /// measure (no recent states yet, or η = 0 / exploiter mode where
    /// π_reg plays no role).
    pub fn refresh_regularization_policy(&mut self) -> Result<Option<f64>> {
        let kl_outer = if self.cfg.eta != 0.0 && !self.recent_steps.is_empty() {
            let refs: Vec<&RnadStep> = self.recent_steps.iter().collect();
            let eval = self.eval_steps(&refs);
            let n = eval.kl_reg.len().max(1) as f64;
            Some(eval.kl_reg.iter().map(|&x| x as f64).sum::<f64>() / n)
        } else {
            None
        };
        self.reg = SnapshotTch::from_model(&self.net)?.hydrate(self.net.device())?;
        Ok(kl_outer)
    }

    /// Phase A: detached evaluation of every step under πθ and π_reg.
    fn eval_steps(&self, steps: &[&RnadStep]) -> StepEval {
        let mut out = StepEval {
            logp: Vec::with_capacity(steps.len()),
            logp_reg: Vec::with_capacity(steps.len()),
            value: Vec::with_capacity(steps.len()),
            c_logit: Vec::with_capacity(steps.len()),
            entropy: Vec::with_capacity(steps.len()),
            kl_reg: Vec::with_capacity(steps.len()),
        };
        // η = 0 (exploiter / plain best-response mode): π_reg plays no
        // role in the loss, so skip its forward — halves phase-A cost.
        let skip_reg = self.cfg.eta == 0.0;
        for chunk in steps.chunks(self.cfg.minibatch_steps) {
            let rows = build_rows(&self.net, &self.tokenizer, chunk);
            tch::no_grad(|| {
                let (lm, val) = forward_rows(&self.net, &rows);
                let logp_full = (&lm + &rows.legal_add).log_softmax(-1, Kind::Float);
                let probs = logp_full.exp();
                let gather_a = |t: &Tensor| {
                    t.gather(1, &rows.action_tok.unsqueeze(-1), false).squeeze_dim(1)
                };
                let logp_a = Vec::<f32>::try_from(gather_a(&logp_full)).expect("logp");
                let (logp_reg_a, kl): (Vec<f32>, Vec<f32>) = if skip_reg {
                    (logp_a.clone(), vec![0.0; chunk.len()])
                } else {
                    let (lm_reg, _) = forward_rows(&self.reg, &rows);
                    let logp_reg_full =
                        (&lm_reg + &rows.legal_add).log_softmax(-1, Kind::Float);
                    let kl = (&probs * (&logp_full - &logp_reg_full))
                        .sum_dim_intlist([-1i64].as_ref(), false, Kind::Float);
                    (
                        Vec::<f32>::try_from(gather_a(&logp_reg_full)).expect("logp_reg"),
                        Vec::<f32>::try_from(kl).expect("kl"),
                    )
                };
                let c = centered_action_logit(&lm, &rows);
                // Illegal tokens carry prob exp(-1e9)=0, so their
                // products vanish without explicit masking.
                let entropy = -(&probs * &logp_full)
                    .sum_dim_intlist([-1i64].as_ref(), false, Kind::Float);
                out.logp.extend(logp_a);
                out.logp_reg.extend(logp_reg_a);
                out.value.extend(Vec::<f32>::try_from(val).expect("value"));
                out.c_logit.extend(Vec::<f32>::try_from(c).expect("c_logit"));
                out.entropy.extend(Vec::<f32>::try_from(entropy).expect("entropy"));
                out.kl_reg.extend(kl);
            });
        }
        out
    }

    /// One R-NaD learner step over a batch of on-policy trajectories:
    /// transformed returns → advantages → gated NeuRD + value updates.
    /// Refreshes π_reg every `cfg.reg_update_every` calls.
    pub fn learner_step(
        &mut self,
        trajs: &[RnadTrajectory],
        rng: &mut StdRng,
    ) -> Result<RnadStats> {
        let cfg = self.cfg;
        let all_steps: Vec<&RnadStep> = trajs.iter().flat_map(|t| t.steps.iter()).collect();
        let n = all_steps.len();
        if n == 0 {
            return Ok(RnadStats::default());
        }
        let eval = self.eval_steps(&all_steps);

        // Transformed returns + advantages, per trajectory, reverse scan.
        // Step s pays its team −η·logratio_s and the other team +η·…;
        // G_t for the actor at t sums its team's stream over s ≥ t plus
        // the terminal payoff.
        let mut returns = vec![0.0_f32; n];
        let mut coefs = vec![0.0_f32; n];
        let mut abs_adv_sum = 0.0_f64;
        let mut offset = 0usize;
        for traj in trajs {
            let k = traj.steps.len();
            let mut team_acc = [0.0_f64; 2];
            let mut own_acc = vec![0.0_f64; traj.payoffs.len()];
            for i in (0..k).rev() {
                let g = offset + i;
                let st = &traj.steps[i];
                let log_ratio = ((eval.logp[g] - eval.logp_reg[g]) as f64)
                    .clamp(-cfg.log_ratio_clip, cfg.log_ratio_clip);
                let penalty = cfg.eta * log_ratio;
                let reg_stream = if cfg.spread_penalty {
                    let np = own_acc.len();
                    let share = penalty / (np - 1).max(1) as f64;
                    for (q, acc) in own_acc.iter_mut().enumerate() {
                        if q == st.player {
                            *acc -= penalty;
                        } else {
                            *acc += share;
                        }
                    }
                    own_acc[st.player]
                } else {
                    let tm = team_of(st.player);
                    team_acc[tm] -= penalty;
                    team_acc[1 - tm] += penalty;
                    team_acc[tm]
                };
                let g_t = traj.payoffs[st.player] + reg_stream;
                returns[g] = g_t as f32;
                let adv = g_t - eval.value[g] as f64;
                abs_adv_sum += adv.abs();
                // Sampled-action NeuRD: importance-correct by 1/π_b,
                // then gate updates pushing a saturated logit outward.
                let w = (1.0 / st.behavior_prob as f64).min(cfg.is_clip);
                let mut coef = w * adv;
                let c = eval.c_logit[g] as f64;
                if (c >= cfg.neurd_clip && coef > 0.0) || (c <= -cfg.neurd_clip && coef < 0.0) {
                    coef = 0.0;
                }
                coefs[g] = coef as f32;
            }
            offset += k;
        }

        // Phase C: gradient minibatches over shuffled steps.
        let device = self.net.device();
        let mut idx: Vec<usize> = (0..n).collect();
        for i in (1..idx.len()).rev() {
            let j = (rng.random::<u64>() as usize) % (i + 1);
            idx.swap(i, j);
        }
        let mut policy_loss_sum = 0.0_f64;
        let mut value_loss_sum = 0.0_f64;
        let mut rows_done = 0usize;
        for chunk in idx.chunks(cfg.minibatch_steps) {
            let chunk_steps: Vec<&RnadStep> = chunk.iter().map(|&i| all_steps[i]).collect();
            let chunk_coefs: Vec<f32> = chunk.iter().map(|&i| coefs[i]).collect();
            let chunk_returns: Vec<f32> = chunk.iter().map(|&i| returns[i]).collect();
            let rows = build_rows(&self.net, &self.tokenizer, &chunk_steps);
            let (lm, val) = forward_rows(&self.net, &rows);
            let c_a = centered_action_logit(&lm, &rows);
            let coef_t = Tensor::from_slice(&chunk_coefs).to_device(device);
            let ret_t = Tensor::from_slice(&chunk_returns).to_device(device);
            let policy_loss = -(coef_t * c_a).mean(Kind::Float);
            let value_loss = (val - ret_t).square().mean(Kind::Float);
            let total = &policy_loss + &value_loss * cfg.value_weight;
            self.opt.zero_grad();
            total.backward();
            self.opt.clip_grad_norm(cfg.grad_clip);
            self.opt.step();
            policy_loss_sum += policy_loss.double_value(&[]) * chunk.len() as f64;
            value_loss_sum += value_loss.double_value(&[]) * chunk.len() as f64;
            rows_done += chunk.len();
        }

        // Retain a sample of this batch's states for the on-distribution
        // kl_outer measurement at the next π_reg refresh. `idx` is
        // already shuffled, so the prefix is an unbiased sample.
        self.recent_steps =
            idx.iter().take(cfg.minibatch_steps).map(|&i| all_steps[i].clone()).collect();

        self.learner_iters += 1;
        let mut kl_outer = None;
        if cfg.reg_update_every > 0 && self.learner_iters % cfg.reg_update_every == 0 {
            kl_outer = self.refresh_regularization_policy()?;
        }

        let nf = rows_done.max(1) as f64;
        Ok(RnadStats {
            policy_loss: policy_loss_sum / nf,
            value_loss: value_loss_sum / nf,
            mean_abs_adv: abs_adv_sum / n as f64,
            mean_entropy: eval.entropy.iter().map(|&x| x as f64).sum::<f64>() / n as f64,
            mean_kl_reg: eval.kl_reg.iter().map(|&x| x as f64).sum::<f64>() / n as f64,
            kl_outer,
            n_steps: n,
        })
    }
}

// =====================================================================
// Eval adapters
// =====================================================================

/// `Policy` view of a trained net: LM head masked to legal actions,
/// softmax at temperature 1.0. Exact tree-walk consumers (the tabular
/// best-response / exploitability oracle) use this.
pub struct RnadNetPolicy<'a, G, T> {
    net: &'a GoMctsTransformerTch,
    tokenizer: T,
    _g: std::marker::PhantomData<G>,
}

impl<'a, G: GameState, T: Tokenizer<G>> RnadNetPolicy<'a, G, T> {
    pub fn new(net: &'a GoMctsTransformerTch, tokenizer: T) -> Self {
        Self { net, tokenizer, _g: std::marker::PhantomData }
    }
}

impl<G: GameState, T: Tokenizer<G>> Policy<G> for RnadNetPolicy<'_, G, T> {
    fn action_probabilities(&mut self, gs: &G) -> ActionVec<f64> {
        let legal = actions!(gs);
        let h = gs.istate_key(gs.cur_player());
        let (logits, _) = forward_histories_batch_tch(self.net, &self.tokenizer, &[h])
            .expect("forward for policy eval");
        let row = &logits[0];
        let vals: Vec<f64> = legal
            .iter()
            .map(|&a| row.get(self.tokenizer.action_token(a) as usize).copied().unwrap_or(f32::MIN) as f64)
            .collect();
        let max = vals.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let exps: Vec<f64> = vals.iter().map(|&v| (v - max).exp()).collect();
        let total: f64 = exps.iter().sum();
        let mut out = ActionVec::new(&legal);
        for (i, &a) in legal.iter().enumerate() {
            out[a] = if total > 0.0 { exps[i] / total } else { 1.0 / legal.len() as f64 };
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::algorithms::exploitability::exploitability;
    use crate::algorithms::gomcts_transformer::{kuhn::KuhnTokenizer, TransformerConfig};
    use games::gamestates::kuhn_poker::KuhnPoker;
    use tch::Device;

    /// End-to-end smoke: a few R-NaD iterations on Kuhn must run NaN-free
    /// and leave exploitability at or below the uniform-policy level
    /// (11/12 ≈ 0.917) — full convergence is exercised by the
    /// kuhn_rnad_train example, not the unit test.
    #[test]
    fn rnad_kuhn_smoke() {
        let tok = KuhnTokenizer;
        let cfg = TransformerConfig::kuhn_small(
            KuhnTokenizer::VOCAB_SIZE,
            KuhnTokenizer::MAX_CONTEXT,
        );
        let net = GoMctsTransformerTch::new(cfg, Device::Cpu).expect("build");
        let rnad_cfg = RnadConfig {
            lr: 1e-3,
            reg_update_every: 10,
            minibatch_steps: 512,
            ..Default::default()
        };
        let mut trainer: RnadTrainer<games::gamestates::kuhn_poker::KPGameState, _> =
            RnadTrainer::new(net, tok, rnad_cfg).expect("trainer");
        let atf: ActionTokenFn = std::sync::Arc::new(move |a| tok.action_token(a));
        let mut rng: StdRng = SeedableRng::seed_from_u64(7);
        let mut last = RnadStats::default();
        let mut saw_kl_outer = false;
        for it in 0..20 {
            let trajs = collect_rnad_games_batched_tch::<_, _, _>(
                &trainer.net,
                &tok,
                |_| KuhnPoker::new_state(),
                64,
                1000 + it as u64 * 64,
                atf.clone(),
                false,
                1,
            );
            last = trainer.learner_step(&trajs, &mut rng).expect("learner step");
            assert!(last.policy_loss.is_finite() && last.value_loss.is_finite());
            if let Some(k) = last.kl_outer {
                assert!(k.is_finite() && k >= 0.0, "kl_outer must be a finite KL: {k}");
                saw_kl_outer = true;
            }
        }
        assert!(last.n_steps > 0);
        // reg_update_every = 10 over 20 iters ⇒ refreshes at 10 and 20,
        // each of which must report the outer-loop movement metric.
        assert!(saw_kl_outer, "π_reg refreshes should emit kl_outer");
        let mut policy = RnadNetPolicy::new(&trainer.net, tok);
        let data = exploitability(|| (KuhnPoker::game().new)(), &mut policy);
        assert!(
            data.nash_conv.is_finite() && data.nash_conv < 1.4,
            "nash_conv after 20 iters should be sane: {}",
            data.nash_conv
        );
    }
}
