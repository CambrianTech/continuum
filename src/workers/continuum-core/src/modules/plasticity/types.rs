//! Plasticity type definitions with ts-rs exports.
//!
//! These types define the neural plasticity optimization system:
//! per-head utilization scoring, topology after compaction, and
//! mixed-precision quantization tiers.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Per-head quantization precision tier, assigned based on utilization score.
///
/// | Utilization   | Precision | Action                    |
/// |---------------|-----------|---------------------------|
/// | Dead (< 0.1)  | Removed   | Physically removed        |
/// | Low (0.1-0.3) | Q4        | 4-bit quantized           |
/// | Med (0.3-0.7) | Q8        | 8-bit quantized           |
/// | High (0.7+)   | BF16      | Full precision            |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../shared/generated/plasticity/HeadPrecision.ts"
)]
#[serde(rename_all = "lowercase")]
pub enum HeadPrecision {
    /// Utilization < 0.1: head physically removed from tensor
    Removed,
    /// Utilization 0.1-0.3: 4-bit quantized (NF4/INT4)
    Q4,
    /// Utilization 0.3-0.7: 8-bit quantized
    Q8,
    /// Utilization 0.7+: full BF16 precision
    BF16,
}

impl HeadPrecision {
    /// Bits per parameter for this precision tier
    pub fn bits(&self) -> u8 {
        match self {
            HeadPrecision::Removed => 0,
            HeadPrecision::Q4 => 4,
            HeadPrecision::Q8 => 8,
            HeadPrecision::BF16 => 16,
        }
    }

    /// Determine precision tier from utilization score
    pub fn from_utilization(score: f64) -> Self {
        if score < 0.1 {
            HeadPrecision::Removed
        } else if score < 0.3 {
            HeadPrecision::Q4
        } else if score < 0.7 {
            HeadPrecision::Q8
        } else {
            HeadPrecision::BF16
        }
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
    pub q4: usize,
    #[ts(type = "number")]
    pub q8: usize,
    #[ts(type = "number")]
    pub bf16: usize,
}

impl PrecisionProfile {
    pub fn total_active(&self) -> usize {
        self.q4 + self.q8 + self.bf16
    }

    pub fn total_original(&self) -> usize {
        self.removed + self.q4 + self.q8 + self.bf16
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
    /// Utilization threshold for dead heads (default: 0.1)
    pub dead_threshold: f64,
    /// Utilization threshold for low-precision heads (default: 0.3)
    pub low_threshold: f64,
    /// Utilization threshold for full-precision heads (default: 0.7)
    pub high_threshold: f64,
    /// Utilization threshold for saturation/mitosis (default: 0.9)
    pub saturated_threshold: f64,
    /// Whether to actually quantize (false = just compute topology without quant)
    pub enable_quantization: bool,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            min_heads_per_layer: 4,
            min_kv_heads_per_layer: 2,
            dead_threshold: 0.1,
            low_threshold: 0.3,
            high_threshold: 0.7,
            saturated_threshold: 0.9,
            enable_quantization: true,
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
