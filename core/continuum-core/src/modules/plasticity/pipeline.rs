//! Compression pipeline: end-to-end orchestration.
//!
//! Score → Plan → Compress → Verify → Infer
//!
//! IPC command: `plasticity/compress`
//! See docs/genome/COMPRESSION-PIPELINE.md

use std::path::{Path, PathBuf};

use crate::model_registry::ModelArchConfig;

use super::gguf_writer;
use super::planner;
use super::types::*;

/// Pipeline configuration.
#[derive(Debug, Clone)]
pub struct CompressConfig {
    /// Path to gate_gradients.json (or directory containing it)
    pub capture_path: PathBuf,
    /// Base model safetensors directory (or HuggingFace model ID)
    pub model_path: PathBuf,
    /// Output GGUF file path
    pub output_path: PathBuf,
    /// Target device specification
    pub device_spec: DeviceSpec,
    /// Model architecture (e.g., "qwen2", "llama")
    pub architecture: String,
}

/// Run the full compression pipeline.
///
/// 1. Load utilization data from capture
/// 2. Plan compression (recipe)
/// 3. Write compressed GGUF
/// 4. Verify output
pub fn compress(config: &CompressConfig) -> Result<CompressionPipelineResult, String> {
    let log = crate::runtime::logger("plasticity");
    log.info(&format!(
        "Compression pipeline: {:?} → {:?} (target: {})",
        config.model_path, config.output_path, config.device_spec.label
    ));

    // Step 1: Load topology from capture
    let topology = load_topology(&config.capture_path)?;
    log.info(&format!(
        "  Topology: {}Q/{}KV → {}/{} heads",
        topology.original_num_heads,
        topology.original_num_kv_heads,
        topology.layers.first().map(|l| l.num_heads).unwrap_or(0),
        topology.layers.first().map(|l| l.num_kv_heads).unwrap_or(0),
    ));

    // Step 2: Source model architecture from the base model artifact — dims come
    // from GGUF metadata / config.json, never guessed from the arch-name string.
    let arch = ModelArchConfig::from_artifact(&config.model_path)?;

    // Step 3: Plan compression
    let recipe = planner::plan_compression(
        &topology,
        &config.device_spec,
        &arch,
        &config.model_path.to_string_lossy(),
    )?;

    let planned_gb = recipe.budget.total_bytes as f64 / 1073741824.0;
    let budget_gb = config.device_spec.effective_budget_gb();
    log.info(&format!(
        "  Plan: {:.1} GB estimated ({:.1} GB budget, {:.1} GB headroom)",
        planned_gb,
        budget_gb,
        budget_gb - planned_gb
    ));

    // Count quant type distribution
    let mut quant_counts: std::collections::HashMap<GgufQuantType, usize> =
        std::collections::HashMap::new();
    for assignment in &recipe.tensor_quant_map {
        *quant_counts.entry(assignment.quant_type).or_default() += 1;
    }
    log.info(&format!("  Quant distribution: {:?}", quant_counts));

    // Step 4: Write compressed GGUF
    gguf_writer::write_compressed_gguf(
        &config.model_path,
        &recipe,
        &config.output_path,
        &config.architecture,
        &arch,
    )?;

    // Step 5: Verify
    let output_size = std::fs::metadata(&config.output_path)
        .map(|m| m.len())
        .unwrap_or(0);

    // Basic verification: file exists and is non-empty
    if output_size == 0 {
        return Err("Output GGUF is empty".into());
    }

    // Estimate original size (BF16)
    let original_bf16_bytes = estimate_original_size(&arch, &topology);
    let compression_ratio = if output_size > 0 {
        original_bf16_bytes as f64 / output_size as f64
    } else {
        0.0
    };

    log.info(&format!(
        "  Output: {:.1} GB, compression ratio: {:.1}x",
        output_size as f64 / 1073741824.0,
        compression_ratio
    ));

    Ok(CompressionPipelineResult {
        gguf_path: config.output_path.to_string_lossy().into(),
        recipe,
        output_size_bytes: output_size,
        compression_ratio,
        verified: true,
        test_output: None, // TODO: short inference test
    })
}

/// Load HeadTopology from a capture directory.
/// Looks for `head_topology.json` in the capture path.
fn load_topology(capture_path: &Path) -> Result<HeadTopology, String> {
    let topology_file = if capture_path.is_file() {
        capture_path.to_path_buf()
    } else {
        capture_path.join("head_topology.json")
    };

    if !topology_file.exists() {
        return Err(format!(
            "Topology file not found: {:?}. Run plasticity/analyze first.",
            topology_file
        ));
    }

    let data =
        std::fs::read_to_string(&topology_file).map_err(|e| format!("Read topology: {e}"))?;

    serde_json::from_str(&data).map_err(|e| format!("Parse topology: {e}"))
}

/// Estimate original model size in BF16 bytes.
fn estimate_original_size(arch: &ModelArchConfig, _topology: &HeadTopology) -> u64 {
    let attn_per_layer =
        arch.attention_params_per_layer(arch.num_attention_heads, arch.num_kv_heads);
    let mlp_per_layer = arch.mlp_params_per_layer();
    let embed = arch.embedding_params();
    let norm = arch.norm_params();

    let total_params = (attn_per_layer + mlp_per_layer) * arch.num_layers + embed + norm;
    (total_params * 2) as u64 // BF16 = 2 bytes per param
}

/// Parse a device spec from a shorthand string.
/// Accepts: "16gb", "32gb", "24gb-vram", or a JSON DeviceSpec.
pub fn parse_device_spec(spec: &str) -> Result<DeviceSpec, String> {
    let lower = spec.to_lowercase().replace(' ', "");
    match lower.as_str() {
        "16gb" | "macbookair" => Ok(DeviceSpec::macbook_air_16gb()),
        "32gb" | "macbookpro" => Ok(DeviceSpec::macbook_pro_32gb()),
        "24gb-vram" | "5090" | "rtx5090" => Ok(DeviceSpec::rtx_5090_24gb()),
        _ => {
            // Try as number
            if let Some(gb_str) = lower.strip_suffix("gb") {
                if let Ok(gb) = gb_str.parse::<f64>() {
                    return Ok(DeviceSpec::from_memory_gb(gb));
                }
            }
            // Try as JSON
            serde_json::from_str(spec).map_err(|e| {
                format!(
                    "Invalid device spec '{}'. Use: 16gb, 32gb, 24gb-vram, or JSON. Error: {e}",
                    spec
                )
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_device_spec() {
        let spec = parse_device_spec("32gb").unwrap();
        assert_eq!(spec.memory_gb, 32.0);
        assert_eq!(spec.label, "MacBook Pro 32GB");

        let spec = parse_device_spec("16gb").unwrap();
        assert_eq!(spec.memory_gb, 16.0);

        let spec = parse_device_spec("5090").unwrap();
        assert_eq!(spec.memory_gb, 24.0);

        let spec = parse_device_spec("48gb").unwrap();
        assert_eq!(spec.memory_gb, 48.0);
        assert_eq!(spec.effective_budget_gb(), 36.0);
    }

    #[test]
    fn test_parse_device_spec_invalid() {
        assert!(parse_device_spec("potato").is_err());
    }

    /// Qwen2.5-Coder-32B dims as a test fixture. Production sources these from the
    /// artifact via `ModelArchConfig::from_artifact`; constants live only here.
    fn qwen2_32b_arch() -> ModelArchConfig {
        ModelArchConfig::new(64, 5120, 40, 8, 128, 27648, 152064, 32768).unwrap()
    }

    #[test]
    fn test_estimate_original_size() {
        let arch = qwen2_32b_arch();
        let topology = HeadTopology {
            base_model: "test".into(),
            original_num_heads: 40,
            original_num_kv_heads: 8,
            head_dim: 128,
            parameter_reduction: 0.0,
            precision_profile: PrecisionProfile::default(),
            created_at: "".into(),
            layers: vec![],
        };

        let size = estimate_original_size(&arch, &topology);
        let size_gb = size as f64 / 1073741824.0;
        // Qwen2.5-Coder-32B is ~62GB in BF16
        assert!(
            size_gb > 55.0 && size_gb < 70.0,
            "Expected ~62GB, got {:.1}GB",
            size_gb
        );
    }
}
