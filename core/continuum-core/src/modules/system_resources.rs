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

use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use crate::system_resources::{MemoryPressureMonitor, SystemResourceMonitor};
use async_trait::async_trait;
use serde_json::Value;
use std::any::Any;
use std::sync::{Arc, OnceLock};

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
    pub fn cpu(&self) -> Result<Value, String> {
        let snapshot = self.monitor.refresh();
        serde_json::to_value(snapshot.cpu)
            .map_err(|e| format!("Failed to serialize CPU stats: {e}"))
    }

    /// Memory stats (total, used, pressure, swap) from a fresh reading.
    pub fn memory(&self) -> Result<Value, String> {
        let snapshot = self.monitor.refresh();
        serde_json::to_value(snapshot.memory)
            .map_err(|e| format!("Failed to serialize memory stats: {e}"))
    }

    /// Full snapshot (CPU + memory + optional top-N process listing).
    pub fn resources(&self, include_processes: bool, top_n: usize) -> Result<Value, String> {
        let snapshot = if include_processes {
            self.monitor.refresh_with_processes(top_n)
        } else {
            self.monitor.refresh()
        };
        serde_json::to_value(snapshot)
            .map_err(|e| format!("Failed to serialize system resources: {e}"))
    }

    /// Memory-pressure snapshot from the autonomous monitor.
    pub fn pressure(&self) -> Result<Value, String> {
        let snapshot = self.pressure_monitor()?.current();
        serde_json::to_value(snapshot).map_err(|e| format!("Failed to serialize pressure: {e}"))
    }

    /// Memory-gate state — whether the gate is closed (critical pressure sustained),
    /// plus the current pressure / RSS. Always succeeds; an unwired pressure monitor
    /// reports `0.0` / `0` rather than erroring (the gate is global state).
    pub fn memory_gate(&self) -> Value {
        let closed = crate::system_resources::is_memory_gate_closed();
        serde_json::json!({
            "closed": closed,
            "pressure": self.pressure_monitor.get().map(|pm| pm.pressure()).unwrap_or(0.0),
            "rss_bytes": self.pressure_monitor.get().map(|pm| pm.rss_bytes()).unwrap_or(0),
        })
    }

    /// Budget snapshot — per-consumer allocation vs actual usage (priority, budget,
    /// usage, headroom, warnings).
    pub fn memory_budget(&self) -> Result<Value, String> {
        let snapshot = self.pressure_monitor()?.budget_snapshot();
        serde_json::to_value(snapshot).map_err(|e| format!("Failed to serialize budget: {e}"))
    }

    /// Docker storage-tier stats (capacity/used/pressure/detected). One probe; always
    /// returns the full shape even when Docker is absent (`detected: false` + zeros).
    pub fn docker_tier_stats(&self) -> Result<Value, String> {
        let stats = crate::modules::docker_tier_pool::DockerTierPool::snapshot_stats();
        serde_json::to_value(&stats)
            .map_err(|e| format!("Failed to serialize docker-tier-stats: {e}"))
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

    async fn handle_command(&self, command: &str, _params: Value) -> Result<CommandResult, String> {
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

    // what this catches: the cpu read returns a well-formed snapshot — core counts ≥ 1,
    // brand is a string, and global usage is a normalized [0,1] fraction.
    #[tokio::test]
    async fn cpu_reports_normalized_snapshot() {
        let service = test_service();
        // sysinfo needs a baseline then a delta — sleep briefly.
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let json = service.cpu().expect("cpu ok");
        assert!(json["physical_cores"].as_u64().unwrap() >= 1);
        assert!(json["logical_cores"].as_u64().unwrap() >= 1);
        assert!(json["brand"].is_string());
        let usage = json["global_usage"].as_f64().unwrap();
        assert!(usage >= 0.0 && usage <= 1.0, "CPU usage 0..1, got {usage}");
    }

    // what this catches: the memory read returns positive total bytes and a normalized
    // [0,1] pressure value.
    #[tokio::test]
    async fn memory_reports_total_and_pressure() {
        let json = test_service().memory().expect("memory ok");
        assert!(json["total_bytes"].as_u64().unwrap() > 0);
        let pressure = json["pressure"].as_f64().unwrap();
        assert!(
            pressure >= 0.0 && pressure <= 1.0,
            "pressure 0..1, got {pressure}"
        );
    }

    // what this catches: the default full snapshot carries cpu + memory + timing and
    // omits the (opt-in) process listing.
    #[tokio::test]
    async fn resources_default_omits_processes() {
        let service = test_service();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let json = service.resources(false, 10).expect("resources ok");
        assert!(json["cpu"]["physical_cores"].as_u64().unwrap() >= 1);
        assert!(json["memory"]["total_bytes"].as_u64().unwrap() > 0);
        assert!(json["timestamp_ms"].as_u64().unwrap() > 0);
        assert!(json["uptime_seconds"].as_u64().unwrap() > 0);
        assert!(json["processes"].is_null(), "processes null by default");
    }

    // what this catches: requesting processes materializes the top-by-cpu/top-by-memory
    // listings instead of leaving them null.
    #[tokio::test]
    async fn resources_with_processes_populates_listings() {
        let service = test_service();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let json = service.resources(true, 5).expect("resources ok");
        assert!(
            json["processes"].is_object(),
            "processes present when requested"
        );
        assert!(json["processes"]["top_by_cpu"].is_array());
        assert!(json["processes"]["top_by_memory"].is_array());
    }

    // what this catches: docker-tier-stats always returns the full four-field shape so
    // callers can structurally match it even on a host without Docker (CI: detected=false,
    // zeros), and pressure is finite & non-negative.
    #[tokio::test]
    async fn docker_tier_stats_has_full_shape() {
        let json = test_service().docker_tier_stats().expect("docker ok");
        assert!(json["capacityBytes"].is_number(), "capacityBytes missing");
        assert!(json["usedBytes"].is_number(), "usedBytes missing");
        assert!(json["pressure"].is_number(), "pressure missing");
        assert!(json["detected"].is_boolean(), "detected missing");
        let pressure = json["pressure"].as_f64().unwrap();
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
        assert_eq!(gate["pressure"].as_f64().unwrap(), 0.0);
        assert_eq!(gate["rss_bytes"].as_u64().unwrap(), 0);
    }

    // what this catches: the legacy string-match surface is retired — handle_command
    // names the unregistered command instead of silently dispatching it.
    #[tokio::test]
    async fn legacy_handle_command_fails_loud() {
        let module = SystemResourceModule::new(Arc::new(SystemResourceMonitor::new()));
        let err = module
            .handle_command("system/cpu", Value::Null)
            .await
            .unwrap_err();
        assert!(err.contains("system/cpu"), "names the command: {err}");
        assert!(err.contains("migrated"), "explains why: {err}");
    }
}
