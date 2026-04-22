//! CompactLlama — Llama variant with per-layer variable head counts.
//!
//! After plasticity compaction, each layer can have a different number of
//! attention heads. This module provides loading and inference for compacted
//! models using the HeadTopology manifest.
//!
//! Key difference from standard Llama:
//! - Each layer reads its own `n_head` and `n_kv_head` from the topology
//! - Attention weight dimensions vary per layer (smaller layers = fewer rows/cols)
//! - MLP, embeddings, layer norms, and lm_head are unchanged
//!
//! This is a modified version of candle-transformers' Llama that reads
//! per-layer dimensions from HeadTopology instead of a uniform config.

use std::path::{Path, PathBuf};

use candle_core::{DType, Device, IndexOp, Result, Tensor};
use candle_nn::{linear_no_bias, Embedding, Linear, Module, RmsNorm, VarBuilder};
use candle_transformers::models::llama::Config as LlamaConfig;

use crate::modules::plasticity::types::HeadTopology;

/// Per-layer attention module with variable head count.
#[allow(dead_code)]
struct CompactAttention {
    q_proj: Linear,
    k_proj: Linear,
    v_proj: Linear,
    o_proj: Linear,
    n_head: usize,
    n_kv_head: usize,
    head_dim: usize,
    hidden_size: usize,
    span: tracing::Span,
}

impl CompactAttention {
    fn load(
        vb: VarBuilder,
        n_head: usize,
        n_kv_head: usize,
        head_dim: usize,
        hidden_size: usize,
    ) -> Result<Self> {
        // Compacted weight dimensions:
        // q_proj: [n_head * head_dim, hidden_size]
        // k_proj: [n_kv_head * head_dim, hidden_size]
        // v_proj: [n_kv_head * head_dim, hidden_size]
        // o_proj: [hidden_size, n_head * head_dim]
        let q_proj = linear_no_bias(hidden_size, n_head * head_dim, vb.pp("q_proj"))?;
        let k_proj = linear_no_bias(hidden_size, n_kv_head * head_dim, vb.pp("k_proj"))?;
        let v_proj = linear_no_bias(hidden_size, n_kv_head * head_dim, vb.pp("v_proj"))?;
        let o_proj = linear_no_bias(n_head * head_dim, hidden_size, vb.pp("o_proj"))?;

        Ok(Self {
            q_proj,
            k_proj,
            v_proj,
            o_proj,
            n_head,
            n_kv_head,
            head_dim,
            hidden_size,
            span: tracing::span!(tracing::Level::TRACE, "compact-attn"),
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
        let (b_sz, seq_len, _hidden) = x.dims3()?;

        let q = self.q_proj.forward(x)?;
        let k = self.k_proj.forward(x)?;
        let v = self.v_proj.forward(x)?;

        // Reshape to per-head
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

        // Rotary embeddings
        let q = self.apply_rotary_emb(&q, index_pos, cos, sin)?;
        let k = self.apply_rotary_emb(&k, index_pos, cos, sin)?;

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
            let mask = self.causal_mask(seq_len, index_pos, x.device())?;
            let att = att.broadcast_add(&mask)?;
            let att = candle_nn::ops::softmax_last_dim(&att)?;
            att.matmul(&v.contiguous()?)?
        };

        // Reshape back to [batch, seq, hidden_for_this_layer]
        let y = y
            .transpose(1, 2)?
            .reshape(&[b_sz, seq_len, self.n_head * self.head_dim])?;
        self.o_proj.forward(&y)
    }

    fn apply_rotary_emb(
        &self,
        x: &Tensor,
        index_pos: usize,
        cos: &Tensor,
        sin: &Tensor,
    ) -> Result<Tensor> {
        let (_b_sz, _n_head, seq_len, _n_embd) = x.dims4()?;
        let cos = cos.narrow(0, index_pos, seq_len)?;
        let sin = sin.narrow(0, index_pos, seq_len)?;
        candle_nn::rotary_emb::rope_i(&x.contiguous()?, &cos, &sin)
    }

    fn causal_mask(&self, seq_len: usize, past_len: usize, device: &Device) -> Result<Tensor> {
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
}

/// MLP block (unchanged from standard Llama — not affected by head compaction).
struct CompactMlp {
    gate_proj: Linear,
    up_proj: Linear,
    down_proj: Linear,
}

impl CompactMlp {
    fn load(vb: VarBuilder, hidden_size: usize, intermediate_size: usize) -> Result<Self> {
        let gate_proj = linear_no_bias(hidden_size, intermediate_size, vb.pp("gate_proj"))?;
        let up_proj = linear_no_bias(hidden_size, intermediate_size, vb.pp("up_proj"))?;
        let down_proj = linear_no_bias(intermediate_size, hidden_size, vb.pp("down_proj"))?;
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

/// A single transformer layer with potentially compacted attention.
struct CompactLayer {
    self_attn: CompactAttention,
    mlp: CompactMlp,
    input_layernorm: RmsNorm,
    post_attention_layernorm: RmsNorm,
}

impl CompactLayer {
    fn load(
        vb: VarBuilder,
        n_head: usize,
        n_kv_head: usize,
        head_dim: usize,
        hidden_size: usize,
        intermediate_size: usize,
        rms_norm_eps: f64,
    ) -> Result<Self> {
        let self_attn =
            CompactAttention::load(vb.pp("self_attn"), n_head, n_kv_head, head_dim, hidden_size)?;
        let mlp = CompactMlp::load(vb.pp("mlp"), hidden_size, intermediate_size)?;
        let input_layernorm =
            candle_nn::rms_norm(hidden_size, rms_norm_eps, vb.pp("input_layernorm"))?;
        let post_attention_layernorm =
            candle_nn::rms_norm(hidden_size, rms_norm_eps, vb.pp("post_attention_layernorm"))?;

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

/// CompactLlama — Llama with per-layer variable head counts from HeadTopology.
#[allow(dead_code)]
pub struct CompactLlama {
    embed_tokens: Embedding,
    layers: Vec<CompactLayer>,
    norm: RmsNorm,
    lm_head: Linear,
    caches: Vec<Option<(Tensor, Tensor)>>,
    cos: Tensor,
    sin: Tensor,
    hidden_size: usize,
    pub context_length: usize,
}

impl CompactLlama {
    /// Load a compacted model from safetensors + topology.
    ///
    /// The topology provides per-layer head counts. Weight tensors in the
    /// safetensors file must already be sliced to match (by the compactor).
    pub fn load(vb: VarBuilder, config: &LlamaConfig, topology: &HeadTopology) -> Result<Self> {
        let hidden_size = config.hidden_size;
        let rms_norm_eps = config.rms_norm_eps;
        let context_length = config.max_position_embeddings;
        let intermediate_size = config.intermediate_size;

        let embed_tokens =
            candle_nn::embedding(config.vocab_size, hidden_size, vb.pp("model.embed_tokens"))?;

        // Rotary embeddings use the original head_dim (unchanged by compaction)
        let head_dim = topology.head_dim;
        let rope_theta = config.rope_theta as f32;
        let (cos, sin) = precompute_freqs_cis(head_dim, rope_theta, context_length, vb.device())?;

        let mut layers = Vec::with_capacity(topology.layers.len());
        for layer_topo in &topology.layers {
            let layer_vb = vb.pp(format!("model.layers.{}", layer_topo.layer_index));
            let layer = CompactLayer::load(
                layer_vb,
                layer_topo.num_heads,
                layer_topo.num_kv_heads,
                head_dim,
                hidden_size,
                intermediate_size,
                rms_norm_eps,
            )?;
            layers.push(layer);
        }

        let norm = candle_nn::rms_norm(hidden_size, rms_norm_eps, vb.pp("model.norm"))?;
        let lm_head = linear_no_bias(hidden_size, config.vocab_size, vb.pp("lm_head"))?;

        let caches = vec![None; layers.len()];

        Ok(Self {
            embed_tokens,
            layers,
            norm,
            lm_head,
            caches,
            cos,
            sin,
            hidden_size,
            context_length,
        })
    }

    /// Forward pass: tokens → logits
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

/// Precompute rotary embedding frequencies.
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

/// Detect if a model directory has a compacted topology file.
pub fn detect_topology(model_dir: &Path) -> Option<PathBuf> {
    let topology_path = model_dir.join("head_topology.json");
    if topology_path.exists() {
        Some(topology_path)
    } else {
        // Also check for .topology.json extension from compactor
        let alt_path = model_dir.join("compacted_model.topology.json");
        if alt_path.exists() {
            Some(alt_path)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_topology_none() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(detect_topology(tmp.path()).is_none());
    }

    #[test]
    fn test_detect_topology_found() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("head_topology.json"), "{}").unwrap();
        assert!(detect_topology(tmp.path()).is_some());
    }

    #[test]
    fn test_detect_topology_alt_path() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("compacted_model.topology.json"), "{}").unwrap();
        assert!(detect_topology(tmp.path()).is_some());
    }

    #[test]
    fn test_precompute_freqs_cis() {
        let (cos, sin) = precompute_freqs_cis(64, 10000.0, 128, &Device::Cpu).unwrap();
        assert_eq!(cos.dims(), &[128, 32]); // context_length x head_dim/2
        assert_eq!(sin.dims(), &[128, 32]);
    }
}
