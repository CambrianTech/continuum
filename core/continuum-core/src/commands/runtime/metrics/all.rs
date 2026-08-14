//! `runtime/metrics/all` — aggregate timing stats for every registered module.

use crate::modules::runtime_control::RuntimeRegistryCell;
use crate::runtime::ModuleStats;

/// Result of `runtime/metrics/all`: one [`ModuleStats`] per module that has
/// recorded timing, plus the count.
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/runtime/RuntimeMetricsAllResult.ts"
)]
pub struct RuntimeMetricsAllResult {
    pub modules: Vec<ModuleStats>,
    #[ts(type = "number")]
    pub count: usize,
}

crate::action_command! {
    /// Get aggregate timing statistics (command count, average/p50/p95/p99
    /// latency, slow-command count) for every registered runtime module. The
    /// runtime tracks timing for all commands automatically; this reads it.
    pub struct RuntimeMetricsAll { registry: RuntimeRegistryCell }
    name: "runtime/metrics/all",
    access: Privileged,
    params: super::super::RuntimeQueryParams,
    output: RuntimeMetricsAllResult,
    run(this, _ctx, _p) => {
        let registry = this.registry.get().ok_or("RuntimeModule not initialized")?;
        let modules: Vec<ModuleStats> = registry
            .module_names()
            .into_iter()
            .filter_map(|name| registry.get_metrics(&name).map(|m| m.stats()))
            .collect();
        let count = modules.len();
        Ok(RuntimeMetricsAllResult { modules, count })
    }
}
