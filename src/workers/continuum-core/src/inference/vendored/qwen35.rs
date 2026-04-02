//! Qwen3.5 — Hybrid linear attention + full attention model.
//!
//! Qwen3.5 is NOT standard Mamba/SSM. It's a hybrid:
//! - 75% layers: linear attention with causal conv1d (fast, O(n) complexity)
//! - 25% layers: full GQA attention with RoPE (every 4th layer)
//!
//! Key architectural features:
//! - `layer_types`: array of "linear_attention" or "full_attention" per layer
//! - `linear_conv_kernel_dim: 4`: conv1d width for linear attention layers
//! - `partial_rotary_factor: 0.25`: only 25% of dims get RoPE
//! - `attn_output_gate: true`: gated output projection
//!
//! The causal conv1d is implemented as a simple depthwise convolution
//! — no external CUDA dependencies needed.

use candle_core::{DType, Device, IndexOp, Result, Tensor};
use candle_nn::{linear_no_bias, Embedding, Linear, Module, RmsNorm, VarBuilder};

// ── Config ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum LayerType {
    LinearAttention,
    FullAttention,
}

#[derive(Debug, Clone)]
pub struct Qwen35Config {
    pub vocab_size: usize,
    pub hidden_size: usize,
    pub intermediate_size: usize,
    pub num_hidden_layers: usize,
    // Full attention config
    pub num_attention_heads: usize,
    pub num_key_value_heads: usize,
    pub head_dim: usize,
    // Linear attention config
    pub linear_num_key_heads: usize,
    pub linear_num_value_heads: usize,
    pub linear_key_head_dim: usize,
    pub linear_value_head_dim: usize,
    pub linear_conv_kernel_dim: usize,
    // Shared
    pub max_position_embeddings: usize,
    pub rms_norm_eps: f64,
    pub rope_theta: f32,
    pub partial_rotary_factor: f64,
    pub tie_word_embeddings: bool,
    pub layer_types: Vec<LayerType>,
}

impl Qwen35Config {
    pub fn from_json(v: &serde_json::Value) -> std::result::Result<Self, String> {
        // Handle nested text_config (Qwen3.5 is a VL model)
        let tc = v.get("text_config").unwrap_or(v);

        let hidden_size = tc["hidden_size"].as_u64().ok_or("missing hidden_size")? as usize;
        let num_attention_heads = tc["num_attention_heads"].as_u64().ok_or("missing num_attention_heads")? as usize;

        let layer_types_raw = tc.get("layer_types")
            .and_then(|v| v.as_array())
            .ok_or("missing layer_types")?;
        let layer_types: Vec<LayerType> = layer_types_raw.iter().map(|lt| {
            match lt.as_str().unwrap_or("full_attention") {
                "linear_attention" => LayerType::LinearAttention,
                _ => LayerType::FullAttention,
            }
        }).collect();

        Ok(Self {
            vocab_size: tc["vocab_size"].as_u64().ok_or("missing vocab_size")? as usize,
            hidden_size,
            intermediate_size: tc["intermediate_size"].as_u64().ok_or("missing intermediate_size")? as usize,
            num_hidden_layers: tc["num_hidden_layers"].as_u64().ok_or("missing num_hidden_layers")? as usize,
            num_attention_heads,
            num_key_value_heads: tc["num_key_value_heads"].as_u64().ok_or("missing num_key_value_heads")? as usize,
            head_dim: tc.get("head_dim").and_then(|v| v.as_u64()).map(|v| v as usize)
                .unwrap_or(hidden_size / num_attention_heads),
            linear_num_key_heads: tc.get("linear_num_key_heads").and_then(|v| v.as_u64()).unwrap_or(16) as usize,
            linear_num_value_heads: tc.get("linear_num_value_heads").and_then(|v| v.as_u64()).unwrap_or(32) as usize,
            linear_key_head_dim: tc.get("linear_key_head_dim").and_then(|v| v.as_u64()).unwrap_or(128) as usize,
            linear_value_head_dim: tc.get("linear_value_head_dim").and_then(|v| v.as_u64()).unwrap_or(128) as usize,
            linear_conv_kernel_dim: tc.get("linear_conv_kernel_dim").and_then(|v| v.as_u64()).unwrap_or(4) as usize,
            max_position_embeddings: tc["max_position_embeddings"].as_u64().ok_or("missing max_position_embeddings")? as usize,
            rms_norm_eps: tc.get("rms_norm_eps").and_then(|v| v.as_f64()).unwrap_or(1e-6),
            rope_theta: tc.get("rope_theta").and_then(|v| v.as_f64()).unwrap_or(10000.0) as f32,
            partial_rotary_factor: tc.get("partial_rotary_factor").and_then(|v| v.as_f64()).unwrap_or(0.25),
            tie_word_embeddings: tc.get("tie_word_embeddings").and_then(|v| v.as_bool()).unwrap_or(true),
            layer_types,
        })
    }
}

// ── Causal Conv1d ─────────────────────────────────────────────────────────
// Pure Rust implementation — no external CUDA deps. Works on any device.

fn causal_conv1d(x: &Tensor, weight: &Tensor, bias: Option<&Tensor>) -> Result<Tensor> {
    // x: (batch, dim, seqlen), weight: (dim, width), bias: (dim,)
    // Equivalent to: F.conv1d(x, weight.unsqueeze(1), bias, padding=width-1, groups=dim)[:,:,:seqlen]
    let (_batch, _dim, seqlen) = x.dims3()?;
    let width = weight.dim(1)?;

    // Pad input on the left (causal — only look back)
    let padding = width - 1;
    let padded = if padding > 0 {
        let pad_zeros = Tensor::zeros(&[x.dim(0)?, x.dim(1)?, padding], x.dtype(), x.device())?;
        Tensor::cat(&[&pad_zeros, x], 2)?
    } else {
        x.clone()
    };

    // Depthwise convolution: for each channel, convolve with its width-sized kernel
    // This is the naive implementation — Candle's conv1d with groups would be better
    // but we implement manually for clarity and correctness first
    let mut outputs = Vec::with_capacity(width);
    for k in 0..width {
        let slice = padded.i((.., .., k..k + seqlen))?;
        let w_k = weight.i((.., k))?.unsqueeze(0)?.unsqueeze(2)?; // (1, dim, 1)
        outputs.push(slice.mul(&w_k.broadcast_as(slice.shape())?)?);
    }

    let mut out = outputs[0].clone();
    for o in &outputs[1..] {
        out = out.add(o)?;
    }

    if let Some(b) = bias {
        let b = b.unsqueeze(0)?.unsqueeze(2)?; // (1, dim, 1)
        out = out.add(&b.broadcast_as(out.shape())?)?;
    }

    Ok(out)
}

// ── RoPE (partial rotary) ─────────────────────────────────────────────────

fn apply_partial_rotary_emb(
    x: &Tensor,
    cos: &Tensor,
    sin: &Tensor,
    rotary_dim: usize,
) -> Result<Tensor> {
    let (_b, _h, _seq, head_dim) = x.dims4()?;
    if rotary_dim == 0 || rotary_dim >= head_dim {
        // Full rotary or no rotary
        return apply_rotary_emb(x, cos, sin);
    }
    let x_rot = x.i((.., .., .., ..rotary_dim))?;
    let x_pass = x.i((.., .., .., rotary_dim..))?;
    let rotated = apply_rotary_emb(&x_rot, cos, sin)?;
    Tensor::cat(&[&rotated, &x_pass], 3)
}

fn apply_rotary_emb(x: &Tensor, cos: &Tensor, sin: &Tensor) -> Result<Tensor> {
    let (b, h, seq, d) = x.dims4()?;
    let half = d / 2;
    let x1 = x.i((.., .., .., ..half))?;
    let x2 = x.i((.., .., .., half..))?;
    let cos = cos.broadcast_as(&[b, 1, seq, half])?;
    let sin = sin.broadcast_as(&[b, 1, seq, half])?;
    let r1 = x1.mul(&cos)?.sub(&x2.mul(&sin)?)?;
    let r2 = x2.mul(&cos)?.add(&x1.mul(&sin)?)?;
    Tensor::cat(&[&r1, &r2], 3)
}

fn build_rope_cache(
    seq_len: usize,
    head_dim: usize,
    theta: f32,
    device: &Device,
    dtype: DType,
) -> Result<(Tensor, Tensor)> {
    let half = head_dim / 2;
    let inv_freq: Vec<f32> = (0..half)
        .map(|i| 1.0 / theta.powf(2.0 * i as f32 / head_dim as f32))
        .collect();
    let inv_freq = Tensor::new(inv_freq, device)?;
    let positions: Vec<f32> = (0..seq_len).map(|i| i as f32).collect();
    let positions = Tensor::new(positions, device)?.unsqueeze(1)?;
    let freqs = positions.matmul(&inv_freq.unsqueeze(0)?)?;
    let cos = freqs.cos()?.to_dtype(dtype)?;
    let sin = freqs.sin()?.to_dtype(dtype)?;
    Ok((cos, sin))
}

// ── Full Attention Layer ──────────────────────────────────────────────────

struct FullAttentionLayer {
    q_proj: Linear,
    k_proj: Linear,
    v_proj: Linear,
    o_proj: Linear,
    input_layernorm: RmsNorm,
    post_attention_layernorm: RmsNorm,
    mlp_gate_proj: Linear,
    mlp_up_proj: Linear,
    mlp_down_proj: Linear,
    num_heads: usize,
    num_kv_heads: usize,
    head_dim: usize,
}

impl FullAttentionLayer {
    fn load(vb: VarBuilder, cfg: &Qwen35Config) -> Result<Self> {
        let h = cfg.hidden_size;
        let hd = cfg.head_dim;
        let nh = cfg.num_attention_heads;
        let nkv = cfg.num_key_value_heads;

        Ok(Self {
            q_proj: linear_no_bias(h, nh * hd, vb.pp("self_attn.q_proj"))?,
            k_proj: linear_no_bias(h, nkv * hd, vb.pp("self_attn.k_proj"))?,
            v_proj: linear_no_bias(h, nkv * hd, vb.pp("self_attn.v_proj"))?,
            o_proj: linear_no_bias(nh * hd, h, vb.pp("self_attn.o_proj"))?,
            input_layernorm: candle_nn::rms_norm(h, cfg.rms_norm_eps, vb.pp("input_layernorm"))?,
            post_attention_layernorm: candle_nn::rms_norm(h, cfg.rms_norm_eps, vb.pp("post_attention_layernorm"))?,
            mlp_gate_proj: linear_no_bias(h, cfg.intermediate_size, vb.pp("mlp.gate_proj"))?,
            mlp_up_proj: linear_no_bias(h, cfg.intermediate_size, vb.pp("mlp.up_proj"))?,
            mlp_down_proj: linear_no_bias(cfg.intermediate_size, h, vb.pp("mlp.down_proj"))?,
            num_heads: nh,
            num_kv_heads: nkv,
            head_dim: hd,
        })
    }

    fn forward(&self, x: &Tensor, cos: &Tensor, sin: &Tensor, rotary_dim: usize) -> Result<Tensor> {
        let (b, seq, _h) = x.dims3()?;
        let residual = x.clone();

        // Pre-norm
        let x = self.input_layernorm.forward(x)?;

        // QKV projections
        let q = self.q_proj.forward(&x)?
            .reshape((b, seq, self.num_heads, self.head_dim))?
            .transpose(1, 2)?;
        let k = self.k_proj.forward(&x)?
            .reshape((b, seq, self.num_kv_heads, self.head_dim))?
            .transpose(1, 2)?;
        let v = self.v_proj.forward(&x)?
            .reshape((b, seq, self.num_kv_heads, self.head_dim))?
            .transpose(1, 2)?;

        // Partial RoPE
        let q = apply_partial_rotary_emb(&q, cos, sin, rotary_dim)?;
        let k = apply_partial_rotary_emb(&k, cos, sin, rotary_dim)?;

        // GQA: repeat KV heads
        let k = repeat_kv(k, self.num_heads / self.num_kv_heads)?;
        let v = repeat_kv(v, self.num_heads / self.num_kv_heads)?;

        // Scaled dot-product attention
        let scale = (self.head_dim as f64).sqrt();
        let attn = (q.matmul(&k.transpose(2, 3)?)? * (1.0 / scale))?;
        // Causal mask
        let mask = Tensor::new(
            (0..seq as u32).flat_map(|i| (0..seq as u32).map(move |j| if j <= i { 0f32 } else { f32::NEG_INFINITY })).collect::<Vec<_>>(),
            x.device(),
        )?.reshape((1, 1, seq, seq))?.to_dtype(attn.dtype())?;
        let attn = candle_nn::ops::softmax(&attn.add(&mask)?, 3)?;
        let attn_out = attn.matmul(&v)?
            .transpose(1, 2)?
            .reshape((b, seq, self.num_heads * self.head_dim))?;

        let attn_out = self.o_proj.forward(&attn_out)?;
        let x = residual.add(&attn_out)?;

        // MLP
        let residual = x.clone();
        let x = self.post_attention_layernorm.forward(&x)?;
        let gate = candle_nn::Activation::Silu.forward(&self.mlp_gate_proj.forward(&x)?)?;
        let up = self.mlp_up_proj.forward(&x)?;
        let x = self.mlp_down_proj.forward(&gate.mul(&up)?)?;

        residual.add(&x)
    }
}

fn repeat_kv(x: Tensor, n_rep: usize) -> Result<Tensor> {
    if n_rep == 1 { return Ok(x); }
    let (b, h, s, d) = x.dims4()?;
    x.unsqueeze(2)?
        .expand((b, h, n_rep, s, d))?
        .reshape((b, h * n_rep, s, d))
}

// ── Linear Attention Layer (with causal conv1d) ───────────────────────────

struct LinearAttentionLayer {
    // Linear attention projections (different head counts from full attention)
    q_proj: Linear,
    k_proj: Linear,
    v_proj: Linear,
    o_proj: Linear,
    // Conv1d for the linear attention path
    conv_weight: Tensor,
    conv_bias: Option<Tensor>,
    // Norms
    input_layernorm: RmsNorm,
    post_attention_layernorm: RmsNorm,
    // MLP
    mlp_gate_proj: Linear,
    mlp_up_proj: Linear,
    mlp_down_proj: Linear,
    // Config
    num_key_heads: usize,
    num_value_heads: usize,
    key_head_dim: usize,
    value_head_dim: usize,
}

impl LinearAttentionLayer {
    fn load(vb: VarBuilder, cfg: &Qwen35Config) -> Result<Self> {
        let h = cfg.hidden_size;
        let nkh = cfg.linear_num_key_heads;
        let nvh = cfg.linear_num_value_heads;
        let khd = cfg.linear_key_head_dim;
        let vhd = cfg.linear_value_head_dim;

        // The mixed QKV projection output dim
        let qkv_dim = nkh * khd + nkh * khd + nvh * vhd;

        // Conv weight: applied to the concatenated QKV
        let conv_weight = vb.pp("self_attn").get((qkv_dim, cfg.linear_conv_kernel_dim), "conv1d.weight")?;
        let conv_bias = vb.pp("self_attn").get(qkv_dim, "conv1d.bias").ok();

        Ok(Self {
            q_proj: linear_no_bias(h, nkh * khd, vb.pp("self_attn.q_proj"))?,
            k_proj: linear_no_bias(h, nkh * khd, vb.pp("self_attn.k_proj"))?,
            v_proj: linear_no_bias(h, nvh * vhd, vb.pp("self_attn.v_proj"))?,
            o_proj: linear_no_bias(nvh * vhd, h, vb.pp("self_attn.o_proj"))?,
            conv_weight,
            conv_bias,
            input_layernorm: candle_nn::rms_norm(h, cfg.rms_norm_eps, vb.pp("input_layernorm"))?,
            post_attention_layernorm: candle_nn::rms_norm(h, cfg.rms_norm_eps, vb.pp("post_attention_layernorm"))?,
            mlp_gate_proj: linear_no_bias(h, cfg.intermediate_size, vb.pp("mlp.gate_proj"))?,
            mlp_up_proj: linear_no_bias(h, cfg.intermediate_size, vb.pp("mlp.up_proj"))?,
            mlp_down_proj: linear_no_bias(cfg.intermediate_size, h, vb.pp("mlp.down_proj"))?,
            num_key_heads: nkh,
            num_value_heads: nvh,
            key_head_dim: khd,
            value_head_dim: vhd,
        })
    }

    fn forward(&self, x: &Tensor) -> Result<Tensor> {
        let (b, seq, _h) = x.dims3()?;
        let residual = x.clone();

        let x = self.input_layernorm.forward(x)?;

        // Project Q, K, V
        let q = self.q_proj.forward(&x)?;
        let k = self.k_proj.forward(&x)?;
        let v = self.v_proj.forward(&x)?;

        // Concatenate for conv1d: (batch, seq, qkv_dim) → (batch, qkv_dim, seq)
        let qkv = Tensor::cat(&[&q, &k, &v], 2)?.transpose(1, 2)?;

        // Causal conv1d
        let qkv = causal_conv1d(&qkv, &self.conv_weight, self.conv_bias.as_ref())?;

        // Apply SiLU activation
        let qkv = candle_nn::Activation::Silu.forward(&qkv.transpose(1, 2)?)?;

        // Split back into Q, K, V
        let q_dim = self.num_key_heads * self.key_head_dim;
        let k_dim = self.num_key_heads * self.key_head_dim;
        let v_dim = self.num_value_heads * self.value_head_dim;

        let q = qkv.i((.., .., ..q_dim))?;
        let k = qkv.i((.., .., q_dim..q_dim + k_dim))?;
        let v = qkv.i((.., .., q_dim + k_dim..q_dim + k_dim + v_dim))?;

        // Reshape to heads
        let q = q.reshape((b, seq, self.num_key_heads, self.key_head_dim))?.transpose(1, 2)?;
        let k = k.reshape((b, seq, self.num_key_heads, self.key_head_dim))?.transpose(1, 2)?;
        let v = v.reshape((b, seq, self.num_value_heads, self.value_head_dim))?.transpose(1, 2)?;

        // Linear attention: Q * K^T * V (no softmax, no scaling)
        // This is the key difference from full attention
        let k_rep = repeat_kv(k, self.num_value_heads / self.num_key_heads)?;
        let attn = q.matmul(&k_rep.transpose(2, 3)?)?;

        // Causal mask (still needed for linear attention to prevent looking ahead)
        let mask = Tensor::new(
            (0..seq as u32).flat_map(|i| (0..seq as u32).map(move |j| if j <= i { 1f32 } else { 0f32 })).collect::<Vec<_>>(),
            x.device(),
        )?.reshape((1, 1, seq, seq))?.to_dtype(attn.dtype())?;
        let attn = attn.mul(&mask)?;

        let v_rep = repeat_kv(v, self.num_key_heads / self.num_value_heads.max(1))?;
        let attn_out = attn.matmul(&v_rep)?
            .transpose(1, 2)?
            .reshape((b, seq, self.num_value_heads * self.value_head_dim))?;

        let attn_out = self.o_proj.forward(&attn_out)?;
        let x = residual.add(&attn_out)?;

        // MLP
        let residual = x.clone();
        let x = self.post_attention_layernorm.forward(&x)?;
        let gate = candle_nn::Activation::Silu.forward(&self.mlp_gate_proj.forward(&x)?)?;
        let up = self.mlp_up_proj.forward(&x)?;
        let x = self.mlp_down_proj.forward(&gate.mul(&up)?)?;

        residual.add(&x)
    }
}

// ── Qwen3.5 Model ─────────────────────────────────────────────────────────

enum Qwen35Layer {
    Full(FullAttentionLayer),
    Linear(LinearAttentionLayer),
}

pub struct Qwen35 {
    embed_tokens: Embedding,
    layers: Vec<Qwen35Layer>,
    final_norm: RmsNorm,
    lm_head: Option<Linear>,
    config: Qwen35Config,
    pub context_length: usize,
}

impl Qwen35 {
    pub fn load(vb: VarBuilder, cfg: &Qwen35Config) -> Result<Self> {
        let embed_tokens = Embedding::new(
            vb.pp("model.embed_tokens").get((cfg.vocab_size, cfg.hidden_size), "weight")?,
            cfg.hidden_size,
        );

        let mut layers = Vec::new();
        for i in 0..cfg.num_hidden_layers {
            let layer_vb = vb.pp(format!("model.layers.{i}"));
            match &cfg.layer_types[i] {
                LayerType::FullAttention => {
                    layers.push(Qwen35Layer::Full(FullAttentionLayer::load(layer_vb, cfg)?));
                }
                LayerType::LinearAttention => {
                    layers.push(Qwen35Layer::Linear(LinearAttentionLayer::load(layer_vb, cfg)?));
                }
            }
        }

        let final_norm = candle_nn::rms_norm(cfg.hidden_size, cfg.rms_norm_eps, vb.pp("model.norm"))?;

        let lm_head = if cfg.tie_word_embeddings {
            None
        } else {
            Some(linear_no_bias(cfg.hidden_size, cfg.vocab_size, vb.pp("lm_head"))?)
        };

        Ok(Self {
            embed_tokens,
            layers,
            final_norm,
            lm_head,
            context_length: cfg.max_position_embeddings,
            config: cfg.clone(),
        })
    }

    pub fn forward(&self, input_ids: &Tensor, _start_pos: usize) -> Result<Tensor> {
        let (_b, seq_len) = input_ids.dims2()?;
        let mut x = self.embed_tokens.forward(input_ids)?;

        // Build RoPE cache for full attention layers
        let rotary_dim = (self.config.head_dim as f64 * self.config.partial_rotary_factor) as usize;
        let (cos, sin) = build_rope_cache(
            seq_len,
            rotary_dim,
            self.config.rope_theta,
            x.device(),
            x.dtype(),
        )?;

        for layer in &self.layers {
            x = match layer {
                Qwen35Layer::Full(l) => l.forward(&x, &cos, &sin, rotary_dim)?,
                Qwen35Layer::Linear(l) => l.forward(&x)?,
            };
        }

        x = self.final_norm.forward(&x)?;

        // LM head
        match &self.lm_head {
            Some(head) => head.forward(&x),
            None => {
                // Tied embeddings
                let w = self.embed_tokens.embeddings();
                x.matmul(&w.t()?)
            }
        }
    }
}
