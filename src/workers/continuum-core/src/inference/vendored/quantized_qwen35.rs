//! Qwen3.5 GGUF Backend — Hybrid DeltaNet + Attention architecture.
//!
//! Qwen3.5 uses a mix of two layer types:
//!   - **Full Attention** (every 4th layer: 7, 11, 15, 19, 23, 27, 31): Standard
//!     multi-head attention with separate Q/K/V projections, GQA, KV cache.
//!   - **DeltaNet** (all other layers): Linear attention with state-space recurrence.
//!     Uses fused QKV, gating, SSM decay/update, and short convolution.
//!
//! Both layer types share the same FFN (SwiGLU) and use partial RoPE — only the
//! first `rope_dim` dimensions of each head get rotary embedding.
//!
//! Key differences from Llama/Qwen2:
//!   - `rope_dim` (64) != `head_dim` (256) — partial RoPE
//!   - `post_attention_norm` instead of `ffn_norm`
//!   - DeltaNet layers have SSM tensors: ssm_a, ssm_alpha, ssm_beta, ssm_conv1d, ssm_dt, ssm_out
//!   - Attention gating on DeltaNet layers: sigmoid(gate) * output
//!   - QK norm on DeltaNet layers (attn_q_norm, attn_k_norm)

use std::collections::HashMap;

use candle_core::quantized::QTensor;
use candle_core::quantized::gguf_file;
use candle_core::{DType, Device, IndexOp, Result, Tensor};
use candle_nn::Module;

// ─── Shared Components (same as quantized_llama.rs) ────────────────────────

#[derive(Debug, Clone)]
struct RmsNorm {
    pub(crate) weight: Tensor,
    eps: f64,
}

impl RmsNorm {
    fn from_qtensor(qtensor: QTensor, eps: f64) -> Result<Self> {
        let weight = qtensor.dequantize(&qtensor.device())?;
        Ok(Self { weight, eps })
    }
}

impl Module for RmsNorm {
    fn forward(&self, x: &Tensor) -> Result<Tensor> {
        candle_nn::ops::rms_norm(x, &self.weight, self.eps as f32)
    }
}

/// Zero-overhead quantized embedding lookup.
#[derive(Debug, Clone)]
struct DeviceEmbedding {
    table: Tensor,
    hidden_size: usize,
}

impl DeviceEmbedding {
    fn from_gguf<R: std::io::Seek + std::io::Read>(
        ct: &gguf_file::Content,
        reader: &mut R,
        tensor_name: &str,
        hidden_size: usize,
        device: &Device,
    ) -> Result<Self> {
        let qt_cpu = ct.tensor(reader, tensor_name, &Device::Cpu)?;
        let table = qt_cpu.dequantize(&Device::Cpu)?.to_device(device)?;
        Ok(Self { table, hidden_size })
    }

    fn forward(&self, token_ids: &Tensor) -> Result<Tensor> {
        let embeddings = self.table.index_select(&token_ids.flatten_all()?, 0)?;
        let orig_dims = token_ids.dims();
        if orig_dims.len() == 2 {
            embeddings.reshape((orig_dims[0], orig_dims[1], self.hidden_size))
        } else {
            Ok(embeddings)
        }
    }
}

#[derive(Debug, Clone)]
struct QMatMul {
    inner: candle_core::quantized::QMatMul,
    span: tracing::Span,
}

impl QMatMul {
    fn from_qtensor(qtensor: QTensor) -> Result<Self> {
        let inner = candle_core::quantized::QMatMul::from_qtensor(qtensor)?;
        let span = tracing::span!(tracing::Level::TRACE, "qmatmul");
        Ok(Self { inner, span })
    }

    fn forward(&self, xs: &Tensor) -> Result<Tensor> {
        let _enter = self.span.enter();
        self.inner.forward(xs)
    }
}

#[derive(Debug, Clone)]
struct Mlp {
    feed_forward_w1: QMatMul, // gate
    feed_forward_w2: QMatMul, // down
    feed_forward_w3: QMatMul, // up
}

impl Module for Mlp {
    fn forward(&self, xs: &Tensor) -> Result<Tensor> {
        let w1 = self.feed_forward_w1.forward(xs)?;
        let w3 = self.feed_forward_w3.forward(xs)?;
        self.feed_forward_w2
            .forward(&(candle_nn::ops::silu(&w1)? * w3)?)
    }
}

/// Cumulative sum along the last dimension — GPU-native via matmul.
/// cumsum(x) = x @ tril(ones(N, N)) — uses Metal's existing matmul kernel.
/// No CPU round-trip. For N=64 (chunk size), the tril matrix is tiny.
fn cumsum_last_dim(x: &Tensor) -> Result<Tensor> {
    let n = x.dims()[x.dims().len() - 1];
    let device = x.device();

    // Lower-triangular ones matrix: tril[i][j] = 1 if j <= i
    let tril_data: Vec<f32> = (0..n)
        .flat_map(|i| (0..n).map(move |j| if j <= i { 1.0 } else { 0.0 }))
        .collect();
    let tril = Tensor::from_slice(&tril_data, (n, n), device)?;

    // Broadcast tril to match x's batch dims, then matmul.
    // x: [B, nh, N] → x @ tril needs tril to be [N, N] broadcast to [B, nh, N, N]
    // Candle matmul: [..., M, K] @ [..., K, N] → [..., M, N]
    // So x needs to be [..., 1, N] and tril needs to be [N, N]
    let ndim = x.dims().len();
    let x_unsq = x.unsqueeze(ndim - 1)?;  // [..., N, 1]... no that's wrong
    // Actually: x is [..., N]. We want output[..., i] = sum(x[..., 0..=i])
    // Matmul approach: reshape x to [..., 1, N], matmul with [N, N] tril
    // But candle needs matching batch dims for matmul.
    // Simpler: broadcast tril to match x's leading dims
    let mut tril_shape = vec![1usize; ndim - 1];
    tril_shape.push(n);
    tril_shape.push(n);
    let tril = tril.reshape(&tril_shape[..])?; // [1, ..., N, N]
    let tril = tril.broadcast_as(
        &[x.dims()[..ndim-1].to_vec(), vec![n, n]].concat()[..]
    )?;
    // x: [..., N] → [..., 1, N]
    let x_row = x.unsqueeze(ndim - 1)?; // [..., 1, N] — no, unsqueeze at ndim-1 gives [..., N, 1]
    // We need [..., 1, N]: unsqueeze at position ndim-1 (before last)
    let x_row = x.unsqueeze(ndim)?; // [..., N, 1]... still wrong
    // Let me think: x is [B, nh, CS]. I want [B, nh, 1, CS] @ [B, nh, CS, CS] → [B, nh, 1, CS] → squeeze → [B, nh, CS]
    let x_row = x.unsqueeze(ndim - 1)?; // [B, nh, 1, CS] — yes, insert before last dim
    // Wait no: unsqueeze(ndim-1) on [B, nh, CS] with ndim=3 inserts at position 2: [B, nh, 1, CS] ✓
    let result = x_row.matmul(&tril)?; // [B, nh, 1, CS]
    result.squeeze(ndim - 1) // [B, nh, CS]
}

fn masked_fill(on_false: &Tensor, mask: &Tensor, on_true: &Tensor) -> Result<Tensor> {
    let shape = mask.shape();
    let m = mask.where_cond(&on_true.broadcast_as(shape.dims())?, on_false)?;
    Ok(m)
}

fn precomput_freqs_cis(
    rope_dim: usize,
    freq_base: f32,
    context_length: usize,
    device: &Device,
) -> Result<(Tensor, Tensor)> {
    let theta: Vec<_> = (0..rope_dim)
        .step_by(2)
        .map(|i| 1f32 / freq_base.powf(i as f32 / rope_dim as f32))
        .collect();
    let theta = Tensor::new(theta.as_slice(), device)?;
    let idx_theta = Tensor::arange(0, context_length as u32, device)?
        .to_dtype(DType::F32)?
        .reshape((context_length, 1))?
        .matmul(&theta.reshape((1, theta.elem_count()))?)?;
    let cos = idx_theta.cos()?;
    let sin = idx_theta.sin()?;
    Ok((cos, sin))
}

// ─── Partial RoPE ──────────────────────────────────────────────────────────
// Qwen3.5: rope_dim=64, head_dim=256. Only first 64 dims of each head get
// rotary embedding. The remaining 192 dims pass through unchanged.

fn apply_partial_rotary_emb(
    x: &Tensor,
    cos: &Tensor,
    sin: &Tensor,
    index_pos: usize,
    rope_dim: usize,
) -> Result<Tensor> {
    let (_b_sz, _n_head, seq_len, head_dim) = x.dims4()?;
    let cos = cos.narrow(0, index_pos, seq_len)?;
    let sin = sin.narrow(0, index_pos, seq_len)?;

    if rope_dim >= head_dim {
        // Full RoPE (shouldn't happen for Qwen3.5, but handle gracefully)
        return candle_nn::rotary_emb::rope(&x.contiguous()?, &cos, &sin);
    }

    // Split: first rope_dim dims get RoPE, rest pass through
    let x_rope = x.narrow(3, 0, rope_dim)?.contiguous()?;
    let x_pass = x.narrow(3, rope_dim, head_dim - rope_dim)?;
    let x_rotated = candle_nn::rotary_emb::rope(&x_rope, &cos, &sin)?;
    Tensor::cat(&[&x_rotated, &x_pass], 3)
}

// ─── Full Attention Layer ──────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct AttentionLayer {
    attention_wq: QMatMul,
    attention_wk: QMatMul,
    attention_wv: QMatMul,
    attention_wo: QMatMul,
    attn_q_norm: RmsNorm,
    attn_k_norm: RmsNorm,
    attention_norm: RmsNorm,
    post_attention_norm: RmsNorm,
    mlp: Mlp,
    n_head: usize,
    n_kv_head: usize,
    head_dim: usize,
    rope_dim: usize,
    cos: Tensor,
    sin: Tensor,
    neg_inf: Tensor,
    kv_cache: Option<(Tensor, Tensor)>,
}

impl AttentionLayer {
    fn forward(&mut self, x: &Tensor, mask: Option<&Tensor>, index_pos: usize) -> Result<Tensor> {
        let (b_sz, seq_len, _hidden) = x.dims3()?;
        let normed = self.attention_norm.forward(x)?;

        // Q proj output is 2x head_dim: first half = query, second half = gate
        let q_full = self.attention_wq.forward(&normed)?; // [B, T, n_head * head_dim * 2]
        let k = self.attention_wk.forward(&normed)?;
        let v = self.attention_wv.forward(&normed)?;

        // Split Q into query + gate (each head_dim=256)
        let q_reshaped = q_full.reshape((b_sz, seq_len, self.n_head, self.head_dim * 2))?;
        let q = q_reshaped.narrow(3, 0, self.head_dim)?;                    // [B, T, n_head, head_dim]
        let attn_gate = q_reshaped.narrow(3, self.head_dim, self.head_dim)?; // [B, T, n_head, head_dim]
        let attn_gate = attn_gate.reshape((b_sz, seq_len, self.n_head * self.head_dim))?; // [B, T, n_head*head_dim]

        let q = q.transpose(1, 2)?;  // [B, n_head, T, head_dim]
        let k = k
            .reshape((b_sz, seq_len, self.n_kv_head, self.head_dim))?
            .transpose(1, 2)?;
        let v = v
            .reshape((b_sz, seq_len, self.n_kv_head, self.head_dim))?
            .transpose(1, 2)?
            .contiguous()?;

        // QK norm (per-head, head_dim=256)
        let q = {
            let (b, nh, s, hd) = q.dims4()?;
            let q_flat = q.reshape((b * nh, s, hd))?;
            let q_normed = self.attn_q_norm.forward(&q_flat)?;
            q_normed.reshape((b, nh, s, hd))?
        };
        let k = {
            let (b, nh, s, hd) = k.dims4()?;
            let k_flat = k.reshape((b * nh, s, hd))?;
            let k_normed = self.attn_k_norm.forward(&k_flat)?;
            k_normed.reshape((b, nh, s, hd))?
        };

        // Partial RoPE
        let q = apply_partial_rotary_emb(&q, &self.cos, &self.sin, index_pos, self.rope_dim)?;
        let k = apply_partial_rotary_emb(&k, &self.cos, &self.sin, index_pos, self.rope_dim)?;

        // KV cache
        let (k, v) = match &self.kv_cache {
            None => (k, v),
            Some((k_cache, v_cache)) => {
                if index_pos == 0 {
                    (k, v)
                } else {
                    let k = Tensor::cat(&[k_cache, &k], 2)?;
                    let v = Tensor::cat(&[v_cache, &v], 2)?;
                    (k, v)
                }
            }
        };
        self.kv_cache = Some((k.clone(), v.clone()));

        // Attention
        let y = if q.device().is_metal() && seq_len == 1 {
            candle_nn::ops::sdpa(
                &q, &k, &v, None, false,
                1. / (self.head_dim as f32).sqrt(), 1.,
            )?
        } else {
            let k = candle_transformers::utils::repeat_kv(k, self.n_head / self.n_kv_head)?;
            let v = candle_transformers::utils::repeat_kv(v, self.n_head / self.n_kv_head)?;
            // Metal: Q after QK-norm chunk reshape may be non-contiguous
            let att = (q.contiguous()?.matmul(&k.contiguous()?.t()?)? / (self.head_dim as f64).sqrt())?;
            let att = match mask {
                None => att,
                Some(mask) => {
                    let mask = mask.broadcast_as(att.shape())?;
                    masked_fill(&att, &mask, &self.neg_inf)?
                }
            };
            let att = candle_nn::ops::softmax_last_dim(&att)?;
            att.matmul(&v.contiguous()?)?
        };

        let y = y
            .transpose(1, 2)?
            .reshape(&[b_sz, seq_len, self.n_head * self.head_dim])?;

        // Apply sigmoid gate (second half of Q proj output)
        let y = (y * candle_nn::ops::sigmoid(&attn_gate)?)?;

        let attn_out = self.attention_wo.forward(&y)?;

        // Residual + post_attention_norm + FFN + residual
        let h = (x + attn_out)?;
        let normed = self.post_attention_norm.forward(&h)?;
        let ffn_out = self.mlp.forward(&normed)?;
        &h + ffn_out
    }
}

// ─── DeltaNet Layer ────────────────────────────────────────────────────────
// Linear attention with state-space recurrence.

/// DeltaNet layer — Gated Delta Rule linear attention.
///
/// Reference: HuggingFace modeling_qwen3_5.py Qwen3_5GatedDeltaNet
///
/// Tensor mapping (GGUF → HF):
///   attn_qkv    → in_proj_qkv   [hidden, key_dim*2 + value_dim]
///   attn_gate   → in_proj_z     [hidden, value_dim]        (output gate)
///   ssm_alpha   → in_proj_a     [hidden, num_v_heads]      (decay input)
///   ssm_beta    → in_proj_b     [hidden, num_v_heads]      (write strength)
///   ssm_a       → A_log         [num_v_heads]              (log-decay per V-head)
///   ssm_dt.bias → dt_bias       [num_v_heads]              (timestep bias)
///   ssm_conv1d  → conv1d.weight [kernel_width, qkv_dim]    (depthwise causal conv)
///   ssm_norm    → norm.weight   [head_v_dim]               (RMSNorm per V-head)
///   ssm_out     → out_proj      [value_dim, hidden]        (output projection)
#[derive(Debug, Clone)]
struct DeltaNetLayer {
    attn_qkv: QMatMul,         // in_proj_qkv: [hidden, key_dim*2 + value_dim]
    attn_gate: QMatMul,        // in_proj_z: [hidden, value_dim] (output gate)
    ssm_alpha: QMatMul,        // in_proj_a: [hidden, num_v_heads] (decay input)
    ssm_beta: QMatMul,         // in_proj_b: [hidden, num_v_heads] (write strength)
    ssm_a: Tensor,             // A_log: [num_v_heads] (log-decay)
    ssm_dt_bias: Tensor,       // dt_bias: [num_v_heads]
    ssm_conv1d_weight: Tensor, // conv1d: [kernel_width, qkv_dim] (depthwise causal)
    ssm_norm: RmsNorm,         // norm: [head_v_dim] (per V-head RMSNorm)
    ssm_out: QMatMul,          // out_proj: [value_dim, hidden]
    attention_norm: RmsNorm,
    post_attention_norm: RmsNorm,
    mlp: Mlp,
    // Config (derived from tensor shapes)
    num_k_heads: usize,        // 16 (K-heads, same as Q-heads)
    num_v_heads: usize,        // 32 (V-heads, 2x K-heads)
    head_k_dim: usize,         // 128 (per K/Q head)
    head_v_dim: usize,         // 128 (per V head)
    // State
    recurrence_state: Option<Tensor>, // [batch, num_v_heads, head_k_dim, head_v_dim]
    conv_state: Option<Tensor>,       // [batch, kernel_width-1, qkv_dim]
}

impl DeltaNetLayer {
    fn forward(&mut self, x: &Tensor, _index_pos: usize) -> Result<Tensor> {
        let (b_sz, seq_len, _hidden_size) = x.dims3()?;
        let normed = self.attention_norm.forward(x)?;

        // Step 1: Input projections
        let mixed_qkv = self.attn_qkv.forward(&normed)?;  // [B, T, key_dim*2 + value_dim]
        let z = self.attn_gate.forward(&normed)?;          // [B, T, value_dim] (output gate)
        let b = self.ssm_beta.forward(&normed)?;           // [B, T, num_v_heads] (write strength)
        let a = self.ssm_alpha.forward(&normed)?;          // [B, T, num_v_heads] (decay input)

        // Step 2: Depthwise causal conv1d on QKV, then SiLU
        // conv1d_weight: [kernel_width=4, qkv_dim=8192] (depthwise: each channel has own kernel)
        // Causal: pad kernel_width-1 zeros on left
        let mixed_qkv = {
            let conv_dims = self.ssm_conv1d_weight.dims();
            // GGUF may store as [kernel, channels] or [channels, kernel] — kernel is the small dim
            let (kernel_width, qkv_dim) = if conv_dims[0] < conv_dims[1] {
                (conv_dims[0], conv_dims[1])
            } else {
                (conv_dims[1], conv_dims[0])
            };
            // mixed_qkv: [B, T, qkv_dim] → transpose to [B, qkv_dim, T] for conv
            let x_t = mixed_qkv.transpose(1, 2)?; // [B, C, T]

            // Causal padding: prepend kernel_width-1 zeros (or conv_state for generation)
            let pad_width = kernel_width - 1;
            let x_padded = match &self.conv_state {
                Some(state) if seq_len == 1 => {
                    // Generation: use stored state
                    Tensor::cat(&[state, &x_t], 2)? // [B, C, pad+1]
                }
                _ => {
                    // Prefill: zero-pad
                    let zeros = Tensor::zeros((b_sz, qkv_dim, pad_width), DType::F32, x.device())?;
                    Tensor::cat(&[&zeros, &x_t], 2)? // [B, C, pad+T]
                }
            };

            // Save last kernel_width-1 timesteps for next generation step
            let total_len = x_padded.dims()[2];
            if total_len >= kernel_width {
                self.conv_state = Some(x_padded.narrow(2, total_len - pad_width, pad_width)?);
            }

            // Depthwise conv: weight needs shape [C, 1, K] for groups=C
            let weight = if self.ssm_conv1d_weight.dims()[0] < self.ssm_conv1d_weight.dims()[1] {
                // [K, C] → transpose → [C, K] → unsqueeze → [C, 1, K]
                self.ssm_conv1d_weight.t()?.unsqueeze(1)?
            } else {
                // [C, K] → unsqueeze → [C, 1, K]
                self.ssm_conv1d_weight.unsqueeze(1)?
            };
            // x_padded: [B, C, T+pad] → conv1d with groups=C
            let conv_out = x_padded
                .conv1d(&weight, 0, 1, 1, qkv_dim)?; // [B, C, T]
            conv_out.transpose(1, 2)? // [B, T, C]
        };
        let mixed_qkv = candle_nn::ops::silu(&mixed_qkv)?;

        // Step 3: Split QKV
        let key_dim = self.num_k_heads * self.head_k_dim;   // 16 * 128 = 2048
        let value_dim = self.num_v_heads * self.head_v_dim;  // 32 * 128 = 4096
        let q = mixed_qkv.narrow(2, 0, key_dim)?;
        let k = mixed_qkv.narrow(2, key_dim, key_dim)?;
        let v = mixed_qkv.narrow(2, key_dim * 2, value_dim)?;

        // Reshape to [B, T, num_heads, head_dim] → [B, num_heads, T, head_dim]
        let q = q.reshape((b_sz, seq_len, self.num_k_heads, self.head_k_dim))?.transpose(1, 2)?;
        let k = k.reshape((b_sz, seq_len, self.num_k_heads, self.head_k_dim))?.transpose(1, 2)?;
        let v = v.reshape((b_sz, seq_len, self.num_v_heads, self.head_v_dim))?.transpose(1, 2)?;

        // Step 4: L2-normalize Q and K (per-head)
        let q = {
            let norm = q.sqr()?.sum_keepdim(3)?.sqrt()?.clamp(1e-12, f64::INFINITY)?;
            q.broadcast_div(&norm)?
        };
        let k = {
            let norm = k.sqr()?.sum_keepdim(3)?.sqrt()?.clamp(1e-12, f64::INFINITY)?;
            k.broadcast_div(&norm)?
        };

        // Step 5: Compute decay g and write strength beta
        let beta = candle_nn::ops::sigmoid(&b)?;             // [B, T, num_v_heads]
        // g = -exp(A_log) * softplus(a + dt_bias)
        let a_plus_dt = a.broadcast_add(&self.ssm_dt_bias)?;
        let softplus_a = {
            let abs_a = a_plus_dt.abs()?;
            let pos_a = a_plus_dt.maximum(&Tensor::zeros_like(&a_plus_dt)?)?;
            (pos_a + abs_a.neg()?.exp()?.affine(1.0, 1.0)?.log()?)?
        };
        let g = self.ssm_a.exp()?.neg()?.broadcast_mul(&softplus_a)?; // [B, T, num_v_heads]

        // Step 6: Broadcast K-heads to V-heads (GQA: each K-head serves 2 V-heads)
        let repeat_factor = self.num_v_heads / self.num_k_heads;
        let q = candle_transformers::utils::repeat_kv(q, repeat_factor)?; // [B, num_v_heads, T, head_k_dim]
        let k = candle_transformers::utils::repeat_kv(k, repeat_factor)?;

        // Step 7: Chunked DeltaNet recurrence (torch_chunk_gated_delta_rule).
        // Chunks of 64 → N/64 sequential steps with chunk-size matrix ops.
        // Requires cumsum (implemented above as cumsum_last_dim).

        const CS: usize = 64;
        let scale = 1.0 / (self.head_k_dim as f64).sqrt();
        let device = x.device();

        // State and all DeltaNet ops run on whatever device the input is on.
        // With hybrid routing, DeltaNet layers receive CPU tensors → Accelerate BLAS.
        let mut state = match &self.recurrence_state {
            Some(s) => s.to_device(x.device())?,
            None => Tensor::zeros(
                (b_sz, self.num_v_heads, self.head_k_dim, self.head_v_dim),
                DType::F32, x.device(),
            )?,
        };

        let attn_out = if seq_len == 1 {
            // Single-token generation — llama.cpp approach: elementwise ops, no matmul.
            // State is [B, nh, hk, hv]. k/q/v are [B, nh, dim].
            // This avoids tiny matmuls that Metal can't optimize.
            let q_t = (q.squeeze(2)? * scale)?;       // [B, nh, hk]
            let k_t = k.squeeze(2)?;                   // [B, nh, hk]
            let v_t = v.squeeze(2)?;                   // [B, nh, hv]
            let g_t = g.i((.., 0, ..))?.exp()?;        // [B, nh]
            let beta_t = beta.i((.., 0, ..))?;         // [B, nh]

            // 1. Decay: S *= exp(g)
            // g: [B, nh] → [B, nh, 1, 1] for broadcast with state [B, nh, hk, hv]
            state = state.broadcast_mul(&g_t.unsqueeze(2)?.unsqueeze(3)?)?;

            // 2. Retrieve: sk = sum_rows(S * k)
            // k: [B, nh, hk] → [B, nh, hk, 1] for elementwise mul with S [B, nh, hk, hv]
            let sk = state.broadcast_mul(&k_t.unsqueeze(3)?)? // [B, nh, hk, hv]
                .sum(2)?;                                       // [B, nh, hv] (sum over hk dim)

            // 3. Delta: d = beta * (v - sk)
            let d = beta_t.unsqueeze(2)?.broadcast_mul(&(&v_t - &sk)?)?; // [B, nh, hv]

            // 4. Write: S += k outer d (elementwise: S[i,j] += k[i] * d[j])
            // k: [B, nh, hk, 1], d: [B, nh, 1, hv] → broadcast mul = [B, nh, hk, hv]
            state = (state + k_t.unsqueeze(3)?.broadcast_mul(&d.unsqueeze(2)?)?)?;

            // 5. Read: o = sum_rows(S * q)
            let o = state.broadcast_mul(&q_t.unsqueeze(3)?)? // [B, nh, hk, hv]
                .sum(2)?;                                      // [B, nh, hv]

            o.unsqueeze(2)? // [B, nh, 1, hv]
        } else {
            // Chunked prefill
            let q = (q.contiguous()? * scale)?;
            let k = k.contiguous()?;
            let v = v.contiguous()?;
            // g, beta: [B, T, nh] → [B, nh, T]
            let g = g.transpose(1, 2)?.contiguous()?;
            let beta = beta.transpose(1, 2)?.contiguous()?;

            // Pad to multiple of CS
            let pad = (CS - seq_len % CS) % CS;
            let tlen = seq_len + pad;
            let nc = tlen / CS;
            let (q, k, v, g, beta) = if pad > 0 {
                let pq = Tensor::zeros((b_sz, self.num_v_heads, pad, self.head_k_dim), DType::F32, device)?;
                let pv = Tensor::zeros((b_sz, self.num_v_heads, pad, self.head_v_dim), DType::F32, device)?;
                let ps = Tensor::zeros((b_sz, self.num_v_heads, pad), DType::F32, device)?;
                (Tensor::cat(&[&q, &pq], 2)?, Tensor::cat(&[&k, &pq], 2)?,
                 Tensor::cat(&[&v, &pv], 2)?, Tensor::cat(&[&g, &ps], 2)?,
                 Tensor::cat(&[&beta, &ps], 2)?)
            } else { (q, k, v, g, beta) };

            // v_beta, k_beta
            let bu = beta.unsqueeze(3)?;
            let v_beta = v.broadcast_mul(&bu)?;
            let k_beta = k.broadcast_mul(&bu)?;

            // Reshape to [B, nh, nc, CS, dim]
            let q = q.reshape((b_sz, self.num_v_heads, nc, CS, self.head_k_dim))?;
            let k = k.reshape((b_sz, self.num_v_heads, nc, CS, self.head_k_dim))?;
            let v_beta = v_beta.reshape((b_sz, self.num_v_heads, nc, CS, self.head_v_dim))?;
            let k_beta = k_beta.reshape((b_sz, self.num_v_heads, nc, CS, self.head_k_dim))?;
            let g = g.reshape((b_sz, self.num_v_heads, nc, CS))?;

            let mut outs = Vec::with_capacity(nc);
            for c in 0..nc {
                let qc = q.i((.., .., c, .., ..))?.contiguous()?;
                let kc = k.i((.., .., c, .., ..))?.contiguous()?;
                let vbc = v_beta.i((.., .., c, .., ..))?.contiguous()?;
                let kbc = k_beta.i((.., .., c, .., ..))?.contiguous()?;
                let gc = g.i((.., .., c, ..))?.contiguous()?;

                // Cumulative decay within chunk
                let g_cum = cumsum_last_dim(&gc)?; // [B, nh, CS]
                let g_last = g_cum.i((.., .., CS - 1))?; // [B, nh]
                let chunk_decay = g_last.exp()?.unsqueeze(2)?.unsqueeze(3)?;

                // Inter-chunk: query state with per-token decay
                let q_decayed = qc.broadcast_mul(&g_cum.unsqueeze(3)?.exp()?)?;
                let inter = q_decayed.matmul(&state)?;

                // Intra-chunk: causal attention (all on Metal — big parallel matmuls)
                let intra_scores = qc.matmul(&kc.t()?)?;
                let m: Vec<f32> = (0..CS).flat_map(|i| (0..CS).map(move |j| if j <= i { 1.0 } else { 0.0 })).collect();
                let mask = Tensor::from_slice(&m, (CS, CS), device)?;
                let intra = intra_scores.broadcast_mul(&mask)?.matmul(&vbc)?;

                outs.push((inter + intra)?);

                // Update state (already on correct device via hybrid routing)
                let kv_update = kbc.t()?.matmul(&vbc)?;
                state = (chunk_decay.broadcast_mul(&state)? + kv_update)?;

                device.synchronize()?;
            }

            let full = Tensor::cat(&outs, 2)?;
            if pad > 0 { full.narrow(2, 0, seq_len)? } else { full }
        };

        self.recurrence_state = Some(state);

        // Step 8: RMSNorm per V-head, gated by SiLU(z)
        let attn_out = {
            let (b, nh, s, hd) = attn_out.dims4()?;
            let flat = attn_out.reshape((b * nh, s, hd))?;
            let normed = self.ssm_norm.forward(&flat)?;
            normed.reshape((b, nh, s, hd))?
        };

        // Reshape to [B, T, value_dim]
        let attn_out = attn_out
            .transpose(1, 2)?
            .reshape(&[b_sz, seq_len, value_dim])?;

        // Gate: rms_norm(attn_out) * silu(z)
        let z_gate = candle_nn::ops::silu(&z)?;
        let attn_out = (attn_out * z_gate)?;

        // Step 9: Output projection
        let attn_out = self.ssm_out.forward(&attn_out)?;

        // Residual + post_attention_norm + FFN + residual
        let h = (x + attn_out)?;
        let normed2 = self.post_attention_norm.forward(&h)?;
        let ffn_out = self.mlp.forward(&normed2)?;
        &h + ffn_out
    }
}

// ─── Layer Dispatch ────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
enum LayerKind {
    Attention(AttentionLayer),
    DeltaNet(DeltaNetLayer),
}

// ─── Model Weights ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ModelWeights {
    tok_embeddings: DeviceEmbedding,
    layers: Vec<LayerKind>,
    norm: RmsNorm,
    output: QMatMul,
    masks: HashMap<usize, Tensor>,
    span: tracing::Span,
    span_output: tracing::Span,
    pub context_length: usize,
}

impl ModelWeights {
    pub fn from_gguf<R: std::io::Seek + std::io::Read>(
        ct: gguf_file::Content,
        reader: &mut R,
        device: &Device,
    ) -> Result<Self> {
        let log = crate::runtime::logger("candle");

        let arch = ct
            .metadata
            .get("general.architecture")
            .and_then(|v| v.to_string().ok())
            .cloned()
            .unwrap_or_else(|| "qwen35".to_string());

        let md_get = |s: &str| match ct.metadata.get(s) {
            None => candle_core::bail!("cannot find {s} in metadata"),
            Some(v) => Ok(v),
        };

        let arch_key = |param: &str| format!("{arch}.{param}");

        let context_length = md_get(&arch_key("context_length"))
            .and_then(|v| v.to_u32())
            .map(|v| v as usize)
            .unwrap_or(32768);

        let head_count = md_get(&arch_key("attention.head_count"))?.to_u32()? as usize;
        let head_count_kv = md_get(&arch_key("attention.head_count_kv"))?.to_u32()? as usize;
        let block_count = md_get(&arch_key("block_count"))?.to_u32()? as usize;
        let embedding_length = md_get(&arch_key("embedding_length"))?.to_u32()? as usize;

        let head_dim = md_get(&arch_key("attention.key_length"))
            .and_then(|v| v.to_u32())
            .map(|v| v as usize)
            .unwrap_or(embedding_length / head_count);

        let rope_dim = md_get(&arch_key("rope.dimension_count"))
            .and_then(|v| v.to_u32())
            .map(|v| v as usize)
            .unwrap_or(head_dim);

        let rms_norm_eps =
            md_get(&arch_key("attention.layer_norm_rms_epsilon"))?.to_f32()? as f64;

        let rope_freq_base = md_get(&arch_key("rope.freq_base"))
            .and_then(|m| m.to_f32())
            .unwrap_or(10000000f32);

        // SSM dimensions: derive from tensor shapes in the GGUF
        // ssm_a: [n_ssm_head] — gives us the SSM head count directly
        // ssm_out: [n_ssm_head * ssm_head_dim, hidden] — gives us ssm output dim
        let n_ssm_head = ct.tensor_infos.get("blk.0.ssm_a")
            .map(|info| {
                eprintln!("  ssm_a tensor_info dims: {:?}", info.shape.dims());
                info.shape.dims()[0]
            })
            .unwrap_or(32);
        // ssm_out GGUF shape is [hidden, out_dim] — out_dim is the SSM output size
        let ssm_head_dim = ct.tensor_infos.get("blk.0.ssm_out.weight")
            .map(|info| {
                let dims = info.shape.dims();
                eprintln!("  ssm_out tensor_info dims: {:?}", dims);
                // GGUF stores as [in_features, out_features] — ssm output dim is the larger one
                let ssm_out_dim = dims[0].max(dims[1]);
                ssm_out_dim / n_ssm_head
            })
            .unwrap_or(128);

        log.info(&format!(
            "Qwen3.5 config: {}L, {}Qh, {}KVh, head_dim={}, rope_dim={}, hidden={}, ctx={}, freq_base={}, ssm_heads={}, ssm_head_dim={}",
            block_count, head_count, head_count_kv, head_dim, rope_dim, embedding_length, context_length, rope_freq_base, n_ssm_head, ssm_head_dim
        ));

        // RoPE tables: sized for rope_dim (64), NOT head_dim (256)
        let (cos, sin) = precomput_freqs_cis(rope_dim, rope_freq_base, context_length, device)?;
        let neg_inf = Tensor::new(f32::NEG_INFINITY, device)?;

        // Embeddings
        let tok_embeddings = DeviceEmbedding::from_gguf(
            &ct, reader, "token_embd.weight", embedding_length, device,
        )?;
        let norm = RmsNorm::from_qtensor(
            ct.tensor(reader, "output_norm.weight", device)?,
            rms_norm_eps,
        )?;
        let output = match ct.tensor(reader, "output.weight", device) {
            Ok(tensor) => tensor,
            Err(_) => ct.tensor(reader, "token_embd.weight", device)?,
        };

        let mut layers = Vec::with_capacity(block_count);
        for layer_idx in 0..block_count {
            let prefix = format!("blk.{layer_idx}");

            // Detect layer type by checking tensor index (no I/O, just hashmap lookup)
            let is_attention = ct.tensor_infos.contains_key(&format!("{prefix}.attn_q.weight"));

            // Hybrid device routing: DeltaNet on CPU (Accelerate BLAS), Attention on Metal
            let layer_device = if is_attention { device } else { &Device::Cpu };

            // Shared: FFN (both layer types) — loaded on layer's device
            let ffn_gate = ct.tensor(reader, &format!("{prefix}.ffn_gate.weight"), layer_device)?;
            let ffn_down = ct.tensor(reader, &format!("{prefix}.ffn_down.weight"), layer_device)?;
            let ffn_up = ct.tensor(reader, &format!("{prefix}.ffn_up.weight"), layer_device)?;
            let mlp = Mlp {
                feed_forward_w1: QMatMul::from_qtensor(ffn_gate)?,
                feed_forward_w2: QMatMul::from_qtensor(ffn_down)?,
                feed_forward_w3: QMatMul::from_qtensor(ffn_up)?,
            };

            // Shared: norms
            let attention_norm = RmsNorm::from_qtensor(
                ct.tensor(reader, &format!("{prefix}.attn_norm.weight"), layer_device)?,
                rms_norm_eps,
            )?;
            let post_attention_norm = RmsNorm::from_qtensor(
                ct.tensor(reader, &format!("{prefix}.post_attention_norm.weight"), layer_device)?,
                rms_norm_eps,
            )?;

            if is_attention {
                // Full attention layer: separate Q/K/V
                let attention_wq = ct.tensor(reader, &format!("{prefix}.attn_q.weight"), layer_device)?;
                let attention_wk = ct.tensor(reader, &format!("{prefix}.attn_k.weight"), layer_device)?;
                let attention_wv = ct.tensor(reader, &format!("{prefix}.attn_v.weight"), layer_device)?;
                let attention_wo = ct.tensor(reader, &format!("{prefix}.attn_output.weight"), layer_device)?;
                let attn_q_norm_t = ct.tensor(reader, &format!("{prefix}.attn_q_norm.weight"), layer_device)?;
                let attn_k_norm_t = ct.tensor(reader, &format!("{prefix}.attn_k_norm.weight"), layer_device)?;

                if layer_idx == 7 {
                    log.info(&format!("Layer {}: Attention (separate Q/K/V)", layer_idx));
                }

                layers.push(LayerKind::Attention(AttentionLayer {
                    attention_wq: QMatMul::from_qtensor(attention_wq)?,
                    attention_wk: QMatMul::from_qtensor(attention_wk)?,
                    attention_wv: QMatMul::from_qtensor(attention_wv)?,
                    attention_wo: QMatMul::from_qtensor(attention_wo)?,
                    attn_q_norm: RmsNorm::from_qtensor(attn_q_norm_t, rms_norm_eps)?,
                    attn_k_norm: RmsNorm::from_qtensor(attn_k_norm_t, rms_norm_eps)?,
                    attention_norm,
                    post_attention_norm,
                    mlp,
                    n_head: head_count,
                    n_kv_head: head_count_kv,
                    head_dim,
                    rope_dim,
                    cos: cos.clone(),
                    sin: sin.clone(),
                    neg_inf: neg_inf.clone(),
                    kv_cache: None,
                }));
            } else {
                // DeltaNet layer: fused QKV + SSM
                let attn_qkv = ct.tensor(reader, &format!("{prefix}.attn_qkv.weight"), layer_device)?;
                let attn_gate = ct.tensor(reader, &format!("{prefix}.attn_gate.weight"), layer_device)?;

                // SSM tensors — on CPU for Accelerate BLAS
                let ssm_a = ct.tensor(reader, &format!("{prefix}.ssm_a"), layer_device)?
                    .dequantize(layer_device)?;
                let ssm_alpha = ct.tensor(reader, &format!("{prefix}.ssm_alpha.weight"), layer_device)?;
                let ssm_beta = ct.tensor(reader, &format!("{prefix}.ssm_beta.weight"), layer_device)?;
                let ssm_conv1d = ct.tensor(reader, &format!("{prefix}.ssm_conv1d.weight"), layer_device)?
                    .dequantize(layer_device)?;
                let ssm_dt_bias = ct.tensor(reader, &format!("{prefix}.ssm_dt.bias"), layer_device)?
                    .dequantize(layer_device)?;
                let ssm_norm = ct.tensor(reader, &format!("{prefix}.ssm_norm.weight"), layer_device)?;
                let ssm_out = ct.tensor(reader, &format!("{prefix}.ssm_out.weight"), layer_device)?;

                if layer_idx == 0 {
                    log.info(&format!("Layer {}: DeltaNet (fused QKV + SSM)", layer_idx));
                    log.info(&format!("  ssm_a shape: {:?}", ssm_a.dims()));
                    log.info(&format!("  ssm_conv1d shape: {:?}", ssm_conv1d.dims()));
                }

                // Derive DeltaNet head geometry from tensor shapes
                let num_v_heads = ssm_a.dims()[0]; // ssm_a = [num_v_heads]
                let ssm_out_dim = {
                    let d = ssm_out.shape().dims();
                    d[0].max(d[1]) // GGUF may store transposed
                };
                let head_v_dim = ssm_out_dim / num_v_heads;
                let qkv_total = {
                    let d = attn_qkv.shape().dims();
                    d[0].max(d[1])
                };
                // qkv_total = key_dim*2 + value_dim
                let key_dim = (qkv_total - ssm_out_dim) / 2;
                let num_k_heads = key_dim / head_v_dim; // head_k_dim == head_v_dim for Qwen3.5
                let head_k_dim = key_dim / num_k_heads;

                if layer_idx == 0 {
                    log.info(&format!("  DeltaNet heads: K={} V={}, head_k={} head_v={}", num_k_heads, num_v_heads, head_k_dim, head_v_dim));
                }

                layers.push(LayerKind::DeltaNet(DeltaNetLayer {
                    attn_qkv: QMatMul::from_qtensor(attn_qkv)?,
                    attn_gate: QMatMul::from_qtensor(attn_gate)?,
                    ssm_alpha: QMatMul::from_qtensor(ssm_alpha)?,
                    ssm_beta: QMatMul::from_qtensor(ssm_beta)?,
                    ssm_a,
                    ssm_dt_bias,
                    ssm_conv1d_weight: ssm_conv1d,
                    ssm_norm: RmsNorm::from_qtensor(ssm_norm, rms_norm_eps)?,
                    ssm_out: QMatMul::from_qtensor(ssm_out)?,
                    attention_norm,
                    post_attention_norm,
                    mlp,
                    num_k_heads,
                    num_v_heads,
                    head_k_dim,
                    head_v_dim,
                    recurrence_state: None,
                    conv_state: None,
                }));
            }
        }

        let attn_count = layers.iter().filter(|l| matches!(l, LayerKind::Attention(_))).count();
        let delta_count = layers.iter().filter(|l| matches!(l, LayerKind::DeltaNet(_))).count();
        log.info(&format!("Loaded {} layers: {} attention + {} DeltaNet", layers.len(), attn_count, delta_count));

        let span = tracing::span!(tracing::Level::TRACE, "qwen35-model");
        let span_output = tracing::span!(tracing::Level::TRACE, "qwen35-output");
        Ok(Self {
            tok_embeddings,
            layers,
            norm,
            output: QMatMul::from_qtensor(output)?,
            masks: HashMap::new(),
            span,
            span_output,
            context_length,
        })
    }

    fn mask(&mut self, t: usize, device: &Device) -> Result<Tensor> {
        if let Some(mask) = self.masks.get(&t) {
            Ok(mask.clone())
        } else {
            let mask: Vec<_> = (0..t)
                .flat_map(|i| (0..t).map(move |j| u8::from(j > i)))
                .collect();
            let mask = Tensor::from_slice(&mask, (t, t), device)?;
            self.masks.insert(t, mask.clone());
            Ok(mask)
        }
    }

    pub fn forward(&mut self, x: &Tensor, index_pos: usize) -> Result<Tensor> {
        let (_b_sz, seq_len, _) = x.dims3()?;

        let mask = if seq_len == 1 {
            None
        } else {
            Some(self.mask(seq_len, x.device())?)
        };

        let _enter = self.span.enter();

        // Hybrid device routing: DeltaNet layers on CPU (Accelerate BLAS),
        // attention layers on Metal (SDPA). On Apple Silicon unified memory,
        // the to_device() between CPU↔Metal is a memcpy but the state stays
        // in the same physical RAM. DeltaNet's sequential 128x128 matmuls are
        // faster on CPU BLAS than Metal kernel dispatch overhead.
        let metal_device = x.device().clone();
        let cpu_device = Device::Cpu;
        let mut layer_in = x.clone();
        let mut prev_is_delta = false;

        for (i, layer) in self.layers.iter_mut().enumerate() {
            let is_delta = matches!(layer, LayerKind::DeltaNet(_));

            // Move tensor to correct device at layer type transitions
            if i == 0 {
                // First layer: move to its device
                if is_delta {
                    layer_in = layer_in.to_device(&cpu_device)?;
                }
            } else if is_delta != prev_is_delta {
                // Layer type changed: DeltaNet→Attention or Attention→DeltaNet
                if is_delta {
                    layer_in = layer_in.to_device(&cpu_device)?;
                } else {
                    layer_in = layer_in.to_device(&metal_device)?;
                }
            }
            prev_is_delta = is_delta;

            let layer_out = match layer {
                LayerKind::Attention(attn) => attn.forward(&layer_in, mask.as_ref(), index_pos)
                    .map_err(|e| candle_core::Error::Msg(format!("Layer {i} (attn): {e}")))?,
                LayerKind::DeltaNet(delta) => delta.forward(&layer_in, index_pos)
                    .map_err(|e| candle_core::Error::Msg(format!("Layer {i} (delta): {e}")))?,
            };
            layer_in = layer_out;
        }

        // Final output goes back to Metal for the output head
        if prev_is_delta {
            layer_in = layer_in.to_device(&metal_device)?;
        }

        let layer_in = self.norm.forward(&layer_in)?;
        let _enter = self.span_output.enter();
        self.output.forward(&layer_in)
    }

    /// Forward pass from token IDs (used by the backend).
    pub fn forward_from_ids(&mut self, input: &Tensor, index_pos: usize) -> Result<Tensor> {
        let x = self.tok_embeddings.forward(input)?;
        self.forward(&x, index_pos)
    }

    pub fn clear_cache(&mut self) {
        for layer in self.layers.iter_mut() {
            match layer {
                LayerKind::Attention(attn) => attn.kv_cache = None,
                LayerKind::DeltaNet(delta) => {
                    delta.recurrence_state = None;
                    delta.conv_state = None;
                }
            }
        }
        self.masks.clear();
    }
}
