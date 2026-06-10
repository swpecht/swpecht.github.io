//! Tch (libtorch) backed transformer for GO-MCTS. Sole ML backend after
//! the candle removal; the same safetensors files written by either
//! historical backend continue to load (PyTorch-style `[out, in]` linear
//! layout, F32 parameters, identical name scheme).
//!
//! Module layout (top to bottom):
//!   * Backend-agnostic types: `Tokenizer`, `TransformerConfig`,
//!     `TrainExample`, `McfsConfig`, `InferenceMode`.
//!   * Tch model: `GoMctsTransformerTch`, `forward_histories_batch_tch`.
//!   * CUDA graph capture/replay (`ForwardGraph`,
//!     `forward_histories_batch_tch_graph`).
//!   * Inference service (`serve_batched_tch`) + `RemoteModel` over mpsc.
//!   * Self-play / eval / h2h / pop batched paths
//!     (`collect_self_play_games_batched_alphazero_tch`,
//!      `eval_vs_random_batched_tch`, `head_to_head_eval_batched_tch`,
//!      `collect_population_games_batched_tch`,
//!      `collect_pop_examples_batched_tch`).
//!   * Training (`train_tch`, `train_tch_with_callback`).
//!   * Population / snapshot (`PopulationTch`, `SnapshotTch`).
//!   * Per-game tokenizer impls (`kuhn`, `euchre`).

use anyhow::{anyhow, Result};
use safetensors::SafeTensors;
use tch::{nn, nn::Module, nn::OptimizerConfig, Device, Kind, Tensor};

use games::{istate::IStateKey, Action, GameState};
use rand::rngs::StdRng;
use rand::seq::IndexedRandom;
use rand::{RngExt, SeedableRng};

use super::gomcts::GenerativeModel;

// =====================================================================
// Backend-agnostic types (moved from the deleted candle module).
// =====================================================================

/// Per-game adapter that turns an observation history into a token sequence.
///
/// Token id 0 is reserved for PAD. All real game tokens start at 1.
pub trait Tokenizer<G: GameState>: Send + Sync {
    fn vocab_size(&self) -> usize;
    fn max_context(&self) -> usize;
    fn pad_token(&self) -> u32 {
        0
    }
    /// Encode the search player's observation history into tokens.
    /// Implementations should map `IStateKey` actions → game-specific
    /// tokens. Length must be ≤ `max_context()`.
    fn encode(&self, history: &IStateKey) -> Vec<u32>;
    /// Map a game `Action` (as it appears in legal-action lists) to its
    /// token id. Used for masking + sampling.
    fn action_token(&self, a: Action) -> u32;
}

#[derive(Clone, Copy, Debug)]
pub struct TransformerConfig {
    pub vocab_size: usize,
    pub max_context: usize,
    pub d_model: usize,
    pub n_heads: usize,
    pub n_layers: usize,
    pub d_ff: usize,
}

impl TransformerConfig {
    /// Tiny config for unit-test speed. Not for serious training.
    pub fn kuhn_small(vocab_size: usize, max_context: usize) -> Self {
        Self { vocab_size, max_context, d_model: 32, n_heads: 2, n_layers: 2, d_ff: 64 }
    }

    /// Mid-sized config; reasonable Euchre smoke-test size.
    pub fn euchre_smoke(vocab_size: usize, max_context: usize) -> Self {
        Self { vocab_size, max_context, d_model: 64, n_heads: 4, n_layers: 2, d_ff: 128 }
    }

    /// Larger Euchre config. 4 layers / 4 heads / d=128 — about 4× the
    /// parameter count of `euchre_smoke`. Still CPU-trainable in
    /// minutes-to-hours but big enough to start representing
    /// trick-taking-game structure.
    pub fn euchre_medium(vocab_size: usize, max_context: usize) -> Self {
        Self { vocab_size, max_context, d_model: 128, n_heads: 4, n_layers: 4, d_ff: 256 }
    }

    /// Paper-faithful size: 8 layers, 8 heads, 256-d embedding, 1024 FF.
    /// Practical only with GPU acceleration (or substantial CPU patience).
    pub fn paper_default(vocab_size: usize, max_context: usize) -> Self {
        Self { vocab_size, max_context, d_model: 256, n_heads: 8, n_layers: 8, d_ff: 1024 }
    }
}

/// One self-play step from the search player's POV.
#[derive(Clone, Debug)]
pub struct TrainExample {
    pub history: IStateKey,
    pub action: Action,
    pub value: f32,
    /// Optional MCTS-driven soft policy target as `(legal_action, prob)`
    /// pairs. When present the LM head trains against this distribution
    /// (AlphaZero-style); when `None` we fall back to a hard target
    /// using `action` (REINFORCE-style sampled-action imitation).
    pub policy_target: Option<Vec<(Action, f32)>>,
}

impl TrainExample {
    pub fn hard(history: IStateKey, action: Action, value: f32) -> Self {
        Self { history, action, value, policy_target: None }
    }
    pub fn soft(
        history: IStateKey,
        action: Action,
        value: f32,
        policy_target: Vec<(Action, f32)>,
    ) -> Self {
        Self { history, action, value, policy_target: Some(policy_target) }
    }
}

/// Configuration knobs for MCTS-driven self-play.
#[derive(Clone, Copy, Debug)]
pub struct McfsConfig {
    /// Dirichlet noise concentration α applied to the root visit
    /// distribution before sampling the played action.
    pub root_dirichlet_alpha: f64,
    /// Mixing weight: played-action prob = (1-ε)·visit_prob + ε·dirichlet.
    pub root_dirichlet_eps: f64,
    /// MCTS rollout phase length per leaf expansion.
    pub n_rollout_steps: usize,
    /// Parallel-sim width inside one game's MCTS (virtual loss).
    pub n_parallel_sims: usize,
}

impl Default for McfsConfig {
    fn default() -> Self {
        Self {
            root_dirichlet_alpha: f64::INFINITY,
            root_dirichlet_eps: 0.0,
            n_rollout_steps: 0,
            n_parallel_sims: 1,
        }
    }
}

/// How the `GenerativeModel::sample` / `policy` calls produce a
/// distribution over legal actions. Currently only consumed by external
/// inference callers (e.g. `euchre_gomcts_eval`) — the in-process
/// service-based path uses ArgmaxVal\* semantics implicitly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InferenceMode {
    /// AlphaZero-style: for each legal `a`, query `V(h⊕a)`, softmax
    /// over those scalar values. Default.
    ArgmaxVal,
    /// LM-head-softmax: forward `h` once, read the LM logits at the
    /// last position, mask to legal-action tokens, softmax.
    LmSoftmax,
}

impl Default for InferenceMode {
    fn default() -> Self {
        InferenceMode::ArgmaxVal
    }
}

/// Loss weights from the paper (Table 4).
const LM_LOSS_WEIGHT_AGNOSTIC: f64 = 0.9;
const VALUE_LOSS_WEIGHT_AGNOSTIC: f64 = 0.1;

/// Softmax temperature for the ArgmaxVal\*-style policy.
const POLICY_SOFTMAX_TEMP: f64 = 0.5;

/// Pad `tokens` to length `max_context` with `pad_token`. Returns the
/// padded vec and an "attention length" telling the caller where the real
/// data ends (so it can index the right last-position logits).
pub(crate) fn pad_to(tokens: &[u32], max_context: usize, pad_token: u32) -> (Vec<u32>, usize) {
    let n = tokens.len().min(max_context);
    let mut out = vec![pad_token; max_context];
    out[..n].copy_from_slice(&tokens[..n]);
    (out, n)
}

pub(crate) fn finish_mean_se(scores: &[f64]) -> (f64, f64) {
    let n = scores.len() as f64;
    if n == 0.0 {
        return (0.0, 0.0);
    }
    let mean: f64 = scores.iter().sum::<f64>() / n;
    let var: f64 = scores.iter().map(|x| (x - mean) * (x - mean)).sum::<f64>() / n;
    let se = (var / n).max(0.0).sqrt();
    (mean, se)
}

// =====================================================================
// Inference service wire types (used by `RemoteModel` and the batching
// service threads).
// =====================================================================

/// Channel message for the batching service.
pub enum ServiceRequest {
    /// Forward this list of histories, reply on `response_tx` with
    /// per-history (logits, value).
    Forward {
        histories: Vec<IStateKey>,
        response_tx: std::sync::mpsc::Sender<(Vec<Vec<f32>>, Vec<f32>)>,
    },
}

/// A `GenerativeModel` implementation that forwards every call over an
/// mpsc channel to a central batching service. Owned per game thread;
/// the sender is `Send` so it can be cloned into each thread.
#[derive(Clone)]
pub struct RemoteModel {
    pub request_tx: std::sync::mpsc::Sender<ServiceRequest>,
}

impl RemoteModel {
    /// Block until the service responds. The (logits, value) vectors are
    /// aligned with the input `histories` order.
    fn forward(&self, histories: Vec<IStateKey>) -> (Vec<Vec<f32>>, Vec<f32>) {
        let (response_tx, response_rx) = std::sync::mpsc::channel();
        self.request_tx
            .send(ServiceRequest::Forward { histories, response_tx })
            .expect("batching service has terminated");
        response_rx.recv().expect("service dropped response channel")
    }

    fn masked_policy_via_service(&self, history: &IStateKey, legal: &[Action]) -> Vec<f64> {
        let histories: Vec<IStateKey> = legal
            .iter()
            .map(|&a| {
                let mut h = *history;
                h.push(a);
                h
            })
            .collect();
        let (_, values) = self.forward(histories);
        let temp = POLICY_SOFTMAX_TEMP;
        let values: Vec<f64> = values.into_iter().map(|v| v as f64).collect();
        let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let exps: Vec<f64> = values.iter().map(|s| ((s - max) / temp).exp()).collect();
        let total: f64 = exps.iter().sum();
        if total == 0.0 || !total.is_finite() {
            return vec![1.0 / legal.len() as f64; legal.len()];
        }
        exps.into_iter().map(|e| e / total).collect()
    }
}

impl<G: GameState> GenerativeModel<G> for RemoteModel {
    fn sample(&mut self, history: &IStateKey, legal: &[Action], rng: &mut StdRng) -> Action {
        let probs = self.masked_policy_via_service(history, legal);
        let mut r: f64 = rng.random::<f64>();
        for (i, p) in probs.iter().enumerate() {
            r -= *p;
            if r <= 0.0 {
                return legal[i];
            }
        }
        *legal.choose(rng).expect("non-empty legal")
    }

    fn value(&mut self, history: &IStateKey) -> f64 {
        let (_, values) = self.forward(vec![*history]);
        values[0] as f64
    }

    fn policy(&mut self, history: &IStateKey, legal: &[Action]) -> Vec<f64> {
        self.masked_policy_via_service(history, legal)
    }

    /// Override: send ALL histories in one service request → one
    /// batched forward at the service. Key win for parallel-sim MCTS.
    fn batch_value(&mut self, histories: &[IStateKey]) -> Vec<f64> {
        if histories.is_empty() {
            return Vec::new();
        }
        let (_, values) = self.forward(histories.to_vec());
        values.into_iter().map(|v| v as f64).collect()
    }
}

// =====================================================================
// Per-hand helpers used by the eval / h2h / pop batched paths.
// Backend-agnostic — they only touch `RemoteModel` + `GameState`.
// =====================================================================

pub(crate) fn play_one_hand_subject_vs_random<G>(
    subject: &mut RemoteModel,
    new_state: impl Fn() -> G,
    game_idx: usize,
    rng: &mut StdRng,
) -> f64
where
    G: GameState,
{
    let mut gs = new_state();
    let mut buf = Vec::new();
    while gs.is_chance_node() {
        buf.clear();
        gs.legal_actions(&mut buf);
        let a = *buf.choose(rng).expect("non-empty chance");
        gs.apply_action(a);
    }
    let n_players = gs.num_players();
    let subject_seat = game_idx % n_players;
    while !gs.is_terminal() {
        let p = gs.cur_player();
        buf.clear();
        gs.legal_actions(&mut buf);
        let a = if p == subject_seat {
            let h = gs.istate_key(p);
            <RemoteModel as GenerativeModel<G>>::sample(subject, &h, &buf, rng)
        } else {
            *buf.choose(rng).expect("non-empty legal")
        };
        gs.apply_action(a);
    }
    gs.evaluate(subject_seat)
}

pub(crate) fn play_one_hand_a_vs_b<G>(
    a: &mut RemoteModel,
    b: &mut RemoteModel,
    new_state: impl Fn() -> G,
    game_idx: usize,
    rng: &mut StdRng,
) -> f64
where
    G: GameState,
{
    let mut gs = new_state();
    let mut buf = Vec::new();
    while gs.is_chance_node() {
        buf.clear();
        gs.legal_actions(&mut buf);
        let act = *buf.choose(rng).expect("non-empty chance");
        gs.apply_action(act);
    }
    let n_players = gs.num_players();
    let a_offset = game_idx % 2;
    let is_a = |seat: usize| -> bool {
        if n_players == 2 {
            seat == a_offset
        } else {
            (seat % 2) == a_offset
        }
    };
    while !gs.is_terminal() {
        let p = gs.cur_player();
        buf.clear();
        gs.legal_actions(&mut buf);
        let history = gs.istate_key(p);
        let act = if is_a(p) {
            <RemoteModel as GenerativeModel<G>>::sample(a, &history, &buf, rng)
        } else {
            <RemoteModel as GenerativeModel<G>>::sample(b, &history, &buf, rng)
        };
        gs.apply_action(act);
    }
    let a_seat = (0..n_players).find(|s| is_a(*s)).unwrap_or(0);
    gs.evaluate(a_seat)
}

pub(crate) fn play_one_hand_pop<G>(
    live: &mut RemoteModel,
    frozen: &mut RemoteModel,
    new_state: impl Fn() -> G,
    game_idx: usize,
    rng: &mut StdRng,
) -> Vec<TrainExample>
where
    G: GameState,
{
    let mut gs = new_state();
    let mut buf = Vec::new();
    while gs.is_chance_node() {
        buf.clear();
        gs.legal_actions(&mut buf);
        let act = *buf.choose(rng).expect("non-empty chance");
        gs.apply_action(act);
    }
    let n_players = gs.num_players();
    let live_seat = game_idx % n_players;
    let mut per_player: Vec<Vec<(IStateKey, Action)>> = vec![Vec::new(); n_players];
    while !gs.is_terminal() {
        let p = gs.cur_player();
        buf.clear();
        gs.legal_actions(&mut buf);
        let history = gs.istate_key(p);
        let act = if p == live_seat {
            <RemoteModel as GenerativeModel<G>>::sample(live, &history, &buf, rng)
        } else {
            <RemoteModel as GenerativeModel<G>>::sample(frozen, &history, &buf, rng)
        };
        per_player[p].push((history, act));
        gs.apply_action(act);
    }
    let mut out = Vec::new();
    for p in 0..n_players {
        let v = gs.evaluate(p) as f32;
        for (h, a) in per_player[p].drain(..) {
            out.push(TrainExample::hard(h, a, v));
        }
    }
    out
}

// =====================================================================
// MCTS-driven self-play (backend-agnostic; takes any `GenerativeModel`).
// =====================================================================

/// Draw a Dirichlet(α, K) sample using K Gamma(α, 1) draws normalised
/// to sum to 1.
fn dirichlet_sample(n: usize, alpha: f64, rng: &mut StdRng) -> Vec<f64> {
    let mut samples = vec![0.0_f64; n];
    let (d, c) = if alpha >= 1.0 {
        let d = alpha - 1.0 / 3.0;
        (d, 1.0 / (9.0 * d).sqrt())
    } else {
        let d = alpha + 1.0 - 1.0 / 3.0;
        (d, 1.0 / (9.0 * d).sqrt())
    };
    let mut cache: Option<f64> = None;
    for s in samples.iter_mut() {
        loop {
            let x: f64 = if let Some(z) = cache.take() {
                z
            } else {
                let mut z0 = 0.0_f64;
                let mut z1 = 0.0_f64;
                loop {
                    let u1: f64 = rng.random::<f64>();
                    let u2: f64 = rng.random::<f64>();
                    if u1 > 1e-12 {
                        z0 = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
                        z1 = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).sin();
                        break;
                    }
                }
                cache = Some(z1);
                z0
            };
            let v_cube = 1.0 + c * x;
            if v_cube <= 0.0 {
                continue;
            }
            let v = v_cube * v_cube * v_cube;
            let u: f64 = rng.random::<f64>();
            if u < 1.0 - 0.0331 * x.powi(4) {
                *s = d * v;
                break;
            }
            if u.ln() < 0.5 * x * x + d * (1.0 - v + v.ln()) {
                *s = d * v;
                break;
            }
        }
        if alpha < 1.0 {
            let u: f64 = rng.random::<f64>();
            *s *= u.powf(1.0 / alpha);
        }
    }
    let total: f64 = samples.iter().sum();
    if total > 0.0 {
        for s in samples.iter_mut() {
            *s /= total;
        }
    } else {
        let uniform = 1.0 / n as f64;
        for s in samples.iter_mut() {
            *s = uniform;
        }
    }
    samples
}

/// AlphaZero-style self-play. Value-head target is the MCTS root value
/// at each decision (not the eventual terminal payoff); policy target is
/// the search's visit distribution.
pub fn collect_self_play_game_alphazero<G, M>(
    new_state: impl Fn() -> G,
    search: &mut super::gomcts::GoMcts<G, M>,
    cfg: McfsConfig,
    rng: &mut StdRng,
) -> Vec<TrainExample>
where
    G: GameState + games::resample::ResampleFromInfoState,
    M: GenerativeModel<G>,
{
    use crate::policy::Policy;
    use games::actions;
    let mut gs = new_state();
    let mut buf = Vec::new();
    while gs.is_chance_node() {
        buf.clear();
        gs.legal_actions(&mut buf);
        let a = *buf.choose(rng).expect("non-empty chance");
        gs.apply_action(a);
    }
    let n_players = gs.num_players();
    let mut per_player: Vec<Vec<(IStateKey, Action, Vec<(Action, f32)>, f32)>> =
        vec![Vec::new(); n_players];
    while !gs.is_terminal() {
        let p = gs.cur_player();
        let history = gs.istate_key(p);
        let probs = search.action_probabilities(&gs);
        let actions_legal = actions!(gs);
        let mut soft: Vec<(Action, f32)> =
            actions_legal.iter().map(|a| (*a, probs[*a] as f32)).collect();
        let sum: f32 = soft.iter().map(|(_, p)| *p).sum();
        if sum <= 1e-8 {
            let u = 1.0 / soft.len() as f32;
            for (_, p) in soft.iter_mut() {
                *p = u;
            }
        }
        let root_v = search.root_value(&history).unwrap_or(0.0) as f32;
        let play_probs: Vec<f64> = if cfg.root_dirichlet_eps > 0.0 && soft.len() > 1 {
            let noise = dirichlet_sample(soft.len(), cfg.root_dirichlet_alpha, rng);
            let eps = cfg.root_dirichlet_eps;
            soft.iter()
                .zip(noise.iter())
                .map(|((_, p), n)| (1.0 - eps) * (*p as f64) + eps * *n)
                .collect()
        } else {
            soft.iter().map(|(_, p)| *p as f64).collect()
        };
        let total_play: f64 = play_probs.iter().sum();
        let mut r: f64 = rng.random::<f64>() * total_play.max(1e-9);
        let mut chosen = soft[0].0;
        for ((a, _), pp) in soft.iter().zip(play_probs.iter()) {
            r -= *pp;
            if r <= 0.0 {
                chosen = *a;
                break;
            }
        }
        per_player[p].push((history, chosen, soft, root_v));
        gs.apply_action(chosen);
    }
    let mut out = Vec::new();
    for p in 0..n_players {
        for (h, a, soft, v) in per_player[p].drain(..) {
            out.push(TrainExample::soft(h, a, v, soft));
        }
    }
    out
}

// Suppress unused warnings for backend-shared loss-weight constants. The
// real LM_LOSS_WEIGHT / VALUE_LOSS_WEIGHT used by `train_tch_with_callback`
// live further down in this file; the duplicates here are retained as
// public-API documentation for callers that want to weight their own
// composite losses identically.
#[allow(dead_code)]
const _: f64 = LM_LOSS_WEIGHT_AGNOSTIC;
#[allow(dead_code)]
const _: f64 = VALUE_LOSS_WEIGHT_AGNOSTIC;


/// libtorch-backed transformer matching the candle `GoMctsTransformer`
/// in shape and parameter layout.
///
/// `unsafe impl Sync`: tch `Tensor` is `!Sync` because it wraps a raw
/// pointer with no compiler-visible thread-safety guarantee, but
/// libtorch's per-tensor refcounts are atomic and read-only access
/// (forward inference) from a single service thread is safe by
/// construction in this codebase. Game threads never touch the model
/// directly — they go through `RemoteModel` over an mpsc channel.
pub struct GoMctsTransformerTch {
    cfg: TransformerConfig,
    device: Device,
    vs: nn::VarStore,
    token_emb: nn::Embedding,
    pos_emb: nn::Embedding,
    blocks: Vec<Block>,
    ln_f: nn::LayerNorm,
    lm_head: nn::Linear,
    value_head: nn::Linear,
}

unsafe impl Sync for GoMctsTransformerTch {}
// libtorch tensors are Send-safe as long as the owning thread releases
// them before another touches them; the service-thread architecture in
// this module never crosses that boundary mid-call. Required so
// `Arc<GoMctsTransformerTch>` is Send (matters for any
// `GenerativeModel` impl built on top of one).
unsafe impl Send for GoMctsTransformerTch {}

struct Block {
    ln1: nn::LayerNorm,
    qkv: nn::Linear,
    out: nn::Linear,
    ln2: nn::LayerNorm,
    fc1: nn::Linear,
    fc2: nn::Linear,
    n_heads: i64,
    head_dim: i64,
}

impl GoMctsTransformerTch {
    pub fn new(cfg: TransformerConfig, device: Device) -> Result<Self> {
        let vs = nn::VarStore::new(device);
        let root = &vs.root();
        let d = cfg.d_model as i64;
        let v = cfg.vocab_size as i64;
        let t = cfg.max_context as i64;
        let ff = cfg.d_ff as i64;
        let h = cfg.n_heads as i64;
        assert!(d % h == 0);
        let head_dim = d / h;

        let token_emb = nn::embedding(root / "token_emb", v, d, Default::default());
        let pos_emb = nn::embedding(root / "pos_emb", t, d, Default::default());

        let mut blocks = Vec::with_capacity(cfg.n_layers);
        for i in 0..cfg.n_layers {
            let blk = root / format!("block_{i}");
            let ln_cfg = nn::LayerNormConfig { eps: 1e-5, ..Default::default() };
            blocks.push(Block {
                ln1: nn::layer_norm(&blk / "ln1", vec![d], ln_cfg),
                qkv: nn::linear(&blk / "attn" / "qkv", d, 3 * d, Default::default()),
                out: nn::linear(&blk / "attn" / "out", d, d, Default::default()),
                ln2: nn::layer_norm(&blk / "ln2", vec![d], ln_cfg),
                fc1: nn::linear(&blk / "mlp" / "fc1", d, ff, Default::default()),
                fc2: nn::linear(&blk / "mlp" / "fc2", ff, d, Default::default()),
                n_heads: h,
                head_dim,
            });
        }
        let ln_f = nn::layer_norm(
            root / "ln_f",
            vec![d],
            nn::LayerNormConfig { eps: 1e-5, ..Default::default() },
        );
        let lm_head = nn::linear(
            root / "lm_head",
            d,
            v,
            nn::LinearConfig { bias: false, ..Default::default() },
        );
        let value_head = nn::linear(root / "value_head", d, 1, Default::default());

        Ok(Self {
            cfg,
            device,
            vs,
            token_emb,
            pos_emb,
            blocks,
            ln_f,
            lm_head,
            value_head,
        })
    }

    /// Save all parameters to a safetensors file. Produces the same
    /// layout candle's `VarMap::save` writes (one Float32 tensor per
    /// variable, keyed by the same parameter path), so files written by
    /// either backend interoperate.
    pub fn save_safetensors(&self, path: &std::path::Path) -> Result<()> {
        use safetensors::tensor::{Dtype as SDtype, TensorView};
        // Pull every variable to CPU as contiguous f32 and serialize.
        // Hold the f32 vectors alive across `serialize` since TensorView
        // borrows the underlying bytes.
        let mut owned: Vec<(String, Vec<i64>, Vec<f32>)> = Vec::new();
        for (name, t) in self.vs.variables() {
            let cpu = tch::no_grad(|| t.to_kind(Kind::Float).to_device(Device::Cpu).contiguous());
            let shape: Vec<i64> = cpu.size();
            let nbytes = shape.iter().product::<i64>() as usize;
            let mut v = vec![0f32; nbytes];
            cpu.copy_data(&mut v, nbytes);
            owned.push((name, shape, v));
        }
        let views: Vec<(&str, TensorView)> = owned
            .iter()
            .map(|(n, shape, data)| {
                let shape_usize: Vec<usize> = shape.iter().map(|&x| x as usize).collect();
                let bytes: &[u8] = bytemuck::cast_slice(data.as_slice());
                let tv = TensorView::new(SDtype::F32, shape_usize, bytes)
                    .expect("build safetensors TensorView");
                (n.as_str(), tv)
            })
            .collect();
        let serialized = safetensors::tensor::serialize(views, &None)
            .map_err(|e| anyhow!("safetensors serialize: {e}"))?;
        std::fs::write(path, serialized)?;
        Ok(())
    }

    /// Load weights from a candle-saved safetensors file. Parameter
    /// names line up because both backends use the same path scheme.
    /// Linear weights match PyTorch convention `[out, in]` which is
    /// also what candle writes, so no transpose is needed.
    pub fn load_safetensors(&mut self, path: &std::path::Path) -> Result<()> {
        let bytes = std::fs::read(path)?;
        let st = SafeTensors::deserialize(&bytes)?;
        let mut named: std::collections::HashMap<String, Tensor> = self
            .vs
            .variables()
            .into_iter()
            .collect();
        let mut missing = Vec::new();
        for (name, var) in named.iter_mut() {
            let tv = match st.tensor(name) {
                Ok(t) => t,
                Err(_) => {
                    missing.push(name.clone());
                    continue;
                }
            };
            if tv.dtype() != safetensors::Dtype::F32 {
                return Err(anyhow!("unsupported dtype for {name}: {:?}", tv.dtype()));
            }
            let shape: Vec<i64> = tv.shape().iter().map(|&s| s as i64).collect();
            let data: &[u8] = tv.data();
            // Convert bytes (host) to a CPU f32 tensor, then move to the
            // VarStore's device and copy in place.
            let n_elems = shape.iter().product::<i64>() as usize;
            assert_eq!(data.len(), n_elems * 4);
            let floats: Vec<f32> = data
                .chunks_exact(4)
                .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                .collect();
            let cpu = Tensor::from_slice(&floats).reshape(&shape);
            let target = cpu.to_device(self.device);
            tch::no_grad(|| {
                var.copy_(&target);
            });
        }
        if !missing.is_empty() {
            return Err(anyhow!("missing tensors in safetensors: {missing:?}"));
        }
        Ok(())
    }

    pub fn device(&self) -> Device {
        self.device
    }

    pub fn config(&self) -> &TransformerConfig {
        &self.cfg
    }

    /// Forward pass. `tokens` is (B, T) int64.
    /// Returns `(lm_logits (B, T, V), value (B, T))`.
    pub fn forward(&self, tokens: &Tensor) -> (Tensor, Tensor) {
        let (_b, t) = tokens.size2().expect("(B,T) input");
        assert!(t <= self.cfg.max_context as i64);

        let tok = self.token_emb.forward(tokens);
        let positions = Tensor::arange(t, (Kind::Int64, self.device));
        let pos = self.pos_emb.forward(&positions);
        let mut x = tok + pos;

        for blk in &self.blocks {
            x = block_forward(blk, &x);
        }
        let x = x.apply(&self.ln_f);
        let lm_logits = self.lm_head.forward(&x);
        let value = self.value_head.forward(&x).squeeze_dim(-1);
        (lm_logits, value)
    }
}

fn block_forward(b: &Block, x: &Tensor) -> Tensor {
    let h = self_attn(b, &x.apply(&b.ln1));
    let x = x + h;
    let h = mlp(b, &x.apply(&b.ln2));
    x + h
}

fn self_attn(b: &Block, x: &Tensor) -> Tensor {
    let dims = x.size();
    let (bs, t, d) = (dims[0], dims[1], dims[2]);
    let h = b.n_heads;
    let hd = b.head_dim;
    let qkv = b.qkv.forward(x).reshape([bs, t, 3, h, hd]);
    // Split out (q, k, v) on the index-2 axis.
    let q = qkv.select(2, 0).transpose(1, 2).contiguous(); // (B, H, T, hd)
    let k = qkv.select(2, 1).transpose(1, 2).contiguous();
    let v = qkv.select(2, 2).transpose(1, 2).contiguous();
    // Use the fused scaled-dot-product attention from PyTorch. This is
    // the closest single-kernel replacement for the candle stack
    // (matmul → mask add → softmax → matmul). is_causal=true applies
    // the same upper-triangle mask candle builds explicitly.
    let attn = Tensor::scaled_dot_product_attention::<Tensor>(
        &q, &k, &v, None, 0.0, /*is_causal=*/ true, None, false,
    );
    let out = attn.transpose(1, 2).contiguous().reshape([bs, t, d]);
    b.out.forward(&out)
}

fn mlp(b: &Block, x: &Tensor) -> Tensor {
    let h = b.fc1.forward(x).gelu("none");
    b.fc2.forward(&h)
}

// =====================================================================
// CUDA graph capture / replay (FFI to at::cuda::CUDAGraph via the
// `cuda_graph_shim` C++ wrapper). Captures the whole transformer
// forward as one launch — collapses the per-launch driver overhead
// that dominates self-play wall time on WSL2.
// =====================================================================

extern "C" {
    fn cgs_use_pooled_stream();
    fn cgs_new() -> *mut std::ffi::c_void;
    fn cgs_free(g: *mut std::ffi::c_void);
    fn cgs_capture_begin(g: *mut std::ffi::c_void);
    fn cgs_capture_end(g: *mut std::ffi::c_void);
    fn cgs_replay(g: *mut std::ffi::c_void);
    fn cgs_empty_cache();
    fn cgs_set_allow_tf32_matmul(on: bool);
    fn cgs_set_allow_tf32_cudnn(on: bool);
}

/// Return every unused cached block in PyTorch's CUDACachingAllocator
/// to the driver. Cheap (~ms) but the next allocation pays a
/// `cudaMalloc` cost — call between major training phases (after a
/// snapshot hydrate, after an iter's self-play scope drops), not in a
/// tight inner loop.
pub fn empty_cuda_cache() {
    unsafe { cgs_empty_cache() }
}

/// Globally enable TF32 for cuBLAS matmuls + cuDNN ops. ~5 bits of
/// mantissa traded for the Ampere+ tensor-core path; expected
/// 1.3-2x on matmul throughput. Call once at process startup before
/// any tensor work — the global flag affects all subsequent ops.
pub fn enable_tf32() {
    unsafe {
        cgs_set_allow_tf32_matmul(true);
        cgs_set_allow_tf32_cudnn(true);
    }
}

/// A captured CUDA graph for one fixed batch size. Owns the static
/// input + output tensors that the graph reads/writes.
///
/// Lifetime rules (per PyTorch CUDAGraph): created, captured, and
/// replayed on the same thread, on a pooled (non-default) CUDA stream.
/// Tensors must outlive the graph.
pub struct ForwardGraph {
    handle: *mut std::ffi::c_void,
    pub static_in: Tensor,
    pub static_lm: Tensor,
    pub static_val: Tensor,
    pub max_batch: i64,
}

// We never move it between threads after creation; the service thread
// is the sole owner. Tensors are `!Sync` but read-only inside the same
// thread we created them in.
unsafe impl Send for ForwardGraph {}

impl Drop for ForwardGraph {
    fn drop(&mut self) {
        unsafe { cgs_free(self.handle) }
    }
}

impl ForwardGraph {
    /// Replay the captured forward. The current contents of `static_in`
    /// are read; results land in `static_lm` / `static_val`. Caller
    /// must `tch::Cuda::synchronize(0)` before reading the outputs.
    pub fn replay(&self) {
        unsafe { cgs_replay(self.handle) }
    }
}

impl GoMctsTransformerTch {
    /// Capture a forward pass for fixed batch `max_batch`. MUST be
    /// called on the thread that will later replay it. Smaller real
    /// batches must pad up to `max_batch` before each replay.
    pub fn capture_forward(&self, max_batch: i64) -> ForwardGraph {
        let cfg = self.cfg;
        let dev = self.device;
        let max_ctx = cfg.max_context as i64;

        // Switch the current thread onto a pooled stream — graph
        // capture cannot run on the default stream.
        unsafe { cgs_use_pooled_stream() };

        // Allocate the static input buffer the graph will read from.
        let static_in = Tensor::zeros([max_batch, max_ctx], (Kind::Int64, dev));

        // Warmup forwards so the caching allocator's free lists are
        // populated. Without this, the first capture run hits cold
        // allocations that don't replay deterministically.
        for _ in 0..3 {
            let _ = tch::no_grad(|| self.forward(&static_in));
        }
        tch::Cuda::synchronize(0);

        let handle = unsafe { cgs_new() };
        unsafe { cgs_capture_begin(handle) };
        let (lm, val) = tch::no_grad(|| self.forward(&static_in));
        unsafe { cgs_capture_end(handle) };
        tch::Cuda::synchronize(0);

        ForwardGraph { handle, static_in, static_lm: lm, static_val: val, max_batch }
    }
}

/// Tch + CUDA-graph version of `forward_histories_batch_tch`. Pads the
/// real batch up to `graph.max_batch` with the tokenizer's pad token,
/// copies into the captured input buffer, replays the graph, then
/// gathers last-position logits + value for the real rows only.
pub fn forward_histories_batch_tch_graph<G: GameState, T: Tokenizer<G>>(
    net: &GoMctsTransformerTch,
    graph: &ForwardGraph,
    tokenizer: &T,
    histories: &[IStateKey],
) -> Result<(Vec<Vec<f32>>, Vec<f32>)> {
    let cfg = net.config();
    let pad = tokenizer.pad_token();
    let max_ctx = cfg.max_context;
    let b_real = histories.len();
    if b_real as i64 > graph.max_batch {
        return Err(anyhow!(
            "batch {} exceeds captured graph max_batch {}",
            b_real,
            graph.max_batch
        ));
    }

    // Build the full max_batch × max_ctx token plane, padding unused
    // rows with the pad token. The replay always processes max_batch
    // rows; we just ignore the trailing ones.
    let n_pad = graph.max_batch as usize;
    let mut batch_tokens: Vec<i64> = vec![pad as i64; n_pad * max_ctx];
    let mut last_positions: Vec<i64> = Vec::with_capacity(b_real);
    for (i, h) in histories.iter().enumerate() {
        let mut tokens = tokenizer.encode(h);
        if tokens.is_empty() {
            tokens.push(pad);
        }
        let (padded, real_len) = pad_to(&tokens, max_ctx, pad);
        let row_offset = i * max_ctx;
        for (j, &t) in padded.iter().enumerate() {
            batch_tokens[row_offset + j] = t as i64;
        }
        last_positions.push((real_len - 1) as i64);
    }

    // Copy data into the static input buffer in-place. The replay will
    // see the new tokens. `shallow_clone` gives us a mutable handle to
    // the same underlying tensor without invalidating the original
    // (libtorch tensors are refcounted).
    let cpu_input = Tensor::from_slice(&batch_tokens)
        .reshape([graph.max_batch, max_ctx as i64]);
    graph
        .static_in
        .shallow_clone()
        .copy_(&cpu_input.to_device(net.device()));

    // Replay the captured forward pass.
    unsafe { cgs_replay(graph.handle) };

    // Read back only the real rows.
    let lm = graph.static_lm.narrow(0, 0, b_real as i64);
    let val = graph.static_val.narrow(0, 0, b_real as i64);
    let last_pos = Tensor::from_slice(&last_positions).to_device(net.device());
    let lm_idx = last_pos
        .unsqueeze(-1)
        .unsqueeze(-1)
        .expand([b_real as i64, 1, cfg.vocab_size as i64], false);
    let lm_last = lm.gather(1, &lm_idx, false).squeeze_dim(1);
    let val_idx = last_pos.unsqueeze(-1);
    let val_last = val.gather(1, &val_idx, false).squeeze_dim(1);

    let logits_vec: Vec<f32> =
        Vec::<f32>::try_from(lm_last.flatten(0, -1).to_kind(Kind::Float))?;
    let logits: Vec<Vec<f32>> = logits_vec
        .chunks_exact(cfg.vocab_size)
        .map(|c| c.to_vec())
        .collect();
    let values: Vec<f32> = Vec::<f32>::try_from(val_last.to_kind(Kind::Float))?;
    Ok((logits, values))
}

// =====================================================================
// Inference service (tch-backed mirror of candle's `serve_batched`)
// =====================================================================

/// Tch equivalent of `forward_histories_batch`. Pads, runs forward,
/// gathers last-position logits + value. Inputs/outputs are plain Vec
/// types so it's wire-compatible with the existing `RemoteModel` —
/// game threads don't know which backend they're talking to.
pub fn forward_histories_batch_tch<G: GameState, T: Tokenizer<G>>(
    net: &GoMctsTransformerTch,
    tokenizer: &T,
    histories: &[IStateKey],
) -> Result<(Vec<Vec<f32>>, Vec<f32>)> {
    let cfg = net.config();
    let pad = tokenizer.pad_token();
    let max_ctx = cfg.max_context;
    let b = histories.len();
    let mut batch_tokens: Vec<i64> = Vec::with_capacity(b * max_ctx);
    let mut last_positions: Vec<i64> = Vec::with_capacity(b);
    for h in histories {
        let mut tokens = tokenizer.encode(h);
        if tokens.is_empty() {
            tokens.push(pad);
        }
        let (padded, real_len) = pad_to(&tokens, max_ctx, pad);
        batch_tokens.extend(padded.iter().map(|&u| u as i64));
        last_positions.push((real_len - 1) as i64);
    }
    let device = net.device();
    let input = Tensor::from_slice(&batch_tokens)
        .reshape([b as i64, max_ctx as i64])
        .to_device(device);
    let (lm, val) = tch::no_grad(|| net.forward(&input));

    let last_pos = Tensor::from_slice(&last_positions).to_device(device);
    // Gather lm logits at last_pos.
    let lm_idx = last_pos
        .unsqueeze(-1)
        .unsqueeze(-1)
        .expand([b as i64, 1, cfg.vocab_size as i64], false);
    let lm_last = lm.gather(1, &lm_idx, false).squeeze_dim(1);
    let val_idx = last_pos.unsqueeze(-1);
    let val_last = val.gather(1, &val_idx, false).squeeze_dim(1);

    let logits_vec: Vec<f32> =
        Vec::<f32>::try_from(lm_last.flatten(0, -1).to_kind(Kind::Float))?;
    let logits: Vec<Vec<f32>> = logits_vec
        .chunks_exact(cfg.vocab_size)
        .map(|c| c.to_vec())
        .collect();
    let values: Vec<f32> =
        Vec::<f32>::try_from(val_last.to_kind(Kind::Float))?;
    Ok((logits, values))
}

/// Tch equivalent of `serve_batched`. Same protocol over the same
/// `ServiceRequest` enum so `RemoteModel` works without modification.
///
/// When `use_graph=true`, the service captures a CUDA graph at the
/// fixed batch size `max_batch_size` on first request and routes
/// every forward through replay. Real requests with fewer histories
/// pad up to `max_batch_size` (wasted compute on the pad rows is
/// dwarfed by the single-launch win on WSL2).
pub fn serve_batched_tch<G: GameState, T: Tokenizer<G>>(
    net: &GoMctsTransformerTch,
    tokenizer: &T,
    request_rx: std::sync::mpsc::Receiver<ServiceRequest>,
    max_batch_size: usize,
    use_graph: bool,
    graph_batch_size: i64,
) {
    let graph: Option<ForwardGraph> = if use_graph {
        Some(net.capture_forward(graph_batch_size))
    } else {
        None
    };

    loop {
        let mut requests: Vec<(Vec<IStateKey>, std::sync::mpsc::Sender<_>)> = Vec::new();
        match request_rx.recv() {
            Ok(ServiceRequest::Forward { histories, response_tx }) => {
                requests.push((histories, response_tx));
            }
            Err(_) => return,
        }
        while requests.len() < max_batch_size {
            match request_rx.try_recv() {
                Ok(ServiceRequest::Forward { histories, response_tx }) => {
                    requests.push((histories, response_tx));
                }
                Err(_) => break,
            }
        }
        let mut all_histories: Vec<IStateKey> = Vec::new();
        let mut sizes: Vec<usize> = Vec::with_capacity(requests.len());
        for (histories, _) in &requests {
            sizes.push(histories.len());
            all_histories.extend(histories.iter().cloned());
        }
        let result = match graph.as_ref() {
            Some(g) if all_histories.len() <= g.max_batch as usize => {
                forward_histories_batch_tch_graph(net, g, tokenizer, &all_histories)
            }
            _ => forward_histories_batch_tch(net, tokenizer, &all_histories),
        };
        let (logits, values) = match result {
            Ok(v) => v,
            Err(e) => {
                eprintln!("serve_batched_tch: forward failed: {}", e);
                for (_, response_tx) in requests {
                    let _ = response_tx.send((Vec::new(), Vec::new()));
                }
                continue;
            }
        };
        let mut idx = 0;
        for ((_, response_tx), size) in requests.into_iter().zip(sizes.iter()) {
            let l = logits[idx..idx + *size].to_vec();
            let v = values[idx..idx + *size].to_vec();
            let _ = response_tx.send((l, v));
            idx += *size;
        }
    }
}

/// Drop-in tch backend for `collect_self_play_games_batched_alphazero`.
/// Same scoped-thread architecture: one service thread owning `net`,
/// `n_games` workers each playing one game and talking to the service
/// via `RemoteModel`.
pub fn collect_self_play_games_batched_alphazero_tch<G, T, FNS>(
    net: &GoMctsTransformerTch,
    tokenizer: &T,
    new_state: FNS,
    n_games: usize,
    max_batch_size: usize,
    mcts_iter: usize,
    mcfs_cfg: McfsConfig,
    base_seed: u64,
    use_graph: bool,
    graph_batch_size: i64,
) -> Vec<TrainExample>
where
    G: GameState + games::resample::ResampleFromInfoState + Send,
    T: Tokenizer<G> + Send + Sync,
    FNS: Fn() -> G + Send + Sync + Copy,
{
    use std::sync::mpsc;
    let (request_tx, request_rx) = mpsc::channel::<ServiceRequest>();

    std::thread::scope(|s| {
        let service_handle = s.spawn(move || {
            serve_batched_tch(
                net,
                tokenizer,
                request_rx,
                max_batch_size,
                use_graph,
                graph_batch_size,
            );
        });

        let mut game_handles = Vec::with_capacity(n_games);
        for game_idx in 0..n_games {
            let request_tx_clone = request_tx.clone();
            let seed = base_seed.wrapping_add(100 + game_idx as u64);
            let mcfs = mcfs_cfg;
            game_handles.push(s.spawn(move || {
                let remote = RemoteModel { request_tx: request_tx_clone };
                let mut search = crate::algorithms::gomcts::GoMcts::<G, RemoteModel>::new(
                    crate::algorithms::gomcts::GoMctsConfig {
                        uct_c: 0.4,
                        n_iterations: mcts_iter,
                        mu: 0.01,
                        n_rollout_steps: mcfs.n_rollout_steps,
                        n_parallel_sims: mcfs.n_parallel_sims,
                    },
                    remote,
                    SeedableRng::seed_from_u64(seed.wrapping_add(2)),
                );
                let mut game_rng: StdRng = SeedableRng::seed_from_u64(seed);
                collect_self_play_game_alphazero(
                    new_state,
                    &mut search,
                    mcfs,
                    &mut game_rng,
                )
            }));
        }

        drop(request_tx);

        let mut out = Vec::new();
        for h in game_handles {
            out.extend(h.join().expect("game thread panicked"));
        }
        service_handle.join().expect("service thread panicked");
        out
    })
}

// =====================================================================
// Eval / head-to-head / population batched paths (tch backends)
//
// Same architecture as the candle versions in `gomcts_transformer.rs`:
// one (or two) service threads owning the tch net(s), N game worker
// threads each playing one hand and talking to the service(s) via
// `RemoteModel`. `play_one_hand_*` helpers are backend-agnostic.
// =====================================================================

/// Tch port of `eval_vs_random_batched`. Returns (mean_subject_payoff, SEM).
pub fn eval_vs_random_batched_tch<G, T, FNS>(
    net: &GoMctsTransformerTch,
    tokenizer: &T,
    new_state: FNS,
    n_games: usize,
    base_seed: u64,
    use_graph: bool,
    graph_batch_size: i64,
) -> (f64, f64)
where
    G: GameState + Send,
    T: Tokenizer<G> + Send + Sync,
    FNS: Fn() -> G + Send + Sync + Copy,
{
    use std::sync::mpsc;
    if n_games == 0 {
        return (0.0, 0.0);
    }
    let max_batch = (n_games * 16).max(32);
    let (request_tx, request_rx) = mpsc::channel::<ServiceRequest>();
    let scores: Vec<f64> = std::thread::scope(|s| {
        let svc = s.spawn(move || {
            serve_batched_tch(net, tokenizer, request_rx, max_batch, use_graph, graph_batch_size)
        });
        let mut handles = Vec::with_capacity(n_games);
        for game_idx in 0..n_games {
            let req_tx = request_tx.clone();
            let seed = base_seed.wrapping_add(game_idx as u64);
            handles.push(s.spawn(move || {
                let mut remote = RemoteModel { request_tx: req_tx };
                let mut rng: StdRng = SeedableRng::seed_from_u64(seed);
                play_one_hand_subject_vs_random(&mut remote, new_state, game_idx, &mut rng)
            }));
        }
        drop(request_tx);
        let scores: Vec<f64> = handles.into_iter().map(|h| h.join().expect("game")).collect();
        svc.join().expect("service");
        scores
    });
    finish_mean_se(&scores)
}

/// Tch port of `head_to_head_eval_batched`. Returns (mean_a_payoff, a_win_rate).
pub fn head_to_head_eval_batched_tch<G, T, FNS>(
    net_a: &GoMctsTransformerTch,
    net_b: &GoMctsTransformerTch,
    tokenizer: &T,
    new_state: FNS,
    n_games: usize,
    base_seed: u64,
    use_graph: bool,
    graph_batch_size: i64,
) -> (f64, f64)
where
    G: GameState + Send,
    T: Tokenizer<G> + Send + Sync,
    FNS: Fn() -> G + Send + Sync + Copy,
{
    use std::sync::mpsc;
    if n_games == 0 {
        return (0.0, 0.5);
    }
    let max_batch = (n_games * 16).max(32);
    let (req_a_tx, req_a_rx) = mpsc::channel::<ServiceRequest>();
    let (req_b_tx, req_b_rx) = mpsc::channel::<ServiceRequest>();
    let scores: Vec<f64> = std::thread::scope(|s| {
        let svc_a = s.spawn(move || {
            serve_batched_tch(net_a, tokenizer, req_a_rx, max_batch, use_graph, graph_batch_size)
        });
        let svc_b = s.spawn(move || {
            serve_batched_tch(net_b, tokenizer, req_b_rx, max_batch, use_graph, graph_batch_size)
        });
        let mut handles = Vec::with_capacity(n_games);
        for game_idx in 0..n_games {
            let req_a = req_a_tx.clone();
            let req_b = req_b_tx.clone();
            let seed = base_seed.wrapping_add(game_idx as u64);
            handles.push(s.spawn(move || {
                let mut remote_a = RemoteModel { request_tx: req_a };
                let mut remote_b = RemoteModel { request_tx: req_b };
                let mut rng: StdRng = SeedableRng::seed_from_u64(seed);
                play_one_hand_a_vs_b(&mut remote_a, &mut remote_b, new_state, game_idx, &mut rng)
            }));
        }
        drop(req_a_tx);
        drop(req_b_tx);
        let scores: Vec<f64> = handles.into_iter().map(|h| h.join().expect("game")).collect();
        svc_a.join().expect("service A");
        svc_b.join().expect("service B");
        scores
    });
    let (mean, _se) = finish_mean_se(&scores);
    let decided: Vec<&f64> = scores.iter().filter(|v| v.abs() > 1e-9).collect();
    let win_rate = if decided.is_empty() {
        0.5
    } else {
        decided.iter().filter(|v| ***v > 0.0).count() as f64 / decided.len() as f64
    };
    (mean, win_rate)
}

/// Tch port of `collect_population_games_batched`. Live model at one
/// rotating seat, single frozen model at the others. Returns
/// hard-target `TrainExample`s.
pub fn collect_population_games_batched_tch<G, T, FNS>(
    net_live: &GoMctsTransformerTch,
    net_frozen: &GoMctsTransformerTch,
    tokenizer: &T,
    new_state: FNS,
    n_games: usize,
    base_seed: u64,
    use_graph: bool,
    graph_batch_size: i64,
) -> Vec<TrainExample>
where
    G: GameState + Send,
    T: Tokenizer<G> + Send + Sync,
    FNS: Fn() -> G + Send + Sync + Copy,
{
    use std::sync::mpsc;
    if n_games == 0 {
        return Vec::new();
    }
    let max_batch = (n_games * 16).max(32);
    let (req_live_tx, req_live_rx) = mpsc::channel::<ServiceRequest>();
    let (req_frozen_tx, req_frozen_rx) = mpsc::channel::<ServiceRequest>();
    let all: Vec<Vec<TrainExample>> = std::thread::scope(|s| {
        let svc_live = s.spawn(move || {
            serve_batched_tch(net_live, tokenizer, req_live_rx, max_batch, use_graph, graph_batch_size)
        });
        let svc_frozen = s.spawn(move || {
            serve_batched_tch(net_frozen, tokenizer, req_frozen_rx, max_batch, use_graph, graph_batch_size)
        });
        let mut handles = Vec::with_capacity(n_games);
        for game_idx in 0..n_games {
            let req_live = req_live_tx.clone();
            let req_frozen = req_frozen_tx.clone();
            let seed = base_seed.wrapping_add(game_idx as u64);
            handles.push(s.spawn(move || {
                let mut remote_live = RemoteModel { request_tx: req_live };
                let mut remote_frozen = RemoteModel { request_tx: req_frozen };
                let mut rng: StdRng = SeedableRng::seed_from_u64(seed);
                play_one_hand_pop(
                    &mut remote_live, &mut remote_frozen, new_state, game_idx, &mut rng,
                )
            }));
        }
        drop(req_live_tx);
        drop(req_frozen_tx);
        let out: Vec<Vec<TrainExample>> =
            handles.into_iter().map(|h| h.join().expect("game")).collect();
        svc_live.join().expect("service live");
        svc_frozen.join().expect("service frozen");
        out
    });
    all.into_iter().flatten().collect()
}

// =====================================================================
// Training (tch). Same loss as the candle `train()` — soft cross-entropy
// on lm_logits at the prefix position + MSE on value at both prefix and
// action positions, weighted 0.9 / 0.1. AdamW optimizer constructed
// once and persisted across all epochs so the moment buffers
// accumulate (matches candle's `train_with_callback`).
// =====================================================================

const LM_LOSS_WEIGHT: f64 = 0.9;
const VALUE_LOSS_WEIGHT: f64 = 0.1;

pub fn train_tch<G: GameState, T: Tokenizer<G>>(
    net: &mut GoMctsTransformerTch,
    tokenizer: &T,
    examples: &[TrainExample],
    n_epochs: usize,
    batch_size: usize,
    lr: f64,
    rng: &mut StdRng,
) -> Result<f32> {
    train_tch_with_callback(net, tokenizer, examples, n_epochs, batch_size, lr, rng, |_, _| {})
}

pub fn train_tch_with_callback<G: GameState, T: Tokenizer<G>, F>(
    net: &mut GoMctsTransformerTch,
    tokenizer: &T,
    examples: &[TrainExample],
    n_epochs: usize,
    batch_size: usize,
    lr: f64,
    rng: &mut StdRng,
    mut on_epoch_end: F,
) -> Result<f32>
where
    F: FnMut(usize, f32),
{
    let cfg = *net.config();
    let device = net.device();
    let pad = tokenizer.pad_token();
    let max_context = cfg.max_context;
    let vocab = cfg.vocab_size;

    let mut opt = nn::AdamW::default().build(&net.vs, lr)
        .map_err(|e| anyhow!("build AdamW: {e}"))?;

    let mut idx: Vec<usize> = (0..examples.len()).collect();
    let mut last_loss = f32::NAN;

    for epoch in 0..n_epochs {
        // Fisher-Yates shuffle to match the candle path's RNG semantics.
        for i in (1..idx.len()).rev() {
            let j = (rng.random::<u64>() as usize) % (i + 1);
            idx.swap(i, j);
        }
        for chunk in idx.chunks(batch_size) {
            let b = chunk.len();
            let mut batch_tokens: Vec<i64> = Vec::with_capacity(b * max_context);
            let mut target_values: Vec<f32> = Vec::with_capacity(b);
            let mut prefix_positions: Vec<i64> = Vec::with_capacity(b);
            let mut action_positions: Vec<i64> = Vec::with_capacity(b);
            let mut soft_target_flat: Vec<f32> = Vec::with_capacity(b * vocab);
            for &ex_idx in chunk {
                let ex = &examples[ex_idx];
                let history_tokens = tokenizer.encode(&ex.history);
                let action_token = tokenizer.action_token(ex.action);
                assert!(
                    !history_tokens.is_empty(),
                    "TrainExample with empty history is unsupported; prepend a PAD upstream if needed"
                );
                let prefix_pos = history_tokens.len() - 1;
                let mut full = history_tokens;
                full.push(action_token);
                let action_pos = full.len() - 1;
                let (padded, _) = pad_to(&full, max_context, pad);
                batch_tokens.extend(padded.iter().map(|&u| u as i64));
                prefix_positions.push(prefix_pos as i64);
                action_positions.push(action_pos as i64);
                target_values.push(ex.value);
                let mut row = vec![0.0_f32; vocab];
                match &ex.policy_target {
                    Some(soft) => {
                        for (a, p) in soft {
                            let t = tokenizer.action_token(*a) as usize;
                            if t < vocab {
                                row[t] = *p;
                            }
                        }
                        let s: f32 = row.iter().sum();
                        if s > 0.0 {
                            for x in row.iter_mut() {
                                *x /= s;
                            }
                        } else {
                            row[action_token as usize] = 1.0;
                        }
                    }
                    None => {
                        row[action_token as usize] = 1.0;
                    }
                }
                soft_target_flat.extend_from_slice(&row);
            }

            let input = Tensor::from_slice(&batch_tokens)
                .reshape([b as i64, max_context as i64])
                .to_device(device);
            let (lm_logits, value) = net.forward(&input);

            let prefix_t = Tensor::from_slice(&prefix_positions).to_device(device);
            let action_t = Tensor::from_slice(&action_positions).to_device(device);

            // Gather LM logits at prefix positions: (B, V).
            let lm_idx = prefix_t
                .unsqueeze(-1)
                .unsqueeze(-1)
                .expand([b as i64, 1, vocab as i64], false);
            let lm_at_prefix = lm_logits.gather(1, &lm_idx, false).squeeze_dim(1);

            // Gather value at prefix + action positions: (B,) each.
            let val_at_prefix = value
                .gather(1, &prefix_t.unsqueeze(-1), false)
                .squeeze_dim(1);
            let val_at_action = value
                .gather(1, &action_t.unsqueeze(-1), false)
                .squeeze_dim(1);

            let val_targets = Tensor::from_slice(&target_values).to_device(device);
            let soft_targets = Tensor::from_slice(&soft_target_flat)
                .reshape([b as i64, vocab as i64])
                .to_device(device);

            // Soft cross-entropy: -mean(sum(target * log_softmax(logits))).
            let log_probs = lm_at_prefix.log_softmax(-1, Kind::Float);
            let lm_loss = (&soft_targets * &log_probs)
                .sum_dim_intlist([-1i64].as_ref(), false, Kind::Float)
                .neg()
                .mean(Kind::Float);

            // Value MSE at both prefix and action positions, averaged.
            let diff_pre = &val_at_prefix - &val_targets;
            let diff_post = &val_at_action - &val_targets;
            let val_loss = ((diff_pre.square().mean(Kind::Float)
                + diff_post.square().mean(Kind::Float))
                * 0.5) as Tensor;

            let total_loss = lm_loss * LM_LOSS_WEIGHT + val_loss * VALUE_LOSS_WEIGHT;
            opt.backward_step(&total_loss);
            last_loss = total_loss.double_value(&[]) as f32;
        }
        on_epoch_end(epoch + 1, last_loss);
    }
    Ok(last_loss)
}

// =====================================================================
// Population / Snapshot (tch). Same role as the candle versions —
// freeze the current live weights into a tempfile-backed snapshot,
// hydrate a fresh `GoMctsTransformerTch` on demand. Each snapshot is
// at most one `paper_default` model on disk (~25 MB); cheap enough
// that load-on-demand beats keeping every snapshot resident on GPU.
// =====================================================================

pub struct SnapshotTch {
    file: tempfile::NamedTempFile,
    cfg: TransformerConfig,
}

impl SnapshotTch {
    pub fn config(&self) -> &TransformerConfig {
        &self.cfg
    }

    pub fn from_model(net: &GoMctsTransformerTch) -> Result<Self> {
        let file = tempfile::NamedTempFile::new()
            .map_err(|e| anyhow!("tempfile: {e}"))?;
        net.save_safetensors(file.path())?;
        Ok(Self { file, cfg: *net.config() })
    }

    pub fn hydrate(&self, device: Device) -> Result<GoMctsTransformerTch> {
        let mut net = GoMctsTransformerTch::new(self.cfg, device)?;
        net.load_safetensors(self.file.path())?;
        Ok(net)
    }
}

pub struct PopulationTch {
    pub live: GoMctsTransformerTch,
    snapshots: Vec<SnapshotTch>,
    device: Device,
}

impl PopulationTch {
    pub fn new(live: GoMctsTransformerTch) -> Self {
        let device = live.device();
        Self { live, snapshots: Vec::new(), device }
    }

    pub fn snapshot(&mut self) -> Result<()> {
        self.snapshots.push(SnapshotTch::from_model(&self.live)?);
        Ok(())
    }

    pub fn num_snapshots(&self) -> usize {
        self.snapshots.len()
    }

    pub fn sample_frozen<R: rand::Rng>(
        &self,
        rng: &mut R,
    ) -> Result<Option<GoMctsTransformerTch>> {
        if self.snapshots.is_empty() {
            return Ok(None);
        }
        let idx = (rng.random::<u64>() as usize) % self.snapshots.len();
        Ok(Some(self.snapshots[idx].hydrate(self.device)?))
    }

    pub fn sample_specific_frozen(
        &self,
        idx: usize,
    ) -> Result<Option<GoMctsTransformerTch>> {
        if idx >= self.snapshots.len() {
            return Ok(None);
        }
        Ok(Some(self.snapshots[idx].hydrate(self.device)?))
    }
}

/// Tch port of `collect_pop_examples_batched` (in `euchre_gomcts_train`).
/// Routes self-play between the live tch model and a randomly-sampled
/// frozen tch snapshot. Falls back to live-vs-live (via collect_self_play
/// path) when there are no frozen snapshots yet.
pub fn collect_pop_examples_batched_tch<G, T, FNS>(
    pop: &mut PopulationTch,
    tokenizer: &T,
    new_state: FNS,
    n_games: usize,
    batch_games: usize,
    rng: &mut StdRng,
    seed: u64,
    use_graph: bool,
    graph_batch_size: i64,
) -> Vec<TrainExample>
where
    G: GameState + games::resample::ResampleFromInfoState + Send,
    T: Tokenizer<G> + Send + Sync,
    FNS: Fn() -> G + Send + Sync + Copy,
{
    if n_games == 0 || pop.num_snapshots() == 0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    let chunks = n_games.div_ceil(batch_games);
    for chunk_idx in 0..chunks {
        let games_this_chunk = batch_games.min(n_games - chunk_idx * batch_games);
        let frozen = pop
            .sample_frozen(rng)
            .expect("hydrate frozen")
            .expect("snapshots non-empty");
        let chunk_seed = seed.wrapping_add((chunk_idx as u64) * 1_000_000 + rng.random::<u64>());
        let exs = collect_population_games_batched_tch::<G, _, _>(
            &pop.live,
            &frozen,
            tokenizer,
            new_state,
            games_this_chunk,
            chunk_seed,
            use_graph,
            graph_batch_size,
        );
        out.extend(exs);
    }
    out
}

// =====================================================================
// Per-game tokenizer impls.
// =====================================================================

pub mod kuhn {
    use super::*;
    use games::gamestates::kuhn_poker::KPAction;

    /// 6-token vocab: 0=PAD, 1=Jack, 2=Queen, 3=King, 4=Bet, 5=Pass.
    #[derive(Clone, Copy)]
    pub struct KuhnTokenizer;

    impl KuhnTokenizer {
        pub const VOCAB_SIZE: usize = 6;
        pub const MAX_CONTEXT: usize = 8;
    }

    impl Tokenizer<games::gamestates::kuhn_poker::KPGameState> for KuhnTokenizer {
        fn vocab_size(&self) -> usize {
            Self::VOCAB_SIZE
        }
        fn max_context(&self) -> usize {
            Self::MAX_CONTEXT
        }
        fn encode(&self, history: &IStateKey) -> Vec<u32> {
            history.iter().map(|&a| self.action_token(a)).collect()
        }
        fn action_token(&self, a: Action) -> u32 {
            match KPAction::from(a) {
                KPAction::Jack => 1,
                KPAction::Queen => 2,
                KPAction::King => 3,
                KPAction::Bet => 4,
                KPAction::Pass => 5,
            }
        }
    }
}

pub mod euchre {
    use super::*;
    use games::gamestates::euchre::EuchreGameState;

    /// Euchre's `EAction` enum has 32 unique single-bit discriminants;
    /// shifting +1 reserves 0 for PAD → 33-token vocab.
    #[derive(Clone, Copy)]
    pub struct EuchreTokenizer;

    impl EuchreTokenizer {
        pub const VOCAB_SIZE: usize = 33;
        /// A full Euchre game is bidding (≤ 8 calls + optional discard)
        /// + 5 tricks × 4 cards = ~30 actions; 48 gives headroom.
        pub const MAX_CONTEXT: usize = 48;
    }

    impl Tokenizer<EuchreGameState> for EuchreTokenizer {
        fn vocab_size(&self) -> usize {
            Self::VOCAB_SIZE
        }
        fn max_context(&self) -> usize {
            Self::MAX_CONTEXT
        }
        fn encode(&self, history: &IStateKey) -> Vec<u32> {
            history.iter().map(|&a| self.action_token(a)).collect()
        }
        fn action_token(&self, a: Action) -> u32 {
            let v: u8 = a.into();
            debug_assert!(
                (v as u32) < (Self::VOCAB_SIZE - 1) as u32,
                "Euchre action {} outside expected 0..32 range",
                v
            );
            (v as u32) + 1
        }
    }
}
