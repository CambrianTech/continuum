//! `runtime/metrics/slow` — recent slow commands across all modules.

use crate::modules::runtime_control::RuntimeRegistryCell;

/// A single slow-command timing, attributed to its owning module. The typed,
/// camelCase form of the legacy ad-hoc JSON entry (the old `total_ms`/`execute_ms`/
/// `queue_ms` keys are renamed to the canonical `*TimeMs` matching
/// [`crate::runtime::CommandTiming`]; the `success` flag is omitted — a slow
/// command is reported regardless of outcome).
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/runtime/SlowCommand.ts"
)]
pub struct SlowCommand {
    pub module: String,
    pub command: String,
    #[ts(type = "number")]
    pub total_time_ms: u64,
    #[ts(type = "number")]
    pub execute_time_ms: u64,
    #[ts(type = "number")]
    pub queue_time_ms: u64,
}

/// Result of `runtime/metrics/slow`: slow commands sorted by total time
/// descending, with the count and the threshold (ms) that classifies "slow".
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/runtime/RuntimeMetricsSlowResult.ts"
)]
pub struct RuntimeMetricsSlowResult {
    pub slow_commands: Vec<SlowCommand>,
    #[ts(type = "number")]
    pub count: usize,
    #[ts(type = "number")]
    pub threshold_ms: u64,
}

crate::action_command! {
    /// List recent slow commands across all modules, sorted by total time
    /// (queue + execute) descending. Useful for spotting latency hot spots.
    pub struct RuntimeMetricsSlow { registry: RuntimeRegistryCell }
    name: "runtime/metrics/slow",
    access: Privileged,
    params: super::super::RuntimeQueryParams,
    output: RuntimeMetricsSlowResult,
    run(this, _ctx, _p) => {
        let registry = this.registry.get().ok_or("RuntimeModule not initialized")?;
        let mut slow_commands: Vec<SlowCommand> = Vec::new();
        for name in registry.module_names() {
            if let Some(metrics) = registry.get_metrics(&name) {
                for timing in metrics.slow_commands() {
                    slow_commands.push(SlowCommand {
                        module: name.clone(),
                        command: timing.command,
                        total_time_ms: timing.total_time_ms,
                        execute_time_ms: timing.execute_time_ms,
                        queue_time_ms: timing.queue_time_ms,
                    });
                }
            }
        }
        slow_commands.sort_by(|a, b| b.total_time_ms.cmp(&a.total_time_ms));
        let count = slow_commands.len();
        Ok(RuntimeMetricsSlowResult {
            slow_commands,
            count,
            // The slow-command threshold the runtime metrics layer records against.
            threshold_ms: 50,
        })
    }
}
