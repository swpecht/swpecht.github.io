//! Transformer-backed `GenerativeModel` for GO-MCTS.
//!
//! Built on `candle-core` + `candle-nn` (pure-Rust ML). Small by paper
//! standards (default: 2 layers, 64-dim, 4 heads, FF=128) — chosen so
//! Kuhn Poker training converges in seconds on CPU and Euchre training
//! is at least tractable for a smoke test.
//!
//! Architecture (per block): pre-LN, multi-head self-attention (with a
//! causal mask), pre-LN, two-layer MLP (gelu). Two output heads at the
//! final position: LM head over the vocab (next-token prediction) and a
//! scalar value head. Loss weights mirror the paper's: 0.9 / 0.1.
//!
//! Inference path used by `GenerativeModel`:
//!   - `sample(history, legal)`  → mask LM logits at the last position to
//!     the tokens of `legal`, softmax, draw.
//!   - `policy(history, legal)`  → masked softmax over the same logits.
//!   - `value(history)`          → scalar value head at the last position.
//!
//! See `plans/epimc-gomcts-implementation.md` § "GO-MCTS implementation
//! plan" for the design rationale.

use candle_core::{DType, Device, IndexOp, Module, Result as CandleResult, Tensor, Var, D};
use candle_nn::{
    embedding, layer_norm, linear, linear_no_bias, loss, ops::softmax, AdamW, Embedding, LayerNorm,
    Linear, Optimizer, ParamsAdamW, VarBuilder, VarMap,
};
use games::{istate::IStateKey, Action, GameState, Player};
use rand::{rngs::StdRng, seq::IndexedRandom, RngExt, SeedableRng};

use super::gomcts::GenerativeModel;

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

/// Pick the best available device given compile-time features. Tries
/// CUDA/Metal when their feature is on, falls back to CPU on any
/// initialisation failure so a flag-built binary still runs on a host
/// without the matching hardware.
pub fn default_device() -> Device {
    #[cfg(feature = "gpu_cuda")]
    {
        if let Ok(d) = Device::new_cuda(0) {
            return d;
        }
    }
    #[cfg(feature = "gpu_metal")]
    {
        if let Ok(d) = Device::new_metal(0) {
            return d;
        }
    }
    Device::Cpu
}

/// Build `n` `Device`s on ordinal 0. For CUDA these are independent
/// contexts on the same physical GPU — each with its own stream — so
/// forwards on different replicas overlap. Memory cost is one model
/// copy per device (paper-config: ~25MB each → trivial vs 16GB VRAM).
///
/// Falls back to a single CPU device if CUDA isn't available, so the
/// caller can request `n > 1` and get the expected number of devices
/// regardless of the backend.
pub fn default_devices(n: usize) -> Vec<Device> {
    let n = n.max(1);
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        #[cfg(feature = "gpu_cuda")]
        {
            if let Ok(d) = candle_core::Device::new_cuda_with_stream(0) {
                out.push(d);
                continue;
            }
        }
        out.push(Device::Cpu);
    }
    out
}

/// Save the weights from `primary` to a tempfile and load them into
/// each of `replicas`. Use after a training step to keep the N device
/// replicas in sync for the next round of self-play. Negligible cost
/// at paper-config size (~25MB serialise/deserialise).
pub fn sync_replicas_from_primary(
    primary: &GoMctsTransformer,
    replicas: &mut [GoMctsTransformer],
) -> CandleResult<()> {
    if replicas.is_empty() {
        return Ok(());
    }
    let tmp = tempfile::NamedTempFile::new()
        .map_err(|e| candle_core::Error::Msg(format!("tempfile: {e}")))?;
    primary.save(tmp.path())?;
    for r in replicas.iter_mut() {
        r.load(tmp.path())?;
    }
    Ok(())
}

// =====================================================================
// Model
// =====================================================================

struct MultiHeadSelfAttention {
    qkv: Linear,
    out: Linear,
    n_heads: usize,
    head_dim: usize,
}

impl MultiHeadSelfAttention {
    fn new(d_model: usize, n_heads: usize, vb: VarBuilder) -> CandleResult<Self> {
        assert!(d_model % n_heads == 0, "d_model must be divisible by n_heads");
        let head_dim = d_model / n_heads;
        let qkv = linear(d_model, 3 * d_model, vb.pp("qkv"))?;
        let out = linear(d_model, d_model, vb.pp("out"))?;
        Ok(Self { qkv, out, n_heads, head_dim })
    }

    /// Input shape: (B, T, d_model). Output shape: (B, T, d_model).
    /// Applies a causal mask so position t only attends to ≤ t.
    fn forward(&self, x: &Tensor) -> CandleResult<Tensor> {
        let (b, t, d) = x.dims3()?;
        let qkv = self.qkv.forward(x)?; // (B, T, 3*d_model)
        let qkv = qkv.reshape((b, t, 3, self.n_heads, self.head_dim))?;
        // Split q, k, v along the 3-axis.
        let q = qkv.i((.., .., 0))?; // (B, T, H, hd)
        let k = qkv.i((.., .., 1))?;
        let v = qkv.i((.., .., 2))?;
        let q = q.transpose(1, 2)?.contiguous()?; // (B, H, T, hd)
        let k = k.transpose(1, 2)?.contiguous()?;
        let v = v.transpose(1, 2)?.contiguous()?;
        // attn = q @ k^T / sqrt(hd)
        let scale = 1.0 / (self.head_dim as f64).sqrt();
        let att = (q.matmul(&k.transpose(D::Minus2, D::Minus1)?)? * scale)?;
        // Causal mask: upper triangle → -inf.
        let mask = causal_mask(t, x.device())?;
        let att = att.broadcast_add(&mask)?;
        let att = softmax(&att, D::Minus1)?;
        let out = att.matmul(&v)?; // (B, H, T, hd)
        let out = out.transpose(1, 2)?.contiguous()?; // (B, T, H, hd)
        let out = out.reshape((b, t, d))?;
        self.out.forward(&out)
    }
}

/// Build a (1, 1, T, T) additive causal mask. 0 on/below the diagonal,
/// large-negative above so post-softmax future positions vanish.
fn causal_mask(t: usize, device: &Device) -> CandleResult<Tensor> {
    let mut data = vec![0.0f32; t * t];
    for i in 0..t {
        for j in (i + 1)..t {
            data[i * t + j] = f32::NEG_INFINITY;
        }
    }
    Tensor::from_vec(data, (1, 1, t, t), device)
}

struct Mlp {
    fc1: Linear,
    fc2: Linear,
}

impl Mlp {
    fn new(d_model: usize, d_ff: usize, vb: VarBuilder) -> CandleResult<Self> {
        Ok(Self {
            fc1: linear(d_model, d_ff, vb.pp("fc1"))?,
            fc2: linear(d_ff, d_model, vb.pp("fc2"))?,
        })
    }
    fn forward(&self, x: &Tensor) -> CandleResult<Tensor> {
        let x = self.fc1.forward(x)?.gelu()?;
        self.fc2.forward(&x)
    }
}

struct Block {
    ln1: LayerNorm,
    attn: MultiHeadSelfAttention,
    ln2: LayerNorm,
    mlp: Mlp,
}

impl Block {
    fn new(cfg: &TransformerConfig, vb: VarBuilder) -> CandleResult<Self> {
        Ok(Self {
            ln1: layer_norm(cfg.d_model, 1e-5, vb.pp("ln1"))?,
            attn: MultiHeadSelfAttention::new(cfg.d_model, cfg.n_heads, vb.pp("attn"))?,
            ln2: layer_norm(cfg.d_model, 1e-5, vb.pp("ln2"))?,
            mlp: Mlp::new(cfg.d_model, cfg.d_ff, vb.pp("mlp"))?,
        })
    }
    fn forward(&self, x: &Tensor) -> CandleResult<Tensor> {
        let h = self.attn.forward(&self.ln1.forward(x)?)?;
        let x = (x + h)?;
        let h = self.mlp.forward(&self.ln2.forward(&x)?)?;
        x + h
    }
}

/// The transformer itself. Owns its parameters via the `VarMap` so the
/// AdamW optimiser can step them, and so we can checkpoint them later.
pub struct GoMctsTransformer {
    cfg: TransformerConfig,
    device: Device,
    varmap: VarMap,
    token_emb: Embedding,
    pos_emb: Embedding,
    blocks: Vec<Block>,
    ln_f: LayerNorm,
    lm_head: Linear,
    value_head: Linear,
}

impl GoMctsTransformer {
    pub fn new(cfg: TransformerConfig, device: Device) -> CandleResult<Self> {
        let varmap = VarMap::new();
        let vb = VarBuilder::from_varmap(&varmap, DType::F32, &device);
        let token_emb = embedding(cfg.vocab_size, cfg.d_model, vb.pp("token_emb"))?;
        let pos_emb = embedding(cfg.max_context, cfg.d_model, vb.pp("pos_emb"))?;
        let mut blocks = Vec::with_capacity(cfg.n_layers);
        for i in 0..cfg.n_layers {
            blocks.push(Block::new(&cfg, vb.pp(format!("block_{i}")))?);
        }
        let ln_f = layer_norm(cfg.d_model, 1e-5, vb.pp("ln_f"))?;
        let lm_head = linear_no_bias(cfg.d_model, cfg.vocab_size, vb.pp("lm_head"))?;
        let value_head = linear(cfg.d_model, 1, vb.pp("value_head"))?;
        Ok(Self {
            cfg,
            device,
            varmap,
            token_emb,
            pos_emb,
            blocks,
            ln_f,
            lm_head,
            value_head,
        })
    }

    pub fn config(&self) -> &TransformerConfig {
        &self.cfg
    }
    pub fn device(&self) -> &Device {
        &self.device
    }
    pub fn varmap(&self) -> &VarMap {
        &self.varmap
    }

    /// Save all parameters to a safetensors file. The config is the
    /// caller's responsibility to track (we don't embed it — keep your
    /// `TransformerConfig` alongside the file).
    pub fn save<P: AsRef<std::path::Path>>(&self, path: P) -> CandleResult<()> {
        self.varmap.save(path)
    }

    /// Load parameters from a safetensors file. The transformer must
    /// already exist with the same shape as the saved one.
    pub fn load<P: AsRef<std::path::Path>>(&mut self, path: P) -> CandleResult<()> {
        self.varmap.load(path)
    }

    /// Forward pass.
    /// Input: `tokens` of shape (B, T).
    /// Output: `(lm_logits (B, T, V), value_per_pos (B, T))`.
    pub fn forward(&self, tokens: &Tensor) -> CandleResult<(Tensor, Tensor)> {
        let (_b, t) = tokens.dims2()?;
        assert!(
            t <= self.cfg.max_context,
            "input length {} exceeds max_context {}",
            t,
            self.cfg.max_context
        );
        let tok = self.token_emb.forward(tokens)?; // (B, T, D)
        let positions = Tensor::arange(0u32, t as u32, &self.device)?;
        let pos = self.pos_emb.forward(&positions)?; // (T, D)
        let mut x = tok.broadcast_add(&pos)?;
        for blk in &self.blocks {
            x = blk.forward(&x)?;
        }
        let x = self.ln_f.forward(&x)?;
        let lm_logits = self.lm_head.forward(&x)?; // (B, T, V)
        let value = self.value_head.forward(&x)?.squeeze(D::Minus1)?; // (B, T)
        Ok((lm_logits, value))
    }
}

/// Free-function form of `forward_history_batch`: takes a `&GoMctsTransformer`
/// + `&Tokenizer<G>` + slice of histories, returns per-history
/// (last-position logits, scalar value). Decoupled from
/// `TransformerGenerativeModel` so the cross-game batching service
/// (`serve_batched`) can call it on the service thread without owning
/// a full `TransformerGenerativeModel`.
pub fn forward_histories_batch<G: GameState, T: Tokenizer<G>>(
    net: &GoMctsTransformer,
    tokenizer: &T,
    histories: &[IStateKey],
) -> CandleResult<(Vec<Vec<f32>>, Vec<f32>)> {
    let pad = tokenizer.pad_token();
    let max_ctx = net.cfg.max_context;
    let b = histories.len();
    let mut batch_tokens: Vec<u32> = Vec::with_capacity(b * max_ctx);
    let mut last_positions: Vec<u32> = Vec::with_capacity(b);
    for h in histories {
        let mut tokens = tokenizer.encode(h);
        if tokens.is_empty() {
            tokens.push(pad);
        }
        let (padded, real_len) = pad_to(&tokens, max_ctx, pad);
        batch_tokens.extend_from_slice(&padded);
        last_positions.push((real_len - 1) as u32);
    }
    let device = net.device();
    let input = Tensor::from_vec(batch_tokens, (b, max_ctx), device)?;
    let (lm, val) = net.forward(&input)?;
    let last_pos_t = Tensor::from_vec(last_positions, b, device)?;
    let lm_idx = last_pos_t
        .unsqueeze(1)?
        .unsqueeze(2)?
        .broadcast_as((b, 1, net.cfg.vocab_size))?
        .contiguous()?;
    let lm_last = lm.gather(&lm_idx, 1)?.squeeze(1)?;
    let val_idx = last_pos_t.unsqueeze(1)?.contiguous()?;
    let val_last = val.gather(&val_idx, 1)?.squeeze(1)?;
    let logits: Vec<Vec<f32>> = lm_last.to_vec2::<f32>()?;
    let values: Vec<f32> = val_last.to_vec1::<f32>()?;
    Ok((logits, values))
}

// =====================================================================
// GenerativeModel impl
// =====================================================================

/// How the `GenerativeModel::sample` / `policy` calls produce a
/// distribution over legal actions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InferenceMode {
    /// AlphaZero-style: for each legal `a`, query `V(h⊕a)`, softmax
    /// over those scalar values. Requires the value head to have been
    /// trained against counterfactual actions (MCTS root-value targets
    /// do this naturally). **Default.**
    ArgmaxVal,
    /// LM-head-softmax: forward `h` once, read the LM logits at the
    /// last position, mask to legal-action tokens, softmax. Useful
    /// when the value head wasn't trained on counterfactual actions
    /// (e.g. a supervised cfr-bootstrap that only saw one action per
    /// position).
    LmSoftmax,
}

impl Default for InferenceMode {
    fn default() -> Self {
        InferenceMode::ArgmaxVal
    }
}

/// The piece that pairs a `GoMctsTransformer` with a `Tokenizer<G>`. This
/// is what gets handed to `GoMcts<G, M>` as the model `M`.
pub struct TransformerGenerativeModel<G: GameState, T: Tokenizer<G>> {
    pub net: GoMctsTransformer,
    pub tokenizer: T,
    pub inference_mode: InferenceMode,
    _phantom: std::marker::PhantomData<G>,
}

impl<G: GameState, T: Tokenizer<G>> TransformerGenerativeModel<G, T> {
    pub fn new(net: GoMctsTransformer, tokenizer: T) -> Self {
        Self {
            net,
            tokenizer,
            inference_mode: InferenceMode::default(),
            _phantom: std::marker::PhantomData,
        }
    }

    /// Builder-style override for the inference mode.
    pub fn with_inference_mode(mut self, mode: InferenceMode) -> Self {
        self.inference_mode = mode;
        self
    }

    /// Encode → forward → return (last-position logits over vocab, scalar
    /// value at last position). Used by all three `GenerativeModel`
    /// methods.
    fn forward_history(&self, history: &IStateKey) -> CandleResult<(Vec<f32>, f32)> {
        let mut tokens = self.tokenizer.encode(history);
        if tokens.is_empty() {
            // Empty histories happen at the very first decision before any
            // observations exist. Prepend a single PAD so the transformer
            // has a position to attend over.
            tokens.push(self.tokenizer.pad_token());
        }
        let last_idx = tokens.len() - 1;
        let input = Tensor::from_vec(tokens.clone(), (1, tokens.len()), self.net.device())?;
        let (lm, val) = self.net.forward(&input)?;
        let logits = lm.i((0, last_idx))?.to_vec1::<f32>()?;
        let value = val.i((0, last_idx))?.to_scalar::<f32>()?;
        Ok((logits, value))
    }

    /// Batched variant: forward all `histories` in a single padded batch
    /// and return per-history (last-position logits, scalar value). The
    /// "last position" is the index of the last real (non-pad) token in
    /// each row, gathered out of the (B, T, *) outputs.
    ///
    /// Used by `masked_policy` so a |legal|-way ArgmaxVal\* call becomes
    /// one forward pass instead of |legal|. On GPU this is the single
    /// biggest inference speedup since the d=128 transformer's per-call
    /// overhead dominates the actual matmul for batch=1.
    fn forward_history_batch(
        &self,
        histories: &[IStateKey],
    ) -> CandleResult<(Vec<Vec<f32>>, Vec<f32>)> {
        forward_histories_batch(&self.net, &self.tokenizer, histories)
    }

    /// Compute the action distribution over `legal` using the configured
    /// `inference_mode`. ArgmaxVal\* (default) batches `|legal|` post-
    /// action histories into one forward; LmSoftmax does one forward at
    /// the prefix and masks the LM logits.
    fn masked_policy(&self, history: &IStateKey, legal: &[Action]) -> Vec<f64> {
        match self.inference_mode {
            InferenceMode::ArgmaxVal => self.masked_policy_argmaxval(history, legal),
            InferenceMode::LmSoftmax => self.masked_policy_lm_softmax(history, legal),
        }
    }

    /// ArgmaxVal\* (AlphaZero-style): for each legal action `a`, query
    /// V(h ⊕ a) and softmax over those values. Requires a value head
    /// trained against counterfactual actions (e.g. MCTS root-value
    /// targets). Batched: one forward over `|legal|` post-action
    /// histories.
    fn masked_policy_argmaxval(&self, history: &IStateKey, legal: &[Action]) -> Vec<f64> {
        let histories: Vec<IStateKey> = legal
            .iter()
            .map(|&a| {
                let mut h = *history;
                h.push(a);
                h
            })
            .collect();
        let (_, values) = match self.forward_history_batch(&histories) {
            Ok(x) => x,
            Err(_) => return vec![1.0 / legal.len() as f64; legal.len()],
        };
        let values: Vec<f64> = values.into_iter().map(|v| v as f64).collect();
        softmax_with_temp(&values, POLICY_SOFTMAX_TEMP)
            .unwrap_or_else(|| vec![1.0 / legal.len() as f64; legal.len()])
    }

    /// LM-head softmax: one forward at `history`, take logits at the
    /// last position, restrict to the tokens of `legal`, softmax. Use
    /// when the LM head was trained with cross-entropy on the action
    /// distribution (e.g. CFR-supervised bootstrap) and the value head
    /// has not seen counterfactual actions.
    fn masked_policy_lm_softmax(&self, history: &IStateKey, legal: &[Action]) -> Vec<f64> {
        let logits = match self.forward_history(history) {
            Ok((l, _)) => l,
            Err(_) => return vec![1.0 / legal.len() as f64; legal.len()],
        };
        let scores: Vec<f64> = legal
            .iter()
            .map(|a| {
                let t = self.tokenizer.action_token(*a) as usize;
                logits.get(t).copied().unwrap_or(f32::NEG_INFINITY) as f64
            })
            .collect();
        softmax_with_temp(&scores, POLICY_SOFTMAX_TEMP)
            .unwrap_or_else(|| vec![1.0 / legal.len() as f64; legal.len()])
    }
}

/// Numerically-stable softmax of `scores` with temperature, returning
/// `None` when the inputs are degenerate (all NEG_INFINITY / NaN / zero
/// total). Callers fall back to uniform in that case.
fn softmax_with_temp(scores: &[f64], temperature: f64) -> Option<Vec<f64>> {
    if scores.is_empty() {
        return None;
    }
    let max = scores.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    if !max.is_finite() {
        return None;
    }
    let exps: Vec<f64> = scores.iter().map(|s| ((s - max) / temperature).exp()).collect();
    let total: f64 = exps.iter().sum();
    if total == 0.0 || !total.is_finite() {
        return None;
    }
    Some(exps.into_iter().map(|e| e / total).collect())
}

impl<G: GameState, T: Tokenizer<G>> GenerativeModel<G> for TransformerGenerativeModel<G, T> {
    fn sample(&mut self, history: &IStateKey, legal: &[Action], rng: &mut StdRng) -> Action {
        let probs = self.masked_policy(history, legal);
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
        match self.forward_history(history) {
            Ok((_, v)) => v as f64,
            Err(_) => 0.0,
        }
    }

    fn policy(&mut self, history: &IStateKey, legal: &[Action]) -> Vec<f64> {
        self.masked_policy(history, legal)
    }

    /// Override: single batched forward over all `histories`. Used by
    /// `GoMcts::run_search_parallel` to merge K concurrent sims' leaf
    /// evaluations into one GPU call.
    fn batch_value(&mut self, histories: &[IStateKey]) -> Vec<f64> {
        if histories.is_empty() {
            return Vec::new();
        }
        match self.forward_history_batch(histories) {
            Ok((_, values)) => values.into_iter().map(|v| v as f64).collect(),
            Err(_) => vec![0.0; histories.len()],
        }
    }
}

// =====================================================================
// Training data + loop
// =====================================================================

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

/// Generate a single self-play game using the given action sampler. For
/// each player, every (history, action) pair gets the player's terminal
/// value attached. Returns a flat vec of `TrainExample`s.
///
/// The action sampler is generic so callers can pick between
/// "transformer-only" (sample directly from the transformer) and
/// "GO-MCTS + transformer" (sample from the search's visit distribution).
/// Kuhn validation uses the former for the first pass and the latter
/// for the second pass.
pub fn collect_self_play_game<G, F>(
    new_state: impl Fn() -> G,
    mut sample_action: F,
    rng: &mut StdRng,
) -> Vec<TrainExample>
where
    G: GameState,
    F: FnMut(&G, &mut StdRng) -> Action,
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
    let mut per_player: Vec<Vec<(IStateKey, Action)>> = vec![Vec::new(); n_players];
    while !gs.is_terminal() {
        let p = gs.cur_player();
        let history = gs.istate_key(p);
        let a = sample_action(&gs, rng);
        per_player[p].push((history, a));
        gs.apply_action(a);
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

/// Configuration knobs for MCTS-driven self-play.
#[derive(Clone, Copy, Debug)]
pub struct McfsConfig {
    /// Dirichlet noise concentration α applied to the root visit
    /// distribution before sampling the played action. Lower → more
    /// concentrated noise → more exploration on a single action.
    /// AlphaZero uses 0.3 for chess (avg 35 legal moves), so we scale:
    /// `10.0 / sqrt(legal)` is a reasonable Euchre default. Set to
    /// `f64::INFINITY` (the trait default for `Default`) for *no*
    /// noise, which gives strict on-distribution sampling.
    pub root_dirichlet_alpha: f64,
    /// Mixing weight: played-action prob = (1-ε)·visit_prob + ε·dirichlet.
    /// Set to 0.0 to disable noise entirely. AlphaZero uses 0.25.
    pub root_dirichlet_eps: f64,
    /// MCTS rollout phase length per leaf expansion (paper Algorithm 1
    /// uses ~4-10). 0 = AlphaZero-style (no rollout). Propagated into
    /// `GoMctsConfig` when the self-play helper builds its search.
    pub n_rollout_steps: usize,
    /// Parallel-sim width inside one game's MCTS (virtual loss).
    /// Propagated into `GoMctsConfig.n_parallel_sims`. 1 = sequential
    /// (default).
    pub n_parallel_sims: usize,
}

impl Default for McfsConfig {
    fn default() -> Self {
        // Default: NO noise — preserves prior behaviour. Callers opting
        // into E3 set `root_dirichlet_eps > 0`.
        Self {
            root_dirichlet_alpha: f64::INFINITY,
            root_dirichlet_eps: 0.0,
            n_rollout_steps: 0,
            n_parallel_sims: 1,
        }
    }
}

/// Draw a Dirichlet(α, K) sample using K Gamma(α, 1) draws normalised
/// to sum to 1. K = `n`. Cheap when α≥1; for α<1 the Gamma sampler may
/// reject many candidates — for our K ≤ 33 sample size that's fine.
fn dirichlet_sample(n: usize, alpha: f64, rng: &mut StdRng) -> Vec<f64> {
    // Marsaglia & Tsang for Gamma(α, 1).
    let mut samples = vec![0.0_f64; n];
    let (d, c) = if alpha >= 1.0 {
        let d = alpha - 1.0 / 3.0;
        (d, 1.0 / (9.0 * d).sqrt())
    } else {
        // Use Gamma(α+1) and downscale via u^(1/α).
        let d = alpha + 1.0 - 1.0 / 3.0;
        (d, 1.0 / (9.0 * d).sqrt())
    };
    let mut cache: Option<f64> = None;
    for s in samples.iter_mut() {
        loop {
            // Draw one standard-normal sample. Uses Box-Muller; the
            // companion value is cached so we don't waste it.
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

/// MCTS-driven self-play: run GO-MCTS at every decision and use the
/// root's visit distribution as the policy target. This is the
/// AlphaZero-style training signal — soft targets that bake the search
/// result back into the model.
///
/// Takes `&mut search` so the caller retains ownership of the underlying
/// `GenerativeModel` (and can train it after self-play via
/// `search.model_mut()`). `GoMcts::action_probabilities` clears the tree
/// at the start of each decision, so reusing one search across many
/// games is fine.
///
/// Backwards-compatible wrapper around `collect_self_play_game_mcts_cfg`
/// with default (no-noise) config.
pub fn collect_self_play_game_mcts<G, M>(
    new_state: impl Fn() -> G,
    search: &mut super::gomcts::GoMcts<G, M>,
    rng: &mut StdRng,
) -> Vec<TrainExample>
where
    G: GameState + games::resample::ResampleFromInfoState,
    M: GenerativeModel<G>,
{
    collect_self_play_game_mcts_cfg(new_state, search, McfsConfig::default(), rng)
}

/// AlphaZero-style self-play: identical to
/// `collect_self_play_game_mcts_cfg` except the value-head target is
/// the **MCTS root value at each decision**, not the eventual terminal
/// payoff. Pure self-bootstrap — no perfect-info oracle, no terminal-
/// outcome noise. The value head learns to predict the search's own
/// output; the search then uses the improved value head; loop.
///
/// This sidesteps both PIMCTS strategy fusion (no perfect-info leaf)
/// and the noise of terminal payoffs (random opponent moves dominate
/// the variance of `gs.evaluate(p)`). Concretely: at every decision
/// `t`, after `search.action_probabilities(&gs)` has populated the
/// tree, we extract `search.root_value(&history)` and use it as the
/// value-head training target for that example. The MCTS visit
/// distribution remains the policy target, as in the other variants.
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
    // Per (player, decision) records: (history, action, soft target, root_value).
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
        // AlphaZero value target = the search's own root value at this
        // history. Falls back to 0 if for some reason no sims populated
        // the root (should be unreachable).
        let root_v = search.root_value(&history).unwrap_or(0.0) as f32;
        // Played-action sampling (with optional Dirichlet noise).
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

/// Variant of `collect_self_play_game_mcts_cfg` that replaces the
/// noisy terminal-game payoff with a **perfect-information value
/// oracle** for the value-head training target. This is E2:
/// PIMCTS-bootstrap. The caller provides `value_oracle(gs, player)`
/// that returns a perfect-info value (e.g. `OpenHandSolver` for
/// Euchre). The oracle is called per recorded decision on a *clone* of
/// the live GameState at that position — the same level of signal
/// PIMCTS uses for its leaf evaluation.
///
/// Policy targets are still the MCTS visit distributions.
///
/// Cost: roughly one perfect-info solve per recorded position. For
/// Euchre with `OpenHandSolver` (TT-cached) this is ~10-50ms per
/// position. Expect data collection to take 2-3× longer than the
/// terminal-payoff variant, in exchange for a much lower-variance
/// value-head training signal.
pub fn collect_self_play_game_mcts_with_value_oracle<G, M, F>(
    new_state: impl Fn() -> G,
    search: &mut super::gomcts::GoMcts<G, M>,
    cfg: McfsConfig,
    mut value_oracle: F,
    rng: &mut StdRng,
) -> Vec<TrainExample>
where
    G: GameState + games::resample::ResampleFromInfoState + Clone,
    M: GenerativeModel<G>,
    F: FnMut(&G, Player) -> f64,
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
    // Per-player records hold: (history, chosen_action, soft_target,
    // perfect_info_value). The oracle is queried on a clone of the live
    // GameState before the action is applied — this gives V(s) where s
    // is the state from which the player decided. Game-theoretically
    // this is exactly the leaf value PIMCTS would estimate for the
    // current node, just without the determinisation step (the
    // self-play world is already a single determinisation).
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
        // Played-action distribution (possibly noised).
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
        // Query the oracle on the pre-action state. Cloned so we don't
        // disturb the live trajectory.
        let gs_clone = gs.clone();
        let v = value_oracle(&gs_clone, p) as f32;
        per_player[p].push((history, chosen, soft, v));
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

/// Configurable variant. When `cfg.root_dirichlet_eps > 0`, applies
/// Dirichlet noise to the visit distribution before sampling the played
/// action. The recorded *soft policy target* is the un-noised visit
/// distribution (so the value head learns from clean MCTS output);
/// only the action chosen for game continuation is noise-perturbed.
pub fn collect_self_play_game_mcts_cfg<G, M>(
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
    let mut per_player: Vec<Vec<(IStateKey, Action, Vec<(Action, f32)>)>> =
        vec![Vec::new(); n_players];
    while !gs.is_terminal() {
        let p = gs.cur_player();
        let history = gs.istate_key(p);
        // Get the root visit distribution via the existing
        // `action_probabilities` impl (it clears the tree, runs the
        // configured iterations, and returns the normalised visit
        // probabilities).
        let probs = search.action_probabilities(&gs);
        let actions_legal = actions!(gs);
        let mut soft: Vec<(Action, f32)> = actions_legal
            .iter()
            .map(|a| (*a, probs[*a] as f32))
            .collect();
        // Numerical safety: if every prob came out zero (no simulation
        // produced data), fall back to uniform so we don't store a
        // degenerate target.
        let sum: f32 = soft.iter().map(|(_, p)| *p).sum();
        if sum <= 1e-8 {
            let u = 1.0 / soft.len() as f32;
            for (_, p) in soft.iter_mut() {
                *p = u;
            }
        }
        // Sample the actual move. If Dirichlet noise is configured,
        // mix it into the *played-action* distribution while keeping
        // the recorded soft target clean.
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
        per_player[p].push((history, chosen, soft));
        gs.apply_action(chosen);
    }
    let mut out = Vec::new();
    for p in 0..n_players {
        let v = gs.evaluate(p) as f32;
        for (h, a, soft) in per_player[p].drain(..) {
            out.push(TrainExample::soft(h, a, v, soft));
        }
    }
    out
}

/// Loss weights from the paper (Table 4).
const LM_LOSS_WEIGHT: f64 = 0.9;
const VALUE_LOSS_WEIGHT: f64 = 0.1;

/// Softmax temperature for the ArgmaxVal\*-style policy. Smaller → more
/// greedy; larger → more uniform. 0.5 worked well on Kuhn in practice.
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

/// Train the transformer on `examples` for `n_epochs` epochs with batch
/// size `batch_size`.
///
/// Loss = 0.9 · cross_entropy(lm_logits @ action_pos, action_token)
///      + 0.1 · MSE(value_head @ action_pos, value)
///
/// `action_pos` is the position of the *previous* token — i.e. we predict
/// "next token given prefix", with the previous-action's token at the
/// last input position. For an empty prefix the position is 0 (the
/// prepended PAD).
///
/// Backwards-compatible wrapper around `train_with_callback` that
/// passes a no-op callback. Use `train_with_callback` directly when you
/// want a per-epoch hook (logging, checkpointing, …) WITHOUT discarding
/// the AdamW moment buffers between epochs.
pub fn train<G: GameState, T: Tokenizer<G>>(
    model: &mut TransformerGenerativeModel<G, T>,
    examples: &[TrainExample],
    n_epochs: usize,
    batch_size: usize,
    lr: f64,
    rng: &mut StdRng,
) -> CandleResult<f32> {
    train_with_callback(model, examples, n_epochs, batch_size, lr, rng, |_, _| {})
}

/// As `train`, but invokes `on_epoch_end(epoch_index_1_based, last_step_loss)`
/// after each epoch. The optimizer is constructed once and persists
/// across all epochs, so AdamW's `m_t` / `v_t` moments accumulate
/// normally — strictly equivalent to a single `train(n_epochs=N)` call
/// in terms of optimization trajectory.
pub fn train_with_callback<G: GameState, T: Tokenizer<G>, F>(
    model: &mut TransformerGenerativeModel<G, T>,
    examples: &[TrainExample],
    n_epochs: usize,
    batch_size: usize,
    lr: f64,
    rng: &mut StdRng,
    mut on_epoch_end: F,
) -> CandleResult<f32>
where
    F: FnMut(usize, f32),
{
    let params = ParamsAdamW { lr, ..Default::default() };
    let mut opt = AdamW::new(model.net.varmap.all_vars(), params)?;
    let device = model.net.device().clone();
    let max_context = model.net.cfg.max_context;
    let pad = model.tokenizer.pad_token();

    let mut idx: Vec<usize> = (0..examples.len()).collect();
    let mut last_loss = f32::NAN;
    let vocab = model.net.cfg.vocab_size;
    for epoch in 0..n_epochs {
        for i in (1..idx.len()).rev() {
            let j = (rng.random::<u64>() as usize) % (i + 1);
            idx.swap(i, j);
        }
        for chunk in idx.chunks(batch_size) {
            let b = chunk.len();
            let mut batch_tokens: Vec<u32> = Vec::with_capacity(b * max_context);
            let mut target_tokens: Vec<u32> = Vec::with_capacity(b);
            let mut target_values: Vec<f32> = Vec::with_capacity(b);
            let mut prefix_positions: Vec<usize> = Vec::with_capacity(b);
            let mut action_positions: Vec<usize> = Vec::with_capacity(b);
            // For MCTS-driven examples we also assemble a soft target
            // tensor: shape (B, V), zeros except for the legal actions
            // which carry their visit probabilities. Examples without a
            // soft target fall back to a one-hot at `action_token`,
            // which makes the soft-CE numerically equivalent to the
            // hard-CE we were doing before.
            let mut soft_target_flat: Vec<f32> = Vec::with_capacity(b * vocab);
            for &ex_idx in chunk {
                let ex = &examples[ex_idx];
                let history_tokens = model.tokenizer.encode(&ex.history);
                let action_token = model.tokenizer.action_token(ex.action);
                assert!(
                    !history_tokens.is_empty(),
                    "TrainExample with empty history is unsupported; prepend a PAD upstream if needed"
                );
                let prefix_pos = history_tokens.len() - 1;
                let mut full = history_tokens;
                full.push(action_token);
                let action_pos = full.len() - 1;
                let (padded, _) = pad_to(&full, max_context, pad);
                batch_tokens.extend_from_slice(&padded);
                prefix_positions.push(prefix_pos);
                action_positions.push(action_pos);
                target_tokens.push(action_token);
                target_values.push(ex.value);
                // Build per-example soft target row.
                let mut row = vec![0.0_f32; vocab];
                match &ex.policy_target {
                    Some(soft) => {
                        for (a, p) in soft {
                            let t = model.tokenizer.action_token(*a) as usize;
                            if t < vocab {
                                row[t] = *p;
                            }
                        }
                        // Normalise defensively.
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
            let input = Tensor::from_vec(batch_tokens, (b, max_context), &device)?;
            let (lm_logits, value) = model.net.forward(&input)?;
            let prefix_t = Tensor::from_vec(
                prefix_positions.iter().map(|&p| p as u32).collect::<Vec<_>>(),
                b,
                &device,
            )?;
            let action_t = Tensor::from_vec(
                action_positions.iter().map(|&p| p as u32).collect::<Vec<_>>(),
                b,
                &device,
            )?;
            let prefix_for_lm = prefix_t
                .unsqueeze(1)?
                .unsqueeze(2)?
                .broadcast_as((b, 1, vocab))?
                .contiguous()?;
            let lm_at_prefix = lm_logits.gather(&prefix_for_lm, 1)?.squeeze(1)?; // (B, V)
            let prefix_for_val = prefix_t.unsqueeze(1)?.contiguous()?;
            let val_at_prefix = value.gather(&prefix_for_val, 1)?.squeeze(1)?; // (B,)
            let action_for_val = action_t.unsqueeze(1)?.contiguous()?;
            let val_at_action = value.gather(&action_for_val, 1)?.squeeze(1)?; // (B,)
            let val_targets = Tensor::from_vec(target_values, b, &device)?;
            // Soft cross-entropy: -mean(sum(target * log_softmax(logits)))
            let soft_targets = Tensor::from_vec(soft_target_flat, (b, vocab), &device)?;
            let log_probs = candle_nn::ops::log_softmax(&lm_at_prefix, D::Minus1)?;
            let lm_loss = (soft_targets * log_probs)?
                .sum_keepdim(D::Minus1)?
                .neg()?
                .mean_all()?;
            // Keep target_tokens around as an assertion / future use.
            let _ = target_tokens;
            let diff_pre = val_at_prefix.sub(&val_targets)?;
            let diff_post = val_at_action.sub(&val_targets)?;
            let val_loss =
                ((diff_pre.sqr()?.mean_all()? + diff_post.sqr()?.mean_all()?)? * 0.5)?;
            let total_loss = ((lm_loss * LM_LOSS_WEIGHT)? + (val_loss * VALUE_LOSS_WEIGHT)?)?;
            opt.backward_step(&total_loss)?;
            last_loss = total_loss.to_scalar::<f32>()?;
        }
        on_epoch_end(epoch + 1, last_loss);
    }
    Ok(last_loss)
}

// =====================================================================
// E5: Cross-game batched self-play
// =====================================================================
//
// Architecture: one service thread owns the transformer; N game threads
// each run their own MCTS using a thin `RemoteModel` that sends forward
// requests over an mpsc channel and blocks for the response. The service
// thread drains all currently-pending requests, builds one padded batch,
// runs one `forward_histories_batch` call, and distributes per-request
// results back. This collapses N · (≥1) per-decision forwards into a
// single GPU launch, fixing the WSL2 D3DKMTSubmitCommandToHwQueue
// bottleneck.

/// Channel message for the batching service. `pub(crate)` so the
/// tch-backed sibling module can share the same wire format.
pub(crate) enum ServiceRequest {
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
    pub(crate) request_tx: std::sync::mpsc::Sender<ServiceRequest>,
}

impl RemoteModel {
    /// Block until the service responds. The (logits, value) vectors are
    /// aligned with the input `histories` order.
    fn forward(&self, histories: Vec<IStateKey>) -> (Vec<Vec<f32>>, Vec<f32>) {
        let (response_tx, response_rx) = std::sync::mpsc::channel();
        // The service is alive as long as some Sender is alive (which is
        // us). If it's not, the unwrap fires with a clear error.
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
    /// batched forward at the service. This is the key win for the
    /// parallel-sim path: K sims' leaf values become one GPU call.
    fn batch_value(&mut self, histories: &[IStateKey]) -> Vec<f64> {
        if histories.is_empty() {
            return Vec::new();
        }
        let (_, values) = self.forward(histories.to_vec());
        values.into_iter().map(|v| v as f64).collect()
    }
}

/// Service loop: own the transformer, batch incoming requests, forward,
/// dispatch responses. Exits when all request senders are dropped (the
/// usual signal from the orchestrator after all game threads have
/// joined). `max_batch_size` is a soft cap on requests merged into one
/// forward — we drain all immediately-pending requests up to the cap.
fn serve_batched<G: GameState, T: Tokenizer<G>>(
    net: &GoMctsTransformer,
    tokenizer: &T,
    request_rx: std::sync::mpsc::Receiver<ServiceRequest>,
    max_batch_size: usize,
) {
    loop {
        let mut requests: Vec<(Vec<IStateKey>, std::sync::mpsc::Sender<_>)> = Vec::new();

        // Block on the first request.
        match request_rx.recv() {
            Ok(ServiceRequest::Forward { histories, response_tx }) => {
                requests.push((histories, response_tx));
            }
            Err(_) => return,
        }

        // Drain everything else that's immediately available.
        while requests.len() < max_batch_size {
            match request_rx.try_recv() {
                Ok(ServiceRequest::Forward { histories, response_tx }) => {
                    requests.push((histories, response_tx));
                }
                Err(_) => break,
            }
        }

        // Flatten into one big history list. `sizes[i]` says how many
        // histories belong to request i.
        let mut all_histories: Vec<IStateKey> = Vec::new();
        let mut sizes: Vec<usize> = Vec::with_capacity(requests.len());
        for (histories, _) in &requests {
            sizes.push(histories.len());
            all_histories.extend(histories.iter().cloned());
        }

        // Single batched forward.
        let result = forward_histories_batch(net, tokenizer, &all_histories);
        let (logits, values) = match result {
            Ok(v) => v,
            Err(e) => {
                // On failure, send a uniform-fallback response to all
                // waiters so games don't deadlock. They'll see empty
                // logits/values and degrade to uniform.
                eprintln!("serve_batched: forward failed: {}", e);
                for (_, response_tx) in requests {
                    let _ = response_tx.send((Vec::new(), Vec::new()));
                }
                continue;
            }
        };

        // Distribute back to each waiter.
        let mut idx = 0;
        for ((_, response_tx), size) in requests.into_iter().zip(sizes.iter()) {
            let l = logits[idx..idx + *size].to_vec();
            let v = values[idx..idx + *size].to_vec();
            let _ = response_tx.send((l, v));
            idx += *size;
        }
    }
}

/// Batched eval: play `n_games` hands where the trained `net` sits at
/// one rotating seat and uniform-random fills the rest. Same batching
/// architecture as `collect_self_play_games_batched_alphazero` —
/// service thread owns `net`, `n_games` worker threads each play one
/// hand and query the service via a `RemoteModel`. Random opponents
/// are CPU-only, no service needed for them. Returns (mean_subject_payoff,
/// standard_error_of_mean).
pub fn eval_vs_random_batched<G, T, FNS>(
    net: &GoMctsTransformer,
    tokenizer: &T,
    new_state: FNS,
    n_games: usize,
    base_seed: u64,
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
        let svc = s.spawn(move || serve_batched(net, tokenizer, request_rx, max_batch));
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

/// Batched head-to-head: model A at one team (rotating), model B at the
/// other. Two services (one per model) so per-step requests still batch
/// across games per-model. Returns (mean_a_payoff, a_win_rate).
pub fn head_to_head_eval_batched<G, T, FNS>(
    net_a: &GoMctsTransformer,
    net_b: &GoMctsTransformer,
    tokenizer: &T,
    new_state: FNS,
    n_games: usize,
    base_seed: u64,
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
        let svc_a = s.spawn(move || serve_batched(net_a, tokenizer, req_a_rx, max_batch));
        let svc_b = s.spawn(move || serve_batched(net_b, tokenizer, req_b_rx, max_batch));
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

/// Batched population self-play: live model at one seat (rotating), a
/// single frozen `net_frozen` at the others. Returns hard-target
/// `TrainExample`s. Same architecture as the eval functions but
/// produces training data instead of mean payoffs.
pub fn collect_population_games_batched<G, T, FNS>(
    net_live: &GoMctsTransformer,
    net_frozen: &GoMctsTransformer,
    tokenizer: &T,
    new_state: FNS,
    n_games: usize,
    base_seed: u64,
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
    std::thread::scope(|s| {
        let svc_l = s.spawn(move || serve_batched(net_live, tokenizer, req_live_rx, max_batch));
        let svc_f = s.spawn(move || serve_batched(net_frozen, tokenizer, req_frozen_rx, max_batch));
        let mut handles = Vec::with_capacity(n_games);
        for game_idx in 0..n_games {
            let req_l = req_live_tx.clone();
            let req_f = req_frozen_tx.clone();
            let seed = base_seed.wrapping_add(game_idx as u64);
            handles.push(s.spawn(move || {
                let mut remote_live = RemoteModel { request_tx: req_l };
                let mut remote_frozen = RemoteModel { request_tx: req_f };
                let mut rng: StdRng = SeedableRng::seed_from_u64(seed);
                play_one_hand_pop(
                    &mut remote_live,
                    &mut remote_frozen,
                    new_state,
                    game_idx,
                    &mut rng,
                )
            }));
        }
        drop(req_live_tx);
        drop(req_frozen_tx);
        let mut out = Vec::new();
        for h in handles {
            out.extend(h.join().expect("game"));
        }
        svc_l.join().expect("service live");
        svc_f.join().expect("service frozen");
        out
    })
}

// --- shared free helpers used by the batched eval/h2h/pop paths -------

fn play_one_hand_subject_vs_random<G>(
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

fn play_one_hand_a_vs_b<G>(
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

fn play_one_hand_pop<G>(
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

fn finish_mean_se(scores: &[f64]) -> (f64, f64) {
    let n = scores.len() as f64;
    if n == 0.0 {
        return (0.0, 0.0);
    }
    let mean: f64 = scores.iter().sum::<f64>() / n;
    let var: f64 = scores.iter().map(|x| (x - mean) * (x - mean)).sum::<f64>() / n;
    let se = (var / n).max(0.0).sqrt();
    (mean, se)
}

/// Cross-game batched self-play (AlphaZero target). Runs `n_games`
/// games in parallel on as many threads, each using a `RemoteModel`
/// that talks to a single batching service. Returns flat training
/// examples from all games.
///
/// `max_batch_size` is the cap on histories per forward call —
/// typically `n_games * average_|legal|` (≈ n_games × 4 for Euchre).
/// Set it generously; the service drains whatever's pending and runs.
///
/// Use `EU_BATCH_GAMES` env var in the trainer to invoke this code
/// path; setting it to 1 falls back to the original sequential
/// `collect_self_play_game_alphazero`.
/// N-device version of `collect_self_play_games_batched_alphazero`.
/// Each device runs the full single-device pipeline (service thread +
/// game threads) in parallel; games are split evenly across them.
/// Because each CUDA device has its own context + stream, the GPU
/// kernels and host-side prep on different devices overlap — that's
/// the speedup target (~25% headroom we measured in the single-device
/// profile).
///
/// `nets[i]` must hold the same weights as every other replica (use
/// `sync_replicas_from_primary` before each iter's self-play).
pub fn collect_self_play_games_batched_alphazero_multi_device<G, T, FNS>(
    nets: &[&GoMctsTransformer],
    tokenizer: &T,
    new_state: FNS,
    n_games: usize,
    max_batch_size: usize,
    mcts_iter: usize,
    mcfs_cfg: McfsConfig,
    base_seed: u64,
) -> Vec<TrainExample>
where
    G: GameState + games::resample::ResampleFromInfoState + Send,
    T: Tokenizer<G> + Send + Sync,
    FNS: Fn() -> G + Send + Sync + Copy,
{
    let n_devices = nets.len().max(1);
    if n_devices == 1 {
        return collect_self_play_games_batched_alphazero(
            nets[0], tokenizer, new_state, n_games, max_batch_size, mcts_iter, mcfs_cfg, base_seed,
        );
    }
    let per_device = n_games.div_ceil(n_devices);
    // Per-device chunks (sum == n_games).
    let chunks: Vec<usize> = (0..n_devices)
        .map(|i| {
            let start = i * per_device;
            let end = ((i + 1) * per_device).min(n_games);
            end.saturating_sub(start)
        })
        .collect();
    std::thread::scope(|s| {
        let mut handles = Vec::with_capacity(n_devices);
        for (i, &chunk) in chunks.iter().enumerate() {
            if chunk == 0 {
                continue;
            }
            let net = nets[i];
            // Stagger seeds so devices don't replay identical games.
            let seed = base_seed.wrapping_add((i as u64) * 7_000_003);
            handles.push(s.spawn(move || {
                collect_self_play_games_batched_alphazero::<G, T, FNS>(
                    net,
                    tokenizer,
                    new_state,
                    chunk,
                    max_batch_size,
                    mcts_iter,
                    mcfs_cfg,
                    seed,
                )
            }));
        }
        let mut all = Vec::with_capacity(n_games);
        for h in handles {
            all.extend(h.join().expect("device worker panicked"));
        }
        all
    })
}

pub fn collect_self_play_games_batched_alphazero<G, T, FNS>(
    net: &GoMctsTransformer,
    tokenizer: &T,
    new_state: FNS,
    n_games: usize,
    max_batch_size: usize,
    mcts_iter: usize,
    mcfs_cfg: McfsConfig,
    base_seed: u64,
) -> Vec<TrainExample>
where
    G: GameState + games::resample::ResampleFromInfoState + Send,
    T: Tokenizer<G> + Send + Sync,
    FNS: Fn() -> G + Send + Sync + Copy,
{
    use std::sync::mpsc;
    let (request_tx, request_rx) = mpsc::channel::<ServiceRequest>();

    std::thread::scope(|s| {
        // Spawn the service thread. Borrows `net` + `tokenizer` for the
        // scope's lifetime.
        let service_handle = s.spawn(move || {
            serve_batched(net, tokenizer, request_rx, max_batch_size);
        });

        // Spawn N game threads. Each clones the request sender.
        let mut game_handles = Vec::with_capacity(n_games);
        for game_idx in 0..n_games {
            let request_tx_clone = request_tx.clone();
            let seed = base_seed.wrapping_add(100 + game_idx as u64);
            let mcfs = mcfs_cfg;
            game_handles.push(s.spawn(move || {
                let remote = RemoteModel { request_tx: request_tx_clone };
                let mut search = super::gomcts::GoMcts::<G, RemoteModel>::new(
                    super::gomcts::GoMctsConfig {
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

        // Drop the orchestrator's sender so the service exits once all
        // game threads finish (and drop their own senders).
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
// Population-based self-play
// =====================================================================

/// A frozen snapshot of a transformer's parameters, loadable into a
/// fresh `GoMctsTransformer` with the same config. Implemented by
/// round-tripping safetensors through a tempfile — cheap (~ms) for the
/// model sizes we're using and avoids inventing our own param-copy
/// machinery on top of candle.
pub struct Snapshot {
    file: tempfile::NamedTempFile,
    cfg: TransformerConfig,
}

impl Snapshot {
    pub fn config(&self) -> &TransformerConfig {
        &self.cfg
    }

    pub fn from_model(net: &GoMctsTransformer) -> CandleResult<Self> {
        let file = tempfile::NamedTempFile::new().expect("create tempfile");
        net.save(file.path())?;
        Ok(Self { file, cfg: net.cfg })
    }

    /// Hydrate a fresh `GoMctsTransformer` from the snapshot. The
    /// returned net lives on `device` and shares no Vars with anything
    /// else.
    pub fn hydrate(&self, device: Device) -> CandleResult<GoMctsTransformer> {
        let mut net = GoMctsTransformer::new(self.cfg, device)?;
        net.load(self.file.path())?;
        Ok(net)
    }
}

/// A population of historical model snapshots plus the current
/// (live, trainable) model.
///
/// Iteration k of self-play uses:
///   * the LIVE model at the "ArgmaxVal\*" seat (chosen per game)
///   * one randomly chosen frozen snapshot at every other seat
///
/// This is the paper's training setup: prevents collapse to a fixed
/// point against the current self.
pub struct Population<G: GameState, T: Tokenizer<G> + Clone> {
    pub live: TransformerGenerativeModel<G, T>,
    snapshots: Vec<Snapshot>,
    tokenizer: T,
    device: Device,
}

impl<G: GameState, T: Tokenizer<G> + Clone> Population<G, T> {
    pub fn new(live: TransformerGenerativeModel<G, T>) -> Self {
        let tokenizer = live.tokenizer.clone();
        let device = live.net.device.clone();
        Self { live, snapshots: Vec::new(), tokenizer, device }
    }

    /// Freeze the current live weights into a snapshot. Called once per
    /// training iteration after that iteration's gradient updates.
    pub fn snapshot(&mut self) -> CandleResult<()> {
        self.snapshots.push(Snapshot::from_model(&self.live.net)?);
        Ok(())
    }

    pub fn num_snapshots(&self) -> usize {
        self.snapshots.len()
    }

    /// Hydrate a fresh model from a random snapshot. Returns `None` if
    /// the population has no snapshots yet (caller should fall back to
    /// `live`).
    pub fn sample_frozen<R: rand::Rng>(
        &self,
        rng: &mut R,
    ) -> CandleResult<Option<TransformerGenerativeModel<G, T>>> {
        if self.snapshots.is_empty() {
            return Ok(None);
        }
        let idx = (rng.random::<u64>() as usize) % self.snapshots.len();
        let net = self.snapshots[idx].hydrate(self.device.clone())?;
        Ok(Some(TransformerGenerativeModel::new(net, self.tokenizer.clone())))
    }

    /// Hydrate a specific snapshot by index. Returns `None` if `idx` is
    /// out of bounds. Used by training loops that want the previous
    /// iteration's checkpoint for a convergence-detection head-to-head
    /// against `live`.
    pub fn sample_specific_frozen(
        &self,
        idx: usize,
    ) -> CandleResult<Option<TransformerGenerativeModel<G, T>>> {
        if idx >= self.snapshots.len() {
            return Ok(None);
        }
        let net = self.snapshots[idx].hydrate(self.device.clone())?;
        Ok(Some(TransformerGenerativeModel::new(net, self.tokenizer.clone())))
    }
}

/// Head-to-head eval: play `n_games` Euchre/Kuhn/etc. hands with `a` at
/// seats matching `a_team` (every other seat) and `b` at the rest, then
/// flip the assignment for half the games to wash out seat bias.
///
/// Returns `(mean_a_payoff, a_win_rate_excluding_ties)`. For two-player
/// games "a_team" is just seat 0. For 4-player team games (Euchre)
/// it's seats {0, 2} vs seats {1, 3}. The split is decided by
/// `n_players`.
///
/// Used at iter-end in the training loop to detect convergence: when
/// `mean_a_payoff` of live-vs-recent-frozen drops to ~0 (i.e. live ≈
/// frozen), training has plateaued.
pub fn head_to_head_eval<G, MA, MB>(
    a: &mut MA,
    b: &mut MB,
    new_state: impl Fn() -> G,
    n_games: usize,
    seed: u64,
) -> (f64, f64)
where
    G: GameState,
    MA: GenerativeModel<G>,
    MB: GenerativeModel<G>,
{
    let mut total = 0.0;
    let mut wins = 0usize;
    let mut decided = 0usize;
    for game_idx in 0..n_games {
        let mut rng: StdRng = SeedableRng::seed_from_u64(seed.wrapping_add(game_idx as u64));
        let mut gs = new_state();
        let mut buf = Vec::new();
        while gs.is_chance_node() {
            buf.clear();
            gs.legal_actions(&mut buf);
            let act = *buf.choose(&mut rng).expect("non-empty chance");
            gs.apply_action(act);
        }
        let n_players = gs.num_players();
        // a_offset flips each game so seat bias washes out.
        let a_offset = game_idx % 2;
        // Helper: is seat `s` an `a`-team seat?
        let is_a = |s: usize| -> bool {
            if n_players == 2 {
                s == a_offset
            } else {
                // Team games: even seats vs odd seats. Flip via a_offset.
                (s % 2) == a_offset
            }
        };
        while !gs.is_terminal() {
            let p = gs.cur_player();
            buf.clear();
            gs.legal_actions(&mut buf);
            let history = gs.istate_key(p);
            let act = if is_a(p) {
                a.sample(&history, &buf, &mut rng)
            } else {
                b.sample(&history, &buf, &mut rng)
            };
            gs.apply_action(act);
        }
        // Pick any seat that is on the `a` team. evaluate(p) is team
        // payoff in Euchre, individual payoff in Kuhn — both work.
        let a_seat = (0..n_players).find(|s| is_a(*s)).unwrap_or(0);
        let v = gs.evaluate(a_seat);
        total += v;
        if v.abs() > 1e-9 {
            decided += 1;
            if v > 0.0 {
                wins += 1;
            }
        }
    }
    let mean = total / n_games as f64;
    let win_rate = if decided > 0 { wins as f64 / decided as f64 } else { 0.5 };
    (mean, win_rate)
}

/// Collect a single self-play game using the population schema. Seat
/// `live_seat` plays via `live`; every other seat samples from a
/// randomly chosen frozen snapshot (or `live` if the population is
/// empty). Returns plain (history, action, value) hard targets.
///
/// MCTS-driven targets are a separate code path
/// (`collect_self_play_game_mcts`) — combining the two would require
/// holding the live and the MCTS bot side-by-side, which we don't need
/// for v2 since paper-faithful Population × MCTS is a one-iter-at-a-
/// time loop where the MCTS bot wraps `live`.
pub fn collect_population_game<G, T>(
    population: &mut Population<G, T>,
    new_state: impl Fn() -> G,
    live_seat: Option<usize>,
    rng: &mut StdRng,
) -> CandleResult<Vec<TrainExample>>
where
    G: GameState,
    T: Tokenizer<G> + Clone,
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
    let live_seat = live_seat.unwrap_or_else(|| (rng.random::<u64>() as usize) % n_players);
    // One frozen model per non-live seat for the whole game. Sampling a
    // new opponent every turn is also valid but mixes histories
    // unhelpfully; one-per-game matches the paper.
    let mut frozen_per_seat: Vec<Option<TransformerGenerativeModel<G, T>>> =
        (0..n_players).map(|_| None).collect();
    for p in 0..n_players {
        if p == live_seat {
            continue;
        }
        frozen_per_seat[p] = population.sample_frozen(rng)?;
    }
    let mut per_player: Vec<Vec<(IStateKey, Action)>> = vec![Vec::new(); n_players];
    while !gs.is_terminal() {
        let p = gs.cur_player();
        let history = gs.istate_key(p);
        buf.clear();
        gs.legal_actions(&mut buf);
        let a = if p == live_seat {
            <TransformerGenerativeModel<G, T> as GenerativeModel<G>>::sample(
                &mut population.live,
                &history,
                &buf,
                rng,
            )
        } else if let Some(m) = frozen_per_seat[p].as_mut() {
            <TransformerGenerativeModel<G, T> as GenerativeModel<G>>::sample(
                m, &history, &buf, rng,
            )
        } else {
            // No snapshots → fall back to live for opponents (first
            // pre-snapshot iteration).
            <TransformerGenerativeModel<G, T> as GenerativeModel<G>>::sample(
                &mut population.live,
                &history,
                &buf,
                rng,
            )
        };
        per_player[p].push((history, a));
        gs.apply_action(a);
    }
    let mut out = Vec::new();
    for p in 0..n_players {
        let v = gs.evaluate(p) as f32;
        for (h, a) in per_player[p].drain(..) {
            out.push(TrainExample::hard(h, a, v));
        }
    }
    Ok(out)
}

// =====================================================================
// Kuhn tokenizer (validation)
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

// =====================================================================
// Euchre tokenizer (smoke scale)
// =====================================================================

pub mod euchre {
    use super::*;
    use games::gamestates::euchre::EuchreGameState;

    /// Euchre's `EAction` enum has 32 unique single-bit discriminants
    /// (cards × 4 suits + Spades/Clubs/Hearts/Diamonds suit-calls +
    /// Pickup/Alone/DiscardMarker/Pass). The underlying `Action(u8)`
    /// value happens to be the trailing-zeros count of those bits, so
    /// every action sits in 0..32. We tokenise with `+1` to reserve 0
    /// for PAD, giving a 33-token vocab.
    #[derive(Clone, Copy)]
    pub struct EuchreTokenizer;

    impl EuchreTokenizer {
        pub const VOCAB_SIZE: usize = 33;
        /// Generous bound. A full Euchre game is bidding (≤ 8 calls +
        /// optional discard) + 5 tricks × 4 cards = ~30 actions. 48
        /// gives headroom for variants and the rare sit-out marker.
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
            // Action(u8) values for EAction variants land in 0..32. +1
            // shifts them past PAD=0. Anything ≥ 32 means our
            // upstream invariant has been violated.
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

#[cfg(test)]
mod tests {
    use super::euchre::EuchreTokenizer;
    use super::kuhn::KuhnTokenizer;
    use super::*;
    use games::gamestates::euchre::{Euchre, EuchreGameState};
    use games::gamestates::kuhn_poker::{KPAction, KPGameState, KuhnPoker};
    use rand::SeedableRng;

    fn make_transformer() -> TransformerGenerativeModel<KPGameState, KuhnTokenizer> {
        let cfg = TransformerConfig::kuhn_small(KuhnTokenizer::VOCAB_SIZE, KuhnTokenizer::MAX_CONTEXT);
        let net = GoMctsTransformer::new(cfg, Device::Cpu).expect("build transformer");
        TransformerGenerativeModel::new(net, KuhnTokenizer)
    }

    /// Forward pass produces finite logits + value for an empty history.
    #[test]
    fn transformer_forward_smoke() {
        let mut m = make_transformer();
        let empty = IStateKey::default();
        let legal = vec![KPAction::Bet.into(), KPAction::Pass.into()];
        let probs = m.policy(&empty, &legal);
        assert_eq!(probs.len(), 2);
        let s: f64 = probs.iter().sum();
        assert!(
            (s - 1.0).abs() < 1e-4 && probs.iter().all(|p| p.is_finite()),
            "policy malformed: {:?} sum={}",
            probs,
            s
        );
        let v = m.value(&empty);
        assert!(v.is_finite(), "value must be finite, got {}", v);
    }

    /// Train the transformer on a tiny synthetic dataset and confirm the
    /// loss decreases AND the value-driven inference policy actually
    /// distinguishes good from bad opener actions. The dataset
    /// explicitly provides V(K, Bet) > V(K, Pass) and V(J, Bet) <
    /// V(J, Pass) so the value head has the signal it needs for
    /// ArgmaxVal\*.
    #[test]
    fn transformer_loss_decreases() {
        let mut m = make_transformer();
        let bet = Action::from(KPAction::Bet);
        let pass = Action::from(KPAction::Pass);
        let mk = |card: KPAction, action: Action, v: f32| {
            let mut h = IStateKey::default();
            h.push(Action::from(card));
            TrainExample::hard(h, action, v)
        };
        let examples = vec![
            // King's a winning bet, a wasted pass.
            mk(KPAction::King, bet, 1.0),
            mk(KPAction::King, pass, -0.5),
            // Jack's the opposite.
            mk(KPAction::Jack, bet, -1.0),
            mk(KPAction::Jack, pass, 0.5),
            // Queen is roughly neutral on the opener decision.
            mk(KPAction::Queen, bet, 0.0),
            mk(KPAction::Queen, pass, 0.0),
        ];
        let mut rng: StdRng = SeedableRng::seed_from_u64(1);
        let l_before =
            train(&mut m, &examples, 1, examples.len(), 1e-3, &mut rng).expect("train epoch 1");
        let l_after =
            train(&mut m, &examples, 400, examples.len(), 1e-2, &mut rng).expect("train 400");
        assert!(
            l_after < l_before * 0.9,
            "loss should drop with training: before={}, after={}",
            l_before,
            l_after
        );
        let mut king = IStateKey::default();
        king.push(Action::from(KPAction::King));
        let mut jack = IStateKey::default();
        jack.push(Action::from(KPAction::Jack));
        let legal = vec![bet, pass];
        let king_p = m.policy(&king, &legal);
        let jack_p = m.policy(&jack, &legal);
        // `bet` is index 0 in `legal`.
        assert!(
            king_p[0] > jack_p[0],
            "King should bet more than Jack: king_bet={}, jack_bet={}",
            king_p[0],
            jack_p[0]
        );
        // And the King's bet share should be > 0.5 outright (Bet is the
        // +1 action there); Jack's bet share < 0.5.
        assert!(king_p[0] > 0.5, "King should prefer Bet: {}", king_p[0]);
        assert!(jack_p[0] < 0.5, "Jack should prefer Pass: {}", jack_p[0]);
    }

    /// Save-then-load round-trips the trained weights through
    /// safetensors. Verifies checkpointing is wired correctly.
    #[test]
    fn transformer_save_load_roundtrip() {
        let mut a = make_transformer();
        // Train briefly so weights aren't all-zero / random init.
        let bet = Action::from(KPAction::Bet);
        let pass = Action::from(KPAction::Pass);
        let mk = |card: KPAction, action: Action, v: f32| {
            let mut h = IStateKey::default();
            h.push(Action::from(card));
            TrainExample::hard(h, action, v)
        };
        let examples = vec![
            mk(KPAction::King, bet, 1.0),
            mk(KPAction::Jack, pass, 1.0),
        ];
        let mut rng: StdRng = SeedableRng::seed_from_u64(2);
        train(&mut a, &examples, 50, examples.len(), 1e-2, &mut rng).expect("train");
        let path = tempfile::NamedTempFile::new().expect("tmpfile");
        a.net.save(path.path()).expect("save");

        let cfg = TransformerConfig::kuhn_small(KuhnTokenizer::VOCAB_SIZE, KuhnTokenizer::MAX_CONTEXT);
        let mut net_b = GoMctsTransformer::new(cfg, Device::Cpu).expect("build");
        net_b.load(path.path()).expect("load");
        let mut b = TransformerGenerativeModel::new(net_b, KuhnTokenizer);

        // A and B should now produce identical policies on the same input.
        let mut king = IStateKey::default();
        king.push(Action::from(KPAction::King));
        let legal = [bet, pass];
        let pa = a.policy(&king, &legal);
        let pb = b.policy(&king, &legal);
        for (x, y) in pa.iter().zip(pb.iter()) {
            assert!(
                (x - y).abs() < 1e-5,
                "policy mismatch after roundtrip: {:?} vs {:?}",
                pa,
                pb
            );
        }
    }

    /// Snapshot + hydrate is the same idea as save_load_roundtrip but
    /// via the `Snapshot` helper that the `Population` uses internally.
    #[test]
    fn snapshot_hydrate_preserves_outputs() {
        let mut m = make_transformer();
        let bet = Action::from(KPAction::Bet);
        let mk = |card: KPAction, v: f32| {
            let mut h = IStateKey::default();
            h.push(Action::from(card));
            TrainExample::hard(h, bet, v)
        };
        let examples = vec![mk(KPAction::King, 1.0), mk(KPAction::Jack, -1.0)];
        let mut rng: StdRng = SeedableRng::seed_from_u64(3);
        train(&mut m, &examples, 30, examples.len(), 1e-2, &mut rng).expect("train");
        let snap = Snapshot::from_model(&m.net).expect("snapshot");
        let net_b = snap.hydrate(Device::Cpu).expect("hydrate");
        let mut b = TransformerGenerativeModel::new(net_b, KuhnTokenizer);
        let mut h = IStateKey::default();
        h.push(Action::from(KPAction::King));
        let pa = m.policy(&h, &[bet, Action::from(KPAction::Pass)]);
        let pb = b.policy(&h, &[bet, Action::from(KPAction::Pass)]);
        for (x, y) in pa.iter().zip(pb.iter()) {
            assert!((x - y).abs() < 1e-5, "hydrate mismatch: {:?} vs {:?}", pa, pb);
        }
    }

    /// Population self-play returns examples with hard targets and
    /// labels them with sensible payoffs. Numerical smoke.
    #[test]
    fn population_self_play_smoke() {
        let cfg = TransformerConfig::kuhn_small(KuhnTokenizer::VOCAB_SIZE, KuhnTokenizer::MAX_CONTEXT);
        let net = GoMctsTransformer::new(cfg, Device::Cpu).expect("build");
        let live = TransformerGenerativeModel::new(net, KuhnTokenizer);
        let mut pop = Population::new(live);
        let mut rng: StdRng = SeedableRng::seed_from_u64(4);
        // Snapshot once so opponents have something to load.
        pop.snapshot().expect("snapshot");
        let exs = collect_population_game(
            &mut pop,
            || KuhnPoker::new_state(),
            Some(0),
            &mut rng,
        )
        .expect("game");
        assert!(!exs.is_empty(), "should have at least one decision");
        for ex in &exs {
            assert!(ex.policy_target.is_none(), "population game uses hard targets");
            assert!(ex.value.is_finite());
        }
    }

    /// MCTS-driven self-play produces examples with soft policy targets
    /// that normalise to ~1 over the legal-action set.
    #[test]
    fn mcts_self_play_soft_targets() {
        use super::super::gomcts::{GoMcts, GoMctsConfig, UniformRandomModel};
        let mut rng: StdRng = SeedableRng::seed_from_u64(5);
        let mut search = GoMcts::<KPGameState, UniformRandomModel>::new(
            GoMctsConfig { uct_c: 0.4, n_iterations: 32, mu: 0.01, n_rollout_steps: 0, n_parallel_sims: 1 },
            UniformRandomModel,
            SeedableRng::seed_from_u64(99),
        );
        let exs = collect_self_play_game_mcts(
            || KuhnPoker::new_state(),
            &mut search,
            &mut rng,
        );
        assert!(!exs.is_empty());
        for ex in &exs {
            let soft = ex.policy_target.as_ref().expect("mcts game has soft targets");
            let s: f32 = soft.iter().map(|(_, p)| *p).sum();
            assert!(
                (s - 1.0).abs() < 1e-4,
                "soft target should sum to 1, got {} on history {:?}",
                s,
                ex.history
            );
        }
    }

    /// Batched and unbatched forward_history must produce the same
    /// (logits, value) pair to within float noise. Pins the batching
    /// refactor against future regressions.
    #[test]
    fn forward_history_batch_matches_unbatched() {
        let m = make_transformer();
        let bet = Action::from(KPAction::Bet);
        let pass = Action::from(KPAction::Pass);
        // Two interesting histories: [Jack, Bet] and [King].
        let mut h1 = IStateKey::default();
        h1.push(Action::from(KPAction::Jack));
        h1.push(bet);
        let mut h2 = IStateKey::default();
        h2.push(Action::from(KPAction::King));
        // Unbatched per-history.
        let (l1, v1) = m.forward_history(&h1).expect("unbatched 1");
        let (l2, v2) = m.forward_history(&h2).expect("unbatched 2");
        // Batched together.
        let (logits, values) = m.forward_history_batch(&[h1, h2]).expect("batched");
        assert!((v1 - values[0]).abs() < 1e-4, "value 0 mismatch: {} vs {}", v1, values[0]);
        assert!((v2 - values[1]).abs() < 1e-4, "value 1 mismatch: {} vs {}", v2, values[1]);
        for (a, b) in l1.iter().zip(logits[0].iter()) {
            assert!((a - b).abs() < 1e-3, "logits 0 mismatch: {} vs {}", a, b);
        }
        for (a, b) in l2.iter().zip(logits[1].iter()) {
            assert!((a - b).abs() < 1e-3, "logits 1 mismatch: {} vs {}", a, b);
        }
        // Suppress unused warning.
        let _ = pass;
    }

    /// Head-to-head eval is non-degenerate: identical models tied to
    /// the SAME seed should win ~50% (within sampling noise). Identical
    /// untrained transformers vs a random uniform-fallback model should
    /// also be in a sensible range.
    #[test]
    fn head_to_head_eval_runs() {
        use super::super::gomcts::UniformRandomModel;
        let mut a = make_transformer();
        let mut b = UniformRandomModel;
        let (mean, win) = head_to_head_eval(&mut a, &mut b, || KuhnPoker::new_state(), 60, 99);
        // Just a smoke: mean within sane Kuhn range, win_rate ∈ [0, 1].
        assert!(mean.abs() <= 2.0);
        assert!((0.0..=1.0).contains(&win));
    }

    /// Batched eval / h2h / pop all run end-to-end on CPU and return
    /// well-formed outputs. CPU model is tiny so this is fast.
    #[test]
    fn batched_eval_h2h_pop_smoke() {
        let cfg = TransformerConfig::kuhn_small(KuhnTokenizer::VOCAB_SIZE, KuhnTokenizer::MAX_CONTEXT);
        let net_a = GoMctsTransformer::new(cfg, Device::Cpu).expect("build A");
        let net_b = GoMctsTransformer::new(cfg, Device::Cpu).expect("build B");
        let tokenizer = KuhnTokenizer;
        let (mean, _se) = eval_vs_random_batched::<KPGameState, _, _>(
            &net_a,
            &tokenizer,
            KuhnPoker::new_state,
            12,
            42,
        );
        assert!(mean.is_finite() && mean.abs() <= 2.0, "eval mean: {}", mean);
        let (h2h_mean, win) = head_to_head_eval_batched::<KPGameState, _, _>(
            &net_a,
            &net_b,
            &tokenizer,
            KuhnPoker::new_state,
            12,
            42,
        );
        assert!(h2h_mean.is_finite() && h2h_mean.abs() <= 2.0);
        assert!((0.0..=1.0).contains(&win));
        let pop_examples = collect_population_games_batched::<KPGameState, _, _>(
            &net_a,
            &net_b,
            &tokenizer,
            KuhnPoker::new_state,
            8,
            42,
        );
        assert!(!pop_examples.is_empty());
        for ex in &pop_examples {
            assert!(ex.policy_target.is_none(), "pop games use hard targets");
            assert!(ex.value.is_finite());
        }
    }

    /// E5: cross-game batched self-play runs end-to-end on CPU and the
    /// returned examples are well-formed (soft targets sum to 1, values
    /// finite, etc).
    #[test]
    fn batched_self_play_runs_and_produces_examples() {
        let cfg = TransformerConfig::kuhn_small(KuhnTokenizer::VOCAB_SIZE, KuhnTokenizer::MAX_CONTEXT);
        let net = GoMctsTransformer::new(cfg, Device::Cpu).expect("build");
        let tokenizer = KuhnTokenizer;
        let mcfs = McfsConfig::default();
        let examples = collect_self_play_games_batched_alphazero::<KPGameState, _, _>(
            &net,
            &tokenizer,
            KuhnPoker::new_state,
            4,    // n_games
            16,   // max_batch_size
            8,    // mcts_iter
            mcfs,
            123,
        );
        assert!(!examples.is_empty(), "should produce at least one example");
        for ex in &examples {
            assert!(ex.value.is_finite(), "value should be finite");
            let soft = ex.policy_target.as_ref().expect("alphazero examples carry soft targets");
            let s: f32 = soft.iter().map(|(_, p)| *p).sum();
            assert!((s - 1.0).abs() < 1e-3, "soft target should sum to 1, got {}", s);
        }
    }

    /// Euchre forward smoke: build a real Euchre istate by playing a few
    /// random actions, encode it, run the transformer, and check that
    /// the policy normalises and value is finite. No training — purely
    /// verifying the tokenizer + transformer accept Euchre-sized input.
    #[test]
    fn euchre_transformer_forward_smoke() {
        use rand::seq::IndexedRandom;
        let mut rng: StdRng = SeedableRng::seed_from_u64(11);
        let mut gs: EuchreGameState = Euchre::new_state();
        let mut acts = Vec::new();
        while gs.is_chance_node() {
            gs.legal_actions(&mut acts);
            let a = *acts.choose(&mut rng).expect("non-empty chance");
            gs.apply_action(a);
            acts.clear();
        }
        // Walk a few non-chance actions just to push the istate forward.
        for _ in 0..4 {
            if gs.is_terminal() {
                break;
            }
            gs.legal_actions(&mut acts);
            let a = *acts.choose(&mut rng).expect("non-empty legal");
            gs.apply_action(a);
            acts.clear();
        }
        let cfg = TransformerConfig::euchre_smoke(
            EuchreTokenizer::VOCAB_SIZE,
            EuchreTokenizer::MAX_CONTEXT,
        );
        let net = GoMctsTransformer::new(cfg, Device::Cpu).expect("build");
        let mut m = TransformerGenerativeModel::new(net, EuchreTokenizer);
        let p = gs.cur_player();
        let history = gs.istate_key(p);
        gs.legal_actions(&mut acts);
        let probs = m.policy(&history, &acts);
        assert_eq!(probs.len(), acts.len());
        let s: f64 = probs.iter().sum();
        assert!(
            (s - 1.0).abs() < 1e-4 && probs.iter().all(|p| p.is_finite()),
            "policy malformed: {:?} sum={}",
            probs,
            s
        );
        assert!(m.value(&history).is_finite());
    }
}
