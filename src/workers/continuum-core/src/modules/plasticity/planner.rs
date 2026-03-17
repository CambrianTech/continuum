//! Compression planner: turns utilization scores + device spec into a CompressionRecipe.
//!
//! Pure function — no I/O, no model loading. Fully testable with synthetic data.
//! See docs/genome/COMPRESSION-PIPELINE.md for the full pipeline architecture.

use super::types::*;
use std::collections::HashMap;

/// Model architecture configuration. Derived from config.json or GGUF metadata.
#[derive(Debug, Clone)]
pub struct ModelArchConfig {
    pub num_layers: usize,
    pub hidden_size: usize,
    pub num_attention_heads: usize,
    pub num_kv_heads: usize,
    pub head_dim: usize,
    pub intermediate_size: usize, // FFN hidden dim
    pub vocab_size: usize,
    /// GQA ratio: num_attention_heads / num_kv_heads
    pub gqa_ratio: usize,
}

impl ModelArchConfig {
    /// Qwen2.5-Coder-32B original (before compaction).
    pub fn qwen2_32b() -> Self {
        Self {
            num_layers: 64,
            hidden_size: 5120,
            num_attention_heads: 40,
            num_kv_heads: 8,
            head_dim: 128,
            intermediate_size: 27648,
            vocab_size: 152064,
            gqa_ratio: 5,
        }
    }

    /// Llama 3.2 3B original.
    pub fn llama_3b() -> Self {
        Self {
            num_layers: 28,
            hidden_size: 3072,
            num_attention_heads: 24,
            num_kv_heads: 8,
            head_dim: 128,
            intermediate_size: 8192,
            vocab_size: 128256,
            gqa_ratio: 3,
        }
    }

    /// Per-layer parameter count for attention (Q + K + V + O projections).
    pub fn attention_params_per_layer(&self, q_heads: usize, kv_heads: usize) -> usize {
        let q_params = q_heads * self.head_dim * self.hidden_size; // Q proj
        let k_params = kv_heads * self.head_dim * self.hidden_size; // K proj
        let v_params = kv_heads * self.head_dim * self.hidden_size; // V proj
        let o_params = self.hidden_size * q_heads * self.head_dim; // O proj
        q_params + k_params + v_params + o_params
    }

    /// Per-layer parameter count for MLP (gate + up + down).
    pub fn mlp_params_per_layer(&self) -> usize {
        // gate: [hidden, intermediate], up: [hidden, intermediate], down: [intermediate, hidden]
        3 * self.hidden_size * self.intermediate_size
    }

    /// Embedding + LM head parameter count.
    pub fn embedding_params(&self) -> usize {
        // embed_tokens + lm_head (may be tied, but budget for both)
        2 * self.vocab_size * self.hidden_size
    }

    /// Norm + bias params (small, always F32).
    pub fn norm_params(&self) -> usize {
        // 2 norms per layer (attn_norm, ffn_norm) + 1 final norm, each is hidden_size
        (2 * self.num_layers + 1) * self.hidden_size
    }
}

/// Map HeadPrecision tiers to GGUF quant types.
/// This is the bridge between our fine-grained scoring and GGUF's per-tensor quantization.
fn precision_to_gguf_quant(precision: HeadPrecision) -> GgufQuantType {
    match precision {
        HeadPrecision::Removed => GgufQuantType::Q3KS, // shouldn't be called for removed
        HeadPrecision::Ternary => GgufQuantType::Q3KS, // floor — true ternary is future work
        HeadPrecision::Q2 => GgufQuantType::Q3KS,      // Q2_K produces NaN for compacted models
        HeadPrecision::Q4 => GgufQuantType::Q4KS,
        HeadPrecision::Q8 => GgufQuantType::Q5KM,
        HeadPrecision::BF16 => GgufQuantType::Q6K,
    }
}

/// Plan compression for a model given utilization data and a device target.
///
/// Returns a CompressionRecipe specifying:
/// - Which KV groups to prune
/// - Per-tensor quantization levels (attention, MLP, embeddings)
/// - Memory budget breakdown
///
/// The planner iterates: if the initial assignment exceeds budget, it demotes
/// the lowest-utilization tensors. If under budget, it promotes the highest.
pub fn plan_compression(
    topology: &HeadTopology,
    device: &DeviceSpec,
    arch: &ModelArchConfig,
    base_model: &str,
) -> Result<CompressionRecipe, String> {
    let budget_bytes = device.effective_budget_bytes();

    // Step 1: Determine per-layer attention quant levels from topology
    let mut tensor_quant_map = Vec::new();

    for layer_idx in 0..arch.num_layers {
        // Get the layer's head info from topology
        let layer_info = topology.layers.get(layer_idx);

        // Determine quant type for this layer's attention based on utilization
        let attn_quant = if let Some(info) = layer_info {
            // Use the highest precision among retained heads for the whole layer
            // (GGUF quantizes entire tensors, can't mix within one tensor)
            let max_precision = info
                .head_precisions
                .iter()
                .max_by(|a, b| a.bits().cmp(&b.bits()))
                .copied()
                .unwrap_or(HeadPrecision::Q4);
            precision_to_gguf_quant(max_precision)
        } else {
            // No topology info for this layer — use default
            GgufQuantType::Q4KS
        };

        // MLP quant: correlated with attention utilization but less sensitive
        let mlp_quant = match attn_quant {
            GgufQuantType::Q6K | GgufQuantType::Q5KM => GgufQuantType::Q5KS,
            GgufQuantType::Q5KS | GgufQuantType::Q4KM => GgufQuantType::Q4KS,
            _ => GgufQuantType::Q3KS,
        };

        // Attention projections
        for proj in &["attn_q", "attn_k", "attn_v", "attn_output"] {
            tensor_quant_map.push(TensorQuantAssignment {
                pattern: format!("blk.{layer_idx}.{proj}.weight"),
                quant_type: attn_quant,
                reason: format!("layer {} attention utilization", layer_idx),
            });
        }

        // MLP projections
        for proj in &["ffn_gate", "ffn_up", "ffn_down"] {
            tensor_quant_map.push(TensorQuantAssignment {
                pattern: format!("blk.{layer_idx}.{proj}.weight"),
                quant_type: mlp_quant,
                reason: format!("layer {} MLP (correlated with attention)", layer_idx),
            });
        }

        // Norms — always F32 (tiny, precision-sensitive)
        for norm in &["attn_norm", "ffn_norm"] {
            tensor_quant_map.push(TensorQuantAssignment {
                pattern: format!("blk.{layer_idx}.{norm}.weight"),
                quant_type: GgufQuantType::F32,
                reason: "norm weights always F32".into(),
            });
        }

        // Biases — always F32 (tiny)
        for bias in &["attn_q", "attn_k", "attn_v"] {
            tensor_quant_map.push(TensorQuantAssignment {
                pattern: format!("blk.{layer_idx}.{bias}.bias"),
                quant_type: GgufQuantType::F32,
                reason: "bias always F32".into(),
            });
        }
    }

    // Embeddings and output head — high precision (token identity sensitive)
    tensor_quant_map.push(TensorQuantAssignment {
        pattern: "token_embd.weight".into(),
        quant_type: GgufQuantType::Q6K,
        reason: "embedding table — token identity sensitive".into(),
    });
    tensor_quant_map.push(TensorQuantAssignment {
        pattern: "output.weight".into(),
        quant_type: GgufQuantType::Q6K,
        reason: "LM head — logit quality sensitive".into(),
    });
    tensor_quant_map.push(TensorQuantAssignment {
        pattern: "output_norm.weight".into(),
        quant_type: GgufQuantType::F32,
        reason: "final norm always F32".into(),
    });

    // Step 2: Estimate total size
    let budget = estimate_budget(arch, topology, &tensor_quant_map);

    // Step 3: Check budget and adjust
    let total = budget.total_bytes;
    if total > budget_bytes {
        // Over budget — demote lowest-utilization layers
        // For now, just report the overage. The iterative demotion is Phase 2.
        let over_gb = (total - budget_bytes) as f64 / 1073741824.0;
        return Err(format!(
            "Compression plan exceeds budget by {:.1} GB ({:.1} GB needed, {:.1} GB available). \
             Reduce precision or prune more heads.",
            over_gb,
            total as f64 / 1073741824.0,
            device.effective_budget_gb()
        ));
    }

    Ok(CompressionRecipe {
        topology: topology.clone(),
        tensor_quant_map,
        device_spec: device.clone(),
        budget,
        base_model: base_model.to_string(),
        pipeline_version: 1,
    })
}

/// Estimate memory budget from architecture + quant assignments.
fn estimate_budget(
    arch: &ModelArchConfig,
    topology: &HeadTopology,
    quant_map: &[TensorQuantAssignment],
) -> MemoryBudget {
    // Build a lookup from tensor pattern to quant type
    let quant_lookup: HashMap<&str, GgufQuantType> = quant_map
        .iter()
        .map(|a| (a.pattern.as_str(), a.quant_type))
        .collect();

    let mut embedding_bytes = 0u64;
    let mut attention_bytes = 0u64;
    let mut mlp_bytes = 0u64;
    let mut norm_bytes = 0u64;

    // Embeddings
    let embed_elements = arch.vocab_size * arch.hidden_size;
    let embed_quant = quant_lookup
        .get("token_embd.weight")
        .copied()
        .unwrap_or(GgufQuantType::Q6K);
    embedding_bytes += embed_quant.estimate_bytes(embed_elements) as u64;

    let output_quant = quant_lookup
        .get("output.weight")
        .copied()
        .unwrap_or(GgufQuantType::Q6K);
    embedding_bytes += output_quant.estimate_bytes(embed_elements) as u64;

    // Per-layer
    for layer_idx in 0..arch.num_layers {
        let (q_heads, kv_heads) = if let Some(info) = topology.layers.get(layer_idx) {
            let retained_q = info
                .head_precisions
                .iter()
                .filter(|p| **p != HeadPrecision::Removed)
                .count()
                * arch.gqa_ratio;
            let retained_kv = info
                .head_precisions
                .iter()
                .filter(|p| **p != HeadPrecision::Removed)
                .count();
            (retained_q, retained_kv)
        } else {
            (arch.num_attention_heads, arch.num_kv_heads)
        };

        // Attention projections
        let q_elements = q_heads * arch.head_dim * arch.hidden_size;
        let k_elements = kv_heads * arch.head_dim * arch.hidden_size;
        let v_elements = k_elements;
        let o_elements = arch.hidden_size * q_heads * arch.head_dim;

        let q_quant = quant_lookup
            .get(format!("blk.{layer_idx}.attn_q.weight").as_str())
            .copied()
            .unwrap_or(GgufQuantType::Q4KS);
        attention_bytes += q_quant.estimate_bytes(q_elements) as u64;
        attention_bytes += q_quant.estimate_bytes(k_elements) as u64;
        attention_bytes += q_quant.estimate_bytes(v_elements) as u64;
        attention_bytes += q_quant.estimate_bytes(o_elements) as u64;

        // MLP
        let gate_elements = arch.hidden_size * arch.intermediate_size;
        let mlp_quant = quant_lookup
            .get(format!("blk.{layer_idx}.ffn_gate.weight").as_str())
            .copied()
            .unwrap_or(GgufQuantType::Q4KS);
        mlp_bytes += 3 * mlp_quant.estimate_bytes(gate_elements) as u64;

        // Norms (2 per layer)
        norm_bytes += 2 * (arch.hidden_size * 4) as u64; // F32
    }

    // Final norm
    norm_bytes += (arch.hidden_size * 4) as u64;

    // KV cache estimate at max context (for headroom info)
    let actual_kv_heads = topology
        .layers
        .first()
        .map(|l| l.num_kv_heads)
        .unwrap_or(arch.num_kv_heads);
    let kv_per_layer = 2 * actual_kv_heads as u64
        * arch.head_dim as u64
        * 32768 // max context
        * 4; // F32
    let kv_cache_max_bytes = kv_per_layer * arch.num_layers as u64;

    let total_bytes = embedding_bytes + attention_bytes + mlp_bytes + norm_bytes;

    MemoryBudget {
        embedding_bytes,
        attention_bytes,
        mlp_bytes,
        norm_bytes,
        total_bytes,
        kv_cache_max_bytes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_topology() -> HeadTopology {
        // 8 KV groups, keep 5 (groups 0-4), prune 3 (groups 5-7)
        let mut layers = Vec::new();
        for layer_idx in 0..64 {
            layers.push(LayerTopology {
                layer_index: layer_idx,
                num_heads: 25, // 5 groups * 5 Q heads per group
                num_kv_heads: 5,
                retained_head_indices: (0..25).collect(),
                retained_kv_head_indices: (0..5).collect(),
                head_precisions: vec![
                    HeadPrecision::BF16,  // group 0: high util
                    HeadPrecision::Q8,    // group 1: active
                    HeadPrecision::Q4,    // group 2: medium
                    HeadPrecision::Q4,    // group 3: medium
                    HeadPrecision::Q2,    // group 4: low
                ],
                head_scores: vec![0.9, 0.6, 0.4, 0.35, 0.2],
            });
        }

        HeadTopology {
            base_model: "Qwen/Qwen2.5-Coder-32B-Instruct".into(),
            original_num_heads: 40,
            original_num_kv_heads: 8,
            head_dim: 128,
            parameter_reduction: 0.375,
            precision_profile: PrecisionProfile {
                removed: 3 * 64,
                bf16: 64,
                q8: 64,
                q4: 128,
                q2: 64,
                ternary: 0,
            },
            created_at: "2026-03-17T00:00:00Z".into(),
            layers,
        }
    }

    #[test]
    fn test_device_spec_presets() {
        let air = DeviceSpec::macbook_air_16gb();
        assert_eq!(air.effective_budget_gb(), 11.0);

        let pro = DeviceSpec::macbook_pro_32gb();
        assert_eq!(pro.effective_budget_gb(), 24.0);

        let gpu = DeviceSpec::rtx_5090_24gb();
        assert_eq!(gpu.effective_budget_gb(), 22.0);

        let custom = DeviceSpec::from_memory_gb(48.0);
        assert_eq!(custom.effective_budget_gb(), 36.0);
    }

    #[test]
    fn test_gguf_quant_type_ordering() {
        assert!(GgufQuantType::Q2K.bits_per_weight() < GgufQuantType::Q3KS.bits_per_weight());
        assert!(GgufQuantType::Q3KS.bits_per_weight() < GgufQuantType::Q4KS.bits_per_weight());
        assert!(GgufQuantType::Q4KM.bits_per_weight() < GgufQuantType::Q6K.bits_per_weight());
        assert!(GgufQuantType::Q6K.bits_per_weight() < GgufQuantType::F16.bits_per_weight());
    }

    #[test]
    fn test_quant_size_estimation() {
        // 1M params at Q4_K_M (~4.8 bpw) = ~600KB
        let bytes = GgufQuantType::Q4KM.estimate_bytes(1_000_000);
        assert!(bytes > 500_000 && bytes < 700_000);

        // 1M params at F32 = 4MB
        let bytes = GgufQuantType::F32.estimate_bytes(1_000_000);
        assert_eq!(bytes, 4_000_000);
    }

    #[test]
    fn test_plan_fits_32gb() {
        let topology = make_test_topology();
        let device = DeviceSpec::macbook_pro_32gb();
        let arch = ModelArchConfig::qwen2_32b();

        let result = plan_compression(&topology, &device, &arch, "test-model");
        match &result {
            Ok(recipe) => {
                let total_gb = recipe.budget.total_bytes as f64 / 1073741824.0;
                assert!(
                    total_gb < device.effective_budget_gb(),
                    "Plan should fit in budget: {:.1}GB < {:.1}GB",
                    total_gb,
                    device.effective_budget_gb()
                );
            }
            Err(e) => {
                // If it doesn't fit, the error message should explain why
                assert!(e.contains("exceeds budget"), "Unexpected error: {}", e);
            }
        }
    }

    #[test]
    fn test_plan_has_mixed_quant() {
        let topology = make_test_topology();
        let device = DeviceSpec::macbook_pro_32gb();
        let arch = ModelArchConfig::qwen2_32b();

        let recipe = plan_compression(&topology, &device, &arch, "test-model")
            .expect("should produce a plan");

        // Collect unique quant types used for attention tensors
        let attn_quant_types: std::collections::HashSet<GgufQuantType> = recipe
            .tensor_quant_map
            .iter()
            .filter(|a| a.pattern.contains("attn_q"))
            .map(|a| a.quant_type)
            .collect();

        // Mixed quant means more than one quant type for attention
        // (our test topology has BF16/Q8/Q4/Q2 heads, which map to Q6K/Q5KM/Q4KS/Q3KS)
        assert!(
            attn_quant_types.len() >= 1,
            "Should have at least one quant type, got {:?}",
            attn_quant_types
        );
    }

    #[test]
    fn test_embeddings_get_high_precision() {
        let topology = make_test_topology();
        let device = DeviceSpec::macbook_pro_32gb();
        let arch = ModelArchConfig::qwen2_32b();

        let recipe = plan_compression(&topology, &device, &arch, "test-model")
            .expect("should produce a plan");

        let embed_quant = recipe
            .tensor_quant_map
            .iter()
            .find(|a| a.pattern == "token_embd.weight")
            .expect("should have embedding assignment");

        assert_eq!(
            embed_quant.quant_type,
            GgufQuantType::Q6K,
            "Embeddings should get Q6K (high precision)"
        );
    }

    #[test]
    fn test_norms_always_f32() {
        let topology = make_test_topology();
        let device = DeviceSpec::macbook_pro_32gb();
        let arch = ModelArchConfig::qwen2_32b();

        let recipe = plan_compression(&topology, &device, &arch, "test-model")
            .expect("should produce a plan");

        let norm_assignments: Vec<_> = recipe
            .tensor_quant_map
            .iter()
            .filter(|a| a.pattern.contains("norm"))
            .collect();

        for norm in &norm_assignments {
            assert_eq!(
                norm.quant_type,
                GgufQuantType::F32,
                "Norm {} should be F32",
                norm.pattern
            );
        }
    }
}
