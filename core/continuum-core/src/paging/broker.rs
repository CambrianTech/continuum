//! PressureBroker — cross-pool eviction orchestration.
//!
//! Phase 7 of the resource architecture (RESOURCE-ARCHITECTURE.md).
//!
//! The PagedResourcePool primitive is the per-resource brain. The broker
//! is the cross-resource brain: one orchestrator that reads pressure
//! from every registered pool, decides which to relieve, and pulls the
//! eviction lever. Same broker is the future home of recipe-aware
//! priority arbitration, ML-policy-driven tiering decisions, and
//! eventually LLM-mediated control for novel pressure situations.
//!
//! ## Trait collapse (#1246)
//!
//! Pools register themselves as `ResourcePool` implementors directly —
//! the formerly-parallel `PressureSource` trait was collapsed into
//! `ResourcePool` since both expressed "tier with capacity + eviction +
//! snapshot." `ResourcePool::pressure()` and `stats_snapshot()` carry
//! default impls so `DockerTierPool` / `HFCacheTierPool` / future tiers
//! plug in for free. `PagedResourcePool` overrides `stats_snapshot()` to
//! expose its richer hit/miss/eviction telemetry.
//!
//! Eviction calls `evict_at_least(want)` where `want` = max(overshoot,
//! 10% of capacity). The 10% floor ensures a pool at exactly 100%
//! pressure (overshoot=0) still gets a non-zero eviction request.
//!
//! What's NOT in this commit (intentionally — separate phases):
//!   - ML/LLM policy hook (the broker exposes the lever; the brain
//!     plugs in later via per-tier eviction-priority overrides)
//!   - Recipe activation/deactivation hooks (Phase 9)
//!   - Cross-machine pressure (grid-level paging is its own layer)
//!
//! See: docs/architecture/RESOURCE-ARCHITECTURE.md (Phase 7)

use crate::paging::pool::{PoolStats, ResourcePool};
use crate::runtime;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use ts_rs::TS;

/// Target pressure the broker aims to drop to after an eviction pass.
/// Below the Warning threshold (0.60) so post-eviction the pool sits in
/// the Normal tier with margin. Picked to match the behavior of
/// `PagedResourcePool::evict_under_pressure` which evicted until
/// pressure dropped to "healthy" — the same intent generalized to
/// every `ResourcePool` impl, including tiers (Docker, HF cache) where
/// pressure-aware internal eviction logic doesn't exist.
const HEALTHY_TARGET_PRESSURE: f64 = 0.60;

/// Compute the "want_bytes" eviction request for a pool. Aims to bring
/// pressure to `HEALTHY_TARGET_PRESSURE` (= drop usage to 60% of cap).
/// Falls back to 10% of capacity as a floor so a pool at exactly 100%
/// pressure still gets a non-zero request. This is the canonical
/// broker→pool eviction-amount derivation, kept in one place so every
/// tier sees the same policy regardless of where the call originates.
fn evict_amount_for(pool: &dyn ResourcePool) -> u64 {
    let cap = pool.capacity_bytes();
    if cap == 0 {
        return 0;
    }
    let used = pool.usage_bytes();
    let target_used = (cap as f64 * HEALTHY_TARGET_PRESSURE) as u64;
    let to_drop = used.saturating_sub(target_used);
    let ten_percent_floor = cap / 10;
    to_drop.max(ten_percent_floor)
}

/// Pressure tier — drives the broker's response.
///
/// Serialized as lowercase (`"normal" | "warning" | "high" | "critical"`)
/// to match the existing `label()` impl + every other tier string the
/// system emits in logs and IPC. ts-rs export keeps the TS union honest
/// — operators can pattern-match without stringly-typed comparisons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/paging/PressureTier.ts"
)]
pub enum PressureTier {
    /// All pools comfortably under their budgets.
    Normal,
    /// Some pool approaching its limit. Broker relieves the worst pool.
    Warning,
    /// Multiple pools at or over budget. Broker fires parallel eviction.
    High,
    /// System in real trouble — aggressive eviction across all pools.
    Critical,
}

impl PressureTier {
    /// Map a pressure ratio (0.0..) to a tier. Same thresholds as
    /// GpuPressureWatcher uses — keeps the system's mental model
    /// consistent across resource types.
    pub fn for_pressure(p: f64) -> Self {
        if p >= 0.95 {
            PressureTier::Critical
        } else if p >= 0.80 {
            PressureTier::High
        } else if p >= 0.60 {
            PressureTier::Warning
        } else {
            PressureTier::Normal
        }
    }
}

/// Broker configuration. All fields required (no Option<>) per Joel's
/// required-not-optional discipline.
#[derive(Debug, Clone)]
pub struct BrokerConfig {
    /// Tick period — how often the broker checks pressure and acts.
    /// Default 5 seconds; faster ticks waste CPU on quiet pools, slower
    /// ticks let pressure spike before relief fires.
    pub tick_interval: Duration,
    /// Pressure threshold above which the broker fires eviction.
    /// Below this, the broker watches but doesn't act.
    pub act_above: f64,
}

impl Default for BrokerConfig {
    fn default() -> Self {
        Self {
            tick_interval: Duration::from_secs(5),
            act_above: 0.80, // High tier
        }
    }
}

/// Per-pool snapshot exposed to monitoring / IPC.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/paging/PoolView.ts")]
pub struct PoolView {
    pub name: String,
    pub pressure: f64,
    pub tier: PressureTier,
    pub stats: PoolStats,
}

/// Full broker state snapshot — wire type for `system/pressure-broker-state`
/// IPC (continuum#1299 PR-2). camelCase serde + ts-rs export gives TS
/// consumers a typed surface; counters cast to `number` so the JS side
/// doesn't have to deal with bigint for tracking values that fit fine.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/paging/BrokerSnapshot.ts"
)]
pub struct BrokerSnapshot {
    pub global_pressure: f64,
    pub global_tier: PressureTier,
    pub pools: Vec<PoolView>,
    #[ts(type = "number")]
    pub evictions_fired: u64,
    #[ts(type = "number")]
    pub bytes_freed_total: u64,
}

/// Result of a relief action.
#[derive(Debug, Clone)]
pub struct ReliefReport {
    pub triggered: bool,
    pub global_pressure_before: f64,
    pub bytes_freed: u64,
    pub pools_acted: Vec<String>,
}

/// Pressure alert — emitted by the broker when a tier crosses the
/// High/Critical threshold OR when relief eviction frees bytes.
///
/// This is the SURFACE Joel directive 2026-05-14 demanded ("memory in
/// this system, including the docker allotment needs to be managed by
/// the system, FULLY"). The broker now goes beyond observe + act — it
/// **tells** the operator (via WARN log) AND exposes a typed event
/// other Rust consumers can subscribe to (via `BrokerConfig::sinks`),
/// which is the IPC seam for surfacing alerts to TS / chat / UI.
///
/// `tier_name` keys back to whichever pool drove the alert (one alert
/// per pool that crossed threshold or had relief fire). Operators see
/// "docker tier at 92% — freed 8.2 GiB" instead of guessing.
///
/// Per airc-8a5e directive 2026-05-14: alert producer stays in Rust;
/// TS consumers render-only. ts-rs export keeps the wire type honest.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/paging/PressureAlert.ts"
)]
pub struct PressureAlert {
    pub tier_name: String,
    /// 0.0..1.0+ — same scale as `PressureSource::pressure()`.
    pub pressure: f64,
    pub tier: String,
    /// Bytes freed by relief eviction in this cycle. 0 when the alert
    /// is "threshold crossed but no eviction was possible / fired" so
    /// the operator knows the pool is hot and stuck.
    #[ts(type = "number")]
    pub bytes_freed: u64,
    /// True when relief eviction was attempted (regardless of bytes
    /// freed). False for pure threshold-crossed observations.
    pub action_taken: bool,
    /// Unix milliseconds — alert generation time.
    #[ts(type = "number")]
    pub at_ms: u64,
}

/// Sink for pressure alerts. Default broker has no sinks — alerts go
/// only to the WARN log. Add an Fn sink to forward alerts to IPC, chat
/// substrate, monitoring widgets, etc. Sinks are called synchronously
/// from `relieve()` so they MUST be cheap (queue-and-return is fine;
/// blocking I/O is not).
pub type AlertSink = Arc<dyn Fn(PressureAlert) + Send + Sync>;

impl PressureTier {
    /// Stable string label for IPC + log output. Lowercase to match the
    /// system's other camelCase / lowercase log convention.
    pub fn label(self) -> &'static str {
        match self {
            PressureTier::Normal => "normal",
            PressureTier::Warning => "warning",
            PressureTier::High => "high",
            PressureTier::Critical => "critical",
        }
    }
}

/// Cross-pool pressure orchestrator. Singleton in practice; one per
/// process is sufficient (cross-machine pressure lives at the grid
/// layer, not here).
pub struct PressureBroker {
    pools: RwLock<Vec<Arc<dyn ResourcePool>>>,
    config: BrokerConfig,
    evictions_fired: parking_lot::Mutex<u64>,
    bytes_freed: parking_lot::Mutex<u64>,
    /// Sinks for typed `PressureAlert`s. Default empty — alerts go only
    /// to the WARN log via `runtime::logger("pressure-broker")`. Add
    /// sinks at startup via `add_alert_sink()` to forward into IPC,
    /// chat substrate, monitoring widgets, etc. parking_lot::RwLock
    /// because tick paths read; sink registration is rare (one-shot at
    /// boot in practice).
    alert_sinks: RwLock<Vec<AlertSink>>,
}

impl PressureBroker {
    pub fn new(config: BrokerConfig) -> Self {
        Self {
            pools: RwLock::new(Vec::new()),
            config,
            evictions_fired: parking_lot::Mutex::new(0),
            bytes_freed: parking_lot::Mutex::new(0),
            alert_sinks: RwLock::new(Vec::new()),
        }
    }

    /// Register a sink that receives every emitted `PressureAlert`.
    /// Sinks are called synchronously from the broker tick — keep them
    /// cheap (queue + return is fine; blocking I/O is not). Idempotent
    /// at the call site; the broker does not dedup sinks.
    pub fn add_alert_sink(&self, sink: AlertSink) {
        self.alert_sinks.write().push(sink);
    }

    /// Emit a `PressureAlert` to the WARN log AND every registered sink.
    /// Same emission path used both for "threshold crossed but no
    /// eviction was possible" and "eviction freed N bytes" — operators
    /// see both signals on the same surface.
    fn emit_alert(&self, alert: PressureAlert) {
        let log = runtime::logger("pressure-broker");
        log.warn_fmt(format_args!(
            "PressureAlert tier={} pool={} pressure={:.2} bytes_freed={} action_taken={}",
            alert.tier, alert.tier_name, alert.pressure, alert.bytes_freed, alert.action_taken
        ));
        let sinks = self.alert_sinks.read();
        for sink in sinks.iter() {
            sink(alert.clone());
        }
    }

    /// Register a pool as a pressure source. The broker holds a weak-ish
    /// reference (Arc) so pools that outlive the broker stay valid; the
    /// broker iterates the registered set each tick.
    pub fn register(&self, pool: Arc<dyn ResourcePool>) {
        let mut pools = self.pools.write();
        let name = pool.tier_name().to_string();
        // Dedup by name — registering twice replaces (avoids duplicate eviction calls).
        pools.retain(|p| p.tier_name() != name);
        pools.push(pool);
    }

    /// Drop a pool from the broker's awareness (e.g., on shutdown of
    /// a subsystem that owned the pool).
    pub fn unregister(&self, name: &str) {
        let mut pools = self.pools.write();
        pools.retain(|p| p.tier_name() != name);
    }

    /// Read pressure across all pools — global = max(per-pool). Cheap;
    /// the broker calls this every tick.
    pub fn global_pressure(&self) -> f64 {
        let pools = self.pools.read();
        pools.iter().map(|p| p.pressure()).fold(0.0_f64, f64::max)
    }

    /// Run one relief pass. Returns a report describing what (if anything)
    /// was done. This is the broker's atomic action — eviction strategy
    /// per tier:
    ///
    ///   Normal/Warning  → no action (broker observes)
    ///   High            → evict from highest-pressure pool
    ///   Critical        → evict from all over-budget pools in parallel
    ///                     (sequential here since pool.evict_some returns
    ///                     immediately; parallel only matters if eviction
    ///                     is async, which it isn't in our design)
    pub fn relieve(&self) -> ReliefReport {
        let global = self.global_pressure();
        let tier = PressureTier::for_pressure(global);
        if (tier == PressureTier::Normal || tier == PressureTier::Warning)
            && global < self.config.act_above
        {
            return ReliefReport {
                triggered: false,
                global_pressure_before: global,
                bytes_freed: 0,
                pools_acted: Vec::new(),
            };
        }
        let pools = self.pools.read();
        // Build (pressure, ref) list, sorted descending by pressure.
        let mut pressured: Vec<(f64, Arc<dyn ResourcePool>)> = pools
            .iter()
            .map(|p| (p.pressure(), p.clone()))
            .filter(|(p, _)| *p >= self.config.act_above)
            .collect();
        pressured.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        let act_on: &[(f64, Arc<dyn ResourcePool>)] = match tier {
            PressureTier::High => pressured.first().map(std::slice::from_ref).unwrap_or(&[]),
            PressureTier::Critical => &pressured[..],
            _ => &[],
        };
        let mut bytes_freed = 0u64;
        let mut pools_acted: Vec<String> = Vec::new();
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        for (pre_pressure, pool) in act_on {
            let want = evict_amount_for(pool.as_ref());
            let freed = pool.evict_at_least(want);
            // Always emit ONE alert per pool the broker tried to relieve
            // — even if eviction freed 0 bytes. Zero-byte alert IS the
            // signal "this tier is hot AND stuck" (e.g. fully pinned
            // pool, docker daemon down). Operator needs to know.
            self.emit_alert(PressureAlert {
                tier_name: pool.tier_name().to_string(),
                pressure: *pre_pressure,
                tier: PressureTier::for_pressure(*pre_pressure)
                    .label()
                    .to_string(),
                bytes_freed: freed,
                action_taken: true,
                at_ms: now_ms,
            });
            if freed > 0 {
                bytes_freed += freed;
                pools_acted.push(pool.tier_name().to_string());
            }
        }
        if bytes_freed > 0 {
            *self.evictions_fired.lock() += 1;
            *self.bytes_freed.lock() += bytes_freed;
        }
        ReliefReport {
            triggered: !pools_acted.is_empty(),
            global_pressure_before: global,
            bytes_freed,
            pools_acted,
        }
    }

    /// Full state snapshot — for monitoring widgets, IPC stats commands,
    /// and the future ML-policy layer to consume as input.
    pub fn snapshot(&self) -> BrokerSnapshot {
        let pools = self.pools.read();
        let mut views: Vec<PoolView> = pools
            .iter()
            .map(|p| {
                let pressure = p.pressure();
                PoolView {
                    name: p.tier_name().to_string(),
                    pressure,
                    tier: PressureTier::for_pressure(pressure),
                    stats: p.stats_snapshot(),
                }
            })
            .collect();
        views.sort_by(|a, b| {
            b.pressure
                .partial_cmp(&a.pressure)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let global_pressure = views.iter().map(|v| v.pressure).fold(0.0_f64, f64::max);
        BrokerSnapshot {
            global_pressure,
            global_tier: PressureTier::for_pressure(global_pressure),
            pools: views,
            evictions_fired: *self.evictions_fired.lock(),
            bytes_freed_total: *self.bytes_freed.lock(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// Mock pool for broker testing — exposes a settable pressure value
    /// and counts evict_at_least invocations. Implements ResourcePool
    /// (the unified trait post-#1246); overrides pressure() because the
    /// mock's pressure is settable rather than usage/capacity-derived.
    struct MockPool {
        name: String,
        pressure_val: AtomicU64, // f64 bits
        evict_count: AtomicU64,
        bytes_per_evict: u64,
    }

    impl MockPool {
        fn new(name: &str, pressure: f64, bytes_per_evict: u64) -> Arc<Self> {
            Arc::new(Self {
                name: name.to_string(),
                pressure_val: AtomicU64::new(pressure.to_bits()),
                evict_count: AtomicU64::new(0),
                bytes_per_evict,
            })
        }
        fn set_pressure(&self, p: f64) {
            self.pressure_val.store(p.to_bits(), Ordering::Release);
        }
        fn evict_count(&self) -> u64 {
            self.evict_count.load(Ordering::Acquire)
        }
    }

    impl ResourcePool for MockPool {
        fn tier_name(&self) -> &str {
            &self.name
        }
        fn capacity_bytes(&self) -> u64 {
            // Synthetic capacity: enough that the broker's evict_amount_for
            // request is non-zero. Tests don't validate the request value
            // itself; they validate eviction count + bytes returned.
            1_000
        }
        fn usage_bytes(&self) -> u64 {
            // Synthetic usage tracking the settable pressure value so the
            // 10%-of-capacity floor in evict_amount_for produces a sane
            // request even when tests bypass the usage path.
            (self.pressure() * 1_000.0) as u64
        }
        fn evict_at_least(&self, _want_bytes: u64) -> u64 {
            self.evict_count.fetch_add(1, Ordering::AcqRel);
            // Simulate eviction reducing pressure.
            let cur = self.pressure();
            self.set_pressure((cur - 0.3).max(0.0));
            self.bytes_per_evict
        }
        fn snapshot(&self) -> Vec<crate::paging::pool::ResourcePoolEntry> {
            Vec::new()
        }
        // Override default `pressure()` because mock pressure is settable
        // (not usage/capacity-derived).
        fn pressure(&self) -> f64 {
            f64::from_bits(self.pressure_val.load(Ordering::Acquire))
        }
    }

    #[test]
    fn tier_thresholds_match_gpu_pressure_watcher() {
        assert_eq!(PressureTier::for_pressure(0.0), PressureTier::Normal);
        assert_eq!(PressureTier::for_pressure(0.59), PressureTier::Normal);
        assert_eq!(PressureTier::for_pressure(0.60), PressureTier::Warning);
        assert_eq!(PressureTier::for_pressure(0.79), PressureTier::Warning);
        assert_eq!(PressureTier::for_pressure(0.80), PressureTier::High);
        assert_eq!(PressureTier::for_pressure(0.94), PressureTier::High);
        assert_eq!(PressureTier::for_pressure(0.95), PressureTier::Critical);
        assert_eq!(PressureTier::for_pressure(1.50), PressureTier::Critical);
    }

    #[test]
    fn no_action_when_all_pools_normal() {
        let broker = PressureBroker::new(BrokerConfig::default());
        broker.register(MockPool::new("kv", 0.3, 1024));
        broker.register(MockPool::new("lora", 0.5, 1024));
        let report = broker.relieve();
        assert!(!report.triggered);
        assert_eq!(report.bytes_freed, 0);
    }

    #[test]
    fn high_pressure_evicts_only_worst_pool() {
        let broker = PressureBroker::new(BrokerConfig::default());
        let kv = MockPool::new("kv", 0.85, 100);
        let lora = MockPool::new("lora", 0.70, 100);
        broker.register(kv.clone());
        broker.register(lora.clone());
        let report = broker.relieve();
        assert!(report.triggered);
        // Only KV should have been evicted (highest pressure, above 0.80 threshold).
        // LoRA at 0.70 is below act_above, so even if "highest" it shouldn't fire.
        assert_eq!(kv.evict_count(), 1);
        assert_eq!(lora.evict_count(), 0);
        assert_eq!(report.pools_acted, vec!["kv".to_string()]);
        assert_eq!(report.bytes_freed, 100);
    }

    #[test]
    fn critical_pressure_evicts_all_over_budget_pools() {
        let broker = PressureBroker::new(BrokerConfig::default());
        let kv = MockPool::new("kv", 0.97, 100);
        let lora = MockPool::new("lora", 0.96, 100);
        let model = MockPool::new("model", 0.50, 100); // not over budget
        broker.register(kv.clone());
        broker.register(lora.clone());
        broker.register(model.clone());
        let report = broker.relieve();
        assert!(report.triggered);
        assert_eq!(kv.evict_count(), 1, "KV should be evicted (critical)");
        assert_eq!(lora.evict_count(), 1, "LoRA should be evicted (critical)");
        assert_eq!(model.evict_count(), 0, "Model under budget, not evicted");
        assert_eq!(report.bytes_freed, 200);
    }

    #[test]
    fn registration_dedups_by_name() {
        let broker = PressureBroker::new(BrokerConfig::default());
        broker.register(MockPool::new("kv", 0.85, 100));
        broker.register(MockPool::new("kv", 0.85, 100)); // same name — replaces
        let report = broker.relieve();
        // If dedup works, evict_some fires once. If not, twice (200 bytes).
        assert_eq!(report.bytes_freed, 100);
    }

    #[test]
    fn unregister_removes_pool() {
        let broker = PressureBroker::new(BrokerConfig::default());
        broker.register(MockPool::new("kv", 0.85, 100));
        broker.unregister("kv");
        let report = broker.relieve();
        assert!(!report.triggered);
        let snap = broker.snapshot();
        assert_eq!(snap.pools.len(), 0);
    }

    #[tokio::test]
    async fn real_paged_resource_pool_plugs_into_broker_via_resource_pool() {
        use crate::paging::pool::{lru_priority, PagedResourcePool, PoolConfig};

        // Build a real pool and fill it past the act_above threshold.
        let pool: Arc<PagedResourcePool<String, Vec<u8>>> =
            Arc::new(PagedResourcePool::new(PoolConfig {
                name: "real-embeddings".to_string(),
                max_bytes: 1000,
                sizer: Arc::new(|v: &Vec<u8>| v.len() as u64),
                eviction_priority: lru_priority(),
            }));

        // Insert 900 bytes (90% pressure → above 0.80 act threshold).
        for i in 0..9 {
            pool.load_or_share(format!("k{i}"), |_| async move { Ok(vec![0u8; 100]) })
                .await
                .unwrap();
        }
        assert!(
            pool.pressure() >= 0.80,
            "expected pressure ≥0.80, got {}",
            pool.pressure()
        );
        assert_eq!(pool.tier_name(), "real-embeddings");

        // Register directly — PagedResourcePool implements ResourcePool
        // (post-#1246 trait collapse — no separate PressureSource shim
        // needed).
        let broker = PressureBroker::new(BrokerConfig::default());
        broker.register(pool.clone());

        let report = broker.relieve();
        assert!(
            report.triggered,
            "broker should fire on real pool over budget"
        );
        assert!(report.bytes_freed > 0, "evict_at_least should free bytes");
        assert_eq!(report.pools_acted, vec!["real-embeddings".to_string()]);
        // Pressure should drop after eviction.
        assert!(
            pool.pressure() < 0.80,
            "post-eviction pressure should be <0.80, got {}",
            pool.pressure()
        );
    }

    #[test]
    fn snapshot_orders_pools_by_pressure_descending() {
        let broker = PressureBroker::new(BrokerConfig::default());
        broker.register(MockPool::new("low", 0.2, 1));
        broker.register(MockPool::new("high", 0.9, 1));
        broker.register(MockPool::new("mid", 0.5, 1));
        let snap = broker.snapshot();
        assert_eq!(snap.pools[0].name, "high");
        assert_eq!(snap.pools[1].name, "mid");
        assert_eq!(snap.pools[2].name, "low");
        assert!((snap.global_pressure - 0.9).abs() < 0.001);
        assert_eq!(snap.global_tier, PressureTier::High);
    }

    /// What this catches: PressureTier label() returns the canonical
    /// lowercase string used in IPC + log output. Drift here would break
    /// downstream consumers parsing the alert payload (TS render layer,
    /// Grafana dashboard regex, etc.).
    #[test]
    fn pressure_tier_label_canonical_strings() {
        assert_eq!(PressureTier::Normal.label(), "normal");
        assert_eq!(PressureTier::Warning.label(), "warning");
        assert_eq!(PressureTier::High.label(), "high");
        assert_eq!(PressureTier::Critical.label(), "critical");
    }

    /// What this catches: when relief acts on a pool, the broker emits
    /// exactly one alert per pool with non-zero `bytes_freed`. Drift
    /// here would mean operators stop hearing about tiers actually
    /// being relieved (the whole point of #1222 PR-4).
    #[test]
    fn relieve_emits_alert_per_acted_pool() {
        let broker = PressureBroker::new(BrokerConfig::default());
        let captured: Arc<parking_lot::Mutex<Vec<PressureAlert>>> =
            Arc::new(parking_lot::Mutex::new(Vec::new()));
        let captured_sink = captured.clone();
        broker.add_alert_sink(Arc::new(move |alert: PressureAlert| {
            captured_sink.lock().push(alert);
        }));
        broker.register(MockPool::new("kv", 0.85, 100));
        broker.register(MockPool::new("lora", 0.50, 100));
        let report = broker.relieve();
        assert!(report.triggered);
        let alerts = captured.lock();
        assert_eq!(
            alerts.len(),
            1,
            "exactly one alert for kv (only pool above act_above)"
        );
        let a = &alerts[0];
        assert_eq!(a.tier_name, "kv");
        assert_eq!(a.tier, "high");
        assert!((a.pressure - 0.85).abs() < 1e-9);
        assert_eq!(a.bytes_freed, 100);
        assert!(a.action_taken);
    }

    /// What this catches: in Critical tier, an alert is emitted for
    /// EVERY over-budget pool, not just the worst one. Operators need
    /// the full picture during system-wide pressure.
    #[test]
    fn critical_tier_emits_alert_per_overbudget_pool() {
        let broker = PressureBroker::new(BrokerConfig::default());
        let captured: Arc<parking_lot::Mutex<Vec<PressureAlert>>> =
            Arc::new(parking_lot::Mutex::new(Vec::new()));
        let captured_sink = captured.clone();
        broker.add_alert_sink(Arc::new(move |alert: PressureAlert| {
            captured_sink.lock().push(alert);
        }));
        broker.register(MockPool::new("kv", 0.97, 100));
        broker.register(MockPool::new("lora", 0.96, 100));
        broker.register(MockPool::new("model", 0.50, 100)); // not over budget
        let _ = broker.relieve();
        let alerts = captured.lock();
        assert_eq!(alerts.len(), 2, "alerts for kv + lora, not for model");
        let names: Vec<String> = alerts.iter().map(|a| a.tier_name.clone()).collect();
        assert!(names.contains(&"kv".to_string()));
        assert!(names.contains(&"lora".to_string()));
        assert!(!names.contains(&"model".to_string()));
        for a in alerts.iter() {
            assert_eq!(a.tier, "critical");
        }
    }

    /// What this catches: when no pool is over the act_above threshold,
    /// no alerts fire (the broker is silent below threshold). Spurious
    /// alerts would train operators to ignore them.
    #[test]
    fn relieve_below_threshold_emits_no_alerts() {
        let broker = PressureBroker::new(BrokerConfig::default());
        let captured: Arc<parking_lot::Mutex<Vec<PressureAlert>>> =
            Arc::new(parking_lot::Mutex::new(Vec::new()));
        let captured_sink = captured.clone();
        broker.add_alert_sink(Arc::new(move |alert: PressureAlert| {
            captured_sink.lock().push(alert);
        }));
        broker.register(MockPool::new("kv", 0.30, 100));
        broker.register(MockPool::new("lora", 0.50, 100));
        let report = broker.relieve();
        assert!(!report.triggered);
        assert_eq!(captured.lock().len(), 0);
    }

    /// What this catches: relief alert emits action_taken=true even when
    /// the pool's evict_some returns 0 bytes (e.g. fully-pinned pool,
    /// docker daemon unreachable). Zero-byte alert is the signal "we
    /// tried, can't act" — operator needs that distinct from no alert.
    #[test]
    fn alert_fires_with_zero_bytes_when_pool_cant_evict() {
        struct StuckPool;
        impl ResourcePool for StuckPool {
            fn tier_name(&self) -> &str {
                "stuck"
            }
            fn capacity_bytes(&self) -> u64 {
                100
            }
            fn usage_bytes(&self) -> u64 {
                99 // → pressure 0.99 via the trait default
            }
            fn evict_at_least(&self, _want_bytes: u64) -> u64 {
                0 // simulating fully-pinned / docker-down
            }
            fn snapshot(&self) -> Vec<crate::paging::pool::ResourcePoolEntry> {
                Vec::new()
            }
        }
        let broker = PressureBroker::new(BrokerConfig::default());
        let captured: Arc<parking_lot::Mutex<Vec<PressureAlert>>> =
            Arc::new(parking_lot::Mutex::new(Vec::new()));
        let captured_sink = captured.clone();
        broker.add_alert_sink(Arc::new(move |alert: PressureAlert| {
            captured_sink.lock().push(alert);
        }));
        broker.register(Arc::new(StuckPool));
        let report = broker.relieve();
        // bytes_freed=0 across the report (no pool freed anything).
        assert_eq!(report.bytes_freed, 0);
        assert!(!report.triggered, "no pool acted because none freed bytes");
        // BUT alert MUST fire — operator needs to know about stuck pool.
        let alerts = captured.lock();
        assert_eq!(alerts.len(), 1);
        let a = &alerts[0];
        assert_eq!(a.tier_name, "stuck");
        assert_eq!(a.tier, "critical");
        assert_eq!(a.bytes_freed, 0);
        assert!(
            a.action_taken,
            "broker tried, so action_taken=true even with zero freed"
        );
    }

    /// What this catches: PressureAlert serde round-trip preserves
    /// camelCase field names. The TS render layer reads `tierName`,
    /// `bytesFreed`, etc. — drift would silently break the IPC contract.
    #[test]
    fn pressure_alert_serde_preserves_camelcase_wire_format() {
        let alert = PressureAlert {
            tier_name: "docker".to_string(),
            pressure: 0.92,
            tier: "high".to_string(),
            bytes_freed: 8 * 1024 * 1024 * 1024,
            action_taken: true,
            at_ms: 1_700_000_000_000,
        };
        let json = serde_json::to_string(&alert).unwrap();
        assert!(json.contains("\"tierName\":\"docker\""), "got: {json}");
        assert!(json.contains("\"bytesFreed\":8589934592"), "got: {json}");
        assert!(json.contains("\"actionTaken\":true"), "got: {json}");
        assert!(json.contains("\"atMs\":1700000000000"), "got: {json}");
        let round: PressureAlert = serde_json::from_str(&json).unwrap();
        assert_eq!(round.tier_name, "docker");
        assert_eq!(round.bytes_freed, 8 * 1024 * 1024 * 1024);
    }
}
