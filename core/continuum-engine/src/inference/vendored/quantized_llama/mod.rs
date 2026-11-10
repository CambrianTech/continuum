//! Vendored from candle-transformers 0.8.4 `models/quantized_llama.rs`.
//!
//! Changes from upstream:
//!   1. Replaced hardcoded `MAX_SEQ_LEN = 4096` with `context_length` read from
//!      GGUF metadata (`llama.context_length`). The upstream Llama implementation
//!      forgot to read this, while Qwen2 and Phi3 in the same library do.
//!   2. Exposed `context_length` as a public field on `ModelWeights` so callers
//!      can query the model's true context limit.
//!   3. Changed crate-internal imports (`candle::`, `crate::`) to external crate
//!      imports (`candle_core::`, `candle_transformers::`) since this is vendored
//!      outside of candle-transformers.
//!   4. **CRITICAL FIX**: RoPE convention dispatch based on architecture.
//!      Upstream always uses `rope_i` (interleaved), which is WRONG for NEOX-style
//!      models (Qwen2, Falcon, Phi, etc.). These need `rope` (non-interleaved).
//!      Without this fix, attention patterns are systematically corrupted, causing
//!      +11.7 logit inflation on special tokens vs llama.cpp reference.
//!      See: https://github.com/huggingface/candle/issues/3410
//!   5. Architecture-aware model loading: reads Qwen2 attention biases, handles
//!      compacted (head-pruned) model dimensions, MoE routing.
//!
//! Diagnostic env vars:
//!   CANDLE_MAX_LAYERS=N    — Run only N transformer layers (for A/B testing)
//!   CANDLE_DEBUG_TOKENS=1  — Log top-10 logits after prefill
//!   CANDLE_DUMP_LAYERS=1   — Dump per-layer hidden states
//!   CANDLE_DUMP_LAYER0=1   — Dump layer 0 intermediates (attn_norm, attn_out, etc.)
//!   CANDLE_DUMP_ATTN=1     — Dump attention internals (Q/K/V after bias+RoPE, kqv_out)
//!   CANDLE_DUMP_HIDDEN=1   — Dump final hidden state and logits to /tmp/*.bin

use std::collections::HashMap;

use candle_core::{Device, IndexOp, Result, Tensor};
use candle_nn::Module;

mod attention;
mod load;
mod lora;
mod weights;

use attention::LayerWeights;
use weights::{DeviceEmbedding, QMatMul, RmsNorm};

#[derive(Debug, Clone)]
pub struct ModelWeights {
    tok_embeddings: DeviceEmbedding,
    layers: Vec<LayerWeights>,
    norm: RmsNorm,
    output: QMatMul,
    masks: HashMap<usize, Tensor>,
    span: tracing::Span,
    span_output: tracing::Span,
    /// Context length read from GGUF metadata. This is the model's true limit.
    pub context_length: usize,
}

impl ModelWeights {
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

    /// Reset KV cache for all layers without reloading weights.
    pub fn clear_kv_cache(&mut self) {
        for layer in &mut self.layers {
            layer.kv_cache = None;
        }
    }

    pub fn forward(&mut self, x: &Tensor, index_pos: usize) -> Result<Tensor> {
        self.forward_inner(x, index_pos, self.layers.len())
    }

    fn forward_inner(&mut self, x: &Tensor, index_pos: usize, max_layers: usize) -> Result<Tensor> {
        // Debug: CANDLE_MAX_LAYERS env var limits layer count for memory profiling
        let effective_max = std::env::var("CANDLE_MAX_LAYERS")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(max_layers)
            .min(max_layers);

        let (_b_sz, seq_len) = x.dims2()?;
        let mask = if seq_len == 1 {
            None
        } else {
            Some(self.mask(seq_len, x.device())?)
        };
        let _enter = self.span.enter();
        let device = x.device().clone();
        let mut layer_in = self.tok_embeddings.forward(x)?;

        // Dump embedding output for divergence debugging
        if std::env::var("CANDLE_DUMP_LAYERS").is_ok() {
            device.synchronize()?;
            if let Ok(flat) = layer_in.flatten_all().and_then(|t| t.to_vec1::<f32>()) {
                let n = flat.len().min(10);
                let first10: Vec<String> = flat[..n].iter().map(|v| format!("{:.6}", v)).collect();
                eprintln!(
                    "EMBED    shape={:?} first10=[{}]",
                    layer_in.dims(),
                    first10.join(", ")
                );
            }
        }

        // Debug: if CANDLE_MAX_LAYERS=0, return embedding directly (skip all layers)
        if effective_max == 0 {
            return self
                .output
                .forward(&self.norm.forward(&layer_in)?.i((.., seq_len - 1, ..))?);
        }
        for (layer_idx, layer) in self.layers.iter_mut().enumerate() {
            if layer_idx >= effective_max {
                break;
            }
            let x = layer_in;
            let residual = &x;
            let x = layer.attention_norm.forward(&x)?;

            // Dump layer 0 intermediate values for divergence debugging
            let dump_layer0 = std::env::var("CANDLE_DUMP_LAYER0").is_ok() && layer_idx == 0;
            if dump_layer0 {
                device.synchronize()?;
                let last = seq_len - 1;
                if let Ok(vals) = x.i((0, last, ..)).and_then(|t| t.to_vec1::<f32>()) {
                    let data: Vec<u8> = vals.iter().flat_map(|v| v.to_le_bytes()).collect();
                    std::fs::write("/tmp/candle_l0_attn_norm.bin", &data).ok();
                    let n = vals.len().min(5);
                    let first: Vec<String> =
                        vals[..n].iter().map(|v| format!("{:.6}", v)).collect();
                    eprintln!(
                        "L0 attn_norm: {} dims, first5=[{}]",
                        vals.len(),
                        first.join(", ")
                    );
                }
            }

            let attn = layer.forward_attn(&x, mask.as_ref(), index_pos)?;

            if dump_layer0 {
                device.synchronize()?;
                let last = seq_len - 1;
                if let Ok(vals) = attn.i((0, last, ..)).and_then(|t| t.to_vec1::<f32>()) {
                    let data: Vec<u8> = vals.iter().flat_map(|v| v.to_le_bytes()).collect();
                    std::fs::write("/tmp/candle_l0_attn_out.bin", &data).ok();
                    let n = vals.len().min(5);
                    let first: Vec<String> =
                        vals[..n].iter().map(|v| format!("{:.6}", v)).collect();
                    eprintln!(
                        "L0 attn_out: {} dims, first5=[{}]",
                        vals.len(),
                        first.join(", ")
                    );
                }
            }

            let x = (attn + residual)?;

            if dump_layer0 {
                device.synchronize()?;
                let last = seq_len - 1;
                if let Ok(vals) = x.i((0, last, ..)).and_then(|t| t.to_vec1::<f32>()) {
                    let data: Vec<u8> = vals.iter().flat_map(|v| v.to_le_bytes()).collect();
                    std::fs::write("/tmp/candle_l0_attn_resid.bin", &data).ok();
                    let n = vals.len().min(5);
                    let first: Vec<String> =
                        vals[..n].iter().map(|v| format!("{:.6}", v)).collect();
                    eprintln!(
                        "L0 attn+resid: {} dims, first5=[{}]",
                        vals.len(),
                        first.join(", ")
                    );
                }
            }

            // MLP
            let _enter = layer.span_mlp.enter();
            let residual = &x;
            let x = layer.ffn_norm.forward(&x)?;

            if dump_layer0 {
                device.synchronize()?;
                let last = seq_len - 1;
                if let Ok(vals) = x.i((0, last, ..)).and_then(|t| t.to_vec1::<f32>()) {
                    let data: Vec<u8> = vals.iter().flat_map(|v| v.to_le_bytes()).collect();
                    std::fs::write("/tmp/candle_l0_ffn_norm.bin", &data).ok();
                    let n = vals.len().min(5);
                    let first: Vec<String> =
                        vals[..n].iter().map(|v| format!("{:.6}", v)).collect();
                    eprintln!(
                        "L0 ffn_norm: {} dims, first5=[{}]",
                        vals.len(),
                        first.join(", ")
                    );
                }
            }

            let x = layer.mlp_or_moe.forward(&x)?;

            if dump_layer0 {
                device.synchronize()?;
                let last = seq_len - 1;
                if let Ok(vals) = x.i((0, last, ..)).and_then(|t| t.to_vec1::<f32>()) {
                    let data: Vec<u8> = vals.iter().flat_map(|v| v.to_le_bytes()).collect();
                    std::fs::write("/tmp/candle_l0_mlp_out.bin", &data).ok();
                    let n = vals.len().min(5);
                    let first: Vec<String> =
                        vals[..n].iter().map(|v| format!("{:.6}", v)).collect();
                    eprintln!(
                        "L0 mlp_out: {} dims, first5=[{}]",
                        vals.len(),
                        first.join(", ")
                    );
                }
            }

            let x = (x + residual)?;
            layer_in = x;

            // Log hidden state stats per layer (CANDLE_DEBUG_LAYERS=1)
            if layer_idx % 16 == 0 || layer_idx == effective_max - 1 {
                if std::env::var("CANDLE_DEBUG_LAYERS").is_ok() {
                    device.synchronize()?;
                    if let Ok(flat) = layer_in.flatten_all().and_then(|t| t.to_vec1::<f32>()) {
                        let mean = flat.iter().sum::<f32>() / flat.len() as f32;
                        let max = flat.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
                        let min = flat.iter().cloned().fold(f32::INFINITY, f32::min);
                        let has_nan = flat.iter().any(|v| v.is_nan());
                        let has_inf = flat.iter().any(|v| v.is_infinite());
                        eprintln!(
                            "  [layer {:>2}] mean={:>8.3} min={:>8.1} max={:>8.1} nan={} inf={}",
                            layer_idx, mean, min, max, has_nan, has_inf
                        );
                    }
                }
            }

            // Commit Metal command buffer every 24 layers so the GPU can start
            // executing early layers while we encode later ones.
            // Too frequent (8) = CPU waits too much. Never = giant stall at end.
            if (layer_idx + 1) % 24 == 0 {
                device.synchronize()?;
            }

            // Dump hidden state for divergence debugging
            if std::env::var("CANDLE_DUMP_LAYERS").is_ok()
                && (layer_idx < 3 || layer_idx == effective_max - 1)
            {
                device.synchronize()?;
                if let Ok(flat) = layer_in.flatten_all().and_then(|t| t.to_vec1::<f32>()) {
                    let n = flat.len().min(10);
                    let first10: Vec<String> =
                        flat[..n].iter().map(|v| format!("{:.6}", v)).collect();
                    let mean: f64 = flat.iter().map(|&v| v as f64).sum::<f64>() / flat.len() as f64;
                    let absmax = flat.iter().cloned().fold(0f32, |a, b| a.max(b.abs()));
                    eprintln!(
                        "LAYER[{:>2}] mean={:.6} absmax={:.3} first10=[{}]",
                        layer_idx,
                        mean,
                        absmax,
                        first10.join(", ")
                    );
                }
            }
        }
        let x = self.norm.forward(&layer_in)?;
        let x = x.i((.., seq_len - 1, ..))?;

        // Dump hidden state (post-norm, pre-lm_head) for comparison with llama.cpp
        if std::env::var("CANDLE_DUMP_HIDDEN").is_ok() {
            device.synchronize()?;
            if let Ok(vals) = x.flatten_all().and_then(|t| t.to_vec1::<f32>()) {
                let n = vals.len().min(10);
                let first: Vec<String> = vals[..n].iter().map(|v| format!("{:.6}", v)).collect();
                eprintln!(
                    "HIDDEN (post-norm, pre-lm_head): {} dims, first10=[{}]",
                    vals.len(),
                    first.join(", ")
                );
                let data: Vec<u8> = vals.iter().flat_map(|v| v.to_le_bytes()).collect();
                std::fs::write("/tmp/candle_hidden.bin", &data).ok();
                eprintln!("  Written to /tmp/candle_hidden.bin");
            }
        }

        let _enter = self.span_output.enter();
        let logits = self.output.forward(&x)?;

        // Dump full logits for comparison
        if std::env::var("CANDLE_DUMP_HIDDEN").is_ok() {
            device.synchronize()?;
            if let Ok(vals) = logits.flatten_all().and_then(|t| t.to_vec1::<f32>()) {
                let data: Vec<u8> = vals.iter().flat_map(|v| v.to_le_bytes()).collect();
                std::fs::write("/tmp/candle_logits.bin", &data).ok();
                eprintln!("  Written {} logits to /tmp/candle_logits.bin", vals.len());
            }
        }

        Ok(logits)
    }
}
