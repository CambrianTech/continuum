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
    weight: Tensor,
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

        let q = self.attention_wq.forward(&normed)?;
        let k = self.attention_wk.forward(&normed)?;
        let v = self.attention_wv.forward(&normed)?;

        let q = q
            .reshape((b_sz, seq_len, self.n_head, self.head_dim))?
            .transpose(1, 2)?;
        let k = k
            .reshape((b_sz, seq_len, self.n_kv_head, self.head_dim))?
            .transpose(1, 2)?;
        let v = v
            .reshape((b_sz, seq_len, self.n_kv_head, self.head_dim))?
            .transpose(1, 2)?
            .contiguous()?;

        // Partial RoPE: only first rope_dim dims
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
            let att = (q.matmul(&k.t()?)? / (self.head_dim as f64).sqrt())?;
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

#[derive(Debug, Clone)]
struct DeltaNetLayer {
    attn_qkv: QMatMul,         // fused Q/K/V projection
    attn_q_norm: RmsNorm,
    attn_k_norm: RmsNorm,
    attn_gate: QMatMul,        // sigmoid gate
    attn_output: QMatMul,      // output projection
    attention_norm: RmsNorm,
    post_attention_norm: RmsNorm,
    mlp: Mlp,
    // SSM weights
    ssm_a: Tensor,             // decay parameter [n_head]
    ssm_alpha: QMatMul,        // input gate
    ssm_beta: QMatMul,         // output gate
    ssm_conv1d_weight: Tensor, // short causal conv [kernel_width, qkv_dim]
    ssm_dt: QMatMul,           // timestep projection
    ssm_dt_bias: Tensor,       // timestep bias [n_head]
    ssm_norm: RmsNorm,         // SSM output norm
    ssm_out: QMatMul,          // SSM output projection
    // Config
    n_head: usize,
    n_kv_head: usize,
    head_dim: usize,
    rope_dim: usize,
    cos: Tensor,
    sin: Tensor,
    // Recurrence state persists across tokens
    recurrence_state: Option<Tensor>, // [batch, n_head, head_dim, head_dim]
    conv_state: Option<Tensor>,       // [batch, kernel_width-1, qkv_dim]
}

impl DeltaNetLayer {
    fn forward(&mut self, x: &Tensor, index_pos: usize) -> Result<Tensor> {
        let (b_sz, seq_len, hidden_size) = x.dims3()?;
        let normed = self.attention_norm.forward(x)?;

        // Fused QKV projection
        let qkv = self.attn_qkv.forward(&normed)?;
        let q_dim = self.n_head * self.head_dim;
        let kv_dim = self.n_kv_head * self.head_dim;

        let q = qkv.narrow(2, 0, q_dim)?;
        let k = qkv.narrow(2, q_dim, kv_dim)?;
        let v = qkv.narrow(2, q_dim + kv_dim, kv_dim)?;

        // Reshape to [batch, heads, seq, head_dim]
        let q = q.reshape((b_sz, seq_len, self.n_head, self.head_dim))?.transpose(1, 2)?;
        let k = k.reshape((b_sz, seq_len, self.n_kv_head, self.head_dim))?.transpose(1, 2)?;
        let v = v.reshape((b_sz, seq_len, self.n_kv_head, self.head_dim))?.transpose(1, 2)?;

        // QK norm (per-head)
        // RmsNorm expects [batch, seq, dim] but we have [batch, heads, seq, head_dim]
        // Reshape to apply norm, then reshape back
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

        // Partial RoPE: only first rope_dim dims
        let q = apply_partial_rotary_emb(&q, &self.cos, &self.sin, index_pos, self.rope_dim)?;
        let k = apply_partial_rotary_emb(&k, &self.cos, &self.sin, index_pos, self.rope_dim)?;

        // GQA expansion: repeat K, V from n_kv_head to n_head
        let k = candle_transformers::utils::repeat_kv(k, self.n_head / self.n_kv_head)?;
        let v = candle_transformers::utils::repeat_kv(v, self.n_head / self.n_kv_head)?;

        // DeltaNet recurrence
        // dt = softplus(ssm_dt(normed) + ssm_dt_bias)
        let dt_proj = self.ssm_dt.forward(&normed)?; // [batch, seq, n_head]
        let dt = dt_proj.broadcast_add(&self.ssm_dt_bias)?;
        // softplus: log(1 + exp(x))
        let ones = Tensor::ones_like(&dt)?;
        let dt = (dt.exp()? + ones)?.log()?;

        // Decay: a = -exp(ssm_a) per head
        let decay_base = self.ssm_a.neg()?.exp()?; // [n_head], positive decay rates

        // Alpha/beta gates
        let alpha = self.ssm_alpha.forward(&normed)?; // [batch, seq, n_head]
        let beta = self.ssm_beta.forward(&normed)?;   // [batch, seq, n_head]

        // Sequential DeltaNet recurrence over time steps
        // State: [batch, n_head, head_dim, head_dim]
        let mut state = match &self.recurrence_state {
            Some(s) => s.clone(),
            None => Tensor::zeros(
                (b_sz, self.n_head, self.head_dim, self.head_dim),
                DType::F32,
                x.device(),
            )?,
        };

        let mut outputs = Vec::with_capacity(seq_len);
        for t in 0..seq_len {
            // Get per-timestep values: [batch, n_head]
            let dt_t = dt.i((.., t, ..))?.unsqueeze(2)?.unsqueeze(3)?; // [batch, n_head, 1, 1]
            let alpha_t = alpha.i((.., t, ..))?.unsqueeze(2)?; // [batch, n_head, 1]
            let beta_t = beta.i((.., t, ..))?.unsqueeze(2)?;   // [batch, n_head, 1]

            // Per-head decay for this timestep
            let dt_scalar = dt_t.squeeze(3)?.squeeze(2)?; // [batch, n_head]
            let decay_raw = decay_base.broadcast_mul(&dt_scalar)?; // [batch, n_head]
            let decay = decay_raw.exp()?.unsqueeze(2)?.unsqueeze(3)?; // [batch, n_head, 1, 1]

            // k_t, v_t: [batch, n_head, 1, head_dim]
            let k_t = k.i((.., .., t..t+1, ..))?; // [batch, n_head, 1, head_dim]
            let v_t = v.i((.., .., t..t+1, ..))?;

            // State update: S = decay * S + beta * (k^T @ v)
            // k_t^T @ v_t: [batch, n_head, head_dim, 1] @ [batch, n_head, 1, head_dim]
            //            = [batch, n_head, head_dim, head_dim]
            let kv_outer = k_t.transpose(2, 3)?.matmul(&v_t)?;
            let beta_scaled = beta_t.unsqueeze(3)?; // [batch, n_head, 1, 1]
            state = (&decay.broadcast_mul(&state)? + &beta_scaled.broadcast_mul(&kv_outer)?)?;

            // Output: o_t = alpha * (q_t @ S)
            // q_t: [batch, n_head, 1, head_dim]
            // S: [batch, n_head, head_dim, head_dim]
            // q_t @ S: [batch, n_head, 1, head_dim]
            let q_t = q.i((.., .., t..t+1, ..))?;
            let o_t = q_t.matmul(&state)?; // [batch, n_head, 1, head_dim]
            let o_t = alpha_t.unsqueeze(3)?.broadcast_mul(&o_t)?;
            outputs.push(o_t.squeeze(2)?); // [batch, n_head, head_dim]
        }

        self.recurrence_state = Some(state);

        // Stack outputs: [batch, n_head, seq, head_dim]
        let deltanet_out = Tensor::stack(&outputs, 2)?;

        // SSM norm on the output
        let deltanet_out = {
            let (b, nh, s, hd) = deltanet_out.dims4()?;
            let flat = deltanet_out.reshape((b * nh, s, hd))?;
            // ssm_norm weight might be smaller than head_dim (128 vs 256)
            // Apply it to the appropriate dimension
            let normed = self.ssm_norm.forward(&flat)?;
            normed.reshape((b, nh, s, hd))?
        };

        // Reshape to [batch, seq, n_head * head_dim]
        let deltanet_out = deltanet_out
            .transpose(1, 2)?
            .reshape(&[b_sz, seq_len, self.n_head * self.head_dim])?;

        // SSM output projection
        let ssm_projected = self.ssm_out.forward(&deltanet_out)?;

        // Gating: sigmoid(gate(normed)) * ssm_projected
        let gate = candle_nn::ops::sigmoid(&self.attn_gate.forward(&normed)?)?;
        let gated = (gate * ssm_projected)?;

        // Output projection
        let attn_out = self.attn_output.forward(&gated)?;

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

        log.info(&format!(
            "Qwen3.5 config: {}L, {}Qh, {}KVh, head_dim={}, rope_dim={}, hidden={}, ctx={}, freq_base={}",
            block_count, head_count, head_count_kv, head_dim, rope_dim, embedding_length, context_length, rope_freq_base
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

            // Detect layer type by checking which tensors exist
            let is_attention = ct.tensor(reader, &format!("{prefix}.attn_q.weight"), device).is_ok();

            // Shared: FFN (both layer types)
            let ffn_gate = ct.tensor(reader, &format!("{prefix}.ffn_gate.weight"), device)?;
            let ffn_down = ct.tensor(reader, &format!("{prefix}.ffn_down.weight"), device)?;
            let ffn_up = ct.tensor(reader, &format!("{prefix}.ffn_up.weight"), device)?;
            let mlp = Mlp {
                feed_forward_w1: QMatMul::from_qtensor(ffn_gate)?,
                feed_forward_w2: QMatMul::from_qtensor(ffn_down)?,
                feed_forward_w3: QMatMul::from_qtensor(ffn_up)?,
            };

            // Shared: norms
            let attention_norm = RmsNorm::from_qtensor(
                ct.tensor(reader, &format!("{prefix}.attn_norm.weight"), device)?,
                rms_norm_eps,
            )?;
            let post_attention_norm = RmsNorm::from_qtensor(
                ct.tensor(reader, &format!("{prefix}.post_attention_norm.weight"), device)?,
                rms_norm_eps,
            )?;

            if is_attention {
                // Full attention layer: separate Q/K/V
                let attention_wq = ct.tensor(reader, &format!("{prefix}.attn_q.weight"), device)?;
                let attention_wk = ct.tensor(reader, &format!("{prefix}.attn_k.weight"), device)?;
                let attention_wv = ct.tensor(reader, &format!("{prefix}.attn_v.weight"), device)?;
                let attention_wo = ct.tensor(reader, &format!("{prefix}.attn_output.weight"), device)?;

                if layer_idx == 7 {
                    log.info(&format!("Layer {}: Attention (separate Q/K/V)", layer_idx));
                }

                layers.push(LayerKind::Attention(AttentionLayer {
                    attention_wq: QMatMul::from_qtensor(attention_wq)?,
                    attention_wk: QMatMul::from_qtensor(attention_wk)?,
                    attention_wv: QMatMul::from_qtensor(attention_wv)?,
                    attention_wo: QMatMul::from_qtensor(attention_wo)?,
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
                let attn_qkv = ct.tensor(reader, &format!("{prefix}.attn_qkv.weight"), device)?;
                let attn_gate = ct.tensor(reader, &format!("{prefix}.attn_gate.weight"), device)?;
                let attn_output = ct.tensor(reader, &format!("{prefix}.attn_output.weight"), device)?;
                let attn_q_norm = ct.tensor(reader, &format!("{prefix}.attn_q_norm.weight"), device)?;
                let attn_k_norm = ct.tensor(reader, &format!("{prefix}.attn_k_norm.weight"), device)?;

                // SSM tensors
                let ssm_a = ct.tensor(reader, &format!("{prefix}.ssm_a"), device)?
                    .dequantize(device)?;
                let ssm_alpha = ct.tensor(reader, &format!("{prefix}.ssm_alpha.weight"), device)?;
                let ssm_beta = ct.tensor(reader, &format!("{prefix}.ssm_beta.weight"), device)?;
                let ssm_conv1d = ct.tensor(reader, &format!("{prefix}.ssm_conv1d.weight"), device)?
                    .dequantize(device)?;
                let ssm_dt = ct.tensor(reader, &format!("{prefix}.ssm_dt.weight"), device)?;
                let ssm_dt_bias = ct.tensor(reader, &format!("{prefix}.ssm_dt.bias"), device)?
                    .dequantize(device)?;
                let ssm_norm = ct.tensor(reader, &format!("{prefix}.ssm_norm.weight"), device)?;
                let ssm_out = ct.tensor(reader, &format!("{prefix}.ssm_out.weight"), device)?;

                if layer_idx == 0 {
                    log.info(&format!("Layer {}: DeltaNet (fused QKV + SSM)", layer_idx));
                    log.info(&format!("  ssm_a shape: {:?}", ssm_a.dims()));
                    log.info(&format!("  ssm_conv1d shape: {:?}", ssm_conv1d.dims()));
                }

                layers.push(LayerKind::DeltaNet(DeltaNetLayer {
                    attn_qkv: QMatMul::from_qtensor(attn_qkv)?,
                    attn_q_norm: RmsNorm::from_qtensor(attn_q_norm, rms_norm_eps)?,
                    attn_k_norm: RmsNorm::from_qtensor(attn_k_norm, rms_norm_eps)?,
                    attn_gate: QMatMul::from_qtensor(attn_gate)?,
                    attn_output: QMatMul::from_qtensor(attn_output)?,
                    attention_norm,
                    post_attention_norm,
                    mlp,
                    ssm_a,
                    ssm_alpha: QMatMul::from_qtensor(ssm_alpha)?,
                    ssm_beta: QMatMul::from_qtensor(ssm_beta)?,
                    ssm_conv1d_weight: ssm_conv1d,
                    ssm_dt: QMatMul::from_qtensor(ssm_dt)?,
                    ssm_dt_bias,
                    ssm_norm: RmsNorm::from_qtensor(ssm_norm, rms_norm_eps)?,
                    ssm_out: QMatMul::from_qtensor(ssm_out)?,
                    n_head: head_count,
                    n_kv_head: head_count_kv,
                    head_dim,
                    rope_dim,
                    cos: cos.clone(),
                    sin: sin.clone(),
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

        let mut layer_in = x.clone();
        for layer in self.layers.iter_mut() {
            let layer_out = match layer {
                LayerKind::Attention(attn) => attn.forward(&layer_in, mask.as_ref(), index_pos)?,
                LayerKind::DeltaNet(delta) => delta.forward(&layer_in, index_pos)?,
            };
            layer_in = layer_out;
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
