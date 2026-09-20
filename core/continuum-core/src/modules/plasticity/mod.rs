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

    /// The `plasticity/*` verbs are migrated to the typed registry — they live as
    /// stateless [`ActionCommand`](crate::sdk_codegen::ActionCommand)s under
    /// `commands/plasticity/` and self-register, so this module owns no legacy
    /// `match` arm. Any call into the legacy path fails loud naming the command.
    async fn handle_command(&self, command: &str, _params: Value) -> Result<CommandResult, String> {
        Err(format!(
            "plasticity command surface is migrated to the typed registry; \
             '{command}' has no legacy handler"
        ))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Build a HeadTopology from utilization data and config.
///
/// When `config.target_size_gb` is set, uses budget-aware allocation that optimally
/// distributes precision tiers to fit within the target size. Otherwise falls back
/// to fixed-threshold assignment.
///
/// `pub(crate)`: the `plasticity/{analyze,compact,pipeline}` command bodies in
/// `commands/plasticity/` orchestrate over it. The topology-construction domain
/// logic stays here in the module (mirrors genome's domain-in-module / wire-in-
/// commands split); the commands are thin.
pub(crate) fn build_topology(
    utilization: &types::UtilizationData,
    config: &types::CompactionConfig,
    arch: &crate::model_registry::ModelArchConfig,
) -> types::HeadTopology {
    let head_dim = arch.head_dim;
    let hidden_size = arch.hidden_size;

    let layers = if let Some(target_gb) = config.target_size_gb {
        // Budget-aware: fit the model into target_gb
        let non_attention_bytes = scoring::estimate_non_attention_bytes(
            utilization.layer_scores.len(),
            hidden_size,
            arch.intermediate_size,
            arch.vocab_size,
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

    /// Qwen2.5-Coder-32B dims as a test fixture. Production sources these from
    /// the artifact via `ModelArchConfig::from_artifact`; the constants live
    /// ONLY here, `#[cfg(test)]`-gated.
    fn qwen2_32b_arch() -> crate::model_registry::ModelArchConfig {
        crate::model_registry::ModelArchConfig::new(64, 5120, 40, 8, 128, 27648, 152064, 32768)
            .unwrap()
    }

    /// Llama-3.2-3B dims as a test fixture (same gating rationale).
    fn llama_3b_arch() -> crate::model_registry::ModelArchConfig {
        crate::model_registry::ModelArchConfig::new(28, 3072, 24, 8, 128, 8192, 128256, 131072)
            .unwrap()
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

        let topo = build_topology(&data, &config, &qwen2_32b_arch());

        assert_eq!(topo.base_model, "Qwen/Qwen2.5-Coder-32B-Instruct");
        assert_eq!(topo.head_dim, 128);
        assert_eq!(topo.original_num_heads, 40);
        assert_eq!(topo.original_num_kv_heads, 8);
        assert_eq!(topo.layers.len(), 64);

        // Should have some reduction since we're going from ~65GB BF16 to 20GB target
        assert!(
            topo.parameter_reduction > 0.0,
            "Should have parameter reduction"
        );

        // Precision profile should have a mix (not all BF16 — budget is tight)
        let pp = &topo.precision_profile;
        let total = pp.total_original();
        assert_eq!(total, 64 * 40, "Total heads should be 64 layers × 40 heads");
        assert!(
            pp.bf16 < total,
            "Not all heads should be BF16 at 20GB target"
        );

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
        let topo_threshold = build_topology(&data, &threshold_config, &llama_3b_arch());

        // With a very tight budget, should compress more aggressively
        let budget_config = types::CompactionConfig {
            target_size_gb: Some(1.0),
            ..types::CompactionConfig::default()
        };
        let topo_budget = build_topology(&data, &budget_config, &llama_3b_arch());

        // Budget mode with tight target should have more compression
        assert!(
            topo_budget.parameter_reduction >= topo_threshold.parameter_reduction
                || topo_budget.precision_profile.bf16 <= topo_threshold.precision_profile.bf16,
            "Budget-aware with tight target should compress more aggressively"
        );
    }

    // what this catches: the legacy string-dispatch path is dead — any call into it
    // fails loud naming the command, never silently no-ops. The plasticity verbs route
    // through the typed registry (commands/plasticity/), not handle_command.
    #[tokio::test]
    async fn legacy_handle_command_fails_loud() {
        let module = PlasticityModule::new();
        let err = module
            .handle_command("plasticity/analyze", Value::Null)
            .await
            .unwrap_err();
        assert!(err.contains("plasticity/analyze"));
        assert!(err.contains("migrated to the typed registry"));
    }
}
