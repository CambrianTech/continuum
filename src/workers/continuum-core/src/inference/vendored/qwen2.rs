//! Qwen2 — Safetensors inference for Qwen2 architecture models.
//!
//! Qwen2 is structurally near-identical to Llama with these differences:
//! - Q, K, V projections have bias (O does not)
//! - RoPE theta, vocab size, and norm epsilon differ (all from config)
//! - Everything else is the same: GQA, RoPE, SwiGLU MLP, RMSNorm
//!
//! This supports both standard and pruned (uniform head count) Qwen2 models.
//! The model reads head counts from config.json — a compacted model just has
//! smaller num_attention_heads/num_key_value_heads in its config.

use candle_core::{DType, Device, IndexOp, Result, Tensor};
use candle_nn::{linear, linear_no_bias, Embedding, Linear, Module, RmsNorm, VarBuilder};

/// Qwen2 model configuration, read from config.json.
#[derive(Debug, Clone)]
pub struct Qwen2Config {
    pub vocab_size: usize,
    pub hidden_size: usize,
    pub intermediate_size: usize,
    pub num_hidden_layers: usize,
    pub num_attention_heads: usize,
    pub num_key_value_heads: usize,
    pub head_dim: usize,
    pub max_position_embeddings: usize,
    pub rms_norm_eps: f64,
    pub rope_theta: f32,
    pub tie_word_embeddings: bool,
}

impl Qwen2Config {
    /// Parse from a serde_json::Value (the raw config.json).
    pub fn from_json(v: &serde_json::Value) -> std::result::Result<Self, String> {
        let hidden_size = v["hidden_size"].as_u64().ok_or("missing hidden_size")? as usize;
        let num_attention_heads = v["num_attention_heads"]
            .as_u64()
            .ok_or("missing num_attention_heads")? as usize;
        let head_dim = v
            .get("head_dim")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(hidden_size / num_attention_heads);

        Ok(Self {
            vocab_size: v["vocab_size"].as_u64().ok_or("missing vocab_size")? as usize,
            hidden_size,
            intermediate_size: v["intermediate_size"]
                .as_u64()
                .ok_or("missing intermediate_size")? as usize,
            num_hidden_layers: v["num_hidden_layers"]
                .as_u64()
                .ok_or("missing num_hidden_layers")? as usize,
            num_attention_heads,
            num_key_value_heads: v["num_key_value_heads"]
                .as_u64()
                .ok_or("missing num_key_value_heads")? as usize,
            head_dim,
            max_position_embeddings: v["max_position_embeddings"]
                .as_u64()
                .ok_or("missing max_position_embeddings")?
                as usize,
            rms_norm_eps: v
                .get("rms_norm_eps")
                .and_then(|v| v.as_f64())
                .unwrap_or(1e-6),
            rope_theta: v
                .get("rope_theta")
                .and_then(|v| v.as_f64())
                .unwrap_or(1_000_000.0) as f32,
            tie_word_embeddings: v
                .get("tie_word_embeddings")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
        })
    }
}

/// Qwen2 attention — Q/K have bias, V/O do not.
struct Qwen2Attention {
    q_proj: Linear,
    k_proj: Linear,
    v_proj: Linear,
    o_proj: Linear,
    n_head: usize,
    n_kv_head: usize,
    head_dim: usize,
    span: tracing::Span,
}

impl Qwen2Attention {
    fn load(vb: VarBuilder, config: &Qwen2Config) -> Result<Self> {
        let hidden = config.hidden_size;
        let n_head = config.num_attention_heads;
        let n_kv = config.num_key_value_heads;
        let hd = config.head_dim;

        // Q, K, V have bias. O does not. (Qwen2 vs Llama difference)
        let q_proj = linear(hidden, n_head * hd, vb.pp("q_proj"))?;
        let k_proj = linear(hidden, n_kv * hd, vb.pp("k_proj"))?;
        let v_proj = linear(hidden, n_kv * hd, vb.pp("v_proj"))?;
        let o_proj = linear_no_bias(n_head * hd, hidden, vb.pp("o_proj"))?;

        Ok(Self {
            q_proj,
            k_proj,
            v_proj,
            o_proj,
            n_head,
            n_kv_head: n_kv,
            head_dim: hd,
            span: tracing::span!(tracing::Level::TRACE, "qwen2-attn"),
        })
    }

    fn forward(
        &self,
        x: &Tensor,
        index_pos: usize,
        cache: &mut Option<(Tensor, Tensor)>,
        cos: &Tensor,
        sin: &Tensor,
    ) -> Result<Tensor> {
        let _enter = self.span.enter();
        let (b_sz, seq_len, _) = x.dims3()?;

        let q = self.q_proj.forward(x)?;
        let k = self.k_proj.forward(x)?;
        let v = self.v_proj.forward(x)?;

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

        // RoPE
        let q = apply_rotary_emb(&q, index_pos, cos, sin)?;
        let k = apply_rotary_emb(&k, index_pos, cos, sin)?;

        // KV cache
        let (k, v) = match cache {
            None => (k, v),
            Some((k_cache, v_cache)) => {
                if index_pos == 0 {
                    (k, v)
                } else {
                    let k = Tensor::cat(&[k_cache.as_ref(), &k], 2)?;
                    let v = Tensor::cat(&[v_cache.as_ref(), &v], 2)?;
                    (k, v)
                }
            }
        };
        *cache = Some((k.clone(), v.clone()));

        // Attention — Metal SDPA for single-token, manual for batch prefill.
        // Metal SDPA is_causal=true corrupts KV cache with real quantized weights.
        let gqa_ratio = self.n_head / self.n_kv_head;
        let y = if x.device().is_metal() && seq_len == 1 {
            candle_nn::ops::sdpa(
                &q,
                &k,
                &v,
                None,
                false,
                1. / (self.head_dim as f32).sqrt(),
                1.,
            )?
        } else {
            let k = candle_transformers::utils::repeat_kv(k, gqa_ratio)?;
            let v = candle_transformers::utils::repeat_kv(v, gqa_ratio)?;
            let att = (q.matmul(&k.t()?)? / (self.head_dim as f64).sqrt())?;
            let mask = causal_mask(seq_len, index_pos, x.device())?;
            let att = att.broadcast_add(&mask)?;
            let att = candle_nn::ops::softmax_last_dim(&att)?;
            att.matmul(&v.contiguous()?)?
        };

        let y = y
            .transpose(1, 2)?
            .reshape(&[b_sz, seq_len, self.n_head * self.head_dim])?;
        self.o_proj.forward(&y)
    }
}

/// SwiGLU MLP (identical to Llama).
struct Qwen2Mlp {
    gate_proj: Linear,
    up_proj: Linear,
    down_proj: Linear,
}

impl Qwen2Mlp {
    fn load(vb: VarBuilder, hidden: usize, intermediate: usize) -> Result<Self> {
        let gate_proj = linear_no_bias(hidden, intermediate, vb.pp("gate_proj"))?;
        let up_proj = linear_no_bias(hidden, intermediate, vb.pp("up_proj"))?;
        let down_proj = linear_no_bias(intermediate, hidden, vb.pp("down_proj"))?;
        Ok(Self {
            gate_proj,
            up_proj,
            down_proj,
        })
    }

    fn forward(&self, x: &Tensor) -> Result<Tensor> {
        let gate = candle_nn::ops::silu(&self.gate_proj.forward(x)?)?;
        let up = self.up_proj.forward(x)?;
        self.down_proj.forward(&(gate * up)?)
    }
}

/// Transformer layer.
struct Qwen2Layer {
    self_attn: Qwen2Attention,
    mlp: Qwen2Mlp,
    input_layernorm: RmsNorm,
    post_attention_layernorm: RmsNorm,
}

impl Qwen2Layer {
    fn load(vb: VarBuilder, config: &Qwen2Config) -> Result<Self> {
        let self_attn = Qwen2Attention::load(vb.pp("self_attn"), config)?;
        let mlp = Qwen2Mlp::load(vb.pp("mlp"), config.hidden_size, config.intermediate_size)?;
        let input_layernorm = candle_nn::rms_norm(
            config.hidden_size,
            config.rms_norm_eps,
            vb.pp("input_layernorm"),
        )?;
        let post_attention_layernorm = candle_nn::rms_norm(
            config.hidden_size,
            config.rms_norm_eps,
            vb.pp("post_attention_layernorm"),
        )?;
        Ok(Self {
            self_attn,
            mlp,
            input_layernorm,
            post_attention_layernorm,
        })
    }

    fn forward(
        &mut self,
        x: &Tensor,
        index_pos: usize,
        cache: &mut Option<(Tensor, Tensor)>,
        cos: &Tensor,
        sin: &Tensor,
    ) -> Result<Tensor> {
        let residual = x.clone();
        let x = self.input_layernorm.forward(x)?;
        let x = self.self_attn.forward(&x, index_pos, cache, cos, sin)?;
        let x = (residual + x)?;

        let residual = x.clone();
        let x = self.post_attention_layernorm.forward(&x)?;
        let x = self.mlp.forward(&x)?;
        residual + x
    }
}

/// Qwen2 model — supports standard and uniformly-pruned models.
pub struct Qwen2 {
    embed_tokens: Embedding,
    layers: Vec<Qwen2Layer>,
    norm: RmsNorm,
    lm_head: Linear,
    caches: Vec<Option<(Tensor, Tensor)>>,
    cos: Tensor,
    sin: Tensor,
    pub context_length: usize,
}

impl Qwen2 {
    /// Load from safetensors with config.
    pub fn load(vb: VarBuilder, config: &Qwen2Config) -> Result<Self> {
        let embed_tokens = candle_nn::embedding(
            config.vocab_size,
            config.hidden_size,
            vb.pp("model.embed_tokens"),
        )?;

        let (cos, sin) = precompute_freqs_cis(
            config.head_dim,
            config.rope_theta,
            config.max_position_embeddings,
            vb.device(),
        )?;

        let mut layers = Vec::with_capacity(config.num_hidden_layers);
        for i in 0..config.num_hidden_layers {
            let layer = Qwen2Layer::load(vb.pp(format!("model.layers.{i}")), config)?;
            layers.push(layer);
        }

        let norm =
            candle_nn::rms_norm(config.hidden_size, config.rms_norm_eps, vb.pp("model.norm"))?;

        let lm_head = if config.tie_word_embeddings {
            // Weight-tied: lm_head shares embed_tokens weights
            Linear::new(embed_tokens.embeddings().clone(), None)
        } else {
            linear_no_bias(config.hidden_size, config.vocab_size, vb.pp("lm_head"))?
        };

        let caches = vec![None; config.num_hidden_layers];

        Ok(Self {
            embed_tokens,
            layers,
            norm,
            lm_head,
            caches,
            cos,
            sin,
            context_length: config.max_position_embeddings,
        })
    }

    /// Forward pass: tokens -> logits.
    pub fn forward(&mut self, input_ids: &Tensor, index_pos: usize) -> Result<Tensor> {
        let (_b_sz, seq_len) = input_ids.dims2()?;
        let mut x = self.embed_tokens.forward(input_ids)?;

        for (i, layer) in self.layers.iter_mut().enumerate() {
            x = layer.forward(&x, index_pos, &mut self.caches[i], &self.cos, &self.sin)?;
        }

        let x = self.norm.forward(&x)?;
        let x = x.i((.., seq_len - 1, ..))?;
        self.lm_head.forward(&x)
    }

    /// Clear all KV caches.
    pub fn clear_cache(&mut self) {
        self.caches = vec![None; self.layers.len()];
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn apply_rotary_emb(x: &Tensor, index_pos: usize, cos: &Tensor, sin: &Tensor) -> Result<Tensor> {
    let (_b_sz, _n_head, seq_len, _n_embd) = x.dims4()?;
    let cos = cos.narrow(0, index_pos, seq_len)?;
    let sin = sin.narrow(0, index_pos, seq_len)?;
    candle_nn::rotary_emb::rope_i(&x.contiguous()?, &cos, &sin)
}

fn causal_mask(seq_len: usize, past_len: usize, device: &Device) -> Result<Tensor> {
    let mask: Vec<f32> = (0..seq_len)
        .flat_map(|i| {
            (0..seq_len + past_len).map(move |j| {
                if j > i + past_len {
                    f32::NEG_INFINITY
                } else {
                    0.0
                }
            })
        })
        .collect();
    Tensor::from_vec(mask, (1, 1, seq_len, seq_len + past_len), device)
}

fn precompute_freqs_cis(
    head_dim: usize,
    rope_theta: f32,
    context_length: usize,
    device: &Device,
) -> Result<(Tensor, Tensor)> {
    let theta: Vec<f32> = (0..head_dim)
        .step_by(2)
        .map(|i| 1.0 / rope_theta.powf(i as f32 / head_dim as f32))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_qwen2_config_from_json() {
        let json = serde_json::json!({
            "vocab_size": 152064,
            "hidden_size": 5120,
            "intermediate_size": 27648,
            "num_hidden_layers": 64,
            "num_attention_heads": 40,
            "num_key_value_heads": 8,
            "max_position_embeddings": 32768,
            "rms_norm_eps": 1e-6,
            "rope_theta": 1000000.0,
            "tie_word_embeddings": false
        });
        let config = Qwen2Config::from_json(&json).unwrap();
        assert_eq!(config.num_attention_heads, 40);
        assert_eq!(config.num_key_value_heads, 8);
        assert_eq!(config.head_dim, 128); // 5120 / 40
        assert_eq!(config.vocab_size, 152064);
    }

    #[test]
    fn test_qwen2_config_explicit_head_dim() {
        // Compacted model: 25 heads, hidden_size still 5120, head_dim=128 explicit
        let json = serde_json::json!({
            "vocab_size": 152064,
            "hidden_size": 5120,
            "intermediate_size": 27648,
            "num_hidden_layers": 64,
            "num_attention_heads": 25,
            "num_key_value_heads": 5,
            "head_dim": 128,
            "max_position_embeddings": 32768,
            "rms_norm_eps": 1e-6,
            "rope_theta": 1000000.0,
            "tie_word_embeddings": false
        });
        let config = Qwen2Config::from_json(&json).unwrap();
        assert_eq!(config.num_attention_heads, 25);
        assert_eq!(config.num_key_value_heads, 5);
        assert_eq!(config.head_dim, 128); // explicit, not 5120/25=204
    }

    #[test]
    fn test_precompute_freqs_cis() {
        let (cos, sin) = precompute_freqs_cis(128, 1000000.0, 256, &Device::Cpu).unwrap();
        assert_eq!(cos.dims(), &[256, 64]); // context_length x head_dim/2
        assert_eq!(sin.dims(), &[256, 64]);
    }
}
