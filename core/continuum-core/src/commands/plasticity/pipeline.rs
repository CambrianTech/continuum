//! `plasticity/pipeline` — the end-to-end "wake up to a compacted model" verb:
//! `gate_gradients.json` → analysis → compaction, with an analysis sidecar.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use ts_rs::TS;

use crate::model_registry::ModelArchConfig;
use crate::modules::plasticity::types::{AnalysisResult, CompactionConfig, CompactionResult};
use crate::modules::plasticity::{build_topology, compactor, quantizer, scoring, topology};

use super::effective_config;

/// Params for `plasticity/pipeline`.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/plasticity/PipelineParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct PipelineParams {
    /// Gate-capture directory containing `gate_gradients.json` (or `results/gate_gradients.json`).
    pub capture_path: String,
    /// Base model — a directory for multi-shard, a file for single-file (auto-detected).
    pub model_path: String,
    /// Output directory. Defaults to `<capturePath>/compacted/`.
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
    /// End-to-end pipeline: `gate_gradients.json` → analysis → compaction. Loads the
    /// gate capture (checking `<capturePath>/results/` as a fallback), computes the
    /// optimization plan + GQA-constrained topology, rewrites the model (multi-shard
    /// aware) with dead heads pruned, and writes the compacted model, its topology,
    /// and an `analysis.json` summary into the output directory. The "wake up to a
    /// compacted model" command.
    pub struct PlasticityPipeline;
    name: "plasticity/pipeline",
    access: Privileged,
    params: PipelineParams,
    output: CompactionResult,
    run(_this, _ctx, p) => {
        let config = effective_config(p.config, p.target_size_gb);

        let output_dir = p
            .output_path
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(&p.capture_path).join("compacted"));

        // Resolve gate_gradients.json: prefer the capture root, fall back to the
        // results/ subdir (RunPod capture downloads land there). Fail loud naming
        // both probed locations if neither exists.
        let gradients_path = PathBuf::from(&p.capture_path).join("gate_gradients.json");
        let gradients_path = if gradients_path.exists() {
            gradients_path
        } else {
            let alt_path = PathBuf::from(&p.capture_path)
                .join("results")
                .join("gate_gradients.json");
            if alt_path.exists() {
                alt_path
            } else {
                return Err(crate::sdk_codegen::CommandError::NotFound(format!(
                    "gate_gradients.json not found in {} or {}/results/",
                    p.capture_path, p.capture_path
                )));
            }
        };

        run_pipeline(&gradients_path, &p.model_path, &output_dir, &config)
    }
}

/// Run the full pipeline from a resolved gradients path. Returns the compaction
/// result; also writes an `analysis.json` summary alongside the compacted model.
fn run_pipeline(
    gradients_path: &Path,
    model_path: &str,
    output_dir: &Path,
    config: &CompactionConfig,
) -> Result<CompactionResult, crate::sdk_codegen::CommandError> {
    eprintln!(
        "[plasticity/pipeline] Loading gate gradients from {}",
        gradients_path.display()
    );
    let utilization = topology::load_utilization_data(gradients_path)?;

    eprintln!(
        "[plasticity/pipeline] Model: {}, {} layers, {} heads ({} KV), {} training steps",
        utilization.model_name,
        utilization.layer_scores.len(),
        utilization.num_heads,
        utilization.num_kv_heads,
        utilization.num_steps
    );

    // Compute topology. Architecture dims come from the base model artifact — never
    // guessed from the model name.
    let arch = ModelArchConfig::from_artifact(Path::new(model_path))?;
    let topo = build_topology(&utilization, config, &arch);

    eprintln!(
        "[plasticity/pipeline] Optimization plan: {:.1}% parameter reduction, profile: removed={} ternary={} q2={} q4={} q8={} bf16={}",
        topo.parameter_reduction * 100.0,
        topo.precision_profile.removed,
        topo.precision_profile.ternary,
        topo.precision_profile.q2,
        topo.precision_profile.q4,
        topo.precision_profile.q8,
        topo.precision_profile.bf16,
    );

    // Create output directory.
    std::fs::create_dir_all(output_dir).map_err(|e| {
        crate::sdk_codegen::CommandError::Internal(format!(
            "Failed to create output directory {}: {}",
            output_dir.display(),
            e
        ))
    })?;

    let output_file = output_dir.join("compacted_model.safetensors");

    // Compact (multi-shard aware).
    let model_path_buf = PathBuf::from(model_path);
    let result = if model_path_buf.is_dir() {
        eprintln!("[plasticity/pipeline] Multi-shard model detected, scanning for shards...");
        compactor::compact_model_sharded(&model_path_buf, &topo, &output_file)?
    } else {
        compactor::compact_model(&model_path_buf, &topo, &output_file)?
    };

    // Save analysis summary alongside.
    let (orig_bytes, quant_bytes) = quantizer::estimate_total_savings(&topo, arch.hidden_size);
    let layer_summaries = scoring::compute_layer_summaries(&utilization, &topo.layers, config);

    let analysis = AnalysisResult {
        topology: topo.clone(),
        layer_summaries,
        estimated_savings_bytes: orig_bytes.saturating_sub(quant_bytes),
        saturated_heads: scoring::find_saturated_heads(&utilization, config),
    };

    let analysis_path = output_dir.join("analysis.json");
    let analysis_json = serde_json::to_string_pretty(&analysis).map_err(|e| {
        crate::sdk_codegen::CommandError::Internal(format!("Failed to serialize analysis: {e}"))
    })?;
    std::fs::write(&analysis_path, analysis_json).map_err(|e| {
        crate::sdk_codegen::CommandError::Internal(format!("Failed to write analysis: {e}"))
    })?;

    eprintln!(
        "[plasticity/pipeline] Complete! Output: {}, topology: {}, analysis: {}",
        result.model_path,
        result.topology_path,
        analysis_path.display()
    );

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::ActionCommand;

    // what this catches: name/access wiring — the pipeline rewrites model weights to
    // arbitrary fs paths, so it is Privileged, never AiSafe.
    #[test]
    fn name_and_access_wired() {
        assert_eq!(PlasticityPipeline::NAME, "plasticity/pipeline");
        assert!(matches!(
            PlasticityPipeline::ACCESS,
            crate::sdk_codegen::AccessLevel::Privileged
        ));
    }

    // what this catches: a missing gate_gradients.json fails loud naming BOTH probed
    // locations (capture root + results/ fallback) rather than silently no-opping.
    #[tokio::test]
    async fn missing_gradients_fails_loud_naming_both_paths() {
        let cmd = PlasticityPipeline;
        let err = cmd
            .run(
                &crate::sdk_codegen::Ctx::default(),
                PipelineParams {
                    capture_path: "/nonexistent/plasticity-capture".to_string(),
                    model_path: "/nonexistent/model".to_string(),
                    output_path: None,
                    config: CompactionConfig::default(),
                    target_size_gb: None,
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(err, crate::sdk_codegen::CommandError::NotFound(_)));
        assert!(err.to_string().contains("gate_gradients.json"));
        assert!(err.to_string().contains("/results/"));
    }
}
