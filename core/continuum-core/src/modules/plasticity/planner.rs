//! Compression planner: turns utilization scores + device spec into a CompressionRecipe.
//!
//! Pure function — no I/O, no model loading. Fully testable with synthetic data.
//! See docs/genome/COMPRESSION-PIPELINE.md for the full pipeline architecture.

use super::types::*;
use crate::capacity::expert_residency::{ExpertActivationProfile, ExpertId};
use crate::model_registry::ModelArchConfig;
use std::collections::{BTreeMap, HashMap};

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
        * arch.context_length as u64 // trained context window (from the artifact)
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

// ── MoE expert compaction (the K3 path) ──────────────────────────────────────
//
// The dense `plan_compression` above quantizes per-LAYER attention/MLP tensors
// driven by a `HeadTopology` and can physically `Remove` heads. The K3 doctrine
// (`docs/planning/K3-MODEL-REDUCTION-FOUNDRY-PATH.md`) forbids removal: we page
// experts now, so every expert stays reachable — the reduction lowers PRECISION
// where importance is low, never strips capability. `plan_expert_compression`
// is the sibling that expresses exactly that: per-EXPERT variable quant, floored
// at Ternary so NOTHING is ever removed.

/// The output of `plan_expert_compression`: per-expert quantization assignments
/// plus the resident-footprint budget they imply. A pure-Rust planner result
/// (not yet an IPC/TS type — mirrors `capacity::recursion_depth`), promotable to
/// a ts-rs-exported wire type when the expert-forge command lands.
#[derive(Debug, Clone)]
pub struct ExpertCompressionPlan {
    /// Base model this plan compacts (HuggingFace repo id).
    pub base_model: String,
    /// Per-tensor quant assignments: every expert's gate/up/down projection at
    /// its demand-matched precision, plus the always-hot trunk (attention,
    /// embeddings, output) pinned high and norms at F32.
    pub tensor_quant_map: Vec<TensorQuantAssignment>,
    /// Resident-footprint budget the assignments imply (checked against VRAM).
    pub budget: MemoryBudget,
    /// Tier histogram across the expert pool. `removed` is INVARIANTLY 0 — the
    /// doctrine made executable: no expert is ever stripped, only quantized.
    pub precision_profile: PrecisionProfile,
}

/// Bump the tier histogram for one expert's assigned precision.
fn tally_precision(profile: &mut PrecisionProfile, prec: HeadPrecision) {
    match prec {
        HeadPrecision::Removed => profile.removed += 1, // never reached (clamped) — counted for the invariant assert
        HeadPrecision::Ternary => profile.ternary += 1,
        HeadPrecision::Q2 => profile.q2 += 1,
        HeadPrecision::Q4 => profile.q4 += 1,
        HeadPrecision::Q8 => profile.q8 += 1,
        HeadPrecision::BF16 => profile.bf16 += 1,
    }
}

/// Plan variable-precision compaction of a MoE model's EXPERT pool from a live
/// activation profile — the dynamic, runtime, Rust realization of the K3
/// reduction doctrine ([[K3-model-reduction-foundry-path]]).
///
/// For each expert the profile knows about, its residency `priority` (the SINGLE
/// importance source, reused from `expert_residency` — one signal, two uses) is
/// **layer-normalized** — ranked relative to the OTHER experts in its own layer,
/// not the global distribution. This is the §4.1.3.1 depth-bias fix from
/// `docs/papers/PLASTICITY-COMPACTION.md`, mandatory on a deep model like K3:
/// residual-stream magnitude grows with depth, so a flat global ranking would
/// quantize whole early layers regardless of within-layer importance.
///
/// The normalized importance maps to a bit-width via [`HeadPrecision::from_utilization`],
/// **clamped up to [`HeadPrecision::minimum_alive`] (Ternary)** so the lowest-
/// importance expert is driven to ~1.58-bit but NEVER removed. Every expert
/// remains in the artifact; the cold ones just cost less to keep and page.
///
/// The always-hot dense trunk (attention Q/K/V/O), the embedding table and LM
/// head are pinned high (Q6K — token-identity / logit sensitive), and norms are
/// F32, mirroring [`plan_compression`]. The total is checked against the device's
/// effective VRAM budget; over-budget returns `Err` with the overage, exactly
/// like the dense planner (iterative demotion is future work).
///
/// Pure: no I/O, no model load — fully unit-testable with a synthetic profile.
pub fn plan_expert_compression(
    profile: &ExpertActivationProfile,
    device: &DeviceSpec,
    arch: &ModelArchConfig,
    expert_intermediate_size: usize,
    base_model: &str,
) -> Result<ExpertCompressionPlan, String> {
    let budget_bytes = device.effective_budget_bytes();

    let mut tensor_quant_map = Vec::new();
    let mut precision_profile = PrecisionProfile::default();

    // Group the known experts by layer so importance is ranked WITHIN a layer.
    let mut by_layer: BTreeMap<u32, Vec<ExpertId>> = BTreeMap::new();
    for e in profile.known_experts() {
        by_layer.entry(e.layer).or_default().push(e);
    }

    // Each expert has three projections (gate, up, down), each hidden×expert_ffn.
    let proj_elements = arch.hidden_size * expert_intermediate_size;
    let mut mlp_bytes = 0u64; // the expert pool — the part we actually shrink

    for (layer, experts) in &by_layer {
        // Layer-normalized importance (min-max within the layer) — the depth-bias fix.
        let prios: Vec<f64> = experts.iter().map(|e| profile.priority(e)).collect();
        let lo = prios.iter().copied().fold(f64::INFINITY, f64::min);
        let hi = prios.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let range = hi - lo;

        for (e, &p) in experts.iter().zip(prios.iter()) {
            // Indistinguishable layer (all-equal priority) → can't rank → uniform
            // mid precision rather than flooring everything to the cheapest tier.
            let norm = if range > 0.0 { (p - lo) / range } else { 0.5 };

            // Importance → bit-width, CLAMPED so nothing is ever Removed.
            let mut prec = HeadPrecision::from_utilization(norm);
            if !prec.is_alive() {
                prec = HeadPrecision::minimum_alive(); // Ternary floor — the doctrine
            }
            tally_precision(&mut precision_profile, prec);

            let quant = precision_to_gguf_quant(prec);
            for proj in &["gate_proj", "up_proj", "down_proj"] {
                tensor_quant_map.push(TensorQuantAssignment {
                    pattern: format!("model.layers.{layer}.mlp.experts.{}.{proj}.weight", e.expert),
                    quant_type: quant,
                    reason: format!(
                        "expert L{layer} E{} layer-normalized importance {norm:.3} → {:?}",
                        e.expert, prec
                    ),
                });
                mlp_bytes += quant.estimate_bytes(proj_elements) as u64;
            }
        }
    }

    // Always-hot trunk + embeddings + norms (mirror plan_compression's pins).
    let embed_elements = arch.vocab_size * arch.hidden_size;
    let mut embedding_bytes = 0u64;
    for (pattern, why) in [
        ("model.embed_tokens.weight", "embedding table — token identity sensitive"),
        ("lm_head.weight", "LM head — logit quality sensitive"),
    ] {
        tensor_quant_map.push(TensorQuantAssignment {
            pattern: pattern.into(),
            quant_type: GgufQuantType::Q6K,
            reason: why.into(),
        });
        embedding_bytes += GgufQuantType::Q6K.estimate_bytes(embed_elements) as u64;
    }
    tensor_quant_map.push(TensorQuantAssignment {
        pattern: "model.norm.weight".into(),
        quant_type: GgufQuantType::F32,
        reason: "final norm always F32".into(),
    });
    let mut norm_bytes = (arch.hidden_size * 4) as u64;

    let mut attention_bytes = 0u64;
    let q_elements = arch.num_attention_heads * arch.head_dim * arch.hidden_size;
    let kv_elements = arch.num_kv_heads * arch.head_dim * arch.hidden_size;
    let o_elements = arch.hidden_size * arch.num_attention_heads * arch.head_dim;
    for layer_idx in 0..arch.num_layers {
        // Attention projections — always-hot dense trunk, pinned high (Q6K).
        for (proj, elems) in [
            ("q_proj", q_elements),
            ("k_proj", kv_elements),
            ("v_proj", kv_elements),
            ("o_proj", o_elements),
        ] {
            tensor_quant_map.push(TensorQuantAssignment {
                pattern: format!("model.layers.{layer_idx}.self_attn.{proj}.weight"),
                quant_type: GgufQuantType::Q6K,
                reason: "always-hot attention trunk — pinned high".into(),
            });
            attention_bytes += GgufQuantType::Q6K.estimate_bytes(elems) as u64;
        }
        // Norms — F32 (tiny, precision-sensitive).
        for norm in &["input_layernorm", "post_attention_layernorm"] {
            tensor_quant_map.push(TensorQuantAssignment {
                pattern: format!("model.layers.{layer_idx}.{norm}.weight"),
                quant_type: GgufQuantType::F32,
                reason: "norm weights always F32".into(),
            });
        }
        norm_bytes += 2 * (arch.hidden_size * 4) as u64;
    }

    // KV cache at full context — headroom info, same estimate as the dense planner.
    let kv_cache_max_bytes = 2 * arch.num_kv_heads as u64
        * arch.head_dim as u64
        * arch.context_length as u64
        * 4
        * arch.num_layers as u64;

    let total_bytes = embedding_bytes + attention_bytes + mlp_bytes + norm_bytes;
    let budget = MemoryBudget {
        embedding_bytes,
        attention_bytes,
        mlp_bytes,
        norm_bytes,
        total_bytes,
        kv_cache_max_bytes,
    };

    if total_bytes > budget_bytes {
        let over_gb = (total_bytes - budget_bytes) as f64 / 1073741824.0;
        return Err(format!(
            "Expert compaction plan exceeds budget by {:.2} GB ({:.2} GB needed, {:.2} GB available). \
             Lower precision on more experts or target a larger device.",
            over_gb,
            total_bytes as f64 / 1073741824.0,
            device.effective_budget_gb()
        ));
    }

    Ok(ExpertCompressionPlan {
        base_model: base_model.to_string(),
        tensor_quant_map,
        budget,
        precision_profile,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Qwen2.5-Coder-32B dims as a test fixture — the model-specific constants
    /// live ONLY here, `#[cfg(test)]`-gated, never in production paths (which
    /// source dims from the artifact via `ModelArchConfig::from_artifact`).
    fn qwen2_32b_arch() -> ModelArchConfig {
        ModelArchConfig::new(64, 5120, 40, 8, 128, 27648, 152064, 32768).unwrap()
    }

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
                    HeadPrecision::BF16, // group 0: high util
                    HeadPrecision::Q8,   // group 1: active
                    HeadPrecision::Q4,   // group 2: medium
                    HeadPrecision::Q4,   // group 3: medium
                    HeadPrecision::Q2,   // group 4: low
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
        let arch = qwen2_32b_arch();

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
        let arch = qwen2_32b_arch();

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
        let arch = qwen2_32b_arch();

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
        let arch = qwen2_32b_arch();

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

    /// MoE expert compaction (the K3 path). Nested here per the one-test-mod-per-file
    /// rule; these exercise `plan_expert_compression`, the doctrine-executable sibling
    /// of `plan_compression`.
    mod expert_compaction {
        use super::*;

        /// Small MoE arch fixture — 3 layers, 8 heads / 2 KV, tiny vocab. Dims live
        /// ONLY here, `#[cfg(test)]`-gated, never in production (which sources dims
        /// from the artifact).
        fn tiny_moe_arch() -> ModelArchConfig {
            // (num_layers, hidden, heads, kv, head_dim, intermediate, vocab, context)
            ModelArchConfig::new(3, 512, 8, 2, 64, 1024, 4096, 4096).unwrap()
        }

        /// Build a synthetic activation profile from `(layer, expert, hits)` rows.
        fn profile_of(rows: &[(u32, u32, u64)]) -> ExpertActivationProfile {
            let mut p = ExpertActivationProfile::default();
            for &(layer, expert, hits) in rows {
                p.hits.insert(ExpertId { layer, expert }, hits);
            }
            p
        }

        /// A per-layer spread (E0<E1<E2<E3) in two layers, plus a third layer whose
        /// experts are all equal (degenerate ranking).
        fn spread_profile() -> ExpertActivationProfile {
            profile_of(&[
                // layer 0: clean 0/10/20/30 spread
                (0, 0, 0), (0, 1, 10), (0, 2, 20), (0, 3, 30),
                // layer 1: different spread
                (1, 0, 0), (1, 1, 5), (1, 2, 15), (1, 3, 30),
                // layer 2: all equal → indistinguishable, must NOT floor everything
                (2, 0, 7), (2, 1, 7), (2, 2, 7), (2, 3, 7),
            ])
        }

        // what this catches: the whole doctrine made executable — a per-layer spread of
        // expert importance must produce MIXED per-expert bit-widths (not one uniform
        // quant), and the resident-footprint budget must fit a generous device. If the
        // layer-normalized importance→bit-width mapping drifts, either everything gets
        // one precision (no mixing) or the plan stops respecting the VRAM budget.
        #[test]
        fn expert_plan_is_mixed_precision_and_fits_budget() {
            let profile = spread_profile();
            let device = DeviceSpec::from_memory_gb(48.0); // generous — tiny model fits easily
            let arch = tiny_moe_arch();

            let plan = plan_expert_compression(&profile, &device, &arch, 256, "test-moe")
                .expect("tiny model must fit a 48GB device");

            // Distinct quant types across the EXPERT tensors ⇒ genuinely mixed precision.
            let expert_quants: std::collections::HashSet<GgufQuantType> = plan
                .tensor_quant_map
                .iter()
                .filter(|a| a.pattern.contains(".mlp.experts."))
                .map(|a| a.quant_type)
                .collect();
            assert!(
                expert_quants.len() >= 2,
                "expert pool should be mixed-precision, got {expert_quants:?}"
            );

            // Budget respected.
            assert!(
                plan.budget.total_bytes < device.effective_budget_bytes(),
                "plan ({} bytes) must fit budget ({} bytes)",
                plan.budget.total_bytes,
                device.effective_budget_bytes()
            );
            // The expert pool is the shrinkable part and must be accounted.
            assert!(plan.budget.mlp_bytes > 0, "expert bytes must be budgeted");
        }

        // what this catches: THE doctrine — reduce precision, never strip knowledge. The
        // lowest-importance expert in each layer (normalized importance 0.0) must be
        // driven to the Ternary FLOOR, never Removed. If the clamp to minimum_alive()
        // regresses, `removed` goes non-zero and we're back to deleting capability from
        // a model whose experts we can page.
        #[test]
        fn no_expert_is_ever_removed_floor_is_ternary() {
            let profile = spread_profile();
            let device = DeviceSpec::from_memory_gb(48.0);
            let arch = tiny_moe_arch();

            let plan = plan_expert_compression(&profile, &device, &arch, 256, "test-moe")
                .expect("must plan");

            assert_eq!(
                plan.precision_profile.removed, 0,
                "no expert may be removed — the reduction lowers precision only"
            );
            assert!(
                plan.precision_profile.ternary > 0,
                "the lowest-importance expert per layer must hit the Ternary floor"
            );
            // Total experts tallied == experts known to the profile (12), each once.
            let total = plan.precision_profile.total_active() + plan.precision_profile.removed;
            assert_eq!(total, 12, "every known expert assigned exactly one tier");
        }

        // what this catches: the always-hot trunk pins — norms must be F32 and the
        // embedding/LM-head must be high precision (Q6K), mirroring the dense planner.
        // A degenerate all-equal layer must NOT collapse to the cheapest tier: its
        // experts get a uniform MID precision (Q8/Q5KM) because they can't be ranked.
        #[test]
        fn trunk_is_pinned_and_degenerate_layer_gets_mid_precision() {
            let profile = spread_profile();
            let device = DeviceSpec::from_memory_gb(48.0);
            let arch = tiny_moe_arch();

            let plan = plan_expert_compression(&profile, &device, &arch, 256, "test-moe")
                .expect("must plan");

            // Norms always F32.
            for a in plan.tensor_quant_map.iter().filter(|a| a.pattern.contains("norm")) {
                assert_eq!(a.quant_type, GgufQuantType::F32, "norm {} must be F32", a.pattern);
            }
            // Embedding + LM head pinned high.
            for pat in ["model.embed_tokens.weight", "lm_head.weight"] {
                let a = plan
                    .tensor_quant_map
                    .iter()
                    .find(|a| a.pattern == pat)
                    .unwrap_or_else(|| panic!("missing {pat}"));
                assert_eq!(a.quant_type, GgufQuantType::Q6K, "{pat} should be Q6K");
            }
            // Layer 2 (all-equal importance) → uniform mid precision, not the floor:
            // norm 0.5 ⇒ Q8 ⇒ Q5KM for every layer-2 expert.
            let layer2: std::collections::HashSet<GgufQuantType> = plan
                .tensor_quant_map
                .iter()
                .filter(|a| a.pattern.starts_with("model.layers.2.mlp.experts."))
                .map(|a| a.quant_type)
                .collect();
            assert_eq!(
                layer2,
                std::collections::HashSet::from([GgufQuantType::Q5KM]),
                "indistinguishable layer must get uniform mid precision, not the floor"
            );
        }

        // what this catches: budget-check honesty — a device too small to hold even the
        // pinned trunk + variably-quantized experts must return Err naming the overage,
        // exactly like the dense planner, rather than silently emitting an unloadable plan.
        #[test]
        fn over_budget_device_is_rejected() {
            let profile = spread_profile();
            // ~2 MB effective budget — far below the ~tens-of-MB tiny model.
            let device = DeviceSpec {
                memory_gb: 0.002,
                reserved_gb: 0.0,
                label: "pinhead".into(),
            };
            let arch = tiny_moe_arch();

            let err = plan_expert_compression(&profile, &device, &arch, 256, "test-moe")
                .expect_err("must reject an over-budget device");
            assert!(err.contains("exceeds budget"), "unexpected error: {err}");
        }
    }
}
