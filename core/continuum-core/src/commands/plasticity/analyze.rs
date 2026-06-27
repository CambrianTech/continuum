//! `plasticity/analyze` — dry-run analysis: compute what compaction WOULD do to a
//! model (pruned/quantized heads, estimated savings) without touching any files.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use ts_rs::TS;

use crate::modules::plasticity::types::{AnalysisResult, CompactionConfig};
use crate::modules::plasticity::{build_topology, infer_hidden_size, quantizer, scoring, topology};

use super::effective_config;

/// Params for `plasticity/analyze`. `config` is a partial override (unspecified
/// fields fall back to `CompactionConfig::default()`); `targetSizeGb` is the
/// top-level convenience that merges into `config` when the latter omits it.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/plasticity/AnalyzeParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct AnalyzeParams {
    /// Path to the adapter directory containing `gate_gradients.json`.
    pub adapter_path: String,
    /// CompactionConfig overrides (partial; unspecified fields use defaults).
    #[serde(default)]
    pub config: CompactionConfig,
    /// Convenience: target compacted size in GB (used only when `config.targetSizeGb` is unset).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub target_size_gb: Option<f64>,
}

crate::action_command! {
    /// Dry-run analysis: compute what compaction WOULD do without modifying files.
    /// Loads per-head utilization from the adapter's `gate_gradients.json`, builds
    /// the precision topology, and reports per-layer summaries, saturated heads, and
    /// the estimated byte savings. Read-only — writes nothing.
    pub struct PlasticityAnalyze;
    name: "plasticity/analyze",
    access: Privileged,
    params: AnalyzeParams,
    output: AnalysisResult,
    run(_this, _ctx, p) => {
        let config = effective_config(p.config, p.target_size_gb);

        let gradients_path = PathBuf::from(&p.adapter_path).join("gate_gradients.json");
        let utilization = topology::load_utilization_data(&gradients_path)?;

        let layer_summaries = scoring::compute_layer_summaries(
            &utilization,
            &scoring::compute_optimization_plan(&utilization, &config),
            &config,
        );
        let saturated_heads = scoring::find_saturated_heads(&utilization, &config);

        let topo = build_topology(&utilization, &config);

        let (orig_bytes, quant_bytes) =
            quantizer::estimate_total_savings(&topo, infer_hidden_size(&utilization));

        Ok(AnalysisResult {
            topology: topo,
            layer_summaries,
            estimated_savings_bytes: orig_bytes.saturating_sub(quant_bytes),
            saturated_heads,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::ActionCommand;

    // what this catches: name/access wiring — analyzing a model reads arbitrary fs
    // paths, so it is on the Privileged surface, not AiSafe.
    #[test]
    fn name_and_access_wired() {
        assert_eq!(PlasticityAnalyze::NAME, "plasticity/analyze");
        assert!(matches!(
            PlasticityAnalyze::ACCESS,
            crate::sdk_codegen::AccessLevel::Privileged
        ));
    }
}
