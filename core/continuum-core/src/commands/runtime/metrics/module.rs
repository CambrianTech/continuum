//! `runtime/metrics/module` — timing stats for one named module.

use crate::modules::runtime_control::RuntimeRegistryCell;
use crate::runtime::ModuleStats;

/// Params for `runtime/metrics/module`: the module to query.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/runtime/RuntimeMetricsModuleParams.ts"
)]
pub struct RuntimeMetricsModuleParams {
    /// The registered module name (e.g. `"ai_provider"`, `"data"`).
    pub module: String,
}

crate::action_command! {
    /// Get aggregate timing statistics for a single named runtime module. Errors
    /// if the module is not registered.
    pub struct RuntimeMetricsModule { registry: RuntimeRegistryCell }
    name: "runtime/metrics/module",
    access: Privileged,
    params: RuntimeMetricsModuleParams,
    output: ModuleStats,
    run(this, _ctx, p) => {
        let registry = this.registry.get().ok_or("RuntimeModule not initialized")?;
        let metrics = registry
            .get_metrics(&p.module)
            .ok_or_else(|| format!("Module '{}' not found", p.module))?;
        Ok(metrics.stats())
    }
}
