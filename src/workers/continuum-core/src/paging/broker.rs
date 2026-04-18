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
//! This commit lands the trait + broker scaffolding + tick loop. Pools
//! register themselves as `PressureSource` implementors; the broker
//! aggregates pressure on a periodic tick; when global pressure crosses
//! threshold, eviction fires on the highest-pressure pool first.
//!
//! What's NOT in this commit (intentionally — separate phases):
//!   - ML/LLM policy hook (the broker exposes the lever; the brain
//!     plugs in later via PressureSource priority overrides)
//!   - Recipe activation/deactivation hooks (Phase 9)
//!   - Cross-machine pressure (grid-level paging is its own layer)
//!
//! See: docs/architecture/RESOURCE-ARCHITECTURE.md (Phase 7)

use std::hash::Hash;
use std::sync::Arc;
use std::time::Duration;
use parking_lot::RwLock;
use crate::paging::pool::{PagedResourcePool, PoolStats};

/// Anything the broker can read pressure from + evict to relieve it.
///
/// Implemented by every paged resource pool in the system. The trait is
/// deliberately minimal — name for diagnostics, pressure for decisions,
/// `evict_some` for action. Eviction strategy lives inside the pool;
/// the broker just asks for some relief.
pub trait PressureSource: Send + Sync {
    /// Stable identifier used in logs and broker diagnostics.
    fn name(&self) -> &str;

    /// Current pressure 0.0..1.0 (or higher if over-budget). Snapshot
    /// only — no side effects. Cheap; called every tick from the broker.
    fn pressure(&self) -> f64;

    /// Drop unpinned entries until pressure returns to a healthy level.
    /// Returns the byte count freed (or 0 if nothing was evictable —
    /// fully pinned pool).
    fn evict_some(&self) -> u64;

    /// Snapshot stats for monitoring / IPC export. Same shape as
    /// `PagedResourcePool::stats()` so the broker can present a
    /// uniform view across pools of any value type.
    fn stats_snapshot(&self) -> PoolStats;
}

/// Blanket impl — every `PagedResourcePool<K, V>` automatically satisfies
/// `PressureSource`. Consumers wrap their pool in `Arc<...>` and pass it
/// straight to `broker.register()`; no per-pool adapter struct needed.
///
/// This is the architectural point of the trait: the broker speaks a tiny
/// interface, every pool plugs in for free, and future ML/LLM policy
/// hooks can specialize behavior per pool by overriding the `evict_some`
/// strategy via `PoolConfig::eviction_priority` instead of by writing a
/// custom `PressureSource`.
impl<K, V> PressureSource for PagedResourcePool<K, V>
where
    K: Hash + Eq + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    fn name(&self) -> &str {
        self.config_name()
    }
    fn pressure(&self) -> f64 {
        self.stats_blocking().pressure
    }
    fn evict_some(&self) -> u64 {
        self.evict_under_pressure()
    }
    fn stats_snapshot(&self) -> PoolStats {
        self.stats_blocking()
    }
}

/// Pressure tier — drives the broker's response.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
#[derive(Debug, Clone)]
pub struct PoolView {
    pub name: String,
    pub pressure: f64,
    pub tier: PressureTier,
    pub stats: PoolStats,
}

/// Full broker state snapshot — for the future PressureBroker IPC command
/// + monitoring widget.
#[derive(Debug, Clone)]
pub struct BrokerSnapshot {
    pub global_pressure: f64,
    pub global_tier: PressureTier,
    pub pools: Vec<PoolView>,
    pub evictions_fired: u64,
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

/// Cross-pool pressure orchestrator. Singleton in practice; one per
/// process is sufficient (cross-machine pressure lives at the grid
/// layer, not here).
pub struct PressureBroker {
    pools: RwLock<Vec<Arc<dyn PressureSource>>>,
    config: BrokerConfig,
    evictions_fired: parking_lot::Mutex<u64>,
    bytes_freed: parking_lot::Mutex<u64>,
}

impl PressureBroker {
    pub fn new(config: BrokerConfig) -> Self {
        Self {
            pools: RwLock::new(Vec::new()),
            config,
            evictions_fired: parking_lot::Mutex::new(0),
            bytes_freed: parking_lot::Mutex::new(0),
        }
    }

    /// Register a pool as a pressure source. The broker holds a weak-ish
    /// reference (Arc) so pools that outlive the broker stay valid; the
    /// broker iterates the registered set each tick.
    pub fn register(&self, pool: Arc<dyn PressureSource>) {
        let mut pools = self.pools.write();
        let name = pool.name().to_string();
        // Dedup by name — registering twice replaces (avoids duplicate eviction calls).
        pools.retain(|p| p.name() != name);
        pools.push(pool);
    }

    /// Drop a pool from the broker's awareness (e.g., on shutdown of
    /// a subsystem that owned the pool).
    pub fn unregister(&self, name: &str) {
        let mut pools = self.pools.write();
        pools.retain(|p| p.name() != name);
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
        let mut pressured: Vec<(f64, Arc<dyn PressureSource>)> = pools
            .iter()
            .map(|p| (p.pressure(), p.clone()))
            .filter(|(p, _)| *p >= self.config.act_above)
            .collect();
        pressured.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        let act_on: &[(f64, Arc<dyn PressureSource>)] = match tier {
            PressureTier::High => pressured.first().map(std::slice::from_ref).unwrap_or(&[]),
            PressureTier::Critical => &pressured[..],
            _ => &[],
        };
        let mut bytes_freed = 0u64;
        let mut pools_acted: Vec<String> = Vec::new();
        for (_, pool) in act_on {
            let freed = pool.evict_some();
            if freed > 0 {
                bytes_freed += freed;
                pools_acted.push(pool.name().to_string());
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
                    name: p.name().to_string(),
                    pressure,
                    tier: PressureTier::for_pressure(pressure),
                    stats: p.stats_snapshot(),
                }
            })
            .collect();
        views.sort_by(|a, b| b.pressure.partial_cmp(&a.pressure).unwrap_or(std::cmp::Ordering::Equal));
        let global_pressure = views.iter().map(|v| v.pressure).fold(0.0_f64, f64::max);
        BrokerSnapshot {
            global_pressure,
            global_tier: PressureTier::for_pressure(global_pressure),
            pools: views,
            evictions_fired: *self.evictions_fired.lock(),
            bytes_freed_total: *self.bytes_freed.lock(),
        }
    }

    /// Spawn a tokio task that calls `relieve()` on `tick_interval`.
    /// Returns the JoinHandle so the caller can abort on shutdown.
    /// Idempotent at the call site — caller decides if/when to spawn.
    pub fn spawn_tick(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        let interval = self.config.tick_interval;
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            // Skip the immediate first tick — let pools warm up before
            // we start measuring + acting.
            ticker.tick().await;
            loop {
                ticker.tick().await;
                let _report = self.relieve();
                // Future: emit IPC event or log when triggered=true.
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// Mock pool for broker testing — exposes a settable pressure value
    /// and counts evict_some invocations.
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

    impl PressureSource for MockPool {
        fn name(&self) -> &str {
            &self.name
        }
        fn pressure(&self) -> f64 {
            f64::from_bits(self.pressure_val.load(Ordering::Acquire))
        }
        fn evict_some(&self) -> u64 {
            self.evict_count.fetch_add(1, Ordering::AcqRel);
            // Simulate eviction reducing pressure.
            let cur = self.pressure();
            self.set_pressure((cur - 0.3).max(0.0));
            self.bytes_per_evict
        }
        fn stats_snapshot(&self) -> PoolStats {
            PoolStats {
                name: self.name.clone(),
                entry_count: 0,
                pinned_count: 0,
                total_bytes: 0,
                max_bytes: 0,
                pressure: self.pressure(),
                hit_count: 0,
                miss_count: 0,
                eviction_count: 0,
                inflight_count: 0,
            }
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
    async fn real_paged_resource_pool_plugs_into_broker_via_blanket_impl() {
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
        assert!(pool.pressure() >= 0.80, "expected pressure ≥0.80, got {}", pool.pressure());
        assert_eq!(pool.name(), "real-embeddings");

        // Register via blanket impl — no adapter struct needed.
        let broker = PressureBroker::new(BrokerConfig::default());
        broker.register(pool.clone());

        let report = broker.relieve();
        assert!(report.triggered, "broker should fire on real pool over budget");
        assert!(report.bytes_freed > 0, "blanket evict_some should free bytes");
        assert_eq!(report.pools_acted, vec!["real-embeddings".to_string()]);
        // Pressure should drop after eviction.
        assert!(pool.pressure() < 0.80, "post-eviction pressure should be <0.80, got {}", pool.pressure());
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
}
