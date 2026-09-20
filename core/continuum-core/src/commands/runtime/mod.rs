//! `commands/runtime/` — runtime introspection (the Ares self-management pattern).
//!
//! Read-only observability over the live `ModuleRegistry`: per-module timing
//! stats (`runtime/metrics/{all,module,slow}`) and the module catalog
//! (`runtime/list`). Each command shares the `RuntimeRegistryCell` the owning
//! [`crate::modules::runtime_control::RuntimeModule`] fills at `initialize`,
//! exposed through that module's `commands()`. Gated `Privileged` — trusted
//! citizens introspect runtime health; untrusted callers do not enumerate the
//! host's modules. See [docs/architecture/COMMAND-ORGANIZATION.md].

use std::sync::Arc;

use crate::modules::runtime_control::RuntimeRegistryCell;
use crate::sdk_codegen::DynCommand;

pub mod list;
pub mod metrics;

/// Shared params for the no-argument runtime-introspection commands
/// (`metrics/all`, `metrics/slow`, `list`): they enumerate the whole registry
/// and take no input.
#[derive(
    Debug, Clone, Default, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema,
)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/runtime/RuntimeQueryParams.ts"
)]
pub struct RuntimeQueryParams {}

/// The `runtime/*` introspection commands as typed self-routing objects, each
/// sharing the module's registry cell.
pub fn command_objects(registry: RuntimeRegistryCell) -> Vec<Arc<dyn DynCommand>> {
    vec![
        Arc::new(metrics::all::RuntimeMetricsAll {
            registry: registry.clone(),
        }),
        Arc::new(metrics::module::RuntimeMetricsModule {
            registry: registry.clone(),
        }),
        Arc::new(metrics::slow::RuntimeMetricsSlow {
            registry: registry.clone(),
        }),
        Arc::new(list::RuntimeList { registry }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::ActionCommand;

    // what this catches: every runtime/* command's registered NAME mirrors its
    // file path under commands/runtime/ — the path==name invariant — and a guard
    // that command_objects() stays in sync with the files.
    #[test]
    fn runtime_command_names_mirror_their_path() {
        assert_eq!(metrics::all::RuntimeMetricsAll::NAME, "runtime/metrics/all");
        assert_eq!(
            metrics::module::RuntimeMetricsModule::NAME,
            "runtime/metrics/module"
        );
        assert_eq!(
            metrics::slow::RuntimeMetricsSlow::NAME,
            "runtime/metrics/slow"
        );
        assert_eq!(list::RuntimeList::NAME, "runtime/list");
    }
}
