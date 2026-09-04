//! InferenceCoordinatorModule — singleton bootstrap for the
//! `InferenceCoordinator` (the multi-persona-one-model lane substrate).
//!
//! Closes the last-mile wiring gap on the realistic-lane build
//! (INFERENCE-LANES-REALISTIC.md): the coordinator, its
//! `evict_under_pressure` walk, and the `CoordinatorResourcePool` bridge
//! (`ResourcePool::evict_at_least → evict_under_pressure`) were all built
//! and unit-tested, but nothing stood the coordinator up in production or
//! registered its pool with the live `PressureBroker`. Without that, the
//! coordinator existed but no broker tick ever drove its lane eviction.
//!
//! This module is that bootstrap, mirroring
//! `modules/pressure_broker_module.rs` exactly:
//!   1. Singleton instantiated at server boot, registered on the runtime
//!      like any other `ServiceModule`.
//!   2. Owns the `Arc<InferenceCoordinator>` so a single instance is shared
//!      — the broker drives its pool, and (follow-up) the handle module
//!      routes `ai/inference/{open,generate,close}` through the SAME Arc.
//!   3. `register_with_broker()` wraps the coordinator in a
//!      `CoordinatorResourcePool` and registers it on the broker, so the
//!      broker's existing 5s tier-monitoring tick fires lane eviction the
//!      same way it fires VRAM eviction on the Docker tier.
//!
//! Why a wrapper module vs a global: every other substrate singleton in
//! this server (gpu_manager, the PressureBroker itself) lives behind a
//! `ServiceModule`. Following that pattern keeps the boot sequence in
//! `ipc/mod.rs` uniform and gives the coordinator the same lifecycle
//! treatment as everything else. Per Joel (2026-06-14): "It lies in
//! continuum as a service module yes."
//!
//! Deferred to a follow-up slice on this thread:
//!   - Route `InferenceHandleModule` through `with_coordinator(self.coordinator())`
//!     so opens actually create lanes (until then the registered pool is
//!     armed but idle — usage 0, so the broker never needs to act on it).
//!   - Per-tier lane BUDGET numbers from the governor's policy file once it
//!     emits its `TierConfig`s. The module already hardware-detects the
//!     SILICON class (`CoordinatorConfig::detected()` → Gpu on a discrete
//!     GPU, UnifiedMemory on Apple Silicon, Cpu on a GPU-less host); only
//!     the per-tier capacity numbers remain governor-deferred.

use crate::inference::coordinator::{CoordinatorConfig, InferenceCoordinator};
use crate::inference::coordinator_pool::CoordinatorResourcePool;
use crate::inference::footprint_registry::FootprintRegistry;
use crate::inference::handle_store::InferenceHandleStore;
use crate::paging::{PressureBroker, ResourcePool};
use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use async_trait::async_trait;
use serde_json::Value;
use std::any::Any;
use std::sync::Arc;

pub struct InferenceCoordinatorModule {
    coordinator: Arc<InferenceCoordinator>,
}

impl InferenceCoordinatorModule {
    /// Construct from an explicit `CoordinatorConfig` with a fresh
    /// `FootprintRegistry` + `InferenceHandleStore`. Tests use this to pin
    /// a deterministic budget; production uses `with_detected_hardware`.
    pub fn new(config: CoordinatorConfig) -> Self {
        let footprint = Arc::new(FootprintRegistry::new());
        let handle_store = Arc::new(InferenceHandleStore::new());
        let coordinator = Arc::new(InferenceCoordinator::new(footprint, handle_store, config));
        Self { coordinator }
    }

    /// Production constructor — **hardware-detected** silicon. Probes the
    /// machine (`CoordinatorConfig::detected`), so the lanes target the
    /// actual accelerator: `Gpu` on an RTX 5090, `UnifiedMemory` on Apple
    /// Silicon, `Cpu` on a GPU-less host. This RETIRES the old hardcoded
    /// `UnifiedMemory` floor default (GPU-or-bust — the default follows the
    /// hardware, not a Mac/CPU floor). Governor policy-file budgets refine
    /// the per-tier numbers in a later slice.
    pub fn with_detected_hardware() -> Self {
        Self::new(CoordinatorConfig::detected())
    }

    /// Borrow the coordinator so the boot sequence can hand the SAME `Arc`
    /// to the handle module (`with_coordinator`) — keeping one coordinator
    /// instance behind both the command surface and the pressure pool.
    pub fn coordinator(&self) -> Arc<InferenceCoordinator> {
        self.coordinator.clone()
    }

    /// Register this coordinator's lanes as a `ResourcePool` on `broker` so
    /// pressure drives lane eviction. Called once at boot from `ipc/mod.rs`
    /// after both this module and the `PressureBrokerModule` exist. The pool
    /// reports `usage = open lanes' KV bytes`, `capacity = lane budget`; the
    /// broker's tick reads `pressure()` and fires `evict_at_least` (→
    /// `evict_under_pressure`) when the tier crosses its act threshold.
    pub fn register_with_broker(&self, broker: &PressureBroker) {
        broker.register(
            Arc::new(CoordinatorResourcePool::new(self.coordinator.clone()))
                as Arc<dyn ResourcePool>,
        );
    }
}

#[async_trait]
impl ServiceModule for InferenceCoordinatorModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "inference-coordinator",
            priority: ModulePriority::Normal,
            // No routed command surface yet — opens/generates/closes flow
            // through `InferenceHandleModule`, which (follow-up) shares this
            // module's coordinator Arc. The broker drives eviction on its
            // own tick, so this module needs no tick of its own.
            command_prefixes: &[],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
        Ok(())
    }

    async fn handle_command(&self, command: &str, _params: Value) -> Result<CommandResult, String> {
        Err(format!(
            "inference-coordinator: no routed commands (got '{command}'); \
             open/generate/close route through the inference handle module"
        ))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cognition::adaptive_throughput::{
        ResourceClass, TargetSilicon, ThroughputLaneBudget,
    };
    use crate::paging::{BrokerConfig, PressureBroker};

    fn test_config() -> CoordinatorConfig {
        CoordinatorConfig {
            lane_budgets: vec![ThroughputLaneBudget {
                resource_class: ResourceClass::LocalGeneration,
                target_silicon: TargetSilicon::Cpu,
                max_concurrency: 8,
                max_cost_units: 50_000,
            }],
            bytes_per_token: 1,
            lease_duration_ms: 5_000_000,
            default_target_silicon: TargetSilicon::Cpu,
        }
    }

    /// What this catches: the module's declared config drifting from the
    /// substrate contract — a routed command_prefix would make the runtime
    /// route IPC here (there is no command surface yet), and a stray
    /// tick_interval would spin a tick the broker already owns.
    #[test]
    fn config_declares_no_commands_and_no_tick() {
        let m = InferenceCoordinatorModule::new(test_config());
        let cfg = m.config();
        assert_eq!(cfg.name, "inference-coordinator");
        assert!(cfg.command_prefixes.is_empty());
        assert!(cfg.tick_interval.is_none());
    }

    /// What this catches: `coordinator()` handing back a different instance
    /// than the one the module owns — the whole point is ONE coordinator
    /// shared between the pressure pool and (follow-up) the handle module.
    #[test]
    fn coordinator_getter_returns_the_owned_instance() {
        let m = InferenceCoordinatorModule::new(test_config());
        let a = m.coordinator();
        let b = m.coordinator();
        assert!(Arc::ptr_eq(&a, &b), "getter must return the same Arc");
        assert!(
            Arc::ptr_eq(&a, &m.coordinator),
            "getter must return the module's owned coordinator"
        );
    }

    /// What this catches: `register_with_broker` not actually registering a
    /// pool (the whole realization — without this the broker never drives
    /// lane eviction). After registration the broker must see the
    /// `inference-lanes` tier in its snapshot.
    #[test]
    fn register_with_broker_adds_the_coordinator_pool() {
        let m = InferenceCoordinatorModule::new(test_config());
        let broker = PressureBroker::new(BrokerConfig::default());
        assert!(broker.snapshot().pools.is_empty());
        m.register_with_broker(&broker);
        let pools = broker.snapshot().pools;
        assert_eq!(pools.len(), 1);
        assert_eq!(
            pools[0].name,
            crate::inference::coordinator_pool::TIER_NAME,
            "registered pool must be the inference-lanes tier"
        );
    }
}
