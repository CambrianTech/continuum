//! Bridge `ResourcePool` (the broad cross-tier control surface) into
//! `PressureSource` (the pressure-broker's narrow contract).
//!
//! ## Why this exists
//!
//! `ResourcePool` and `PressureSource` are parallel traits that cover
//! the same conceptual ground from two angles:
//!
//! | Trait              | Lives in     | Origin                          |
//! |--------------------|--------------|---------------------------------|
//! | `PressureSource`   | broker.rs    | Phase 7 broker (PagedResourcePool-shaped) |
//! | `ResourcePool`     | pool.rs      | Sibling's #1228 (Docker / VRAM / NVMe / Docker tiers) |
//!
//! `PagedResourcePool<K, V>` happens to implement both via two separate
//! manual impls. Tier pools that don't follow the per-key-page shape
//! (DockerTierPool, future HF cache tier, future system-RAM tier) only
//! implement `ResourcePool` — and so couldn't register with the broker
//! at all. That's the gap this adapter closes.
//!
//! Tracking the trait-unification cleanup as a follow-up issue per Joel
//! 2026-05-14: "code concurrency ONCE then incorporate it. Any hard
//! coded into a subclass... are probably WRONG." The adapter is the
//! safe NOW move; the follow-up issue tracks the right LATER move
//! (collapse the two traits into one).
//!
//! ## Derivation rules
//!
//! - **`pressure()`**: `usage_bytes / capacity_bytes`. When capacity is
//!   0 (probe returned `Unsupported` or `NotFound`), pressure is 0 —
//!   meaning "not under management" so the broker neither alerts nor
//!   acts on it. Distinct from "under management at 0% used" which
//!   would also be 0, but that case is benign anyway.
//! - **`evict_some()`**: forwards to `evict_at_least(want)`. The `want`
//!   amount is the over-budget byte count (max of: 10% of capacity,
//!   the actual overshoot). 10%-floor ensures a request even at exactly
//!   100% pressure does meaningful eviction work, not zero.
//! - **`stats_snapshot()`**: derived from the cross-tier shape. Fields
//!   `ResourcePool` doesn't expose (hit/miss/eviction counts, inflight)
//!   default to 0. The broker uses pressure + name + total_bytes for
//!   decisions; the absent fields are diagnostics-only.

use crate::paging::broker::PressureSource;
use crate::paging::pool::{PoolStats, ResourcePool};
use std::sync::Arc;

/// Adapter wrapping any `ResourcePool` so the broker can treat it as a
/// `PressureSource`. Used by tier pools (Docker, HF cache, NVMe future)
/// that don't follow the per-key-page `PagedResourcePool` shape.
///
/// Cheap to construct (just an Arc clone). Stateless aside from the
/// inner pool reference — all reads delegate.
pub struct ResourcePoolAdapter {
    inner: Arc<dyn ResourcePool>,
}

impl ResourcePoolAdapter {
    /// Wrap a `ResourcePool` for broker registration. Take Arc so the
    /// adapter can be cloned cheaply when the broker holds it under its
    /// internal `Arc<dyn PressureSource>` slot.
    pub fn new(inner: Arc<dyn ResourcePool>) -> Arc<Self> {
        Arc::new(Self { inner })
    }
}

impl PressureSource for ResourcePoolAdapter {
    fn name(&self) -> &str {
        self.inner.tier_name()
    }

    /// Pressure = usage / capacity. Returns 0.0 when capacity is 0
    /// (tier is "not under management" — probe returned Unsupported or
    /// NotFound). Returns >1.0 when over-budget so the broker's
    /// Critical-tier branch fires.
    fn pressure(&self) -> f64 {
        let cap = self.inner.capacity_bytes();
        if cap == 0 {
            return 0.0;
        }
        self.inner.usage_bytes() as f64 / cap as f64
    }

    /// Forward to `evict_at_least`. Asks for either 10% of capacity OR
    /// the actual overshoot, whichever is larger — so a 100%-pressure
    /// pool gets a non-zero eviction request, not zero.
    fn evict_some(&self) -> u64 {
        let cap = self.inner.capacity_bytes();
        let used = self.inner.usage_bytes();
        if cap == 0 {
            return 0;
        }
        let overshoot = used.saturating_sub(cap);
        let ten_percent = cap / 10;
        let want = overshoot.max(ten_percent);
        self.inner.evict_at_least(want)
    }

    /// Derived `PoolStats` — name + capacity + usage + pressure are
    /// real; hit/miss/eviction/inflight default to 0 because
    /// `ResourcePool` doesn't expose them. Broker only consumes
    /// pressure + name for decisions; the rest is diagnostics.
    fn stats_snapshot(&self) -> PoolStats {
        let cap = self.inner.capacity_bytes();
        let used = self.inner.usage_bytes();
        let snap = self.inner.snapshot();
        let pressure = if cap == 0 { 0.0 } else { used as f64 / cap as f64 };
        PoolStats {
            name: self.inner.tier_name().to_string(),
            entry_count: snap.len(),
            pinned_count: snap.iter().map(|e| e.pinned_count as usize).sum(),
            total_bytes: used,
            max_bytes: cap,
            pressure,
            hit_count: 0,
            miss_count: 0,
            eviction_count: 0,
            inflight_count: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paging::pool::ResourcePoolEntry;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// Mock ResourcePool with settable capacity / usage and a counter for
    /// `evict_at_least` to verify forwarding + want-bytes argument.
    struct MockResourcePool {
        name: &'static str,
        capacity: AtomicU64,
        usage: AtomicU64,
        last_evict_want: AtomicU64,
        evict_returns: AtomicU64,
    }

    impl MockResourcePool {
        fn new(name: &'static str, capacity: u64, usage: u64) -> Arc<Self> {
            Arc::new(Self {
                name,
                capacity: AtomicU64::new(capacity),
                usage: AtomicU64::new(usage),
                last_evict_want: AtomicU64::new(0),
                evict_returns: AtomicU64::new(0),
            })
        }
        fn set_evict_returns(&self, v: u64) {
            self.evict_returns.store(v, Ordering::Release);
        }
        fn last_evict_want(&self) -> u64 {
            self.last_evict_want.load(Ordering::Acquire)
        }
    }

    impl ResourcePool for MockResourcePool {
        fn tier_name(&self) -> &str {
            self.name
        }
        fn capacity_bytes(&self) -> u64 {
            self.capacity.load(Ordering::Acquire)
        }
        fn usage_bytes(&self) -> u64 {
            self.usage.load(Ordering::Acquire)
        }
        fn evict_at_least(&self, want_bytes: u64) -> u64 {
            self.last_evict_want.store(want_bytes, Ordering::Release);
            self.evict_returns.load(Ordering::Acquire)
        }
        fn snapshot(&self) -> Vec<ResourcePoolEntry> {
            vec![ResourcePoolEntry {
                key: format!("{}:0", self.name),
                size_bytes: self.usage.load(Ordering::Acquire),
                pinned_count: 0,
                loaded_at: 0,
                last_access_at: 0,
                access_count: 0,
            }]
        }
    }

    /// What this catches: pressure derivation. usage/capacity must round
    /// to the right ratio so the broker's tier thresholds (0.60/0.80/0.95)
    /// fire at the documented points.
    #[test]
    fn pressure_is_usage_over_capacity() {
        let pool = MockResourcePool::new("kv", 1000, 500);
        let adapter = ResourcePoolAdapter::new(pool);
        assert!((adapter.pressure() - 0.5).abs() < 1e-9);
    }

    /// What this catches: capacity==0 means "not under management" —
    /// pressure must be 0 so the broker does NOT alert on / evict from
    /// tiers it can't manage. Distinct from a managed-but-empty tier.
    #[test]
    fn pressure_is_zero_when_capacity_unknown() {
        let pool = MockResourcePool::new("docker", 0, 999_999_999);
        let adapter = ResourcePoolAdapter::new(pool);
        assert_eq!(adapter.pressure(), 0.0);
    }

    /// What this catches: at exact 100% pressure, evict_some must ask for
    /// at least 10% of capacity (not 0 from overshoot==0). Otherwise a
    /// pool that just hit 100% would be asked to free 0 bytes, defeating
    /// the broker's purpose.
    #[test]
    fn evict_some_floors_to_ten_percent_of_capacity_at_full_pressure() {
        let pool = MockResourcePool::new("kv", 1000, 1000); // exactly 100%
        let evict_pool = pool.clone();
        let adapter = ResourcePoolAdapter::new(pool);
        let _ = adapter.evict_some();
        assert_eq!(
            evict_pool.last_evict_want(),
            100,
            "10% of 1000 capacity = 100 bytes minimum eviction request"
        );
    }

    /// What this catches: when over-budget, evict_some asks for the
    /// overshoot amount (which exceeds 10% floor). Otherwise a tier 200%
    /// over budget would only ever be asked to free 10%, leaving it
    /// chronically over.
    #[test]
    fn evict_some_asks_for_overshoot_when_over_budget() {
        let pool = MockResourcePool::new("kv", 1000, 1500); // 150% pressure, 500 over
        let evict_pool = pool.clone();
        let adapter = ResourcePoolAdapter::new(pool);
        let _ = adapter.evict_some();
        assert_eq!(
            evict_pool.last_evict_want(),
            500,
            "want=overshoot when overshoot > 10%-of-capacity floor"
        );
    }

    /// What this catches: evict_some forwards the return value from
    /// evict_at_least without alteration. The broker uses this to
    /// decide whether the relief action did anything.
    #[test]
    fn evict_some_returns_what_inner_returned() {
        let pool = MockResourcePool::new("kv", 1000, 1500);
        pool.set_evict_returns(250);
        let adapter = ResourcePoolAdapter::new(pool);
        assert_eq!(adapter.evict_some(), 250);
    }

    /// What this catches: capacity==0 short-circuits evict_some to 0.
    /// We must not call evict_at_least with garbage 'want' on
    /// unprobeable tiers — that would force the impl to handle the
    /// unmanaged case defensively, defeating the safety the adapter
    /// provides.
    #[test]
    fn evict_some_is_zero_when_capacity_unknown() {
        let pool = MockResourcePool::new("docker-unsupported", 0, 0);
        let evict_pool = pool.clone();
        let adapter = ResourcePoolAdapter::new(pool);
        assert_eq!(adapter.evict_some(), 0);
        assert_eq!(
            evict_pool.last_evict_want(),
            0,
            "evict_at_least must NOT be called when capacity is unknown"
        );
    }

    /// What this catches: name forwards from tier_name. Broker logs +
    /// dispatch keys off this; rename via the adapter wrapper would
    /// silently break log filtering / per-tier dashboards.
    #[test]
    fn name_forwards_from_tier_name() {
        let pool = MockResourcePool::new("docker", 100, 50);
        let adapter = ResourcePoolAdapter::new(pool);
        assert_eq!(adapter.name(), "docker");
    }

    /// What this catches: stats_snapshot derives the expected shape.
    /// Broker uses `total_bytes` + `max_bytes` for the diagnostic UI;
    /// drift here would confuse the operator about "how much is this
    /// tier actually using." Drift in `pressure` would defeat the
    /// broker's tier classification.
    #[test]
    fn stats_snapshot_carries_real_capacity_usage_and_pressure() {
        let pool = MockResourcePool::new("kv", 1000, 800);
        let adapter = ResourcePoolAdapter::new(pool);
        let stats = adapter.stats_snapshot();
        assert_eq!(stats.name, "kv");
        assert_eq!(stats.total_bytes, 800);
        assert_eq!(stats.max_bytes, 1000);
        assert!((stats.pressure - 0.8).abs() < 1e-9);
        // Diagnostics-only fields default to zero.
        assert_eq!(stats.hit_count, 0);
        assert_eq!(stats.miss_count, 0);
        assert_eq!(stats.eviction_count, 0);
        assert_eq!(stats.inflight_count, 0);
    }

    /// What this catches: dyn-dispatching the adapter through
    /// `PressureSource` works. The broker stores sources as
    /// `Arc<dyn PressureSource>`; if this trait-object cast breaks (e.g.
    /// someone added a generic method to PressureSource), this fails to
    /// compile. Realistic call path.
    #[test]
    fn implements_pressure_source_via_dyn() {
        let pool = MockResourcePool::new("kv", 1000, 500);
        let adapter: Arc<dyn PressureSource> = ResourcePoolAdapter::new(pool);
        assert_eq!(adapter.name(), "kv");
        let _ = adapter.pressure();
        let _ = adapter.evict_some();
        let _ = adapter.stats_snapshot();
    }
}
