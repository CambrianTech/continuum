//! `runtime/list` — the catalog of registered modules and their configs.

use crate::modules::runtime_control::RuntimeRegistryCell;

/// A registered module's config, as surfaced by `runtime/list`. `priority` is the
/// `ModulePriority` rendered as its debug string (e.g. `"Normal"`). Distinct from
/// the richer [`crate::runtime::ModuleInfo`] (default/effective priority + live
/// stats) used by `RuntimeControl` — this is the flat config snapshot `runtime/list`
/// has always returned.
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/runtime/RuntimeListModuleInfo.ts"
)]
pub struct RuntimeListModuleInfo {
    pub name: String,
    pub priority: String,
    pub command_prefixes: Vec<String>,
    pub needs_dedicated_thread: bool,
    #[ts(type = "number")]
    pub max_concurrency: usize,
}

/// Result of `runtime/list`: every registered module's config, plus the count.
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/runtime/RuntimeListResult.ts"
)]
pub struct RuntimeListResult {
    pub modules: Vec<RuntimeListModuleInfo>,
    #[ts(type = "number")]
    pub count: usize,
}

crate::action_command! {
    /// List every registered runtime module with its config: priority, command
    /// prefixes, dedicated-thread requirement, and max concurrency.
    pub struct RuntimeList { registry: RuntimeRegistryCell }
    name: "runtime/list",
    access: Privileged,
    params: super::RuntimeQueryParams,
    output: RuntimeListResult,
    run(this, _ctx, _p) => {
        let registry = this.registry.get().ok_or("RuntimeModule not initialized")?;
        let modules: Vec<RuntimeListModuleInfo> = registry
            .module_names()
            .into_iter()
            .filter_map(|name| registry.get_config(&name))
            .map(|config| RuntimeListModuleInfo {
                name: config.name.to_string(),
                priority: format!("{:?}", config.priority),
                command_prefixes: config
                    .command_prefixes
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
                needs_dedicated_thread: config.needs_dedicated_thread,
                max_concurrency: config.max_concurrency,
            })
            .collect();
        let count = modules.len();
        Ok(RuntimeListResult { modules, count })
    }
}
