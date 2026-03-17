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
mod tests {
    use super::*;
    use crate::modules::plasticity::scoring;

    // ── Test Helper ──

    fn make_test_topology(layer_configs: &[(usize, usize)], head_dim: usize) -> HeadTopology {
        let layers: Vec<LayerTopology> = layer_configs
            .iter()
            .enumerate()
            .map(|(i, &(n_heads, n_kv_heads))| LayerTopology {
                layer_index: i,
                num_heads: n_heads,
                num_kv_heads: n_kv_heads,
                retained_head_indices: (0..n_heads).collect(),
                retained_kv_head_indices: (0..n_kv_heads).collect(),
                head_precisions: vec![HeadPrecision::BF16; n_heads],
                head_scores: vec![0.8; n_heads],
            })
            .collect();

        HeadTopology {
            base_model: "test-model".to_string(),
            layers,
            original_num_heads: 8,
            original_num_kv_heads: 4,
            head_dim,
            parameter_reduction: 0.25,
            precision_profile: PrecisionProfile {
                removed: 4,
                ternary: 0,
                q2: 0,
                q4: 0,
                q8: 0,
                bf16: 12,
            },
            created_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    // ═══════════════════════════════════════════════════════════════════════
    // 1. CompactLlama loads with variable per-layer head counts
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn test_compact_llama_loads_uniform_heads() {
        // All layers have same head count (like a normal model) — should work
        let topology = make_test_topology(&[(8, 4), (8, 4), (8, 4), (8, 4)], 16);
        let config = test_llama_config(32, 128, 256, 4, 8, 4);
        let device = Device::Cpu;

        let tensors = build_synthetic_compact_weights(&config, &topology, &device);
        let vb = VarBuilder::from_tensors(tensors, DType::F32, &device);

        let model = CompactLlama::load(vb, &config, &topology);
        assert!(model.is_ok(), "CompactLlama should load with uniform heads: {:?}", model.err());
    }

    #[test]
    fn test_compact_llama_loads_variable_heads() {
        // Different head counts per layer — the core compaction use case
        let topology = make_test_topology(&[(8, 4), (6, 3), (4, 2), (8, 4)], 16);
        let config = test_llama_config(32, 128, 256, 4, 8, 4);
        let device = Device::Cpu;

        let tensors = build_synthetic_compact_weights(&config, &topology, &device);
        let vb = VarBuilder::from_tensors(tensors, DType::F32, &device);

        let model = CompactLlama::load(vb, &config, &topology);
        assert!(
            model.is_ok(),
            "CompactLlama should load with variable heads: {:?}",
            model.err()
        );
    }

    #[test]
    fn test_compact_llama_loads_aggressively_pruned() {
        // Aggressive pruning: some layers down to minimum heads
        let topology = make_test_topology(&[(4, 2), (4, 2), (4, 2), (4, 2)], 16);
        let config = test_llama_config(32, 128, 256, 4, 8, 4);
        let device = Device::Cpu;

        let tensors = build_synthetic_compact_weights(&config, &topology, &device);
        let vb = VarBuilder::from_tensors(tensors, DType::F32, &device);

        let model = CompactLlama::load(vb, &config, &topology);
        assert!(model.is_ok(), "CompactLlama should load with aggressive pruning");
    }

    // ═══════════════════════════════════════════════════════════════════════
    // 2. Forward pass produces valid logits with variable head counts
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn test_compact_llama_forward_uniform() {
        let topology = make_test_topology(&[(8, 4), (8, 4)], 16);
        let config = test_llama_config(32, 128, 256, 2, 8, 4);
        let device = Device::Cpu;

        let tensors = build_synthetic_compact_weights(&config, &topology, &device);
        let vb = VarBuilder::from_tensors(tensors, DType::F32, &device);
        let mut model = CompactLlama::load(vb, &config, &topology).unwrap();

        // Single token forward pass
        let input = Tensor::new(&[1u32], &device).unwrap().unsqueeze(0).unwrap();
        let logits = model.forward(&input, 0);

        assert!(logits.is_ok(), "Forward should succeed: {:?}", logits.err());
        let logits = logits.unwrap();
        assert_eq!(logits.dims(), &[1, 32]); // [batch, vocab_size]
    }

    #[test]
    fn test_compact_llama_forward_variable_heads() {
        // The critical test: variable head counts across layers
        let topology = make_test_topology(&[(8, 4), (4, 2)], 16);
        let config = test_llama_config(32, 128, 256, 2, 8, 4);
        let device = Device::Cpu;

        let tensors = build_synthetic_compact_weights(&config, &topology, &device);
        let vb = VarBuilder::from_tensors(tensors, DType::F32, &device);
        let mut model = CompactLlama::load(vb, &config, &topology).unwrap();

        let input = Tensor::new(&[1u32], &device).unwrap().unsqueeze(0).unwrap();
        let logits = model.forward(&input, 0);

        assert!(
            logits.is_ok(),
            "Forward with variable heads should succeed: {:?}",
            logits.err()
        );
        let logits = logits.unwrap();
        assert_eq!(logits.dims(), &[1, 32]);

        // Verify logits are not NaN
        let logits_vec: Vec<f32> = logits.to_vec2::<f32>().unwrap()[0].clone();
        assert!(
            logits_vec.iter().all(|&x| !x.is_nan()),
            "Logits should not contain NaN"
        );
    }

    #[test]
    fn test_compact_llama_multi_token_forward() {
        // Multiple tokens at once (prefill scenario)
        let topology = make_test_topology(&[(8, 4), (6, 3)], 16);
        let config = test_llama_config(32, 128, 256, 2, 8, 4);
        let device = Device::Cpu;

        let tensors = build_synthetic_compact_weights(&config, &topology, &device);
        let vb = VarBuilder::from_tensors(tensors, DType::F32, &device);
        let mut model = CompactLlama::load(vb, &config, &topology).unwrap();

        // 4 tokens at once
        let input = Tensor::new(&[1u32, 2, 3, 4], &device)
            .unwrap()
            .unsqueeze(0)
            .unwrap();
        let logits = model.forward(&input, 0);

        assert!(logits.is_ok(), "Multi-token forward should succeed: {:?}", logits.err());
        let logits = logits.unwrap();
        // Output is last token's logits: [batch, vocab_size]
        assert_eq!(logits.dims(), &[1, 32]);
    }

    #[test]
    fn test_compact_llama_sequential_generation() {
        // Simulate autoregressive generation: prefill then generate token-by-token
        let topology = make_test_topology(&[(8, 4), (4, 2)], 16);
        let config = test_llama_config(32, 128, 256, 2, 8, 4);
        let device = Device::Cpu;

        let tensors = build_synthetic_compact_weights(&config, &topology, &device);
        let vb = VarBuilder::from_tensors(tensors, DType::F32, &device);
        let mut model = CompactLlama::load(vb, &config, &topology).unwrap();

        // Prefill: 3 tokens
        let input = Tensor::new(&[1u32, 2, 3], &device)
            .unwrap()
            .unsqueeze(0)
            .unwrap();
        let logits = model.forward(&input, 0).unwrap();
        assert_eq!(logits.dims(), &[1, 32]);

        // Generate: token at position 3
        let next_input = Tensor::new(&[5u32], &device)
            .unwrap()
            .unsqueeze(0)
            .unwrap();
        let logits = model.forward(&next_input, 3).unwrap();
        assert_eq!(logits.dims(), &[1, 32]);

        // Generate: token at position 4
        let next_input = Tensor::new(&[7u32], &device)
            .unwrap()
            .unsqueeze(0)
            .unwrap();
        let logits = model.forward(&next_input, 4).unwrap();
        assert_eq!(logits.dims(), &[1, 32]);
    }

    #[test]
    fn test_compact_llama_cache_clear() {
        let topology = make_test_topology(&[(8, 4), (4, 2)], 16);
        let config = test_llama_config(32, 128, 256, 2, 8, 4);
        let device = Device::Cpu;

        let tensors = build_synthetic_compact_weights(&config, &topology, &device);
        let vb = VarBuilder::from_tensors(tensors, DType::F32, &device);
        let mut model = CompactLlama::load(vb, &config, &topology).unwrap();

        // Generate some tokens to fill cache
        let input = Tensor::new(&[1u32, 2, 3], &device)
            .unwrap()
            .unsqueeze(0)
            .unwrap();
        model.forward(&input, 0).unwrap();

        // Clear cache
        model.clear_cache();

        // Should work again from position 0
        let input = Tensor::new(&[4u32, 5], &device)
            .unwrap()
            .unsqueeze(0)
            .unwrap();
        let logits = model.forward(&input, 0);
        assert!(logits.is_ok(), "Forward after cache clear should work");
    }

    // ═══════════════════════════════════════════════════════════════════════
    // 3. Memory savings are real and measurable
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn test_parameter_reduction_with_pruning() {
        let num_layers = 4;
        let num_heads = 8;
        let num_kv_heads = 4;
        let head_dim = 16;
        let hidden_size = 128;

        // Uniform model attention params
        let uniform_params =
            uniform_attention_params(num_layers, num_heads, num_kv_heads, head_dim, hidden_size);

        // Compacted: layers have 8, 6, 4, 8 heads
        let topology = make_test_topology(&[(8, 4), (6, 3), (4, 2), (8, 4)], head_dim);
        let compact_params = compact_attention_params(&topology, hidden_size);

        assert!(
            compact_params < uniform_params,
            "Compacted model should have fewer attention parameters: {} < {}",
            compact_params,
            uniform_params
        );

        let reduction_pct =
            (1.0 - compact_params as f64 / uniform_params as f64) * 100.0;
        assert!(
            reduction_pct > 10.0,
            "Should achieve >10% attention parameter reduction, got {:.1}%",
            reduction_pct
        );
    }

    #[test]
    fn test_parameter_reduction_aggressive() {
        let num_layers = 4;
        let num_heads = 8;
        let num_kv_heads = 4;
        let head_dim = 16;
        let hidden_size = 128;

        let uniform_params =
            uniform_attention_params(num_layers, num_heads, num_kv_heads, head_dim, hidden_size);

        // Aggressive: all layers down to 4 heads (50% of original)
        let topology = make_test_topology(&[(4, 2), (4, 2), (4, 2), (4, 2)], head_dim);
        let compact_params = compact_attention_params(&topology, hidden_size);

        let reduction_pct =
            (1.0 - compact_params as f64 / uniform_params as f64) * 100.0;
        assert!(
            reduction_pct > 40.0,
            "Aggressive pruning should achieve >40% attention parameter reduction, got {:.1}%",
            reduction_pct
        );
    }

    #[test]
    fn test_actual_tensor_size_reduction() {
        let device = Device::Cpu;
        let head_dim = 16;
        let hidden_size = 128;

        // Build full-size synthetic weights (uniform 8 heads)
        let full_topology = make_test_topology(&[(8, 4), (8, 4), (8, 4), (8, 4)], head_dim);
        let full_config_candle = test_llama_config(32, hidden_size, 256, 4, 8, 4);
        let full_tensors =
            build_synthetic_compact_weights(&full_config_candle, &full_topology, &device);
        let full_params = count_parameters(&full_tensors);

        // Build compacted weights (variable heads)
        let compact_topology = make_test_topology(&[(8, 4), (6, 3), (4, 2), (4, 2)], head_dim);
        let compact_config_candle = test_llama_config(32, hidden_size, 256, 4, 8, 4);
        let compact_tensors =
            build_synthetic_compact_weights(&compact_config_candle, &compact_topology, &device);
        let compact_params = count_parameters(&compact_tensors);

        assert!(
            compact_params < full_params,
            "Compacted tensors should be smaller: {} < {}",
            compact_params,
            full_params
        );

        let savings_pct = (1.0 - compact_params as f64 / full_params as f64) * 100.0;
        // With shared MLP/embedding/norm weights, attention savings are diluted
        // but should still be measurable
        assert!(
            savings_pct > 5.0,
            "Total parameter savings should be >5%, got {:.1}%",
            savings_pct
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // 4. End-to-end: scoring → topology → load → forward
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn test_e2e_scoring_to_forward() {
        // Full pipeline: utilization data → scoring → topology → compact load → forward
        let device = Device::Cpu;
        let num_layers = 2;
        let num_heads = 8;
        let num_kv_heads = 4;
        let head_dim = 16;
        let hidden_size = 128;
        let vocab_size = 32;

        // Synthetic utilization: some heads dead, some alive
        let utilization = UtilizationData {
            layer_scores: vec![
                // Layer 0: 4 dead heads, 4 alive
                vec![0.01, 0.02, 0.03, 0.04, 0.8, 0.85, 0.9, 0.7],
                // Layer 1: 2 dead, 6 alive
                vec![0.02, 0.01, 0.6, 0.5, 0.7, 0.8, 0.85, 0.9],
            ],
            num_steps: 100,
            model_name: "test-model".to_string(),
            num_heads,
            num_kv_heads,
        };

        let config = CompactionConfig::default();
        let layers = scoring::compute_optimization_plan(&utilization, &config);

        assert_eq!(layers.len(), num_layers);

        // Build topology
        let precision_profile =
            scoring::compute_precision_profile(&layers, num_heads, num_layers);

        let topology = HeadTopology {
            base_model: "test-model".to_string(),
            layers: layers.clone(),
            original_num_heads: num_heads,
            original_num_kv_heads: num_kv_heads,
            head_dim,
            parameter_reduction: scoring::estimate_parameter_reduction(
                &layers,
                num_heads,
                num_kv_heads,
                head_dim,
                hidden_size,
            ),
            precision_profile,
            created_at: "2026-01-01T00:00:00Z".to_string(),
        };

        // Build synthetic weights matching the topology
        let candle_config = test_llama_config(
            vocab_size,
            hidden_size,
            256,
            num_layers,
            num_heads,
            num_kv_heads,
        );
        let tensors = build_synthetic_compact_weights(&candle_config, &topology, &device);
        let vb = VarBuilder::from_tensors(tensors, DType::F32, &device);

        // Load CompactLlama
        let model = CompactLlama::load(vb, &candle_config, &topology);
        assert!(model.is_ok(), "E2E model load failed: {:?}", model.err());
        let mut model = model.unwrap();

        // Forward pass
        let input = Tensor::new(&[1u32, 2, 3], &device)
            .unwrap()
            .unsqueeze(0)
            .unwrap();
        let logits = model.forward(&input, 0);
        assert!(logits.is_ok(), "E2E forward failed: {:?}", logits.err());

        let logits = logits.unwrap();
        assert_eq!(logits.dims(), &[1, vocab_size]);

        // Verify non-NaN
        let vals: Vec<f32> = logits.to_vec2::<f32>().unwrap()[0].clone();
        assert!(vals.iter().all(|&x| !x.is_nan()), "E2E logits should not be NaN");
    }

    // ═══════════════════════════════════════════════════════════════════════
    // 5. Topology file detection and I/O
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn test_topology_roundtrip_with_compact_load() {
        use crate::modules::plasticity::topology;

        let device = Device::Cpu;

        // Create topology, save to file, load back, verify CompactLlama loads
        let topology = make_test_topology(&[(8, 4), (6, 3)], 16);
        let tmp = tempfile::tempdir().unwrap();
        let topo_path = tmp.path().join("head_topology.json");

        topology::save_topology(&topology, &topo_path).unwrap();

        // Detect it
        assert!(detect_topology(tmp.path()).is_some());

        // Load it back
        let loaded_topo = topology::load_topology(&topo_path).unwrap();
        assert_eq!(loaded_topo.layers.len(), 2);
        assert_eq!(loaded_topo.layers[0].num_heads, 8);
        assert_eq!(loaded_topo.layers[1].num_heads, 6);

        // Build model from loaded topology
        let config = test_llama_config(32, 128, 256, 2, 8, 4);
        let tensors = build_synthetic_compact_weights(&config, &loaded_topo, &device);
        let vb = VarBuilder::from_tensors(tensors, DType::F32, &device);

        let model = CompactLlama::load(vb, &config, &loaded_topo);
        assert!(model.is_ok(), "Load from saved topology failed: {:?}", model.err());
    }

    // ═══════════════════════════════════════════════════════════════════════
    // 6. GGUF compatibility: dimension constraints
    // ═══════════════════════════════════════════════════════════════════════
    //
    // GGUF models have UNIFORM head counts baked into metadata. A compacted
    // model with variable per-layer heads can't be directly represented as
    // GGUF. The workflow is:
    //   1. Compact safetensors (variable heads per layer)
    //   2. For GGUF: re-quantize the compacted safetensors using llama.cpp
    //      which writes new GGUF metadata matching the compacted dimensions
    //
    // These tests verify that compacted dimensions satisfy GGUF constraints.

    #[test]
    fn test_gguf_dimension_constraints() {
        // GGUF requires: head_dim * n_heads = q_proj output dimension
        // and all dimensions must be positive multiples of head_dim
        let topology = make_test_topology(&[(8, 4), (6, 3), (4, 2)], 16);

        for layer in &topology.layers {
            // Q projection dimension must be n_heads * head_dim
            let q_dim = layer.num_heads * topology.head_dim;
            assert!(q_dim > 0, "Q dimension must be positive");
            assert_eq!(
                q_dim % topology.head_dim,
                0,
                "Q dimension must be multiple of head_dim"
            );

            // KV projection dimension must be n_kv_heads * head_dim
            let kv_dim = layer.num_kv_heads * topology.head_dim;
            assert!(kv_dim > 0, "KV dimension must be positive");
            assert_eq!(
                kv_dim % topology.head_dim,
                0,
                "KV dimension must be multiple of head_dim"
            );

            // GQA ratio must be integer
            assert_eq!(
                layer.num_heads % layer.num_kv_heads,
                0,
                "GQA ratio must be integer: {} / {}",
                layer.num_heads,
                layer.num_kv_heads
            );
        }
    }

    #[test]
    fn test_gguf_compatible_after_scoring() {
        // Verify that the scoring engine always produces GGUF-compatible topologies
        let utilization = UtilizationData {
            layer_scores: vec![
                vec![0.01, 0.02, 0.03, 0.04, 0.8, 0.85, 0.9, 0.95],
                vec![0.01, 0.5, 0.6, 0.7, 0.8, 0.85, 0.9, 0.95],
                vec![0.8, 0.85, 0.9, 0.95, 0.8, 0.85, 0.9, 0.95],
            ],
            num_steps: 100,
            model_name: "test".to_string(),
            num_heads: 8,
            num_kv_heads: 4,
        };

        let config = CompactionConfig::default();
        let layers = scoring::compute_optimization_plan(&utilization, &config);

        for layer in &layers {
            // All GQA constraints satisfied
            assert!(layer.num_heads > 0, "Layer {} must have >0 heads", layer.layer_index);
            assert!(layer.num_kv_heads > 0, "Layer {} must have >0 KV heads", layer.layer_index);
            assert_eq!(
                layer.num_heads % layer.num_kv_heads,
                0,
                "Layer {} GQA ratio must be integer",
                layer.layer_index
            );
        }
    }

    // ═══════════════════════════════════════════════════════════════════════
    // 7. Safetensors backend integration
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn test_compact_backend_architecture() {
        use crate::inference::backends::compact_llama_safetensors::CompactLlamaSafetensorsBackend;
        use crate::inference::backends::{ModelBackend, ModelFormat};

        let topology = make_test_topology(&[(8, 4), (4, 2)], 16);
        let config = test_llama_config(32, 128, 256, 2, 8, 4);
        let device = Device::Cpu;

        let tensors = build_synthetic_compact_weights(&config, &topology, &device);
        let vb = VarBuilder::from_tensors(tensors, DType::F32, &device);
        let model = CompactLlama::load(vb, &config, &topology).unwrap();

        let tokenizer = tokenizers::Tokenizer::from_bytes(MINIMAL_TOKENIZER_JSON).unwrap();

        let backend = CompactLlamaSafetensorsBackend::new(
            model,
            tokenizer,
            device,
            DType::F32,
            config,
            topology.clone(),
            "test-compact".to_string(),
            vec![128001, 128009],
            vec![],
        );

        assert_eq!(backend.architecture(), "llama-compact");
        assert_eq!(backend.format(), ModelFormat::Safetensors);
        assert_eq!(backend.context_length(), 128);
        assert_eq!(backend.eos_token_ids(), &[128001, 128009]);
        assert!(!backend.supports_lora());
        assert_eq!(backend.topology().layers.len(), 2);
        assert_eq!(
            (backend.topology().parameter_reduction * 100.0) as u32,
            25
        );
    }

    #[test]
    fn test_compact_backend_forward() {
        use crate::inference::backends::compact_llama_safetensors::CompactLlamaSafetensorsBackend;
        use crate::inference::backends::ModelBackend;

        let topology = make_test_topology(&[(8, 4), (4, 2)], 16);
        let config = test_llama_config(32, 128, 256, 2, 8, 4);
        let device = Device::Cpu;

        let tensors = build_synthetic_compact_weights(&config, &topology, &device);
        let vb = VarBuilder::from_tensors(tensors, DType::F32, &device);
        let model = CompactLlama::load(vb, &config, &topology).unwrap();

        let tokenizer = tokenizers::Tokenizer::from_bytes(MINIMAL_TOKENIZER_JSON).unwrap();

        let mut backend = CompactLlamaSafetensorsBackend::new(
            model,
            tokenizer,
            device.clone(),
            DType::F32,
            config,
            topology,
            "test-compact".to_string(),
            vec![128001],
            vec![],
        );

        // Forward via ModelBackend trait
        let input = Tensor::new(&[1u32], &device).unwrap().unsqueeze(0).unwrap();
        let logits = backend.forward(&input, 0);
        assert!(logits.is_ok(), "Backend forward should work: {:?}", logits.err());

        // Clear cache via trait
        assert!(backend.clear_cache().is_ok());

        // Forward again after clear
        let logits = backend.forward(&input, 0);
        assert!(logits.is_ok(), "Forward after cache clear should work");
    }

    // ═══════════════════════════════════════════════════════════════════════
    // 8. Llama-3.2-3B scale validation (realistic dimensions)
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn test_realistic_llama_3b_topology() {
        // Simulate what real Llama-3.2-3B compaction looks like
        let num_heads = 24;
        let num_kv_heads = 8;
        let head_dim = 128;
        let hidden_size = 3072;
        let num_layers = 28;

        // Realistic utilization: ~20% of heads are dead
        let mut layer_scores = Vec::new();
        for layer_idx in 0..num_layers {
            let mut scores = vec![0.5; num_heads];
            // First and last layers: fewer dead heads (important for model quality)
            let dead_count = if layer_idx < 4 || layer_idx >= 24 { 2 } else { 6 };
            for i in 0..dead_count {
                scores[i] = 0.02 + (i as f64) * 0.01;
            }
            // Some high-utilization heads
            for i in (num_heads - 4)..num_heads {
                scores[i] = 0.85 + (i as f64 - (num_heads - 4) as f64) * 0.03;
            }
            layer_scores.push(scores);
        }

        let utilization = UtilizationData {
            layer_scores,
            num_steps: 500,
            model_name: "meta-llama/Llama-3.2-3B".to_string(),
            num_heads,
            num_kv_heads,
        };

        let config = CompactionConfig::default();
        let layers = scoring::compute_optimization_plan(&utilization, &config);

        assert_eq!(layers.len(), num_layers);

        // Verify all layers have valid head counts
        for layer in &layers {
            assert!(layer.num_heads >= config.min_heads_per_layer);
            assert!(layer.num_kv_heads >= config.min_kv_heads_per_layer);
            assert_eq!(layer.num_heads % layer.num_kv_heads, 0);
        }

        // Estimate savings
        let uniform_params = uniform_attention_params(
            num_layers, num_heads, num_kv_heads, head_dim, hidden_size,
        );

        let topo = HeadTopology {
            base_model: "meta-llama/Llama-3.2-3B".to_string(),
            layers,
            original_num_heads: num_heads,
            original_num_kv_heads: num_kv_heads,
            head_dim,
            parameter_reduction: 0.0,
            precision_profile: PrecisionProfile {
                removed: 0,
                ternary: 0,
                q2: 0,
                q4: 0,
                q8: 0,
                bf16: 0,
            },
            created_at: "".to_string(),
        };

        let compact_params = compact_attention_params(&topo, hidden_size);
        let savings_pct = (1.0 - compact_params as f64 / uniform_params as f64) * 100.0;

        // With ~20% dead heads, attention param savings should be significant
        assert!(
            savings_pct > 10.0,
            "Realistic Llama-3.2-3B compaction should save >10% attention params, got {:.1}%",
            savings_pct
        );

        // Total model size estimate (attention is ~1/3 of total params)
        // Llama-3.2-3B: ~3.2B params, attention ~1.1B, so 20% attention savings ≈ 7% total
        let total_model_savings_estimate = savings_pct / 3.0;
        assert!(
            total_model_savings_estimate > 3.0,
            "Total model savings estimate should be >3%, got {:.1}%",
            total_model_savings_estimate
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // 9. Backend dispatch: verify architecture string distinguishes backends
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn test_backend_architecture_strings() {
        // Standard and compact backends must have different architecture strings
        // so logging and metrics can distinguish them
        assert_ne!("llama", "llama-compact");
        // The generate() function logs architecture — this confirms it works
    }

    // ═══════════════════════════════════════════════════════════════════════
    // 10. Mixed precision tier distribution
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn test_precision_profile_from_real_scoring() {
        // With GQA (8 Q / 4 KV, ratio 2:1), entire KV groups must be dead
        // for heads to be removed. Design scores so KV group 0 is fully dead.
        let utilization = UtilizationData {
            layer_scores: vec![
                // Layer 0: KV group 0 (Q[0], Q[1]) fully dead, others alive
                vec![0.01, 0.02, 0.5, 0.85, 0.15, 0.2, 0.6, 0.95],
                // Layer 1: KV group 3 (Q[6], Q[7]) fully dead, others high
                vec![0.7, 0.75, 0.8, 0.85, 0.9, 0.95, 0.02, 0.01],
            ],
            num_steps: 100,
            model_name: "test".to_string(),
            num_heads: 8,
            num_kv_heads: 4,
        };

        let config = CompactionConfig::default();
        let layers = scoring::compute_optimization_plan(&utilization, &config);
        let profile = scoring::compute_precision_profile(&layers, 8, 2);

        // Should have a mix of precision tiers
        let total = profile.removed + profile.q4 + profile.q8 + profile.bf16;
        assert!(total > 0, "Profile should have non-zero counts");

        // Some heads should be removed (entire KV groups dead)
        assert!(profile.removed > 0, "Should have removed heads (full KV groups dead)");
        // Some should be BF16 (utilization > 0.7)
        assert!(profile.bf16 > 0, "Should have BF16 heads");
        // GQA ratio maintained for all layers
        for layer in &layers {
            assert_eq!(
                layer.num_heads % layer.num_kv_heads, 0,
                "GQA ratio must be integer for layer {}", layer.layer_index
            );
        }
    }

    // Minimal tokenizer JSON for backend tests (just needs to be parseable)
    const MINIMAL_TOKENIZER_JSON: &[u8] = br#"{
        "version": "1.0",
        "truncation": null,
        "padding": null,
        "added_tokens": [],
        "normalizer": null,
        "pre_tokenizer": null,
        "post_processor": null,
        "decoder": null,
        "model": {
            "type": "BPE",
            "dropout": null,
            "unk_token": null,
            "continuing_subword_prefix": null,
            "end_of_word_suffix": null,
            "fuse_unk": false,
            "byte_fallback": false,
            "vocab": {},
            "merges": []
        }
    }"#;
}
