//! `plasticity/compress` — compress a model to a mixed-quantization GGUF fitted to
//! a target device, driven by a captured head topology.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use ts_rs::TS;

use crate::modules::plasticity::pipeline::{self, CompressConfig};
use crate::modules::plasticity::types::CompressionPipelineResult;

/// Params for `plasticity/compress`.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/plasticity/CompressParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct CompressParams {
    /// Directory containing `head_topology.json` (the gate-capture output).
    pub capture_path: String,
    /// Base model safetensors directory (or HuggingFace model ID).
    pub model_path: String,
    /// Target device: `"32gb"`, `"16gb"`, `"5090"`, … Defaults to `"32gb"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub device_spec: Option<String>,
    /// Output GGUF path. Defaults to `~/.continuum/genome/models/<model>-compressed.gguf`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub output_path: Option<String>,
    /// Model architecture (`"qwen2"` or `"llama"`). Defaults to `"qwen2"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub architecture: Option<String>,
}

crate::action_command! {
    /// Compress a model to a mixed-quantization GGUF, fitted to a target device.
    /// Reads the captured `head_topology.json`, assigns per-head precision tiers to
    /// fit the device's VRAM budget, and writes a single mixed-quant GGUF ready to
    /// page into the serving lane.
    pub struct PlasticityCompress;
    name: "plasticity/compress",
    access: Privileged,
    params: CompressParams,
    output: CompressionPipelineResult,
    run(_this, _ctx, p) => {
        let device_spec = pipeline::parse_device_spec(p.device_spec.as_deref().unwrap_or("32gb"))?;

        let output_path = p.output_path.map(PathBuf::from).unwrap_or_else(|| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
            PathBuf::from(home)
                .join(".continuum/genome/models")
                .join(format!(
                    "{}-compressed.gguf",
                    PathBuf::from(&p.model_path)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("model")
                ))
        });

        let architecture = p.architecture.unwrap_or_else(|| "qwen2".to_string());

        let config = CompressConfig {
            capture_path: PathBuf::from(&p.capture_path),
            model_path: PathBuf::from(&p.model_path),
            output_path,
            device_spec,
            architecture,
        };

        let result = pipeline::compress(&config)?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::ActionCommand;

    // what this catches: name/access wiring — compression writes a GGUF to arbitrary
    // fs paths and reads model weights, so it is Privileged, never AiSafe.
    #[test]
    fn name_and_access_wired() {
        assert_eq!(PlasticityCompress::NAME, "plasticity/compress");
        assert!(matches!(
            PlasticityCompress::ACCESS,
            crate::sdk_codegen::AccessLevel::Privileged
        ));
    }
}
