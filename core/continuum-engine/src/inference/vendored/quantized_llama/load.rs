//! Building the model from a GGUF file.

use std::collections::HashMap;

use candle_core::quantized::gguf_file;
use candle_core::{DType, Device, Result, Tensor};

use super::*;
use super::weights::{DeviceEmbedding, Mlp, MlpOrMoe, QMatMul, RmsNorm};

/// Default fallback if GGUF metadata doesn't contain context_length.
const DEFAULT_CONTEXT_LENGTH: usize = 4096;

fn precomput_freqs_cis(
    head_dim: usize,
    freq_base: f32,
    context_length: usize,
    device: &Device,
) -> Result<(Tensor, Tensor)> {
    let theta: Vec<_> = (0..head_dim)
        .step_by(2)
        .map(|i| 1f32 / freq_base.powf(i as f32 / head_dim as f32))
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

impl ModelWeights {
    pub fn from_gguf<R: std::io::Seek + std::io::Read>(
        ct: gguf_file::Content,
        reader: &mut R,
        device: &Device,
    ) -> Result<Self> {
        // Determine architecture prefix (llama, qwen2, etc.) from metadata.
        // GGUF stores metadata keys as "{arch}.{param}", e.g. "qwen2.block_count".
        let arch = ct
            .metadata
            .get("general.architecture")
            .and_then(|v| v.to_string().ok())
            .cloned()
            .unwrap_or_else(|| "llama".to_string());

        let md_get = |s: &str| match ct.metadata.get(s) {
            None => candle_core::bail!("cannot find {s} in metadata"),
            Some(v) => Ok(v),
        };

        // Architecture-aware metadata key lookup
        let arch_key = |param: &str| format!("{arch}.{param}");

        let context_length = md_get(&arch_key("context_length"))
            .and_then(|v| v.to_u32())
            .map(|v| v as usize)
            .unwrap_or(DEFAULT_CONTEXT_LENGTH);

        let n_expert = md_get(&arch_key("expert_count"))
            .and_then(|v| v.to_u32())
            .unwrap_or(0) as usize;
        let n_expert_used = md_get(&arch_key("expert_used_count"))
            .and_then(|v| v.to_u32())
            .unwrap_or(0) as usize;
        let head_count = md_get(&arch_key("attention.head_count"))?.to_u32()? as usize;
        let head_count_kv = md_get(&arch_key("attention.head_count_kv"))?.to_u32()? as usize;
        let block_count = md_get(&arch_key("block_count"))?.to_u32()? as usize;
        let embedding_length = md_get(&arch_key("embedding_length"))?.to_u32()? as usize;
        // Head dimension: try explicit metadata first, then derive from K weight shape.
        // NEVER use embedding_length / head_count — it gives wrong answers for compacted
        // models where head_count was reduced (e.g. 5120/25=204 instead of 128).
        let head_dim = md_get(&arch_key("attention.key_length"))
            .and_then(|v| v.to_u32())
            .or_else(|_| md_get(&arch_key("attention.value_length")).and_then(|v| v.to_u32()))
            .or_else(|_| md_get(&arch_key("rope.dimension_count")).and_then(|v| v.to_u32()))
            .map(|v| v as usize)
            .unwrap_or_else(|_| {
                // Last resort: derive from K projection weight shape.
                // K weight is [embedding_length, kv_heads * head_dim], so:
                // head_dim = K_weight_dim1 / kv_heads
                // But we don't have the tensor yet. Use embedding_length / ORIGINAL head_count
                // approximation: for standard models this is correct, for compacted we need metadata.
                // Qwen2 always uses 128.
                if arch == "qwen2" {
                    128
                } else {
                    embedding_length / head_count
                }
            });
        let rope_dim = head_dim;
        let rms_norm_eps = md_get(&arch_key("attention.layer_norm_rms_epsilon"))?.to_f32()? as f64;

        let rope_freq_base = md_get(&arch_key("rope.freq_base"))
            .and_then(|m| m.to_f32())
            .unwrap_or(10000f32);

        // RoPE convention depends on model architecture (matching llama.cpp).
        // NEOX (non-interleaved): pairs (i, i+d/2) — Qwen, Qwen2, Falcon, Phi, BERT, etc.
        // NORM (interleaved): pairs (2i, 2i+1) — Llama, Mistral, DeepSeek, etc.
        let rope_is_neox = matches!(
            arch.as_str(),
            "qwen"
                | "qwen2"
                | "qwen2moe"
                | "qwen3"
                | "qwen3moe"
                | "falcon"
                | "phi"
                | "phi2"
                | "phi3"
                | "stablelm"
                | "bert"
                | "nomic-bert"
                | "plamo"
                | "grok"
                | "dbrx"
                | "olmo2"
                | "olmoe"
                | "codeshell"
                | "starcoder2"
        );

        {
            let log = crate::runtime::logger("candle");
            log.info(&format!(
                "RoPE config: arch={}, rope_is_neox={}, rope_dim={}, freq_base={}",
                arch, rope_is_neox, rope_dim, rope_freq_base
            ));
        }

        let (cos, sin) = precomput_freqs_cis(rope_dim, rope_freq_base, context_length, device)?;
        let neg_inf = Tensor::new(f32::NEG_INFINITY, device)?;

        // Load embedding directly to CPU — bypasses Metal buffer pool entirely.
        let tok_embeddings =
            DeviceEmbedding::from_gguf(&ct, reader, "token_embd.weight", embedding_length, device)?;
        let norm = RmsNorm::from_qtensor(
            ct.tensor(reader, "output_norm.weight", device)?,
            rms_norm_eps,
        )?;
        // Output head: use dedicated output.weight if present, otherwise re-read
        // token_embd.weight (tied embeddings).
        let output = match ct.tensor(reader, "output.weight", device) {
            Ok(tensor) => tensor,
            Err(_) => ct.tensor(reader, "token_embd.weight", device)?,
        };
        let mut layers = Vec::with_capacity(block_count);
        for layer_idx in 0..block_count {
            let prefix = format!("blk.{layer_idx}");
            let attention_wq = ct.tensor(reader, &format!("{prefix}.attn_q.weight"), device)?;
            let attention_wk = ct.tensor(reader, &format!("{prefix}.attn_k.weight"), device)?;
            let attention_wv = ct.tensor(reader, &format!("{prefix}.attn_v.weight"), device)?;
            let attention_wo =
                ct.tensor(reader, &format!("{prefix}.attn_output.weight"), device)?;

            // Optional attention biases (Qwen2 has Q/K/V bias; Llama does not)
            let attention_bq = ct
                .tensor(reader, &format!("{prefix}.attn_q.bias"), device)
                .ok()
                .map(|qt| qt.dequantize(device))
                .transpose()?;
            let attention_bk = ct
                .tensor(reader, &format!("{prefix}.attn_k.bias"), device)
                .ok()
                .map(|qt| qt.dequantize(device))
                .transpose()?;
            let attention_bv = ct
                .tensor(reader, &format!("{prefix}.attn_v.bias"), device)
                .ok()
                .map(|qt| qt.dequantize(device))
                .transpose()?;

            // Log shapes for first layer to verify compacted model dimensions
            if layer_idx == 0 {
                let log = crate::runtime::logger("candle");
                log.info(&format!(
                    "Layer 0 weight shapes: Q={:?} K={:?} V={:?} O={:?}",
                    attention_wq.shape(),
                    attention_wk.shape(),
                    attention_wv.shape(),
                    attention_wo.shape()
                ));
                if let Some(ref bq) = attention_bq {
                    log.info(&format!(
                        "Layer 0 bias shapes: Q={:?} K={:?} V={:?}",
                        bq.dims(),
                        attention_bk.as_ref().map(|t| t.dims()),
                        attention_bv.as_ref().map(|t| t.dims())
                    ));
                }
                log.info(&format!(
                    "Layer 0 config: n_head={}, n_kv_head={}, head_dim={}, rope_dim={}",
                    head_count, head_count_kv, head_dim, rope_dim
                ));
            }

            let mlp_or_moe = if n_expert <= 1 {
                let feed_forward_w1 =
                    ct.tensor(reader, &format!("{prefix}.ffn_gate.weight"), device)?;
                let feed_forward_w2 =
                    ct.tensor(reader, &format!("{prefix}.ffn_down.weight"), device)?;
                let feed_forward_w3 =
                    ct.tensor(reader, &format!("{prefix}.ffn_up.weight"), device)?;
                MlpOrMoe::Mlp(Mlp {
                    feed_forward_w1: QMatMul::from_qtensor(feed_forward_w1)?,
                    feed_forward_w2: QMatMul::from_qtensor(feed_forward_w2)?,
                    feed_forward_w3: QMatMul::from_qtensor(feed_forward_w3)?,
                })
            } else {
                let feed_forward_gate_inp =
                    ct.tensor(reader, &format!("{prefix}.ffn_gate_inp.weight"), device)?;
                let mut experts = Vec::with_capacity(n_expert);
                for i in 0..n_expert {
                    let feed_forward_w1 =
                        ct.tensor(reader, &format!("{prefix}.ffn_gate.{i}.weight"), device)?;
                    let feed_forward_w2 =
                        ct.tensor(reader, &format!("{prefix}.ffn_down.{i}.weight"), device)?;
                    let feed_forward_w3 =
                        ct.tensor(reader, &format!("{prefix}.ffn_up.{i}.weight"), device)?;
                    experts.push(Mlp {
                        feed_forward_w1: QMatMul::from_qtensor(feed_forward_w1)?,
                        feed_forward_w2: QMatMul::from_qtensor(feed_forward_w2)?,
                        feed_forward_w3: QMatMul::from_qtensor(feed_forward_w3)?,
                    })
                }
                MlpOrMoe::MoE {
                    n_expert_used,
                    feed_forward_gate_inp: QMatMul::from_qtensor(feed_forward_gate_inp)?,
                    experts,
                }
            };
            let attention_norm =
                ct.tensor(reader, &format!("{prefix}.attn_norm.weight"), device)?;
            let ffn_norm = ct.tensor(reader, &format!("{prefix}.ffn_norm.weight"), device)?;
            let span_attn = tracing::span!(tracing::Level::TRACE, "attn");
            let span_rot = tracing::span!(tracing::Level::TRACE, "attn-rot");
            let span_mlp = tracing::span!(tracing::Level::TRACE, "attn-mlp");
            layers.push(LayerWeights {
                attention_wq: QMatMul::from_qtensor(attention_wq)?,
                attention_wk: QMatMul::from_qtensor(attention_wk)?,
                attention_wv: QMatMul::from_qtensor(attention_wv)?,
                attention_wo: QMatMul::from_qtensor(attention_wo)?,
                attention_bq,
                attention_bk,
                attention_bv,
                attention_norm: RmsNorm::from_qtensor(attention_norm, rms_norm_eps)?,
                mlp_or_moe,
                ffn_norm: RmsNorm::from_qtensor(ffn_norm, rms_norm_eps)?,
                n_head: head_count,
                n_kv_head: head_count_kv,
                // Use rope_dim as head_dim: for standard models rope_dim == head_dim,
                // and for compacted models where embedding_length / head_count is non-integer,
                // rope_dim (from metadata or defaulting to embedding_length/head_count) is correct.
                head_dim: rope_dim,
                rope_is_neox,
                cos: cos.clone(),
                sin: sin.clone(),
                neg_inf: neg_inf.clone(),
                kv_cache: None,
                span_attn,
                span_rot,
                span_mlp,
            })
        }
        let span = tracing::span!(tracing::Level::TRACE, "model");
        let span_output = tracing::span!(tracing::Level::TRACE, "output");
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

}
