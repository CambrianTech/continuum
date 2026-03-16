//! PlasticityModule — Adaptive neural plasticity optimization engine.
//!
//! Commands:
//! - `plasticity/analyze`: Dry-run analysis showing what would be pruned/quantized
//! - `plasticity/compact`: Compact a model (prune + quantize based on utilization data)
//! - `plasticity/topology`: Get topology of a compacted model
//!
//! The plasticity system uses per-head utilization data from LoRA training to make
//! four optimization decisions with ONE formula:
//!   utilization = 0.8 * gate_value + 0.2 * gradient_magnitude
//!
//! | Utilization | Action         | Precision | LoRA   |
//! |-------------|----------------|-----------|--------|
//! | < 0.1       | Remove (prune) | N/A       | None   |
//! | 0.1 - 0.3   | Keep, compress | Q4        | Skip   |
//! | 0.3 - 0.7   | Standard       | Q8        | Target |
//! | 0.7 - 0.9   | Full precision | BF16      | Target |
//! | > 0.9       | Split (future) | BF16 × 2  | Both   |

pub mod compactor;
pub mod quantizer;
pub mod scoring;
pub mod topology;
pub mod types;
#[cfg(test)]
mod validation;

use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use async_trait::async_trait;
use serde_json::Value;
use std::any::Any;
use std::path::PathBuf;

pub struct PlasticityModule;

impl PlasticityModule {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ServiceModule for PlasticityModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "plasticity",
            priority: ModulePriority::Background,
            command_prefixes: &["plasticity/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 1, // Compaction is memory-intensive
            tick_interval: None,
        }
    }

    async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
        Ok(())
    }

    async fn handle_command(&self, command: &str, params: Value) -> Result<CommandResult, String> {
        match command {
            "plasticity/analyze" => self.handle_analyze(params).await,
            "plasticity/compact" => self.handle_compact(params).await,
            "plasticity/topology" => self.handle_topology(params).await,
            _ => Err(format!("Unknown plasticity command: {command}")),
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl PlasticityModule {
    /// Dry-run analysis: compute what compaction WOULD do without modifying files.
    ///
    /// Params:
    /// - `adapterPath` (string): Path to adapter directory containing gate_gradients.json
    /// - `config` (object, optional): CompactionConfig overrides
    async fn handle_analyze(&self, params: Value) -> Result<CommandResult, String> {
        let adapter_path = params
            .get("adapterPath")
            .and_then(|v| v.as_str())
            .ok_or("plasticity/analyze requires 'adapterPath' string param")?;

        let config = parse_config(&params);

        let gradients_path = PathBuf::from(adapter_path).join("gate_gradients.json");
        let utilization = topology::load_utilization_data(&gradients_path)?;

        let layers = scoring::compute_optimization_plan(&utilization, &config);
        let layer_summaries = scoring::compute_layer_summaries(&utilization, &layers, &config);
        let saturated_heads = scoring::find_saturated_heads(&utilization, &config);

        let precision_profile = scoring::compute_precision_profile(
            &layers,
            utilization.num_heads,
            utilization.layer_scores.len(),
        );

        let parameter_reduction = scoring::estimate_parameter_reduction(
            &layers,
            utilization.num_heads,
            utilization.num_kv_heads,
            // head_dim must be inferred or passed — use common default for now
            infer_head_dim(&utilization),
            infer_hidden_size(&utilization),
        );

        let topology = types::HeadTopology {
            base_model: utilization.model_name.clone(),
            layers,
            original_num_heads: utilization.num_heads,
            original_num_kv_heads: utilization.num_kv_heads,
            head_dim: infer_head_dim(&utilization),
            parameter_reduction,
            precision_profile,
            created_at: chrono_now(),
        };

        let (orig_bytes, quant_bytes) = quantizer::estimate_total_savings(
            &topology,
            infer_hidden_size(&utilization),
        );

        let result = types::AnalysisResult {
            topology,
            layer_summaries,
            estimated_savings_bytes: orig_bytes.saturating_sub(quant_bytes),
            saturated_heads,
        };

        let json = serde_json::to_value(result)
            .map_err(|e| format!("Failed to serialize analysis result: {e}"))?;
        Ok(CommandResult::Json(json))
    }

    /// Compact a model: physically remove pruned heads and write compacted safetensors.
    ///
    /// Params:
    /// - `adapterPath` (string): Path to adapter directory
    /// - `modelPath` (string): Path to base model safetensors file
    /// - `outputPath` (string, optional): Output path (defaults to adapter_path/compacted_model.safetensors)
    /// - `config` (object, optional): CompactionConfig overrides
    async fn handle_compact(&self, params: Value) -> Result<CommandResult, String> {
        let adapter_path = params
            .get("adapterPath")
            .and_then(|v| v.as_str())
            .ok_or("plasticity/compact requires 'adapterPath' string param")?;

        let model_path = params
            .get("modelPath")
            .and_then(|v| v.as_str())
            .ok_or("plasticity/compact requires 'modelPath' string param")?;

        let output_path = params
            .get("outputPath")
            .and_then(|v| v.as_str())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(adapter_path).join("compacted_model.safetensors"));

        let config = parse_config(&params);

        // Load utilization data
        let gradients_path = PathBuf::from(adapter_path).join("gate_gradients.json");
        let utilization = topology::load_utilization_data(&gradients_path)?;

        // Compute optimization plan
        let layers = scoring::compute_optimization_plan(&utilization, &config);

        let precision_profile = scoring::compute_precision_profile(
            &layers,
            utilization.num_heads,
            utilization.layer_scores.len(),
        );

        let head_dim = infer_head_dim(&utilization);
        let hidden_size = infer_hidden_size(&utilization);

        let parameter_reduction = scoring::estimate_parameter_reduction(
            &layers,
            utilization.num_heads,
            utilization.num_kv_heads,
            head_dim,
            hidden_size,
        );

        let topo = types::HeadTopology {
            base_model: utilization.model_name.clone(),
            layers,
            original_num_heads: utilization.num_heads,
            original_num_kv_heads: utilization.num_kv_heads,
            head_dim,
            parameter_reduction,
            precision_profile,
            created_at: chrono_now(),
        };

        // Perform compaction
        let result = compactor::compact_model(
            &PathBuf::from(model_path),
            &topo,
            &output_path,
        )?;

        let json = serde_json::to_value(result)
            .map_err(|e| format!("Failed to serialize compaction result: {e}"))?;
        Ok(CommandResult::Json(json))
    }

    /// Get topology of an already-compacted model.
    ///
    /// Params:
    /// - `topologyPath` (string): Path to head_topology.json
    async fn handle_topology(&self, params: Value) -> Result<CommandResult, String> {
        let topo_path = params
            .get("topologyPath")
            .and_then(|v| v.as_str())
            .ok_or("plasticity/topology requires 'topologyPath' string param")?;

        let topo = topology::load_topology(&PathBuf::from(topo_path))?;

        let json = serde_json::to_value(topo)
            .map_err(|e| format!("Failed to serialize topology: {e}"))?;
        Ok(CommandResult::Json(json))
    }
}

/// Parse CompactionConfig from command params, using defaults for missing fields.
fn parse_config(params: &Value) -> types::CompactionConfig {
    let config_val = params.get("config");

    let mut config = types::CompactionConfig::default();

    if let Some(c) = config_val {
        if let Some(v) = c.get("minHeadsPerLayer").and_then(|v| v.as_u64()) {
            config.min_heads_per_layer = v as usize;
        }
        if let Some(v) = c.get("minKvHeadsPerLayer").and_then(|v| v.as_u64()) {
            config.min_kv_heads_per_layer = v as usize;
        }
        if let Some(v) = c.get("deadThreshold").and_then(|v| v.as_f64()) {
            config.dead_threshold = v;
        }
        if let Some(v) = c.get("lowThreshold").and_then(|v| v.as_f64()) {
            config.low_threshold = v;
        }
        if let Some(v) = c.get("highThreshold").and_then(|v| v.as_f64()) {
            config.high_threshold = v;
        }
        if let Some(v) = c.get("saturatedThreshold").and_then(|v| v.as_f64()) {
            config.saturated_threshold = v;
        }
        if let Some(v) = c.get("enableQuantization").and_then(|v| v.as_bool()) {
            config.enable_quantization = v;
        }
    }

    config
}

/// Infer head_dim from model architecture.
/// Known models: Llama-3.2-3B = 128, Llama-3.2-1B = 64, SmolLM2 = 64
fn infer_head_dim(data: &types::UtilizationData) -> usize {
    let name = data.model_name.to_lowercase();
    if name.contains("llama-3.2-3b") || name.contains("llama-3.1") || name.contains("llama-3-") {
        128
    } else if name.contains("llama-3.2-1b") {
        64
    } else if name.contains("smollm2") {
        64
    } else {
        // Default for unknown models — this should ideally come from model config
        128
    }
}

/// Infer hidden_size from model architecture.
fn infer_hidden_size(data: &types::UtilizationData) -> usize {
    let name = data.model_name.to_lowercase();
    if name.contains("llama-3.2-3b") {
        3072
    } else if name.contains("llama-3.2-1b") {
        2048
    } else if name.contains("smollm2-135m") {
        576
    } else if name.contains("smollm2-360m") {
        960
    } else if name.contains("smollm2-1.7b") {
        2048
    } else {
        // Default
        data.num_heads * infer_head_dim(data)
    }
}

/// Current timestamp in ISO 8601 format.
fn chrono_now() -> String {
    // Use std time instead of chrono dependency
    use std::time::SystemTime;
    let duration = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    // Simple ISO format without pulling in chrono
    format!("{}Z", duration.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_config_defaults() {
        let config = parse_config(&Value::Null);
        assert_eq!(config.min_heads_per_layer, 4);
        assert_eq!(config.dead_threshold, 0.1);
        assert!(config.enable_quantization);
    }

    #[test]
    fn test_parse_config_overrides() {
        let params = serde_json::json!({
            "config": {
                "minHeadsPerLayer": 2,
                "deadThreshold": 0.05,
                "enableQuantization": false
            }
        });
        let config = parse_config(&params);
        assert_eq!(config.min_heads_per_layer, 2);
        assert_eq!(config.dead_threshold, 0.05);
        assert!(!config.enable_quantization);
    }

    #[test]
    fn test_infer_head_dim_llama_3b() {
        let data = types::UtilizationData {
            layer_scores: vec![],
            num_steps: 0,
            model_name: "meta-llama/Llama-3.2-3B".to_string(),
            num_heads: 24,
            num_kv_heads: 8,
        };
        assert_eq!(infer_head_dim(&data), 128);
    }

    #[test]
    fn test_infer_hidden_size_llama_3b() {
        let data = types::UtilizationData {
            layer_scores: vec![],
            num_steps: 0,
            model_name: "meta-llama/Llama-3.2-3B".to_string(),
            num_heads: 24,
            num_kv_heads: 8,
        };
        assert_eq!(infer_hidden_size(&data), 3072);
    }

    #[tokio::test]
    async fn test_unknown_command() {
        let module = PlasticityModule::new();
        let result = module.handle_command("plasticity/unknown", Value::Null).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown plasticity command"));
    }

    #[tokio::test]
    async fn test_analyze_missing_params() {
        let module = PlasticityModule::new();
        let result = module.handle_command("plasticity/analyze", Value::Null).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("adapterPath"));
    }

    #[tokio::test]
    async fn test_compact_missing_params() {
        let module = PlasticityModule::new();
        let result = module.handle_command("plasticity/compact", Value::Null).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("adapterPath"));
    }

    #[tokio::test]
    async fn test_topology_missing_params() {
        let module = PlasticityModule::new();
        let result = module.handle_command("plasticity/topology", Value::Null).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("topologyPath"));
    }
}
