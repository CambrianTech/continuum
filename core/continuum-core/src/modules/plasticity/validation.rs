//! Plasticity validation — end-to-end verification of the compaction pipeline.
//!
//! These tests prove that:
//! 1. Compaction produces correct tensor dimensions
//! 2. CompactLlama loads and runs forward passes with variable head counts
//! 3. The topology detection mechanism works in the loading path
//! 4. Memory savings are real and measurable
//! 5. Compacted models are compatible with GGUF conversion (dimension constraints)
//!
//! Tests use synthetic mini-models (tiny Llama-like architecture) to avoid
//! requiring multi-GB real models during CI/unit testing.

use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::llama::Config as LlamaConfig;
use std::collections::HashMap;

use super::types::*;
use crate::inference::vendored::compact_llama::{detect_topology, CompactLlama};

/// Create a minimal LlamaConfig for testing.
/// Uses tiny dimensions that are still architecturally valid.
/// Returns the inner `Config` type (not `LlamaConfig`) since CompactLlama
/// takes the already-converted config.
fn test_llama_config(
    vocab_size: usize,
    hidden_size: usize,
    intermediate_size: usize,
    num_layers: usize,
    num_heads: usize,
    num_kv_heads: usize,
) -> LlamaConfig {
    LlamaConfig {
        hidden_size,
        intermediate_size,
        vocab_size,
        num_hidden_layers: num_layers,
        num_attention_heads: num_heads,
        num_key_value_heads: num_kv_heads,
        use_flash_attn: false,
        rms_norm_eps: 1e-5,
        rope_theta: 10000.0,
        bos_token_id: None,
        eos_token_id: None,
        rope_scaling: None,
        max_position_embeddings: 128,
        tie_word_embeddings: false,
    }
}

/// Build a tensor map with synthetic weights for a mini Llama model.
///
/// Creates all the tensors that CompactLlama::load() expects, with
/// per-layer dimensions matching the topology.
fn build_synthetic_compact_weights(
    config: &LlamaConfig,
    topology: &HeadTopology,
    device: &Device,
) -> HashMap<String, Tensor> {
    let hidden_size = config.hidden_size;
    let intermediate_size = config.intermediate_size;
    let vocab_size = config.vocab_size;
    let head_dim = topology.head_dim;
    let dtype = DType::F32;

    let mut tensors = HashMap::new();

    // Embedding: [vocab_size, hidden_size]
    tensors.insert(
        "model.embed_tokens.weight".to_string(),
        Tensor::randn(0f32, 0.02, &[vocab_size, hidden_size], device)
            .unwrap()
            .to_dtype(dtype)
            .unwrap(),
    );

    // Per-layer weights with topology-specific dimensions
    for layer_topo in &topology.layers {
        let idx = layer_topo.layer_index;
        let n_head = layer_topo.num_heads;
        let n_kv_head = layer_topo.num_kv_heads;
        let prefix = format!("model.layers.{idx}");

        // Attention projections with compacted dimensions
        // q_proj: [n_head * head_dim, hidden_size]
        tensors.insert(
            format!("{prefix}.self_attn.q_proj.weight"),
            Tensor::randn(0f32, 0.02, &[n_head * head_dim, hidden_size], device)
                .unwrap()
                .to_dtype(dtype)
                .unwrap(),
        );

        // k_proj: [n_kv_head * head_dim, hidden_size]
        tensors.insert(
            format!("{prefix}.self_attn.k_proj.weight"),
            Tensor::randn(0f32, 0.02, &[n_kv_head * head_dim, hidden_size], device)
                .unwrap()
                .to_dtype(dtype)
                .unwrap(),
        );

        // v_proj: [n_kv_head * head_dim, hidden_size]
        tensors.insert(
            format!("{prefix}.self_attn.v_proj.weight"),
            Tensor::randn(0f32, 0.02, &[n_kv_head * head_dim, hidden_size], device)
                .unwrap()
                .to_dtype(dtype)
                .unwrap(),
        );

        // o_proj: [hidden_size, n_head * head_dim]
        tensors.insert(
            format!("{prefix}.self_attn.o_proj.weight"),
            Tensor::randn(0f32, 0.02, &[hidden_size, n_head * head_dim], device)
                .unwrap()
                .to_dtype(dtype)
                .unwrap(),
        );

        // MLP (unchanged by compaction)
        tensors.insert(
            format!("{prefix}.mlp.gate_proj.weight"),
            Tensor::randn(0f32, 0.02, &[intermediate_size, hidden_size], device)
                .unwrap()
                .to_dtype(dtype)
                .unwrap(),
        );
        tensors.insert(
            format!("{prefix}.mlp.up_proj.weight"),
            Tensor::randn(0f32, 0.02, &[intermediate_size, hidden_size], device)
                .unwrap()
                .to_dtype(dtype)
                .unwrap(),
        );
        tensors.insert(
            format!("{prefix}.mlp.down_proj.weight"),
            Tensor::randn(0f32, 0.02, &[hidden_size, intermediate_size], device)
                .unwrap()
                .to_dtype(dtype)
                .unwrap(),
        );

        // Layer norms
        tensors.insert(
            format!("{prefix}.input_layernorm.weight"),
            Tensor::ones(&[hidden_size], dtype, device).unwrap(),
        );
        tensors.insert(
            format!("{prefix}.post_attention_layernorm.weight"),
            Tensor::ones(&[hidden_size], dtype, device).unwrap(),
        );
    }

    // Final norm + lm_head
    tensors.insert(
        "model.norm.weight".to_string(),
        Tensor::ones(&[hidden_size], dtype, device).unwrap(),
    );
    tensors.insert(
        "lm_head.weight".to_string(),
        Tensor::randn(0f32, 0.02, &[vocab_size, hidden_size], device)
            .unwrap()
            .to_dtype(dtype)
            .unwrap(),
    );

    tensors
}

/// Count total parameters in a tensor map.
fn count_parameters(tensors: &HashMap<String, Tensor>) -> usize {
    tensors.values().map(|t| t.elem_count()).sum()
}

/// Estimate attention parameters for a uniform (non-compacted) model.
fn uniform_attention_params(
    num_layers: usize,
    num_heads: usize,
    num_kv_heads: usize,
    head_dim: usize,
    hidden_size: usize,
) -> usize {
    let per_layer = {
        let q = num_heads * head_dim * hidden_size;
        let k = num_kv_heads * head_dim * hidden_size;
        let v = num_kv_heads * head_dim * hidden_size;
        let o = hidden_size * num_heads * head_dim;
        q + k + v + o
    };
    per_layer * num_layers
}

/// Estimate attention parameters for a compacted model from topology.
fn compact_attention_params(topology: &HeadTopology, hidden_size: usize) -> usize {
    let head_dim = topology.head_dim;
    topology
        .layers
        .iter()
        .map(|layer| {
            let q = layer.num_heads * head_dim * hidden_size;
            let k = layer.num_kv_heads * head_dim * hidden_size;
            let v = layer.num_kv_heads * head_dim * hidden_size;
            let o = hidden_size * layer.num_heads * head_dim;
            q + k + v + o
        })
        .sum()
}

#[cfg(test)]
mod tests;
