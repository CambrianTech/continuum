//! Plasticity type definitions with ts-rs exports.
//!
//! These types define the neural plasticity optimization system:
//! per-head utilization scoring, topology after compaction, and
//! mixed-precision quantization tiers.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Per-head quantization precision tier, assigned based on utilization score.
///
/// Seven tiers from physical removal to full precision, enabling aggressive
/// task-specific compaction. Sub-4-bit tiers (Ternary, Q2) allow dormant heads
/// to be retained at near-zero cost — critical for fitting large models (32B+)
/// on consumer hardware when most heads are unused for the target task.
///
/// | Utilization     | Precision | Bits | Action                           |
/// |-----------------|-----------|------|----------------------------------|
/// | Dead (< 0.1)    | Removed   | 0    | Physically removed from tensor   |
/// | Dormant (0.1-0.2)| Ternary  | 1.58 | {-1, 0, +1} with scale factor   |
/// | Low (0.2-0.3)   | Q2        | 2    | 2-bit signed integer [-1,0,1,2]  |
/// | Medium (0.3-0.5) | Q4       | 4    | 4-bit quantized (NF4/INT4)       |
/// | Active (0.5-0.7) | Q8       | 8    | 8-bit quantized                  |
/// | Hot (0.7+)       | BF16     | 16   | Full precision                   |
///
/// Research basis: BitNet (1-bit LLMs), AQLM, QuIP# demonstrate that
/// sub-4-bit quantization preserves model quality when applied selectively.
/// Our advantage: we KNOW which heads matter (utilization data), so we apply
/// aggressive quantization only where it's safe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../shared/generated/plasticity/HeadPrecision.ts"
)]
#[serde(rename_all = "lowercase")]
pub enum HeadPrecision {
    /// Utilization < 0.1: head physically removed from tensor
    Removed,
    /// Utilization 0.1-0.2: ternary quantization {-1, 0, +1} with per-block scale
    /// 1.58 bits per parameter (log2(3)). Inspired by BitNet b1.58.
    /// Three values packed efficiently: 5 ternary values per byte (3^5 = 243 < 256).
    Ternary,
    /// Utilization 0.2-0.3: 2-bit signed integer [-1, 0, 1, 2] with per-block scale
    /// 4 values packed per byte. Low fidelity but retains directional information.
    Q2,
    /// Utilization 0.3-0.5: 4-bit quantized (NF4/INT4)
    Q4,
    /// Utilization 0.5-0.7: 8-bit quantized
    Q8,
    /// Utilization 0.7+: full BF16 precision
    BF16,
}

impl HeadPrecision {
    /// Bits per parameter for this precision tier.
    /// Ternary is 1.58 bits (log2(3)) but we report 2 for byte-aligned calculation.
    /// Use `bits_effective()` for the true information-theoretic bits.
    pub fn bits(&self) -> u8 {
        match self {
            HeadPrecision::Removed => 0,
            HeadPrecision::Ternary => 2, // Packed: 5 values per byte = 1.6 bits/value
            HeadPrecision::Q2 => 2,
            HeadPrecision::Q4 => 4,
            HeadPrecision::Q8 => 8,
            HeadPrecision::BF16 => 16,
        }
    }

    /// Effective bits per parameter (information-theoretic).
    /// Ternary: log2(3) ≈ 1.585. Others match their nominal bit width.
    pub fn bits_effective(&self) -> f64 {
        match self {
            HeadPrecision::Removed => 0.0,
            HeadPrecision::Ternary => 1.585, // log2(3)
            HeadPrecision::Q2 => 2.0,
            HeadPrecision::Q4 => 4.0,
            HeadPrecision::Q8 => 8.0,
            HeadPrecision::BF16 => 16.0,
        }
    }

    /// Bytes per parameter for memory estimation.
    /// Accounts for packing efficiency:
    /// - Ternary: 5 values per byte (1.6 bits/value, wastes 0.015 bits)
    /// - Q2: 4 values per byte (exact)
    /// - Q4: 2 values per byte (exact)
    pub fn bytes_per_param(&self) -> f64 {
        match self {
            HeadPrecision::Removed => 0.0,
            HeadPrecision::Ternary => 0.2,  // 1 byte per 5 values
            HeadPrecision::Q2 => 0.25,      // 1 byte per 4 values
            HeadPrecision::Q4 => 0.5,       // 1 byte per 2 values
            HeadPrecision::Q8 => 1.0,
            HeadPrecision::BF16 => 2.0,
        }
    }

    /// Determine precision tier from utilization score using default thresholds.
    pub fn from_utilization(score: f64) -> Self {
        if score < 0.1 {
            HeadPrecision::Removed
        } else if score < 0.2 {
            HeadPrecision::Ternary
        } else if score < 0.3 {
            HeadPrecision::Q2
        } else if score < 0.5 {
            HeadPrecision::Q4
        } else if score < 0.7 {
            HeadPrecision::Q8
        } else {
            HeadPrecision::BF16
        }
    }

    /// Whether this precision level retains the head (i.e., not physically removed).
    pub fn is_alive(&self) -> bool {
        *self != HeadPrecision::Removed
    }

    /// Minimum precision for GQA group integrity promotion.
    /// When a dead head must survive because its KV group has live Q heads,
    /// promote to this tier (cheapest non-removed precision).
    pub fn minimum_alive() -> Self {
        HeadPrecision::Ternary
    }
}

/// Per-layer topology after compaction: which heads survived and at what precision.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../shared/generated/plasticity/LayerTopology.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct LayerTopology {
    /// Layer index in the original model
    pub layer_index: usize,
    /// Number of active Q heads after compaction
    #[ts(type = "number")]
    pub num_heads: usize,
    /// Number of active KV heads after compaction
    #[ts(type = "number")]
    pub num_kv_heads: usize,
    /// Which original Q head indices survived (ordered)
    pub retained_head_indices: Vec<usize>,
    /// Which original KV head indices survived (ordered)
    pub retained_kv_head_indices: Vec<usize>,
    /// Per-retained-head precision assignment
    pub head_precisions: Vec<HeadPrecision>,
    /// Per-retained-head utilization score (for debugging/display)
    pub head_scores: Vec<f64>,
}

/// Complete model topology after compaction.
/// This is the manifest that Candle reads to know per-layer dimensions.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../shared/generated/plasticity/HeadTopology.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct HeadTopology {
    /// Base model identifier (e.g., "meta-llama/Llama-3.2-3B")
    pub base_model: String,
    /// Per-layer topology (length = num_layers)
    pub layers: Vec<LayerTopology>,
    /// Original model's Q head count (before compaction)
    #[ts(type = "number")]
    pub original_num_heads: usize,
    /// Original model's KV head count (before compaction)
    #[ts(type = "number")]
    pub original_num_kv_heads: usize,
    /// Dimension per attention head (e.g., 128 for Llama-3.2-3B)
    #[ts(type = "number")]
    pub head_dim: usize,
    /// Fraction of parameters removed (e.g., 0.30 = 30% smaller)
    pub parameter_reduction: f64,
    /// Summary of precision assignments across all layers
    pub precision_profile: PrecisionProfile,
    /// ISO 8601 creation timestamp
    pub created_at: String,
}

/// Summary counts of head precision assignments across the whole model.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../shared/generated/plasticity/PrecisionProfile.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct PrecisionProfile {
    #[ts(type = "number")]
    pub removed: usize,
    #[ts(type = "number")]
    pub ternary: usize,
    #[ts(type = "number")]
    pub q2: usize,
    #[ts(type = "number")]
    pub q4: usize,
    #[ts(type = "number")]
    pub q8: usize,
    #[ts(type = "number")]
    pub bf16: usize,
}

impl PrecisionProfile {
    pub fn total_active(&self) -> usize {
        self.ternary + self.q2 + self.q4 + self.q8 + self.bf16
    }

    pub fn total_original(&self) -> usize {
        self.removed + self.total_active()
    }

    /// Weighted average bits per active parameter across all retained heads.
    pub fn average_bits(&self) -> f64 {
        let total = self.total_active();
        if total == 0 {
            return 0.0;
        }
        let weighted = self.ternary as f64 * 1.585
            + self.q2 as f64 * 2.0
            + self.q4 as f64 * 4.0
            + self.q8 as f64 * 8.0
            + self.bf16 as f64 * 16.0;
        weighted / total as f64
    }
}

/// Raw utilization data from training (gate gradients + gate values).
/// Produced by peft-train.py's GateGradientCallback, consumed by scoring.rs.
///
/// Note: snake_case — this is deserialized from Python-written JSON, not TypeScript.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UtilizationData {
    /// Per-layer, per-head utilization scores: [layer][head]
    pub layer_scores: Vec<Vec<f64>>,
    /// Number of training steps that contributed to these scores
    pub num_steps: usize,
    /// Model name (for validation against base model)
    pub model_name: String,
    /// Number of Q attention heads per layer in the original model
    pub num_heads: usize,
    /// Number of KV attention heads per layer
    pub num_kv_heads: usize,
}

/// Configuration for the compaction process.
///
/// Thresholds divide the [0, 1] utilization range into precision tiers:
/// ```text
/// 0.0 ──dead── 0.1 ──dormant── 0.2 ──low── 0.3 ──medium── 0.5 ──active── 0.7 ──hot── 0.9 ──saturated── 1.0
///   Removed      Ternary         Q2           Q4              Q8            BF16       BF16+mitosis
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../shared/generated/plasticity/CompactionConfig.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct CompactionConfig {
    /// Minimum Q heads to retain per layer (safety floor)
    #[ts(type = "number")]
    pub min_heads_per_layer: usize,
    /// Minimum KV heads to retain per layer
    #[ts(type = "number")]
    pub min_kv_heads_per_layer: usize,
    /// Below this: physically removed (default: 0.1)
    pub dead_threshold: f64,
    /// Below this: ternary 1.58-bit (default: 0.2)
    pub dormant_threshold: f64,
    /// Below this: Q2 2-bit (default: 0.3)
    pub low_threshold: f64,
    /// Below this: Q4 4-bit (default: 0.5)
    pub medium_threshold: f64,
    /// Below this: Q8 8-bit; above: BF16 full precision (default: 0.7)
    pub high_threshold: f64,
    /// Above this: saturated, candidate for mitosis (default: 0.9)
    pub saturated_threshold: f64,
    /// Whether to actually quantize (false = just compute topology without quant)
    pub enable_quantization: bool,
    /// Target model size in GB. When set, thresholds are dynamically adjusted
    /// to fit the model within this budget. Higher-utilization heads get higher
    /// precision, lower-utilization heads get aggressive quantization.
    /// The thresholds above become initial estimates that get overridden.
    #[ts(optional)]
    pub target_size_gb: Option<f64>,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            min_heads_per_layer: 4,
            min_kv_heads_per_layer: 2,
            dead_threshold: 0.1,
            dormant_threshold: 0.2,
            low_threshold: 0.3,
            medium_threshold: 0.5,
            high_threshold: 0.7,
            saturated_threshold: 0.9,
            enable_quantization: true,
            target_size_gb: None,
        }
    }
}

/// Result of a dry-run analysis (plasticity/analyze command).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../shared/generated/plasticity/AnalysisResult.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisResult {
    /// The computed topology (what WOULD happen)
    pub topology: HeadTopology,
    /// Per-layer summary of decisions
    pub layer_summaries: Vec<LayerSummary>,
    /// Estimated memory savings in bytes
    #[ts(type = "number")]
    pub estimated_savings_bytes: u64,
    /// Saturated heads that would benefit from mitosis
    pub saturated_heads: Vec<SaturatedHead>,
}

/// Per-layer summary for analysis output.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../shared/generated/plasticity/LayerSummary.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct LayerSummary {
    pub layer_index: usize,
    #[ts(type = "number")]
    pub heads_removed: usize,
    #[ts(type = "number")]
    pub heads_ternary: usize,
    #[ts(type = "number")]
    pub heads_q2: usize,
    #[ts(type = "number")]
    pub heads_q4: usize,
    #[ts(type = "number")]
    pub heads_q8: usize,
    #[ts(type = "number")]
    pub heads_bf16: usize,
    #[ts(type = "number")]
    pub heads_saturated: usize,
    pub min_score: f64,
    pub max_score: f64,
    pub mean_score: f64,
}

/// A head identified as saturated (candidate for mitosis/splitting).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../shared/generated/plasticity/SaturatedHead.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct SaturatedHead {
    pub layer_index: usize,
    pub head_index: usize,
    pub utilization: f64,
}

/// Result of a compaction operation.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../shared/generated/plasticity/CompactionResult.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct CompactionResult {
    /// Path to the compacted model safetensors
    pub model_path: String,
    /// Path to the topology JSON
    pub topology_path: String,
    /// The topology
    pub topology: HeadTopology,
    /// Original model size in bytes
    #[ts(type = "number")]
    pub original_size_bytes: u64,
    /// Compacted model size in bytes
    #[ts(type = "number")]
    pub compacted_size_bytes: u64,
}
