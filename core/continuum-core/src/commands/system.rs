//! `system/*` — the resource-monitoring reads (`system/cpu`, `system/memory`,
//! `system/resources`, `system/pressure`, `system/memory-gate`,
//! `system/memory-budget`, `system/docker-tier-stats`) in the sibling files under
//! `system/`. Substrate identity (build #, sha, built-at) is `ping`.
//!
//! The resource reads are the dep-holding shape: they capture the module's
//! [`SystemResourceService`] and are assembled by [`command_objects`].

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::modules::system_resources::SystemResourceService;
use crate::sdk_codegen::DynCommand;

pub mod cpu;
pub mod docker_tier_stats;
pub mod launch_mode;
pub mod memory;
pub mod memory_budget;
pub mod memory_gate;
pub mod pressure;
pub mod pressure_broker_state;
pub mod reclaim_build_residue;
pub mod resources;
// `system/shutdown` holds no deps, so `action_command!`'s stateless arm registers it
// onto the one registry by itself — it is deliberately NOT in `command_objects` below,
// which is only for commands that need construction with a service.
pub mod shutdown;

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
