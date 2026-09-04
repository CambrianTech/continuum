//! SystemResourceModule — CPU, memory, and system-resource monitoring.
//!
//! The seven `system/<...>` reads (cpu, memory, resources, pressure, memory-gate,
//! memory-budget, docker-tier-stats) are typed [`ActionCommand`]s under
//! `crate::commands::system` — each a non-secret snapshot read on the `AiSafe`
//! surface. Their shared state lives in [`SystemResourceService`], which this module
//! owns as an `Arc` and hands to the command objects via [`ServiceModule::commands`].
//!
//! The pressure monitor is wired in *after* construction (see `ipc/mod.rs`), so the
//! service holds it behind a shared `OnceLock` — the command objects read whatever
//! value is current, never a stale clone.

use crate::modules::docker_tier_pool::DockerTierStats;
use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use crate::system_resources::{
    CpuStats, MemoryBudgetSnapshot, MemoryPressureMonitor, MemoryStats, PressureSnapshot,
    SystemResourceMonitor, SystemResourceSnapshot,
};
use async_trait::async_trait;
use serde::Serialize;
use std::any::Any;
use std::sync::{Arc, OnceLock};
use ts_rs::TS;

/// Memory-gate state — whether the global gate is closed (critical pressure sustained),
/// plus the current pressure / RSS. The typed projection behind `system/memory-gate`.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/system/MemoryGateState.ts"
)]
pub struct MemoryGateState {
    /// `true` when the global memory gate is closed (critical pressure sustained).
    pub closed: bool,
    /// Current memory pressure as a normalized 0..1 fraction (`0.0` if unwired).
    pub pressure: f64,
    /// Resident set size in bytes (`0` if the pressure monitor is unwired).
    #[ts(type = "number")]
    pub rss_bytes: u64,
}

/// Shared state behind the `system/*` resource reads. Holds the live resource
/// monitor and (once wired) the memory-pressure monitor; every read method below is
/// a thin, side-effect-free projection of a fresh snapshot.
pub struct SystemResourceService {
    monitor: Arc<SystemResourceMonitor>,
    /// Set after construction via [`Self::set_pressure_monitor`]. Shared
    /// (`Arc<OnceLock>`) so command objects built at registration time observe the
    /// late-wired value.
    pressure_monitor: Arc<OnceLock<Arc<MemoryPressureMonitor>>>,
}

impl SystemResourceService {
    pub fn new(monitor: Arc<SystemResourceMonitor>) -> Self {
        Self {
            monitor,
            pressure_monitor: Arc::new(OnceLock::new()),
        }
    }

    /// Wire the memory-pressure monitor. Idempotent (first write wins) — safe to call
    /// through a shared `&self` after the service is already in an `Arc`.
    pub fn set_pressure_monitor(&self, pm: Arc<MemoryPressureMonitor>) {
        let _ = self.pressure_monitor.set(pm);
    }

    fn pressure_monitor(&self) -> Result<&Arc<MemoryPressureMonitor>, String> {
        self.pressure_monitor
            .get()
            .ok_or_else(|| "Memory pressure monitor not initialized".to_string())
    }

    /// CPU stats (cores, usage, brand) from a fresh reading.
    pub fn cpu(&self) -> Result<CpuStats, String> {
        Ok(self.monitor.refresh().cpu)
    }

    /// Memory stats (total, used, pressure, swap) from a fresh reading.
    pub fn memory(&self) -> Result<MemoryStats, String> {
        Ok(self.monitor.refresh().memory)
    }

    /// Full snapshot (CPU + memory + optional top-N process listing).
    pub fn resources(
        &self,
        include_processes: bool,
        top_n: usize,
    ) -> Result<SystemResourceSnapshot, String> {
        let snapshot = if include_processes {
            self.monitor.refresh_with_processes(top_n)
        } else {
            self.monitor.refresh()
        };
        Ok(snapshot)
    }

    /// Memory-pressure snapshot from the autonomous monitor.
    pub fn pressure(&self) -> Result<PressureSnapshot, String> {
        Ok(self.pressure_monitor()?.current())
    }

    /// Memory-gate state — whether the gate is closed (critical pressure sustained),
    /// plus the current pressure / RSS. Always succeeds; an unwired pressure monitor
    /// reports `0.0` / `0` rather than erroring (the gate is global state).
    pub fn memory_gate(&self) -> MemoryGateState {
        MemoryGateState {
            closed: crate::system_resources::is_memory_gate_closed(),
            pressure: self
                .pressure_monitor
                .get()
                .map(|pm| pm.pressure())
                .unwrap_or(0.0),
            rss_bytes: self
                .pressure_monitor
                .get()
                .map(|pm| pm.rss_bytes())
                .unwrap_or(0),
        }
    }

    /// Budget snapshot — per-consumer allocation vs actual usage (priority, budget,
    /// usage, headroom, warnings).
    pub fn memory_budget(&self) -> Result<MemoryBudgetSnapshot, String> {
        Ok(self.pressure_monitor()?.budget_snapshot())
    }

    /// Docker storage-tier stats (capacity/used/pressure/detected). One probe; always
    /// returns the full shape even when Docker is absent (`detected: false` + zeros).
    pub fn docker_tier_stats(&self) -> Result<DockerTierStats, String> {
        Ok(crate::modules::docker_tier_pool::DockerTierPool::snapshot_stats())
    }
}

pub struct SystemResourceModule {
    service: Arc<SystemResourceService>,
}

impl SystemResourceModule {
    pub fn new(monitor: Arc<SystemResourceMonitor>) -> Self {
        Self {
            service: Arc::new(SystemResourceService::new(monitor)),
        }
    }

    /// Wire the memory pressure monitor into the shared service (see module docs).
    pub fn set_pressure_monitor(&self, pm: Arc<MemoryPressureMonitor>) {
        self.service.set_pressure_monitor(pm);
    }
}

#[async_trait]
impl ServiceModule for SystemResourceModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "system",
            priority: ModulePriority::Normal,
            command_prefixes: &["system/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
        Ok(())
    }

    async fn handle_command(
        &self,
        command: &str,
        _params: serde_json::Value,
    ) -> Result<CommandResult, String> {
        // MIGRATED: every `system/<...>` resource read is a typed command object under
        // `crate::commands::system`, dispatched via the typed registry. Nothing routes
        // here anymore — fail loud rather than silently swallow a stray name.
        Err(format!(
            "system command surface is migrated to the typed registry; '{command}' has no legacy handler"
        ))
    }

    fn commands(&self) -> Vec<Arc<dyn crate::sdk_codegen::DynCommand>> {
        crate::commands::system::command_objects(self.service.clone())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_service() -> Arc<SystemResourceService> {
        Arc::new(SystemResourceService::new(Arc::new(
            SystemResourceMonitor::new(),
        )))
    }

    // what this catches: the cpu read returns a well-formed snapshot — core counts ≥ 1
    // and global usage is a normalized [0,1] fraction. The typed shape guarantees the
    // remaining fields exist.
    #[tokio::test]
    async fn cpu_reports_normalized_snapshot() {
        let service = test_service();
        // sysinfo needs a baseline then a delta — sleep briefly.
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let stats = service.cpu().expect("cpu ok");
        assert!(stats.physical_cores >= 1);
        assert!(stats.logical_cores >= 1);
        let usage = stats.global_usage;
        assert!((0.0..=1.0).contains(&usage), "CPU usage 0..1, got {usage}");
    }

    // what this catches: the memory read returns positive total bytes and a normalized
    // [0,1] pressure value.
    #[tokio::test]
    async fn memory_reports_total_and_pressure() {
        let stats = test_service().memory().expect("memory ok");
        assert!(stats.total_bytes > 0);
        let pressure = stats.pressure;
        assert!(
            (0.0..=1.0).contains(&pressure),
            "pressure 0..1, got {pressure}"
        );
    }

    // what this catches: the default full snapshot carries cpu + memory + timing and
    // omits the (opt-in) process listing.
    #[tokio::test]
    async fn resources_default_omits_processes() {
        let service = test_service();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let snap = service.resources(false, 10).expect("resources ok");
        assert!(snap.cpu.physical_cores >= 1);
        assert!(snap.memory.total_bytes > 0);
        assert!(snap.timestamp_ms > 0);
        assert!(snap.uptime_seconds > 0);
        assert!(snap.processes.is_none(), "processes omitted by default");
    }

    // what this catches: requesting processes materializes the top-by-cpu/top-by-memory
    // listings instead of leaving them absent.
    #[tokio::test]
    async fn resources_with_processes_populates_listings() {
        let service = test_service();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let snap = service.resources(true, 5).expect("resources ok");
        assert!(snap.processes.is_some(), "processes present when requested");
    }

    // what this catches: docker-tier-stats always returns the full four-field shape so
    // callers can structurally match it even on a host without Docker (CI: detected=false,
    // zeros), and pressure is finite & non-negative.
    #[tokio::test]
    async fn docker_tier_stats_has_full_shape() {
        let stats = test_service().docker_tier_stats().expect("docker ok");
        let pressure = stats.pressure;
        assert!(pressure.is_finite(), "pressure must not be NaN/Inf");
        assert!(pressure >= 0.0, "pressure must be ≥ 0.0");
    }

    // what this catches: pressure / budget reads fail loud (not panic, not a fake zero)
    // when the pressure monitor was never wired.
    #[tokio::test]
    async fn unwired_pressure_monitor_fails_loud() {
        let service = test_service();
        assert!(service.pressure().is_err());
        assert!(service.memory_budget().is_err());
        // memory_gate degrades to 0.0/0 rather than erroring — it reads global state.
        let gate = service.memory_gate();
        assert_eq!(gate.pressure, 0.0);
        assert_eq!(gate.rss_bytes, 0);
    }

    // what this catches: the legacy string-match surface is retired — handle_command
    // names the unregistered command instead of silently dispatching it.
    #[tokio::test]
    async fn legacy_handle_command_fails_loud() {
        let module = SystemResourceModule::new(Arc::new(SystemResourceMonitor::new()));
        let err = module
            .handle_command("system/cpu", serde_json::Value::Null)
            .await
            .unwrap_err();
        assert!(err.contains("system/cpu"), "names the command: {err}");
        assert!(err.contains("migrated"), "explains why: {err}");
    }
}
