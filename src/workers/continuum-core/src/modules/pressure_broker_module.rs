//! PressureBrokerModule — singleton bootstrap for the cross-pool PressureBroker.
//!
//! Phase 2 of continuum#1239. Phase 1 (PR #1297) shipped the data-surface
//! `system/docker-tier-stats` IPC that bypassed the broker. This module
//! brings the broker online so disk-tier pressure can drive real eviction
//! instead of just sitting in the data layer:
//!
//!   1. Singleton instantiated at server boot (registered on the runtime
//!      like any other ServiceModule)
//!   2. DockerTierPool registered as a ResourcePool on the broker
//!   3. Periodic tick calls `PressureBroker::relieve()` on the broker's
//!      configured cadence (default 5s, matching DMR_TICK_INTERVAL)
//!
//! The runtime's `start_tick_loops()` machinery owns the cadence — we just
//! declare `tick_interval` in `config()` and implement `tick()`. Pattern
//! matches `modules/ai_provider.rs::AiProviderModule` exactly.
//!
//! Deferred to follow-up slices on this same card:
//!   - `system/pressure-broker-state` IPC + `bin/continuum status` row
//!     (PR-2): exposes broker snapshot to TS/CLI
//!   - Chat-substrate alert sink (PR-3): when threshold crosses, post a
//!     `📢 PressureAlert ...` to the AIRC #cambriantech room via the
//!     existing airc bridge
//!
//! Why a wrapper module vs `OnceLock<Arc<PressureBroker>>` directly: every
//! other singleton in this server (gpu_manager, system_monitor, etc.)
//! either lives behind a ServiceModule or is owned by one. Following that
//! pattern keeps the boot sequence in `ipc/mod.rs` uniform and gives the
//! broker the same shutdown / metrics treatment as everything else.

use crate::modules::docker_tier_pool::DockerTierPool;
use crate::paging::{BrokerConfig, PressureBroker, ResourcePool};
use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use async_trait::async_trait;
use serde_json::Value;
use std::any::Any;
use std::sync::Arc;

/// Single IPC command surface for the broker — returns a typed
/// `BrokerSnapshot` (see `paging::broker::BrokerSnapshot`, ts-rs exported
/// to `shared/generated/paging/BrokerSnapshot.ts`). PR-2 surface; the
/// CLI / status row consumes this in PR-3.
const SYSTEM_PRESSURE_BROKER_STATE: &str = "system/pressure-broker-state";

pub struct PressureBrokerModule {
    broker: Arc<PressureBroker>,
    tick_interval: std::time::Duration,
}

impl PressureBrokerModule {
    /// Construct with default `BrokerConfig` (5s tick, act_above=0.80) and
    /// `DockerTierPool` pre-registered. Other pools (VRAM via
    /// `GpuMemoryManager`, KV cache via `PagedResourcePool`) are added at
    /// their owning subsystems' construction sites via `broker()` getter.
    pub fn new() -> Self {
        Self::with_config(BrokerConfig::default())
    }

    /// Construct with an explicit `BrokerConfig`. Used by tests that want
    /// to drive a faster tick or a different threshold without mutating
    /// the singleton in production code.
    pub fn with_config(config: BrokerConfig) -> Self {
        let tick_interval = config.tick_interval;
        let broker = Arc::new(PressureBroker::new(config));
        broker.register(Arc::new(DockerTierPool::new()) as Arc<dyn ResourcePool>);
        Self {
            broker,
            tick_interval,
        }
    }

    /// Borrow the broker so other subsystems can register their own
    /// pools or attach alert sinks at boot. Public so the ipc/mod.rs
    /// bootstrap can `runtime.module_of_type::<PressureBrokerModule>()`,
    /// downcast, and wire follow-on slices without re-instantiating.
    pub fn broker(&self) -> Arc<PressureBroker> {
        self.broker.clone()
    }
}

impl Default for PressureBrokerModule {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ServiceModule for PressureBrokerModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "pressure-broker",
            priority: ModulePriority::Normal,
            // PR-2 of #1299: typed `system/pressure-broker-state` IPC.
            // Only this one command routes here; the alert sink (PR-3)
            // is a push surface, not a routed command.
            command_prefixes: &[SYSTEM_PRESSURE_BROKER_STATE],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: Some(self.tick_interval),
        }
    }

    async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
        Ok(())
    }

    /// Return a typed `BrokerSnapshot` describing global pressure, tier,
    /// per-pool state, and lifetime eviction counters. Single probe per
    /// call — cheap (pressure reads are atomic loads + a max over the
    /// pool list; no eviction is fired). Same shape ts-rs exports to
    /// `shared/generated/paging/BrokerSnapshot.ts`, so the TS mixin can
    /// consume it without a manual remap layer.
    async fn handle_command(&self, command: &str, _params: Value) -> Result<CommandResult, String> {
        match command {
            SYSTEM_PRESSURE_BROKER_STATE => {
                let snapshot = self.broker.snapshot();
                let json = serde_json::to_value(&snapshot).map_err(|e| {
                    format!("pressure-broker: failed to serialize BrokerSnapshot: {e}")
                })?;
                Ok(CommandResult::Json(json))
            }
            other => Err(format!(
                "pressure-broker: unknown command '{other}' (handled: {SYSTEM_PRESSURE_BROKER_STATE})"
            )),
        }
    }

    /// One relief pass per tick. The broker itself logs WARN-level alerts
    /// and forwards them to any registered sinks; we just drive the cadence.
    ///
    /// `relieve()` is sync and may invoke `evict_at_least()` on pools — for
    /// `DockerTierPool` that's a `docker system prune` subprocess call which
    /// can take seconds. Wrap in `spawn_blocking` so the broker tick never
    /// stalls other tokio tasks sharing the runtime.
    async fn tick(&self) -> Result<(), String> {
        let broker = self.broker.clone();
        tokio::task::spawn_blocking(move || {
            broker.relieve();
        })
        .await
        .map_err(|e| format!("pressure-broker tick join error: {e}"))?;
        Ok(())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paging::{ResourcePool, ResourcePoolEntry};
    use std::sync::atomic::{AtomicU64, Ordering};

    /// Fake pool whose pressure is driven by a test-controlled atomic.
    /// `evict_at_least` records the bytes requested so the test can
    /// assert the broker actually called eviction on this pool when
    /// threshold was crossed.
    struct FakePool {
        capacity: u64,
        usage: Arc<AtomicU64>,
        evict_called_with: Arc<AtomicU64>,
    }

    impl ResourcePool for FakePool {
        fn tier_name(&self) -> &str {
            "fake-test"
        }
        fn capacity_bytes(&self) -> u64 {
            self.capacity
        }
        fn usage_bytes(&self) -> u64 {
            self.usage.load(Ordering::Relaxed)
        }
        fn evict_at_least(&self, want_bytes: u64) -> u64 {
            self.evict_called_with.store(want_bytes, Ordering::Relaxed);
            // Pretend we freed everything requested so the broker reports
            // success — the assertion is on whether evict was CALLED.
            self.usage.fetch_sub(
                want_bytes.min(self.usage.load(Ordering::Relaxed)),
                Ordering::Relaxed,
            );
            want_bytes
        }
        fn snapshot(&self) -> Vec<ResourcePoolEntry> {
            Vec::new()
        }
    }

    #[test]
    fn module_registers_docker_pool_at_construction() {
        let module = PressureBrokerModule::new();
        // The broker should know about exactly one pool right after
        // construction — the DockerTierPool we pre-register.
        let snapshot = module.broker().snapshot();
        assert_eq!(
            snapshot.pools.len(),
            1,
            "expected docker tier pre-registered; got {} pools",
            snapshot.pools.len()
        );
        assert_eq!(snapshot.pools[0].name, "docker");
    }

    #[test]
    fn module_advertises_tick_interval_from_config() {
        let config = BrokerConfig {
            tick_interval: std::time::Duration::from_secs(7),
            act_above: 0.75,
        };
        let module = PressureBrokerModule::with_config(config);
        assert_eq!(
            module.config().tick_interval,
            Some(std::time::Duration::from_secs(7)),
            "tick_interval in ModuleConfig must mirror BrokerConfig so runtime cadence matches broker policy"
        );
    }

    #[test]
    fn module_routes_only_pressure_broker_state_command() {
        // PR-2 adds exactly ONE command prefix. Guard against a future
        // change accidentally adding more (or removing this one) without
        // updating handle_command's match arms — that combination would
        // route commands here that we'd then return "unknown" for.
        let module = PressureBrokerModule::new();
        let prefixes = module.config().command_prefixes;
        assert_eq!(prefixes.len(), 1);
        assert_eq!(prefixes[0], SYSTEM_PRESSURE_BROKER_STATE);
    }

    #[tokio::test]
    async fn tick_drives_relieve_and_fires_eviction_over_threshold() {
        // Build a module with a fresh broker, register a fake pool at
        // ~95% pressure, drive one tick, assert the broker actually
        // asked the pool to evict (i.e. tick → relieve → eviction path
        // is wired end-to-end, not just the call to relieve()).
        let module = PressureBrokerModule::with_config(BrokerConfig::default());
        let usage = Arc::new(AtomicU64::new(950));
        let evict_called_with = Arc::new(AtomicU64::new(0));
        let fake = Arc::new(FakePool {
            capacity: 1000,
            usage: usage.clone(),
            evict_called_with: evict_called_with.clone(),
        });
        module
            .broker()
            .register(fake.clone() as Arc<dyn ResourcePool>);

        // Sanity: pre-tick the broker should see global pressure ≥ 0.95
        // (max across docker tier + fake). Docker tier reports 0.0 on
        // CI (no Docker present + detected=false), so the fake drives
        // the max.
        let pre = module.broker().global_pressure();
        assert!(
            pre >= 0.90,
            "fake pool should drive global pressure ≥ 0.90; got {pre}"
        );

        module.tick().await.expect("tick should not error");

        let called = evict_called_with.load(Ordering::Relaxed);
        assert!(
            called > 0,
            "tick → relieve should have invoked evict_at_least on the over-threshold pool; got called_with={called}"
        );
    }

    #[tokio::test]
    async fn tick_is_a_noop_when_all_pools_below_threshold() {
        // Mirror of the previous test but with the fake pool at ~30%
        // — broker should observe and decide NOT to evict.
        let module = PressureBrokerModule::with_config(BrokerConfig::default());
        let evict_called_with = Arc::new(AtomicU64::new(0));
        let fake = Arc::new(FakePool {
            capacity: 1000,
            usage: Arc::new(AtomicU64::new(300)),
            evict_called_with: evict_called_with.clone(),
        });
        module
            .broker()
            .register(fake.clone() as Arc<dyn ResourcePool>);

        module.tick().await.expect("tick should not error");

        assert_eq!(
            evict_called_with.load(Ordering::Relaxed),
            0,
            "below-threshold tick must not invoke evict_at_least"
        );
    }

    #[tokio::test]
    async fn handle_command_returns_typed_snapshot_for_routed_command() {
        // The IPC handler must return a `BrokerSnapshot` JSON payload
        // with the expected camelCase keys ts-rs emitted — anything
        // else means the wire contract drifted and the TS mixin would
        // get stringly-typed garbage.
        let module = PressureBrokerModule::new();
        let result = module
            .handle_command(SYSTEM_PRESSURE_BROKER_STATE, Value::Null)
            .await;
        assert!(
            result.is_ok(),
            "broker-state should succeed; got: {:?}",
            result
        );
        let CommandResult::Json(json) = result.unwrap() else {
            panic!("expected Json result");
        };
        // Every BrokerSnapshot field, camelCase, must be present so
        // the TS side can structurally match without optional-chain
        // checks every key.
        assert!(json["globalPressure"].is_number(), "globalPressure missing");
        assert!(json["globalTier"].is_string(), "globalTier missing");
        assert!(json["pools"].is_array(), "pools missing");
        assert!(
            json["evictionsFired"].is_number(),
            "evictionsFired missing"
        );
        assert!(
            json["bytesFreedTotal"].is_number(),
            "bytesFreedTotal missing"
        );
        // globalTier is the PressureTier enum serialized lowercase —
        // pin the contract so a future serde rename doesn't silently
        // change the wire format.
        let tier = json["globalTier"].as_str().unwrap();
        assert!(
            matches!(tier, "normal" | "warning" | "high" | "critical"),
            "globalTier must be one of normal|warning|high|critical; got: {tier}"
        );
    }

    #[tokio::test]
    async fn handle_command_rejects_unknown_command() {
        let module = PressureBrokerModule::new();
        let result = module.handle_command("system/no-such-thing", Value::Null).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains(SYSTEM_PRESSURE_BROKER_STATE),
            "error should name the actually-handled command; got: {err}"
        );
    }
}
