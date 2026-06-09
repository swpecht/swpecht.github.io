//! Tch (libtorch) port of `GoMctsTransformer`. Spike for evaluating
//! whether switching backends would lift the per-launch overhead
//! bottleneck observed under candle on WSL2. Mirrors the candle model
//! parameter-for-parameter so the same safetensors file loads cleanly.

use anyhow::{anyhow, Result};
use safetensors::SafeTensors;
use tch::{nn, nn::Module, Device, Kind, Tensor};

use games::{istate::IStateKey, GameState};
use rand::SeedableRng;
use rand::rngs::StdRng;

use crate::algorithms::gomcts_transformer::{
    collect_self_play_game_alphazero, pad_to, McfsConfig, RemoteModel,
    ServiceRequest, Tokenizer, TrainExample, TransformerConfig,
};

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
fn serve_batched_tch<G: GameState, T: Tokenizer<G>>(
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
