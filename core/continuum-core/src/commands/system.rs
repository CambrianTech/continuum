//! `system/*` — substrate identity (`system/info`, stateless below) plus the
//! resource-monitoring reads (`system/cpu`, `system/memory`, `system/resources`,
//! `system/pressure`, `system/memory-gate`, `system/memory-budget`,
//! `system/docker-tier-stats`) in the sibling files under `system/`.
//!
//! `system/info` is the zero-ceremony stateless example (after `commands/list`): one
//! file, one `run` body, `register_stateless_command!`, instantly callable via
//! `continuum system/info`, the persona's tools, and every SDK — derived param schema + ACL
//! gating, no wiring anywhere else. The resource reads are the dep-holding shape: they
//! capture the module's [`SystemResourceService`] and are assembled by
//! [`command_objects`].

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::modules::system_resources::SystemResourceService;
use crate::sdk_codegen::{ActionCommand, CommandError, Ctx, DynCommand};

pub mod cpu;
pub mod docker_tier_stats;
pub mod launch_mode;
pub mod memory;
pub mod memory_budget;
pub mod memory_gate;
pub mod pressure;
pub mod pressure_broker_state;
pub mod resources;

/// Shared params for the no-argument `system/*` reads (cpu, memory, pressure,
/// memory-gate, memory-budget, docker-tier-stats). One empty contract reused across
/// the six rather than six identical placeholder structs (compression principle).
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/system/SystemQuery.ts"
)]
pub struct SystemQuery {}

/// Build the dep-holding `system/*` resource-read command objects over the shared
/// [`SystemResourceService`]. Called from `SystemResourceModule::commands`.
pub fn command_objects(service: Arc<SystemResourceService>) -> Vec<Arc<dyn DynCommand>> {
    vec![
        Arc::new(cpu::SystemCpu {
            service: service.clone(),
        }),
        Arc::new(memory::SystemMemory {
            service: service.clone(),
        }),
        Arc::new(resources::SystemResources {
            service: service.clone(),
        }),
        Arc::new(pressure::SystemPressure {
            service: service.clone(),
        }),
        Arc::new(memory_gate::SystemMemoryGate {
            service: service.clone(),
        }),
        Arc::new(memory_budget::SystemMemoryBudget {
            service: service.clone(),
        }),
        Arc::new(docker_tier_stats::SystemDockerTierStats { service }),
    ]
}

/// Params for `system/info` — none today (a placeholder struct so the command has
/// a canonical, schema-able params type like every other command).
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/system/SystemInfoParams.ts"
)]
pub struct SystemInfoParams {}

/// Result of `system/info` — what a client needs to confirm it's talking to a
/// compatible, live substrate.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/system/SystemInfoResult.ts"
)]
pub struct SystemInfoResult {
    /// The continuum-core crate version (`CARGO_PKG_VERSION`).
    pub version: String,
    /// OS process id of the running core — handy for ops (`continuum stop` targets it).
    pub pid: u32,
}

/// `system/info` — build + process identity. Stateless, AiSafe.
#[derive(Default)]
pub struct SystemInfo;

#[async_trait]
impl ActionCommand for SystemInfo {
    const NAME: &'static str = "system/info";
    const DESCRIPTION: &'static str =
        "Report the running substrate's version and process id — for client \
         compatibility checks and ops.";
    type Params = SystemInfoParams;
    type Output = SystemInfoResult;

    async fn run(
        &self,
        _ctx: &Ctx,
        _params: SystemInfoParams,
    ) -> Result<SystemInfoResult, CommandError> {
        Ok(SystemInfoResult {
            version: env!("CARGO_PKG_VERSION").to_string(),
            pid: std::process::id(),
        })
    }
}
crate::register_stateless_command!(SystemInfo);

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: system/info is a complete, routable command from one
    // file — it reports a non-empty version and the live pid, and (via the stateless
    // registration proven elsewhere) is on the typed object map with no wiring. The
    // "minimal code per command" guarantee, exercised.
    #[tokio::test]
    async fn reports_version_and_pid() {
        let out = SystemInfo
            .run(&Ctx::default(), SystemInfoParams {})
            .await
            .expect("ok");
        assert!(!out.version.is_empty(), "version present");
        assert_eq!(out.pid, std::process::id(), "live process id");
    }
}
