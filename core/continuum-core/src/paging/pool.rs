//! PagedResourcePool — the unified paging primitive.
//!
//! Same shape used by every resource that needs paging:
//!   - LoRA adapter weights (genome registry adopts this)
//!   - KV cache pages (PagedAttention via vllm-metal handles natively;
//!     thin wrapper exposes its semantics through the same interface)
//!   - MoE expert weights (when MoE forge ships)
//!   - Model weights (multiple loaded models per host)
//!   - Embedding vectors (content-addressed dedup; fixes the 0/64 hit rate)
//!   - Memory recall results (TieredMemoryCache reformulation)
//!
//! Each consumer hand-rolled its own implementation today. The drift between
//! them IS the bug — different pressure interpretations, eviction policies,
//! single-flight handling. This primitive is the shared shape.
//!
//! Operations:
//!   - `get(k)` — L1 hit, returns owned value if cached, no load
//!   - `load_or_share(k, loader)` — single-flight load + cache; concurrent
//!     calls for the same key share ONE loader invocation
//!   - `pin(k)` — reference-counted hold; entry not evicted while pinned
//!   - `evict(k)` — forced drop regardless of pin count
//!   - `stats()` — pressure, hit rate, count for the PressureBroker
//!
//! Properties:
//!   - **Thread-safe** by default (parking_lot::RwLock + tokio Mutex for
//!     the inflight map). Built for the multi-persona concurrent case.
//!   - **Required config** — no Option<>, every choice declared explicitly
//!     per Joel's required-not-optional discipline.
//!   - **Reject-promise cleanup** — failed loads don't poison the cache;
//!     the inflight slot is removed on both Ok and Err.
//!   - **Pressure-driven eviction** — `maybe_evict` triggers when occupancy
//!     exceeds `max_bytes`, drops unpinned entries by eviction priority
//!     (LRU default) until back to 75% of capacity.
//!
//! See: docs/architecture/UNIFIED-PAGING.md

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::future::Future;
use std::hash::Hash;
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;
use ts_rs::TS;

/// Default refusal threshold for disk-backed tiers. 9500 basis points = 95%.
/// Callers that can project post-operation usage must refuse before crossing
/// this line instead of waiting for ENOSPC.
pub const DISK_CAPACITY_REFUSAL_BASIS_POINTS: u64 = 9_500;

/// Typed resource-pool failures exported through ts-rs so callers see a
/// stable discriminant instead of parsing strings.
#[derive(Debug, Clone, Serialize, Deserialize, TS, thiserror::Error)]
#[serde(rename_all = "camelCase", tag = "kind")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/paging/ResourceError.ts"
)]
pub enum ResourceError {
    #[error(
        "tier '{tier}' exhausted: requested {requested_bytes} bytes, \
         available {available_bytes} bytes, eviction freed {evicted_bytes} bytes"
    )]
    TierExhausted {
        tier: String,
        #[serde(rename = "requestedBytes")]
        requested_bytes: u64,
        #[serde(rename = "availableBytes")]
        available_bytes: u64,
        #[serde(rename = "evictedBytes")]
        evicted_bytes: u64,
    },
    #[error(
        "tier '{tier}' disk capacity refusal: used {used_bytes} bytes + projected \
         {projected_bytes} bytes exceeds {max_pressure_basis_points}bp of \
         {capacity_bytes} bytes"
    )]
    DiskCapacity {
        tier: String,
        #[serde(rename = "usedBytes")]
        used_bytes: u64,
        #[serde(rename = "capacityBytes")]
        capacity_bytes: u64,
        #[serde(rename = "projectedBytes")]
        projected_bytes: u64,
        #[serde(rename = "maxPressureBasisPoints")]
        max_pressure_basis_points: u64,
    },
    #[error("tier '{tier}' is unavailable: {reason}")]
    TierUnavailable { tier: String, reason: String },
}

/// Refuse a projected disk-tier allocation before it can push the tier past
/// the configured pressure threshold.
///
/// Uses integer basis points instead of floats so hot paths (model pull,
/// container start, image build) all enforce the same deterministic capacity
/// contract. The check is strict `>`: exactly 95% is allowed, 95% + 1 byte is
/// refused.
pub fn ensure_projected_disk_capacity(
    tier: impl Into<String>,
    used_bytes: u64,
    capacity_bytes: u64,
    projected_bytes: u64,
) -> Result<(), ResourceError> {
    ensure_projected_disk_capacity_bps(
        tier,
        used_bytes,
        capacity_bytes,
        projected_bytes,
        DISK_CAPACITY_REFUSAL_BASIS_POINTS,
    )
}

pub fn ensure_projected_disk_capacity_bps(
    tier: impl Into<String>,
    used_bytes: u64,
    capacity_bytes: u64,
    projected_bytes: u64,
    max_pressure_basis_points: u64,
) -> Result<(), ResourceError> {
    let tier = tier.into();
    if capacity_bytes == 0 {
        return Err(ResourceError::TierUnavailable {
            tier,
            reason: "disk tier capacity is unknown".to_string(),
        });
    }
    if max_pressure_basis_points == 0 || max_pressure_basis_points > 10_000 {
        return Err(ResourceError::TierUnavailable {
            tier,
            reason: format!(
                "invalid disk capacity threshold: {max_pressure_basis_points} basis points"
            ),
        });
    }

    let projected_used = used_bytes.saturating_add(projected_bytes);
    let max_allowed_bytes = capacity_bytes.saturating_mul(max_pressure_basis_points) / 10_000;
    if projected_used > max_allowed_bytes {
        return Err(ResourceError::DiskCapacity {
            tier,
            used_bytes,
            capacity_bytes,
            projected_bytes,
            max_pressure_basis_points,
        });
    }
    Ok(())
}

/// Cross-tier entry snapshot for diagnostics, status output, and future
/// scheduler decisions. Pool-specific values stay inside the pool; this is
/// the uniform RTOS-facing shape.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/paging/ResourcePoolEntry.ts"
)]
pub struct ResourcePoolEntry {
    pub key: String,
    pub size_bytes: u64,
    pub pinned_count: u32,
    pub loaded_at: u64,
    pub last_access_at: u64,
    pub access_count: u64,
}

/// Shared control surface every memory/storage tier should expose.
///
/// This intentionally sits above the concrete [`PagedResourcePool`]
/// implementation. VRAM, Docker, HF cache, KV cache, and future NVMe
/// pools can all report pressure and take eviction commands through the
/// same interface instead of reimplementing capacity math in each tier.
///
/// `PressureBroker` consumes `Arc<dyn ResourcePool>` directly for
/// cross-tier orchestration — the formerly-parallel `PressureSource`
/// trait was collapsed into this one (#1246) since both expressed
/// "tier with capacity + eviction + snapshot." `pressure()` and
/// `stats_snapshot()` carry default impls so existing tier implementors
/// (e.g. `DockerTierPool`) get broker integration for free; tiers that
/// already track richer telemetry (like `PagedResourcePool`) override
/// `stats_snapshot()` to expose their internal hit/miss/eviction counts.
pub trait ResourcePool: Send + Sync {
    fn tier_name(&self) -> &str;
    fn capacity_bytes(&self) -> u64;
    fn usage_bytes(&self) -> u64;
    fn evict_at_least(&self, want_bytes: u64) -> u64;
    fn snapshot(&self) -> Vec<ResourcePoolEntry>;

    /// Current pressure ratio in `0.0..1.0+` (over-budget ⇒ >1.0).
    /// Default = `usage_bytes / capacity_bytes`. Returns 0 when capacity
    /// is 0 (tier "not under management" — broker neither alerts nor
    /// acts on it). Override only if your tier has a non-byte-driven
    /// pressure metric (none currently do).
    fn pressure(&self) -> f64 {
        let cap = self.capacity_bytes();
        if cap == 0 {
            return 0.0;
        }
        self.usage_bytes() as f64 / cap as f64
    }

    /// `PoolStats` for monitoring / broker dashboards. Default derives
    /// name/capacity/usage/pressure from the trait core. Tier impls that
    /// track richer telemetry (`PagedResourcePool` knows hit/miss/
    /// eviction counts internally) override to expose those counts.
    fn stats_snapshot(&self) -> PoolStats {
        let cap = self.capacity_bytes();
        let used = self.usage_bytes();
        let snap = self.snapshot();
        let pressure = if cap == 0 {
            0.0
        } else {
            used as f64 / cap as f64
        };
        PoolStats {
            name: self.tier_name().to_string(),
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

/// Stats snapshot — for monitoring + PressureBroker decisions.
///
/// ts-rs export drives the wire shape for `system/pressure-broker-state`
/// (continuum#1299 PR-2). camelCase serde so TS consumers read the same
/// shape they read for every other system snapshot type — no manual
/// remap layer between Rust and TS for these counters.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/paging/PoolStats.ts")]
pub struct PoolStats {
    pub name: String,
    #[ts(type = "number")]
    pub entry_count: usize,
    #[ts(type = "number")]
    pub pinned_count: usize,
    #[ts(type = "number")]
    pub total_bytes: u64,
    #[ts(type = "number")]
    pub max_bytes: u64,
    /// 0.0..1.0 — ratio of used to capacity. >1.0 means over-budget.
    pub pressure: f64,
    #[ts(type = "number")]
    pub hit_count: u64,
    #[ts(type = "number")]
    pub miss_count: u64,
    #[ts(type = "number")]
    pub eviction_count: u64,
    #[ts(type = "number")]
    pub inflight_count: usize,
}

/// Internal entry — exposed via `EvictionPriority` callbacks.
///
/// Hot fields (last_access_at, access_count, pin_count) are atomics so
/// the pool's read-heavy `get()` path runs under RwLock::read with no
/// serialization point. Concurrent personas hit the cache in parallel.
pub struct PoolEntry<V> {
    pub value: V,
    pub size_bytes: u64,
    pub pin_count: AtomicU32,
    /// Unix ms.
    pub loaded_at: u64,
    /// Unix ms — atomic so concurrent `get()` callers can update without
    /// blocking each other on a write lock.
    pub last_access_at: AtomicU64,
    /// Atomic so concurrent `get()` callers can increment without blocking.
    pub access_count: AtomicU64,
}

/// Snapshot view for eviction-priority callbacks (atomics resolved to
/// owned values so the callback signature is simple).
#[derive(Debug, Clone, Copy)]
pub struct PoolEntryView {
    pub size_bytes: u64,
    pub pin_count: u32,
    pub loaded_at: u64,
    pub last_access_at: u64,
    pub access_count: u64,
}

/// Sizer — returns byte cost of a value. Use `|_| 1` for count-based pools.
pub type Sizer<V> = Arc<dyn Fn(&V) -> u64 + Send + Sync>;

/// Eviction priority — lower priority = evict first.
/// Takes the snapshot view (atomics resolved) AND a borrow of the value
/// so adapter-style consumers can inspect domain-specific metadata
/// (e.g. an adapter's `priority` field, an expert's MoE-routing weight,
/// a memory-recall entry's salience score) without a side-table lookup.
/// Use `i64::MAX` as the "never evict" sentinel.
pub type EvictionPriority<V> = Arc<dyn Fn(&PoolEntryView, &V) -> i64 + Send + Sync>;

/// LRU eviction priority — older `last_access_at` evicts first.
/// Value-blind; works for any V.
///
/// SIGN MATTERS: `evict_at_least` sorts ascending and evicts from the front,
/// so the OLDEST entry needs the LOWEST value — the raw timestamp, unnegated.
/// The first cut returned `-(last_access_at)`, which inverted the whole policy
/// into MRU: every pool using this helper evicted its most-recently-touched
/// entry first (caught 2026-08-26 by the KV slot pool's recycle test — the
/// warm slot that had JUST been refreshed was the one evicted).
pub fn lru_priority<V>() -> EvictionPriority<V> {
    Arc::new(|entry: &PoolEntryView, _value: &V| entry.last_access_at as i64)
}

/// Size-weighted LRU — among similarly-aged entries, larger evicts first.
/// Useful for embedding caches and model-weight pools where some entries
/// are dramatically larger than others (free more memory per eviction).
/// Value-blind; works for any V.
pub fn size_weighted_lru<V>() -> EvictionPriority<V> {
    Arc::new(|entry: &PoolEntryView, _value: &V| {
        // Same ascending-sort contract as [`lru_priority`]: older evicts first,
        // and among similar ages the LARGER entry evicts first (lower value).
        entry.last_access_at as i64 - (entry.size_bytes / 1024) as i64
    })
}

/// Pool configuration. All fields required (no Option<> per Joel's rule).
pub struct PoolConfig<V> {
    pub name: String,
    pub max_bytes: u64,
    pub sizer: Sizer<V>,
    pub eviction_priority: EvictionPriority<V>,
}

/// A pinned reference. While at least one PinHandle is alive for an entry,
/// it cannot be evicted. Drop the handle to release; ref count decrements
/// automatically.
pub struct PinHandle<K, V>
where
    K: Hash + Eq + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    key: K,
    pool: Arc<Inner<K, V>>,
    released: bool,
}

impl<K, V> PinHandle<K, V>
where
    K: Hash + Eq + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    /// Borrow the pinned value. Available for the lifetime of the handle.
    pub fn value(&self) -> Option<V> {
        let entries = self.pool.entries.read();
        entries.get(&self.key).map(|e| e.value.clone())
    }

    /// Explicitly release the pin. Idempotent. Drop also releases.
    pub fn release(mut self) {
        self.do_release();
    }

    fn do_release(&mut self) {
        if self.released {
            return;
        }
        self.released = true;
        // Atomic decrement under read lock — concurrent pin/release on
        // different keys don't serialize.
        let entries = self.pool.entries.read();
        if let Some(entry) = entries.get(&self.key) {
            // Saturating sub via compare-and-swap loop.
            let mut current = entry.pin_count.load(Ordering::Acquire);
            loop {
                if current == 0 {
                    break;
                }
                match entry.pin_count.compare_exchange_weak(
                    current,
                    current - 1,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                ) {
                    Ok(_) => break,
                    Err(actual) => current = actual,
                }
            }
        }
    }
}

impl<K, V> Drop for PinHandle<K, V>
where
    K: Hash + Eq + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    fn drop(&mut self) {
        self.do_release();
    }
}

/// Internal shared state — held behind Arc so PinHandle and the pool
/// share access without ownership friction.
struct Inner<K, V>
where
    K: Hash + Eq + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    config: PoolConfig<V>,
    entries: RwLock<HashMap<K, PoolEntry<V>>>,
    /// Single-flight in-flight loaders. tokio::sync::Mutex because we
    /// hold this across awaits.
    inflight: Mutex<
        HashMap<
            K,
            futures::future::Shared<Pin<Box<dyn Future<Output = Result<V, String>> + Send>>>,
        >,
    >,
    /// Atomic counters — concurrent get/load callers update without lock contention.
    hits: AtomicU64,
    misses: AtomicU64,
    evictions: AtomicU64,
}

/// The unified paging primitive — generic over key and value types.
pub struct PagedResourcePool<K, V>
where
    K: Hash + Eq + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    inner: Arc<Inner<K, V>>,
}

impl<K, V> PagedResourcePool<K, V>
where
    K: Hash + Eq + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    /// Create a new pool with the given configuration.
    pub fn new(config: PoolConfig<V>) -> Self {
        Self {
            inner: Arc::new(Inner {
                config,
                entries: RwLock::new(HashMap::new()),
                inflight: Mutex::new(HashMap::new()),
                hits: AtomicU64::new(0),
                misses: AtomicU64::new(0),
                evictions: AtomicU64::new(0),
            }),
        }
    }

    /// Stable name from PoolConfig — used by PressureBroker for diagnostics
    /// and by IPC stats exports.
    pub fn config_name(&self) -> &str {
        &self.inner.config.name
    }

    pub fn capacity_bytes(&self) -> u64 {
        self.inner.config.max_bytes
    }

    pub fn usage_bytes(&self) -> u64 {
        let entries = self.inner.entries.read();
        entries.values().map(|e| e.size_bytes).sum()
    }

    /// L1 hit — returns the value if cached, None on miss. Concurrent
    /// readers run in parallel under RwLock::read; per-entry atomics
    /// update last_access_at + access_count without serializing.
    pub fn get(&self, key: &K) -> Option<V> {
        let entries = self.inner.entries.read();
        if let Some(entry) = entries.get(key) {
            entry.last_access_at.store(now_ms(), Ordering::Release);
            entry.access_count.fetch_add(1, Ordering::AcqRel);
            self.inner.hits.fetch_add(1, Ordering::Relaxed);
            return Some(entry.value.clone());
        }
        self.inner.misses.fetch_add(1, Ordering::Relaxed);
        None
    }

    /// Load-or-share — if the key isn't present, invoke the loader.
    /// Concurrent calls for the same key share ONE loader invocation
    /// (single-flight). Failed loads don't poison the cache slot.
    pub async fn load_or_share<F, Fut>(&self, key: K, loader: F) -> Result<V, String>
    where
        F: FnOnce(K) -> Fut + Send,
        Fut: Future<Output = Result<V, String>> + Send + 'static,
    {
        // Fast path: cache hit.
        if let Some(value) = self.get(&key) {
            return Ok(value);
        }
        // Check inflight or start one.
        let shared = {
            let mut inflight = self.inner.inflight.lock().await;
            if let Some(existing) = inflight.get(&key) {
                existing.clone()
            } else {
                use futures::future::FutureExt;
                let fut = loader(key.clone()).boxed();
                let shared = fut.shared();
                inflight.insert(key.clone(), shared.clone());
                shared
            }
        };
        // Await the shared future (other callers also waiting see the same).
        let result = shared.await;
        // Cleanup inflight slot regardless of success/failure.
        {
            let mut inflight = self.inner.inflight.lock().await;
            inflight.remove(&key);
        }
        // On success, insert and maybe evict.
        match result {
            Ok(value) => {
                self.insert(key, value.clone());
                Ok(value)
            }
            Err(e) => Err(e),
        }
    }

    /// Manually insert a value. Useful for content-hash pools (e.g.,
    /// embeddings) where the key can't reconstruct the input. Caller has
    /// already produced the value; this records it for future hits.
    pub fn insert(&self, key: K, value: V) {
        let size_bytes = (self.inner.config.sizer)(&value);
        let now = now_ms();
        {
            let mut entries = self.inner.entries.write();
            entries.insert(
                key,
                PoolEntry {
                    value,
                    size_bytes,
                    pin_count: AtomicU32::new(0),
                    loaded_at: now,
                    last_access_at: AtomicU64::new(now),
                    access_count: AtomicU64::new(1),
                },
            );
        }
        self.maybe_evict();
    }

    /// Pin an entry to keep it resident. Returns None on miss.
    /// Pin/release are atomic — concurrent pins on different keys don't
    /// serialize on each other.
    pub fn pin(&self, key: &K) -> Option<PinHandle<K, V>> {
        let entries = self.inner.entries.read();
        let entry = entries.get(key)?;
        entry.pin_count.fetch_add(1, Ordering::AcqRel);
        entry.last_access_at.store(now_ms(), Ordering::Release);
        Some(PinHandle {
            key: key.clone(),
            pool: self.inner.clone(),
            released: false,
        })
    }

    /// Drain every entry and reset hit/miss/eviction counters.
    /// Returns the number of entries dropped. In-flight loads are NOT
    /// canceled — they complete normally and insert into the now-empty
    /// pool. Pinned entries ARE dropped (clear is admin-level reset,
    /// not pressure relief — use `evict_under_pressure` for that).
    pub fn clear(&self) -> usize {
        let mut entries = self.inner.entries.write();
        let dropped = entries.len();
        entries.clear();
        self.inner.hits.store(0, Ordering::Relaxed);
        self.inner.misses.store(0, Ordering::Relaxed);
        self.inner.evictions.store(0, Ordering::Relaxed);
        dropped
    }

    /// Force-evict by key, regardless of pin count. Use sparingly —
    /// the normal path is `maybe_evict` triggered by pressure.
    pub fn evict(&self, key: &K) -> bool {
        let mut entries = self.inner.entries.write();
        if entries.remove(key).is_some() {
            self.inner.evictions.fetch_add(1, Ordering::Relaxed);
            return true;
        }
        false
    }

    /// Trigger an eviction pass without inserting anything new. Used by
    /// the PressureBroker to free bytes when global pressure crosses a
    /// threshold — the pool itself may not be over its own budget yet,
    /// but the broker decides we should give some back. Returns the
    /// total bytes freed in this pass.
    ///
    /// Eviction policy is the same as the insert-triggered path: drop
    /// unpinned entries by `eviction_priority` order until occupancy
    /// is at 75% of `max_bytes`. Pinned entries untouched.
    pub fn evict_under_pressure(&self) -> u64 {
        let target_bytes = (self.inner.config.max_bytes as f64 * 0.75) as u64;
        let mut entries = self.inner.entries.write();
        let initial_bytes: u64 = entries.values().map(|e| e.size_bytes).sum();
        let mut total_bytes = initial_bytes;
        let mut candidates: Vec<(K, i64, u64)> = entries
            .iter()
            .filter(|(_, e)| e.pin_count.load(Ordering::Acquire) == 0)
            .map(|(k, e)| {
                let view = PoolEntryView {
                    size_bytes: e.size_bytes,
                    pin_count: e.pin_count.load(Ordering::Acquire),
                    loaded_at: e.loaded_at,
                    last_access_at: e.last_access_at.load(Ordering::Acquire),
                    access_count: e.access_count.load(Ordering::Acquire),
                };
                (
                    k.clone(),
                    (self.inner.config.eviction_priority)(&view, &e.value),
                    e.size_bytes,
                )
            })
            .collect();
        candidates.sort_by_key(|(_, prio, _)| *prio);
        let mut evicted_count: u64 = 0;
        for (k, _, size) in candidates {
            if total_bytes <= target_bytes {
                break;
            }
            entries.remove(&k);
            total_bytes -= size;
            evicted_count += 1;
        }
        if evicted_count > 0 {
            self.inner
                .evictions
                .fetch_add(evicted_count, Ordering::Relaxed);
        }
        initial_bytes.saturating_sub(total_bytes)
    }

    /// Evict unpinned entries until at least `want_bytes` has been freed
    /// or no evictable entries remain. Returns the actual freed bytes.
    ///
    /// Unlike `evict_under_pressure`, this is request-sized: schedulers and
    /// tier managers can ask for a specific amount of relief without each
    /// tier inventing its own eviction loop.
    pub fn evict_at_least(&self, want_bytes: u64) -> u64 {
        if want_bytes == 0 {
            return 0;
        }

        let mut entries = self.inner.entries.write();
        let mut candidates: Vec<(K, i64, u64)> = entries
            .iter()
            .filter(|(_, e)| e.pin_count.load(Ordering::Acquire) == 0)
            .map(|(k, e)| {
                let view = PoolEntryView {
                    size_bytes: e.size_bytes,
                    pin_count: e.pin_count.load(Ordering::Acquire),
                    loaded_at: e.loaded_at,
                    last_access_at: e.last_access_at.load(Ordering::Acquire),
                    access_count: e.access_count.load(Ordering::Acquire),
                };
                (
                    k.clone(),
                    (self.inner.config.eviction_priority)(&view, &e.value),
                    e.size_bytes,
                )
            })
            .collect();
        candidates.sort_by_key(|(_, prio, _)| *prio);

        let mut freed_bytes = 0u64;
        let mut evicted_count = 0u64;
        for (key, _, size_bytes) in candidates {
            if freed_bytes >= want_bytes {
                break;
            }
            if entries.remove(&key).is_some() {
                freed_bytes = freed_bytes.saturating_add(size_bytes);
                evicted_count += 1;
            }
        }
        if evicted_count > 0 {
            self.inner
                .evictions
                .fetch_add(evicted_count, Ordering::Relaxed);
        }
        freed_bytes
    }

    /// Synchronous version of `stats()` — needed by `PressureSource`
    /// implementors that can't .await (the broker's tick loop wants
    /// non-blocking pressure reads). Excludes inflight count (which
    /// requires async lock); leaves it as 0 since that's a transient
    /// signal anyway, not pressure-relevant.
    pub fn stats_blocking(&self) -> PoolStats {
        let entries = self.inner.entries.read();
        let mut total_bytes: u64 = 0;
        let mut pinned_count: usize = 0;
        for entry in entries.values() {
            total_bytes += entry.size_bytes;
            if entry.pin_count.load(Ordering::Acquire) > 0 {
                pinned_count += 1;
            }
        }
        let max_bytes = self.inner.config.max_bytes;
        let pressure = if max_bytes > 0 {
            total_bytes as f64 / max_bytes as f64
        } else {
            0.0
        };
        PoolStats {
            name: self.inner.config.name.clone(),
            entry_count: entries.len(),
            pinned_count,
            total_bytes,
            max_bytes,
            pressure,
            hit_count: self.inner.hits.load(Ordering::Relaxed),
            miss_count: self.inner.misses.load(Ordering::Relaxed),
            eviction_count: self.inner.evictions.load(Ordering::Relaxed),
            inflight_count: 0,
        }
    }

    /// Snapshot stats for monitoring + PressureBroker queries.
    pub async fn stats(&self) -> PoolStats {
        let entries = self.inner.entries.read();
        let mut total_bytes: u64 = 0;
        let mut pinned_count: usize = 0;
        for entry in entries.values() {
            total_bytes += entry.size_bytes;
            if entry.pin_count.load(Ordering::Acquire) > 0 {
                pinned_count += 1;
            }
        }
        let max_bytes = self.inner.config.max_bytes;
        let pressure = if max_bytes > 0 {
            total_bytes as f64 / max_bytes as f64
        } else {
            0.0
        };
        let inflight_count = self.inner.inflight.lock().await.len();
        PoolStats {
            name: self.inner.config.name.clone(),
            entry_count: entries.len(),
            pinned_count,
            total_bytes,
            max_bytes,
            pressure,
            hit_count: self.inner.hits.load(Ordering::Relaxed),
            miss_count: self.inner.misses.load(Ordering::Relaxed),
            eviction_count: self.inner.evictions.load(Ordering::Relaxed),
            inflight_count,
        }
    }

    /// Reduce occupancy to 75% of max_bytes by evicting unpinned entries
    /// in eviction-priority order (lowest priority first). Pinned entries
    /// are never touched here.
    fn maybe_evict(&self) {
        let target_bytes = (self.inner.config.max_bytes as f64 * 0.75) as u64;
        let mut entries = self.inner.entries.write();
        let mut total_bytes: u64 = entries.values().map(|e| e.size_bytes).sum();
        if total_bytes <= self.inner.config.max_bytes {
            return;
        }
        // Build candidate list: only unpinned, sorted by eviction priority asc.
        // Resolve atomics to PoolEntryView for the priority callback.
        let mut candidates: Vec<(K, i64, u64)> = entries
            .iter()
            .filter(|(_, e)| e.pin_count.load(Ordering::Acquire) == 0)
            .map(|(k, e)| {
                let view = PoolEntryView {
                    size_bytes: e.size_bytes,
                    pin_count: e.pin_count.load(Ordering::Acquire),
                    loaded_at: e.loaded_at,
                    last_access_at: e.last_access_at.load(Ordering::Acquire),
                    access_count: e.access_count.load(Ordering::Acquire),
                };
                (
                    k.clone(),
                    (self.inner.config.eviction_priority)(&view, &e.value),
                    e.size_bytes,
                )
            })
            .collect();
        candidates.sort_by_key(|(_, prio, _)| *prio);
        let mut evicted: u64 = 0;
        for (k, _, size) in candidates {
            if total_bytes <= target_bytes {
                break;
            }
            entries.remove(&k);
            total_bytes -= size;
            evicted += 1;
        }
        if evicted > 0 {
            self.inner.evictions.fetch_add(evicted, Ordering::Relaxed);
        }
    }
}

impl<K, V> PagedResourcePool<K, V>
where
    K: Hash + Eq + Clone + Send + Sync + ToString + 'static,
    V: Clone + Send + Sync + 'static,
{
    pub fn resource_snapshot(&self) -> Vec<ResourcePoolEntry> {
        let entries = self.inner.entries.read();
        entries
            .iter()
            .map(|(key, entry)| ResourcePoolEntry {
                key: key.to_string(),
                size_bytes: entry.size_bytes,
                pinned_count: entry.pin_count.load(Ordering::Acquire),
                loaded_at: entry.loaded_at,
                last_access_at: entry.last_access_at.load(Ordering::Acquire),
                access_count: entry.access_count.load(Ordering::Acquire),
            })
            .collect()
    }
}

impl<K, V> ResourcePool for PagedResourcePool<K, V>
where
    K: Hash + Eq + Clone + Send + Sync + ToString + 'static,
    V: Clone + Send + Sync + 'static,
{
    fn tier_name(&self) -> &str {
        self.config_name()
    }

    fn capacity_bytes(&self) -> u64 {
        self.capacity_bytes()
    }

    fn usage_bytes(&self) -> u64 {
        self.usage_bytes()
    }

    fn evict_at_least(&self, want_bytes: u64) -> u64 {
        self.evict_at_least(want_bytes)
    }

    fn snapshot(&self) -> Vec<ResourcePoolEntry> {
        self.resource_snapshot()
    }

    /// Override the trait default — `PagedResourcePool` tracks
    /// hit/miss/eviction/inflight counts internally via `stats_blocking()`,
    /// so we expose those directly instead of taking the trait's
    /// zero-defaults. Same `PoolStats` shape either way.
    fn stats_snapshot(&self) -> PoolStats {
        self.stats_blocking()
    }
}

/// Current Unix ms — monotonic enough for LRU ordering.
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count_sizer<V>() -> Sizer<V>
    where
        V: 'static,
    {
        Arc::new(|_| 1)
    }

    fn bytes_sizer() -> Sizer<Vec<u8>> {
        Arc::new(|v: &Vec<u8>| v.len() as u64)
    }

    #[tokio::test]
    async fn get_returns_none_on_miss_and_value_on_hit() {
        let pool: PagedResourcePool<String, Vec<u8>> = PagedResourcePool::new(PoolConfig {
            name: "test".to_string(),
            max_bytes: 1024,
            sizer: bytes_sizer(),
            eviction_priority: lru_priority(),
        });
        assert!(pool.get(&"missing".to_string()).is_none());
        pool.insert("k1".to_string(), vec![1, 2, 3]);
        assert_eq!(pool.get(&"k1".to_string()), Some(vec![1, 2, 3]));
    }

    #[tokio::test]
    async fn load_or_share_dedups_concurrent_loads() {
        use std::sync::atomic::{AtomicU32, Ordering};
        let pool: PagedResourcePool<String, u32> = PagedResourcePool::new(PoolConfig {
            name: "test".to_string(),
            max_bytes: 1024,
            sizer: count_sizer(),
            eviction_priority: lru_priority(),
        });
        let load_count = Arc::new(AtomicU32::new(0));
        let lc1 = load_count.clone();
        let lc2 = load_count.clone();
        let lc3 = load_count.clone();
        let key = "shared".to_string();
        let f1 = pool.load_or_share(key.clone(), move |_| {
            let lc = lc1.clone();
            async move {
                lc.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                Ok(42_u32)
            }
        });
        let f2 = pool.load_or_share(key.clone(), move |_| {
            let lc = lc2.clone();
            async move {
                lc.fetch_add(1, Ordering::SeqCst);
                Ok(99_u32)
            }
        });
        let f3 = pool.load_or_share(key.clone(), move |_| {
            let lc = lc3.clone();
            async move {
                lc.fetch_add(1, Ordering::SeqCst);
                Ok(7_u32)
            }
        });
        let (r1, r2, r3) = tokio::join!(f1, f2, f3);
        assert_eq!(r1.unwrap(), 42);
        assert_eq!(r2.unwrap(), 42);
        assert_eq!(r3.unwrap(), 42);
        // All three callers shared one load — counter should be 1, not 3.
        assert_eq!(load_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn pin_prevents_eviction_under_pressure() {
        let pool: PagedResourcePool<String, Vec<u8>> = PagedResourcePool::new(PoolConfig {
            name: "test".to_string(),
            max_bytes: 100,
            sizer: bytes_sizer(),
            eviction_priority: lru_priority(),
        });
        pool.insert("pinned".to_string(), vec![0; 50]);
        let _handle = pool.pin(&"pinned".to_string()).expect("pin should succeed");
        // Push way over budget with unpinned entries.
        for i in 0..5 {
            pool.insert(format!("transient_{i}"), vec![0; 80]);
        }
        // Pinned entry must survive.
        assert!(pool.get(&"pinned".to_string()).is_some());
    }

    #[tokio::test]
    async fn maybe_evict_keeps_total_within_max_bytes() {
        let pool: PagedResourcePool<String, Vec<u8>> = PagedResourcePool::new(PoolConfig {
            name: "test".to_string(),
            max_bytes: 100,
            sizer: bytes_sizer(),
            eviction_priority: lru_priority(),
        });
        // Insert beyond budget. Eviction fires when total > max_bytes,
        // drops back to 75% target. Between firings, total can sit
        // anywhere ≤ max_bytes — that's the contract.
        for i in 0..5 {
            pool.insert(format!("k_{i}"), vec![0; 30]);
        }
        let stats = pool.stats().await;
        assert!(
            stats.total_bytes <= 100,
            "expected total_bytes <= max_bytes (100) after eviction firings, got {}",
            stats.total_bytes
        );
        assert!(
            stats.eviction_count > 0,
            "eviction should have fired at least once"
        );
    }

    // what this catches: the LRU sign inversion. evict_at_least sorts priority
    // ascending and evicts from the front, so lru_priority must give the OLDEST
    // entry the LOWEST value. The first cut negated the timestamp — MRU wearing
    // LRU's name — and every pool using the helper evicted its most-recently-
    // touched entry first (caught live 2026-08-26: the KV slot pool evicted the
    // warm slot that had JUST been refreshed). regression for that inversion.
    #[tokio::test]
    async fn lru_priority_evicts_the_oldest_not_the_newest() {
        let pool: PagedResourcePool<String, u32> = PagedResourcePool::new(PoolConfig {
            name: "lru-order".into(),
            max_bytes: 100,
            sizer: Arc::new(|_| 1),
            eviction_priority: lru_priority(),
        });
        pool.load_or_share("old".to_string(), |_| async { Ok(1u32) })
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        pool.load_or_share("new".to_string(), |_| async { Ok(2u32) })
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        // Refresh "new" so its last_access is strictly freshest.
        let _ = pool.get(&"new".to_string());
        assert_eq!(pool.evict_at_least(1), 1);
        assert!(
            pool.get(&"old".to_string()).is_none(),
            "the OLDEST entry is the one evicted"
        );
        assert!(
            pool.get(&"new".to_string()).is_some(),
            "the freshest entry survives"
        );
    }

    #[tokio::test]
    async fn eviction_drops_to_target_when_far_over() {
        let pool: PagedResourcePool<String, Vec<u8>> = PagedResourcePool::new(PoolConfig {
            name: "test".to_string(),
            max_bytes: 100,
            sizer: bytes_sizer(),
            eviction_priority: lru_priority(),
        });
        // Single insert that's WAY over budget triggers eviction down
        // to 75% target in one pass.
        for i in 0..3 {
            pool.insert(format!("warm_{i}"), vec![0; 30]);
            tokio::time::sleep(tokio::time::Duration::from_millis(2)).await;
        }
        // Now insert a big one that pushes over.
        pool.insert("big".to_string(), vec![0; 50]);
        let stats = pool.stats().await;
        // After this single eviction pass, total ≤ 75 (target).
        assert!(
            stats.total_bytes <= 75,
            "single big insert should trigger eviction to 75% target; got {}",
            stats.total_bytes
        );
    }

    #[tokio::test]
    async fn dropped_pin_handle_releases_ref_count() {
        let pool: PagedResourcePool<String, Vec<u8>> = PagedResourcePool::new(PoolConfig {
            name: "test".to_string(),
            max_bytes: 100,
            sizer: bytes_sizer(),
            eviction_priority: lru_priority(),
        });
        pool.insert("x".to_string(), vec![0; 50]);
        {
            let _handle = pool.pin(&"x".to_string()).unwrap();
            let stats = pool.stats().await;
            assert_eq!(stats.pinned_count, 1);
        }
        // _handle dropped here — pin count should return to 0.
        let stats = pool.stats().await;
        assert_eq!(stats.pinned_count, 0);
    }

    #[tokio::test]
    async fn failed_load_does_not_poison_cache() {
        let pool: PagedResourcePool<String, u32> = PagedResourcePool::new(PoolConfig {
            name: "test".to_string(),
            max_bytes: 1024,
            sizer: count_sizer(),
            eviction_priority: lru_priority(),
        });
        // First call fails.
        let r1 = pool
            .load_or_share("k".to_string(), |_| async {
                Err::<u32, String>("boom".to_string())
            })
            .await;
        assert!(r1.is_err());
        // Second call should succeed (no poisoned slot from rejection).
        let r2 = pool
            .load_or_share("k".to_string(), |_| async { Ok(123_u32) })
            .await;
        assert_eq!(r2.unwrap(), 123);
    }

    #[tokio::test]
    async fn stats_pressure_tracks_occupancy() {
        let pool: PagedResourcePool<String, Vec<u8>> = PagedResourcePool::new(PoolConfig {
            name: "test".to_string(),
            max_bytes: 100,
            sizer: bytes_sizer(),
            eviction_priority: lru_priority(),
        });
        pool.insert("k".to_string(), vec![0; 25]);
        let stats = pool.stats().await;
        assert_eq!(stats.total_bytes, 25);
        assert!((stats.pressure - 0.25).abs() < 0.001);
    }

    #[tokio::test]
    async fn evict_at_least_frees_requested_amount_without_touching_pinned_entries() {
        let pool: PagedResourcePool<String, Vec<u8>> = PagedResourcePool::new(PoolConfig {
            name: "test".to_string(),
            max_bytes: 1_000,
            sizer: bytes_sizer(),
            eviction_priority: lru_priority(),
        });
        pool.insert("pinned".to_string(), vec![0; 100]);
        let _pin = pool.pin(&"pinned".to_string()).unwrap();
        pool.insert("a".to_string(), vec![0; 40]);
        pool.insert("b".to_string(), vec![0; 50]);
        pool.insert("c".to_string(), vec![0; 60]);

        let freed = pool.evict_at_least(75);

        assert!(
            freed >= 75,
            "expected to free at least 75 bytes, got {freed}"
        );
        assert!(pool.get(&"pinned".to_string()).is_some());
        assert_eq!(pool.stats().await.eviction_count, 2);
    }

    #[test]
    fn resource_pool_trait_exposes_uniform_control_surface() {
        let pool: PagedResourcePool<String, Vec<u8>> = PagedResourcePool::new(PoolConfig {
            name: "docker".to_string(),
            max_bytes: 500,
            sizer: bytes_sizer(),
            eviction_priority: lru_priority(),
        });
        pool.insert("image:a".to_string(), vec![0; 25]);

        let resource: &dyn ResourcePool = &pool;

        assert_eq!(resource.tier_name(), "docker");
        assert_eq!(resource.capacity_bytes(), 500);
        assert_eq!(resource.usage_bytes(), 25);
        let snapshot = resource.snapshot();
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].key, "image:a");
        assert_eq!(snapshot[0].size_bytes, 25);
    }

    #[test]
    fn projected_disk_capacity_allows_usage_at_threshold() {
        let result = ensure_projected_disk_capacity("docker", 900, 1_000, 50);
        assert!(
            result.is_ok(),
            "exactly 95% pressure should be allowed; got {result:?}"
        );
    }

    #[test]
    fn projected_disk_capacity_refuses_usage_over_threshold() {
        let result = ensure_projected_disk_capacity("docker", 900, 1_000, 51);
        let Err(ResourceError::DiskCapacity {
            tier,
            used_bytes,
            capacity_bytes,
            projected_bytes,
            max_pressure_basis_points,
        }) = result
        else {
            panic!("expected DiskCapacity refusal, got {result:?}");
        };

        assert_eq!(tier, "docker");
        assert_eq!(used_bytes, 900);
        assert_eq!(capacity_bytes, 1_000);
        assert_eq!(projected_bytes, 51);
        assert_eq!(
            max_pressure_basis_points,
            DISK_CAPACITY_REFUSAL_BASIS_POINTS
        );
    }

    #[test]
    fn projected_disk_capacity_refuses_saturating_overflow() {
        let result = ensure_projected_disk_capacity("docker", u64::MAX - 5, u64::MAX, 10);
        assert!(
            matches!(result, Err(ResourceError::DiskCapacity { .. })),
            "saturating projected usage over threshold must refuse, got {result:?}"
        );
    }

    #[test]
    fn projected_disk_capacity_rejects_unknown_capacity() {
        let result = ensure_projected_disk_capacity("docker", 0, 0, 1);
        let Err(ResourceError::TierUnavailable { tier, reason }) = result else {
            panic!("expected TierUnavailable for unknown capacity, got {result:?}");
        };

        assert_eq!(tier, "docker");
        assert!(
            reason.contains("capacity is unknown"),
            "reason should explain unknown capacity, got: {reason}"
        );
    }

    #[test]
    fn projected_disk_capacity_rejects_invalid_threshold() {
        let result = ensure_projected_disk_capacity_bps("docker", 0, 1_000, 1, 10_001);
        let Err(ResourceError::TierUnavailable { tier, reason }) = result else {
            panic!("expected TierUnavailable for invalid threshold, got {result:?}");
        };

        assert_eq!(tier, "docker");
        assert!(
            reason.contains("invalid disk capacity threshold"),
            "reason should explain invalid threshold, got: {reason}"
        );
    }

    #[test]
    fn resource_error_exports_ts_shape() {
        ResourceError::export_all(&ts_rs::Config::default()).unwrap();
        ResourcePoolEntry::export_all(&ts_rs::Config::default()).unwrap();
    }
}
