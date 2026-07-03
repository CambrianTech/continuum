//! `plasticity/compact` — physically remove pruned heads and write the compacted
//! safetensors. Auto-detects a single `.safetensors` file vs a directory of shards.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use ts_rs::TS;

use crate::model_registry::ModelArchConfig;
use crate::modules::plasticity::types::{CompactionConfig, CompactionResult};
use crate::modules::plasticity::{build_topology, compactor, topology};

use super::effective_config;

/// Params for `plasticity/compact`.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/plasticity/CompactParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct CompactParams {
    /// Path to the adapter directory containing `gate_gradients.json`.
    pub adapter_path: String,
    /// Base model — a single `.safetensors` file OR a directory of
    /// `model-NNNNN-of-NNNNN.safetensors` shards (auto-detected).
    pub model_path: String,
    /// Output path. Defaults to `<adapterPath>/compacted_model.safetensors`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub output_path: Option<String>,
    /// CompactionConfig overrides (partial; unspecified fields use defaults).
    #[serde(default)]
    pub config: CompactionConfig,
    /// Convenience: target compacted size in GB (used only when `config.targetSizeGb` is unset).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub target_size_gb: Option<f64>,
}

crate::action_command! {
    /// Compact a model: physically remove pruned heads and write compacted
    /// safetensors. Builds the precision topology from the adapter's
    /// `gate_gradients.json`, then rewrites the base model (single-file or sharded,
    /// auto-detected) with the dead heads removed. Writes the compacted model + a
    /// `head_topology.json` sidecar.
    pub struct PlasticityCompact;
    name: "plasticity/compact",
    access: Privileged,
    params: CompactParams,
    output: CompactionResult,
    run(_this, _ctx, p) => {
        let config = effective_config(p.config, p.target_size_gb);

        let output_path = p
            .output_path
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(&p.adapter_path).join("compacted_model.safetensors"));

        let gradients_path = PathBuf::from(&p.adapter_path).join("gate_gradients.json");
        let utilization = topology::load_utilization_data(&gradients_path)?;

        let model_path_buf = PathBuf::from(&p.model_path);
        let arch = ModelArchConfig::from_artifact(&model_path_buf)?;
        let topo = build_topology(&utilization, &config, &arch);

        let result = if model_path_buf.is_dir() {
            compactor::compact_model_sharded(&model_path_buf, &topo, &output_path)?
        } else {
            compactor::compact_model(&model_path_buf, &topo, &output_path)?
        };

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::ActionCommand;

    // what this catches: name/access wiring — compaction rewrites model weights to
    // arbitrary fs paths, so it is Privileged, never AiSafe.
    #[test]
    fn name_and_access_wired() {
        assert_eq!(PlasticityCompact::NAME, "plasticity/compact");
        assert!(matches!(
            PlasticityCompact::ACCESS,
            crate::sdk_codegen::AccessLevel::Privileged
        ));
    }
}
