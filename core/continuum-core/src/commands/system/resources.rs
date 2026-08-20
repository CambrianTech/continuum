//! `system/resources` — full system snapshot (CPU + memory + optional top-N process
//! listing). A non-secret read → `AiSafe`.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::modules::system_resources::SystemResourceService;
use crate::sdk_codegen::CommandError;
use crate::system_resources::SystemResourceSnapshot;

fn default_top_n() -> u32 {
    10
}

/// Params for `system/resources`: whether to include the per-process listing, and how
/// many processes per listing.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/system/ResourcesParams.ts"
)]
pub struct ResourcesParams {
    /// Include the top-by-cpu / top-by-memory process listing (default `false`).
    #[serde(default)]
    pub include_processes: bool,
    /// Processes per listing when `include_processes` is set (default `10`).
    #[serde(default = "default_top_n")]
    #[ts(type = "number")]
    pub top_n: u32,
}

impl Default for ResourcesParams {
    fn default() -> Self {
        Self {
            include_processes: false,
            top_n: default_top_n(),
        }
    }
}

crate::action_command! {
    /// Full system snapshot: CPU + memory + timing, and (when `includeProcesses` is set)
    /// the top-N processes by CPU and by memory.
    pub struct SystemResources { service: Arc<SystemResourceService> }
    name: "system/resources",
    access: AiSafe,
    params: ResourcesParams,
    output: SystemResourceSnapshot,
    run(this, _ctx, p) => {
        this.service
            .resources(p.include_processes, p.top_n as usize)
            .map_err(CommandError::Internal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{ActionCommand, Ctx};
    use crate::system_resources::SystemResourceMonitor;

    fn cmd() -> SystemResources {
        SystemResources {
            service: Arc::new(SystemResourceService::new(Arc::new(
                SystemResourceMonitor::new(),
            ))),
        }
    }

    // what this catches: name/access wiring — a system read is on the AiSafe surface.
    #[test]
    fn name_and_access_wired() {
        assert_eq!(SystemResources::NAME, "system/resources");
        assert!(matches!(
            SystemResources::ACCESS,
            crate::sdk_codegen::AccessLevel::AiSafe
        ));
    }

    // what this catches: the default params omit the process listing (leaving it null).
    #[tokio::test]
    async fn default_omits_processes() {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let out = cmd()
            .run(&Ctx::default(), ResourcesParams::default())
            .await
            .unwrap();
        assert!(out.cpu.physical_cores >= 1);
        assert!(out.processes.is_none(), "processes omitted by default");
    }

    // what this catches: opting in materializes the process listings.
    #[tokio::test]
    async fn opt_in_populates_processes() {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let out = cmd()
            .run(
                &Ctx::default(),
                ResourcesParams {
                    include_processes: true,
                    top_n: 5,
                },
            )
            .await
            .unwrap();
        assert!(out.processes.is_some(), "opt-in materializes the listing");
    }
}
