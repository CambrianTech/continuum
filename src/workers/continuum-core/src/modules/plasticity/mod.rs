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
pub mod gguf_writer;
pub mod pipeline;
pub mod planner;
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
use std::path::{Path, PathBuf};

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
            "plasticity/pipeline" => self.handle_pipeline(params).await,
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

        let layer_summaries = scoring::compute_layer_summaries(
            &utilization,
            &scoring::compute_optimization_plan(&utilization, &config),
            &config,
        );
        let saturated_heads = scoring::find_saturated_heads(&utilization, &config);

        let topo = build_topology(&utilization, &config);

        let (orig_bytes, quant_bytes) = quantizer::estimate_total_savings(
            &topo,
            infer_hidden_size(&utilization),
        );

        let result = types::AnalysisResult {
            topology: topo,
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
    /// - `adapterPath` (string): Path to adapter directory containing gate_gradients.json
    /// - `modelPath` (string): Path to base model — either a single .safetensors file
    ///   or a directory containing model-NNNNN-of-NNNNN.safetensors shards
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
        let topo = build_topology(&utilization, &config);

        // Auto-detect single file vs directory of shards
        let model_path_buf = PathBuf::from(model_path);
        let result = if model_path_buf.is_dir() {
            compactor::compact_model_sharded(&model_path_buf, &topo, &output_path)?
        } else {
            compactor::compact_model(&model_path_buf, &topo, &output_path)?
        };

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

    /// End-to-end pipeline: gate_gradients.json → analysis → compaction.
    ///
    /// This is the "wake up to a compacted model" command. Given a gate capture
    /// directory and a model path, it runs the full pipeline:
    ///
    /// 1. Load gate_gradients.json from capture directory
    /// 2. Compute optimization plan (scoring + GQA constraints)
    /// 3. Build topology with per-head precision assignments
    /// 4. Compact model (multi-shard aware, head pruning)
    /// 5. Write compacted model + topology to output directory
    ///
    /// Params:
    /// - `capturePath` (string): Gate capture directory (contains gate_gradients.json)
    /// - `modelPath` (string): Base model path — directory for multi-shard, file for single
    /// - `outputPath` (string, optional): Output directory (defaults to capturePath/compacted/)
    /// - `config` (object, optional): CompactionConfig overrides
    async fn handle_pipeline(&self, params: Value) -> Result<CommandResult, String> {
        let capture_path = params
            .get("capturePath")
            .and_then(|v| v.as_str())
            .ok_or("plasticity/pipeline requires 'capturePath' string param")?;

        let model_path = params
            .get("modelPath")
            .and_then(|v| v.as_str())
            .ok_or("plasticity/pipeline requires 'modelPath' string param")?;

        let output_dir = params
            .get("outputPath")
            .and_then(|v| v.as_str())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(capture_path).join("compacted"));

        let config = parse_config(&params);

        // 1. Load gate gradients
        let gradients_path = PathBuf::from(capture_path).join("gate_gradients.json");
        if !gradients_path.exists() {
            // Also check results subdirectory (RunPod capture downloads to results/)
            let alt_path = PathBuf::from(capture_path).join("results").join("gate_gradients.json");
            if !alt_path.exists() {
                return Err(format!(
                    "gate_gradients.json not found in {} or {}/results/",
                    capture_path, capture_path
                ));
            }
            return self.run_pipeline(&alt_path, model_path, &output_dir, &config).await;
        }

        self.run_pipeline(&gradients_path, model_path, &output_dir, &config).await
    }

    async fn run_pipeline(
        &self,
        gradients_path: &Path,
        model_path: &str,
        output_dir: &Path,
        config: &types::CompactionConfig,
    ) -> Result<CommandResult, String> {
        eprintln!("[plasticity/pipeline] Loading gate gradients from {}", gradients_path.display());
        let utilization = topology::load_utilization_data(gradients_path)?;

        eprintln!(
            "[plasticity/pipeline] Model: {}, {} layers, {} heads ({} KV), {} training steps",
            utilization.model_name,
            utilization.layer_scores.len(),
            utilization.num_heads,
            utilization.num_kv_heads,
            utilization.num_steps
        );

        // 2. Compute topology
        let topo = build_topology(&utilization, config);

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

        // 3. Create output directory
        std::fs::create_dir_all(output_dir)
            .map_err(|e| format!("Failed to create output directory {}: {}", output_dir.display(), e))?;

        let output_file = output_dir.join("compacted_model.safetensors");

        // 4. Compact
        let model_path_buf = PathBuf::from(model_path);
        let result = if model_path_buf.is_dir() {
            eprintln!("[plasticity/pipeline] Multi-shard model detected, scanning for shards...");
            compactor::compact_model_sharded(&model_path_buf, &topo, &output_file)?
        } else {
            compactor::compact_model(&model_path_buf, &topo, &output_file)?
        };

        // 5. Also save analysis summary alongside
        let hidden_size = infer_hidden_size(&utilization);
        let (orig_bytes, quant_bytes) = quantizer::estimate_total_savings(&topo, hidden_size);
        let layer_summaries = scoring::compute_layer_summaries(
            &utilization,
            &topo.layers,
            config,
        );

        let analysis = types::AnalysisResult {
            topology: topo.clone(),
            layer_summaries,
            estimated_savings_bytes: orig_bytes.saturating_sub(quant_bytes),
            saturated_heads: scoring::find_saturated_heads(&utilization, config),
        };

        let analysis_path = output_dir.join("analysis.json");
        let analysis_json = serde_json::to_string_pretty(&analysis)
            .map_err(|e| format!("Failed to serialize analysis: {e}"))?;
        std::fs::write(&analysis_path, analysis_json)
            .map_err(|e| format!("Failed to write analysis: {e}"))?;

        eprintln!(
            "[plasticity/pipeline] Complete! Output: {}, topology: {}, analysis: {}",
            result.model_path,
            result.topology_path,
            analysis_path.display()
        );

        let json = serde_json::to_value(&result)
            .map_err(|e| format!("Failed to serialize pipeline result: {e}"))?;
        Ok(CommandResult::Json(json))
    }
}

/// Build a HeadTopology from utilization data and config.
///
/// When `config.target_size_gb` is set, uses budget-aware allocation that optimally
/// distributes precision tiers to fit within the target size. Otherwise falls back
/// to fixed-threshold assignment.
fn build_topology(utilization: &types::UtilizationData, config: &types::CompactionConfig) -> types::HeadTopology {
    let arch = lookup_model_arch(&utilization.model_name);
    let head_dim = arch.as_ref().map(|a| a.head_dim).unwrap_or(128);
    let hidden_size = arch.as_ref().map(|a| a.hidden_size)
        .unwrap_or_else(|| utilization.num_heads * head_dim);

    let layers = if let Some(target_gb) = config.target_size_gb {
        // Budget-aware: fit the model into target_gb
        let (intermediate_size, vocab_size) = arch.as_ref()
            .map(|a| (a.intermediate_size, a.vocab_size))
            .unwrap_or_else(|| {
                // Reasonable defaults: intermediate ≈ 3.5× hidden, vocab ≈ 32K
                (hidden_size * 7 / 2, 32000)
            });

        let non_attention_bytes = scoring::estimate_non_attention_bytes(
            utilization.layer_scores.len(),
            hidden_size,
            intermediate_size,
            vocab_size,
        );

        eprintln!(
            "[plasticity] Budget-aware mode: target={:.1}GB, non-attention={:.2}GB, attention budget={:.2}GB",
            target_gb,
            non_attention_bytes as f64 / 1_073_741_824.0,
            (target_gb * 1_073_741_824.0 - non_attention_bytes as f64) / 1_073_741_824.0,
        );

        scoring::compute_budget_aware_plan(
            utilization,
            target_gb,
            non_attention_bytes,
            head_dim,
            hidden_size,
            config,
        )
    } else {
        scoring::compute_optimization_plan(utilization, config)
    };

    let precision_profile = scoring::compute_precision_profile(
        &layers,
        utilization.num_heads,
        utilization.layer_scores.len(),
    );
    let parameter_reduction = scoring::estimate_parameter_reduction(
        &layers,
        utilization.num_heads,
        utilization.num_kv_heads,
        head_dim,
        hidden_size,
    );

    types::HeadTopology {
        base_model: utilization.model_name.clone(),
        layers,
        original_num_heads: utilization.num_heads,
        original_num_kv_heads: utilization.num_kv_heads,
        head_dim,
        parameter_reduction,
        precision_profile,
        created_at: chrono_now(),
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
        if let Some(v) = c.get("dormantThreshold").and_then(|v| v.as_f64()) {
            config.dormant_threshold = v;
        }
        if let Some(v) = c.get("lowThreshold").and_then(|v| v.as_f64()) {
            config.low_threshold = v;
        }
        if let Some(v) = c.get("mediumThreshold").and_then(|v| v.as_f64()) {
            config.medium_threshold = v;
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
        if let Some(v) = c.get("targetSizeGb").and_then(|v| v.as_f64()) {
            config.target_size_gb = Some(v);
        }
    }

    // Also check top-level param (convenience — can pass targetSizeGb outside config block)
    if config.target_size_gb.is_none() {
        if let Some(v) = params.get("targetSizeGb").and_then(|v| v.as_f64()) {
            config.target_size_gb = Some(v);
        }
    }

    config
}

/// Known model architecture configurations.
/// Maps model name patterns to dimensions needed for compaction.
struct ModelArchConfig {
    head_dim: usize,
    hidden_size: usize,
    intermediate_size: usize,
    vocab_size: usize,
}

/// Look up model architecture from name.
fn lookup_model_arch(name: &str) -> Option<ModelArchConfig> {
    let name = name.to_lowercase();

    // Qwen 2.5 family (from HuggingFace config.json files)
    if name.contains("qwen2.5") || name.contains("qwen-2.5") {
        if name.contains("32b") {
            return Some(ModelArchConfig { head_dim: 128, hidden_size: 5120, intermediate_size: 27648, vocab_size: 152064 });
        } else if name.contains("14b") {
            return Some(ModelArchConfig { head_dim: 128, hidden_size: 5120, intermediate_size: 13824, vocab_size: 152064 });
        } else if name.contains("7b") {
            return Some(ModelArchConfig { head_dim: 128, hidden_size: 3584, intermediate_size: 18944, vocab_size: 152064 });
        } else if name.contains("3b") {
            return Some(ModelArchConfig { head_dim: 128, hidden_size: 2048, intermediate_size: 11008, vocab_size: 152064 });
        } else if name.contains("1.5b") {
            return Some(ModelArchConfig { head_dim: 128, hidden_size: 1536, intermediate_size: 8960, vocab_size: 152064 });
        } else if name.contains("0.5b") {
            return Some(ModelArchConfig { head_dim: 64, hidden_size: 896, intermediate_size: 4864, vocab_size: 152064 });
        }
    }

    // Llama 3.x family
    if name.contains("llama-3.2-3b") || name.contains("llama-3.1") || name.contains("llama-3-") {
        return Some(ModelArchConfig { head_dim: 128, hidden_size: 3072, intermediate_size: 8192, vocab_size: 128256 });
    }
    if name.contains("llama-3.2-1b") {
        return Some(ModelArchConfig { head_dim: 64, hidden_size: 2048, intermediate_size: 8192, vocab_size: 128256 });
    }

    // SmolLM2 family
    if name.contains("smollm2-135m") {
        return Some(ModelArchConfig { head_dim: 64, hidden_size: 576, intermediate_size: 1536, vocab_size: 49152 });
    }
    if name.contains("smollm2-360m") {
        return Some(ModelArchConfig { head_dim: 64, hidden_size: 960, intermediate_size: 2560, vocab_size: 49152 });
    }
    if name.contains("smollm2-1.7b") || name.contains("smollm2") {
        return Some(ModelArchConfig { head_dim: 64, hidden_size: 2048, intermediate_size: 8192, vocab_size: 49152 });
    }

    None
}

/// Infer head_dim from model architecture.
fn infer_head_dim(data: &types::UtilizationData) -> usize {
    lookup_model_arch(&data.model_name)
        .map(|c| c.head_dim)
        .unwrap_or_else(|| {
            // Fallback: compute from num_heads if we know hidden_size, else default 128
            128
        })
}

/// Infer hidden_size from model architecture.
fn infer_hidden_size(data: &types::UtilizationData) -> usize {
    lookup_model_arch(&data.model_name)
        .map(|c| c.hidden_size)
        .unwrap_or_else(|| data.num_heads * infer_head_dim(data))
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
        assert!(config.target_size_gb.is_none());
    }

    #[test]
    fn test_parse_config_overrides() {
        let params = serde_json::json!({
            "config": {
                "minHeadsPerLayer": 2,
                "deadThreshold": 0.05,
                "enableQuantization": false,
                "targetSizeGb": 20.0
            }
        });
        let config = parse_config(&params);
        assert_eq!(config.min_heads_per_layer, 2);
        assert_eq!(config.dead_threshold, 0.05);
        assert!(!config.enable_quantization);
        assert_eq!(config.target_size_gb, Some(20.0));
    }

    #[test]
    fn test_parse_config_target_size_top_level() {
        let params = serde_json::json!({
            "targetSizeGb": 18.5
        });
        let config = parse_config(&params);
        assert_eq!(config.target_size_gb, Some(18.5));
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

    #[test]
    fn test_infer_qwen25_coder_32b() {
        let data = types::UtilizationData {
            layer_scores: vec![],
            num_steps: 0,
            model_name: "Qwen/Qwen2.5-Coder-32B-Instruct".to_string(),
            num_heads: 40,
            num_kv_heads: 8,
        };
        assert_eq!(infer_head_dim(&data), 128);
        assert_eq!(infer_hidden_size(&data), 5120);
    }

    #[test]
    fn test_infer_qwen25_coder_0_5b() {
        let data = types::UtilizationData {
            layer_scores: vec![],
            num_steps: 0,
            model_name: "Qwen/Qwen2.5-Coder-0.5B-Instruct".to_string(),
            num_heads: 14,
            num_kv_heads: 2,
        };
        assert_eq!(infer_head_dim(&data), 64);
        assert_eq!(infer_hidden_size(&data), 896);
    }

    #[test]
    fn test_budget_aware_topology_qwen32b() {
        // Simulate 64 layers, 40 Q heads, 8 KV heads with mixed utilization
        let mut layer_scores = Vec::new();
        for layer_idx in 0..64 {
            let mut scores = Vec::new();
            for head_idx in 0..40 {
                // Vary scores: higher layers and lower head indices tend to be more utilized
                let base = 0.4 + 0.3 * (layer_idx as f64 / 63.0);
                let variation = 0.2 * ((head_idx as f64 * 7.0).sin() * 0.5 + 0.5);
                scores.push((base + variation).min(0.95));
            }
            layer_scores.push(scores);
        }

        let data = types::UtilizationData {
            layer_scores,
            num_steps: 10,
            model_name: "Qwen/Qwen2.5-Coder-32B-Instruct".to_string(),
            num_heads: 40,
            num_kv_heads: 8,
        };

        // Target: 20GB (fits in 32GB M1 with headroom)
        let config = types::CompactionConfig {
            target_size_gb: Some(20.0),
            ..types::CompactionConfig::default()
        };

        let topo = build_topology(&data, &config);

        assert_eq!(topo.base_model, "Qwen/Qwen2.5-Coder-32B-Instruct");
        assert_eq!(topo.head_dim, 128);
        assert_eq!(topo.original_num_heads, 40);
        assert_eq!(topo.original_num_kv_heads, 8);
        assert_eq!(topo.layers.len(), 64);

        // Should have some reduction since we're going from ~65GB BF16 to 20GB target
        assert!(topo.parameter_reduction > 0.0, "Should have parameter reduction");

        // Precision profile should have a mix (not all BF16 — budget is tight)
        let pp = &topo.precision_profile;
        let total = pp.total_original();
        assert_eq!(total, 64 * 40, "Total heads should be 64 layers × 40 heads");
        assert!(pp.bf16 < total, "Not all heads should be BF16 at 20GB target");

        eprintln!(
            "Budget-aware 32B → 20GB: removed={} ternary={} q2={} q4={} q8={} bf16={}, reduction={:.1}%",
            pp.removed, pp.ternary, pp.q2, pp.q4, pp.q8, pp.bf16,
            topo.parameter_reduction * 100.0
        );
    }

    #[test]
    fn test_budget_aware_vs_threshold() {
        // Same data, threshold-based vs budget-aware should give different results
        let layer_scores = vec![vec![0.3, 0.5, 0.7, 0.9]; 4];
        let data = types::UtilizationData {
            layer_scores,
            num_steps: 100,
            model_name: "meta-llama/Llama-3.2-3B".to_string(),
            num_heads: 4,
            num_kv_heads: 4,
        };

        let threshold_config = types::CompactionConfig::default();
        let topo_threshold = build_topology(&data, &threshold_config);

        // With a very tight budget, should compress more aggressively
        let budget_config = types::CompactionConfig {
            target_size_gb: Some(1.0),
            ..types::CompactionConfig::default()
        };
        let topo_budget = build_topology(&data, &budget_config);

        // Budget mode with tight target should have more compression
        assert!(
            topo_budget.parameter_reduction >= topo_threshold.parameter_reduction
                || topo_budget.precision_profile.bf16 <= topo_threshold.precision_profile.bf16,
            "Budget-aware with tight target should compress more aggressively"
        );
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
