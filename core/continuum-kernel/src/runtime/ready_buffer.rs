//! ReadyBuffer — the publish/peek surface that every brain region
//! uses to hand off pre-staged results to handlers without blocking.
//!
//! Doctrine (from docs/architecture/BRAIN-REGIONS-SUBSTRATE.md):
//!
//! > Empty buffer is a signal, not a block. If a handler reads and
//! > gets None, it proceeds with whatever degraded path the algorithm
//! > specifies. Slightly-stale context > stalled persona.
//!
//! ## Semantic rules
//!
//! - **Reads MUST NOT block** — handlers call `peek` on the hot path;
//!   it MUST complete in microseconds and MUST NOT `await`. The
//!   [`DashMapReadyBuffer`] default impl honors this via DashMap's
//!   sharded locks.
//! - **Staleness is acceptable** — a ready value might be 100ms old;
//!   that's better than blocking the handler 500ms to recompute.
//! - **Per-region buffers, not a global one** — hippocampus owns its
//!   engram-prefetch buffer; motor cortex owns its candidate-utterance
//!   buffer. They share the same trait shape but live in their own
//!   region structs.
//! - **TTL eviction** is region-owned — regions decide what "stale"
//!   means for their value type.
//!
//! ## L0-3a.0 scope (this slice)
//!
//! Trait definition + a single default `DashMap`-backed implementation.
//! No region-specific buffers yet (those land with their owning regions
//! in L0-3a.1+, L0-4a, L0-4b, etc.).

use dashmap::DashMap;
use std::hash::Hash;
use std::sync::Arc;
use std::time::{Duration, Instant};

// ─── The trait ──────────────────────────────────────────────────────

/// Pre-staged result publishing for brain regions. Regions write
/// (`publish`), handlers read (`peek`). The buffer holds the freshest
/// value per key; older values are dropped on overwrite.
pub trait ReadyBuffer: Send + Sync {
    /// The key type. Typically `(persona_id, channel_id)` or similar
    /// composite identifying what the staged value is for.
    type Key: Hash + Eq + Clone;

    /// The value type. Region-specific (engram set, candidate-utterance
    /// list, salience snapshot, ...).
    type Value: Clone;

    /// Synchronous read. Returns the freshest staged value for the
    /// key, or `None`.
    ///
    /// Handlers call this on the hot path — it MUST NOT block, MUST
    /// NOT await, and MUST complete in microseconds.
    fn peek(&self, key: &Self::Key) -> Option<Self::Value>;

    /// Region-side write. Atomically replaces the value for the key.
    /// Older value (if any) is dropped.
    fn publish(&self, key: Self::Key, value: Self::Value);

    /// TTL-style eviction sweep. Removes entries whose published-at
    /// timestamp is older than `max_age`. Called by the substrate
    /// under memory pressure or by the region itself on a sweep tick.
    ///
    /// Returns the number of entries evicted.
    fn evict_stale(&self, max_age: Duration) -> usize;

    /// Current entry count. Used for telemetry and pressure reporting.
    fn len(&self) -> usize;

    /// Convenience — most call sites care whether the buffer is empty
    /// before deciding to sweep / report pressure.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

// ─── Default implementation ─────────────────────────────────────────

/// Each entry stores its value plus the instant it was published, so
/// `evict_stale` can compute age without walking external state.
#[derive(Clone)]
struct TimestampedEntry<V> {
    value: V,
    published_at: Instant,
}

/// DashMap-backed [`ReadyBuffer`]. The default implementation for
/// regions that need a key→value mapping with sharded concurrent
/// access.
///
/// Reads are sharded by key hash, so peek is wait-free in the common
/// case. Writes acquire the per-shard lock briefly to replace the
/// entry — well within the "microseconds" budget the peek contract
/// asks for.
pub struct DashMapReadyBuffer<K, V>
where
    K: Hash + Eq + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    inner: Arc<DashMap<K, TimestampedEntry<V>>>,
}

impl<K, V> DashMapReadyBuffer<K, V>
where
    K: Hash + Eq + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    pub fn new() -> Self {
        Self {
            inner: Arc::new(DashMap::new()),
        }
    }

    /// Create with an initial shard capacity hint. Useful when the
    /// region knows the working set size up front (e.g., one entry per
    /// active persona).
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            inner: Arc::new(DashMap::with_capacity(capacity)),
        }
    }

    /// Snapshot every `(key, value)` currently published — for consumers that
    /// aggregate ACROSS keys (the vitals radiator summing a persona's staged
    /// unread depth over its `(persona, room)` digest entries). Clones under
    /// the per-shard read locks, one shard at a time; never holds a lock
    /// across the whole map, so publishers are not stalled.
    pub fn entries(&self) -> Vec<(K, V)> {
        self.inner
            .iter()
            .map(|e| (e.key().clone(), e.value().value.clone()))
            .collect()
    }
}

impl<K, V> Default for DashMapReadyBuffer<K, V>
where
    K: Hash + Eq + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<K, V> Clone for DashMapReadyBuffer<K, V>
where
    K: Hash + Eq + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    /// Cheap clone — shares the underlying DashMap via `Arc`. Multiple
    /// handles to the same buffer is the expected pattern (region
    /// publishes, handlers read).
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<K, V> ReadyBuffer for DashMapReadyBuffer<K, V>
where
    K: Hash + Eq + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    type Key = K;
    type Value = V;

    fn peek(&self, key: &Self::Key) -> Option<Self::Value> {
        self.inner.get(key).map(|entry| entry.value.clone())
    }

    fn publish(&self, key: Self::Key, value: Self::Value) {
        self.inner.insert(
            key,
            TimestampedEntry {
                value,
                published_at: Instant::now(),
            },
        );
    }

    fn evict_stale(&self, max_age: Duration) -> usize {
        let now = Instant::now();
        let stale_keys: Vec<K> = self
            .inner
            .iter()
            .filter(|entry| now.duration_since(entry.value().published_at) > max_age)
            .map(|entry| entry.key().clone())
            .collect();
        let evicted = stale_keys.len();
        for key in stale_keys {
            self.inner.remove(&key);
        }
        evicted
    }

    fn len(&self) -> usize {
        self.inner.len()
    }
}

// ─── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_publish_then_peek_returns_value() {
        let buf: DashMapReadyBuffer<u64, String> = DashMapReadyBuffer::new();
        buf.publish(1, "engram-set-1".to_string());
        assert_eq!(buf.peek(&1), Some("engram-set-1".to_string()));
    }

    #[test]
    fn test_peek_missing_key_returns_none() {
        let buf: DashMapReadyBuffer<u64, String> = DashMapReadyBuffer::new();
        assert_eq!(buf.peek(&42), None);
    }

    #[test]
    fn test_publish_overwrites_previous_value() {
        let buf: DashMapReadyBuffer<u64, String> = DashMapReadyBuffer::new();
        buf.publish(1, "old".to_string());
        buf.publish(1, "new".to_string());
        assert_eq!(buf.peek(&1), Some("new".to_string()));
    }

    #[test]
    fn test_evict_stale_removes_old_entries_keeps_fresh() {
        let buf: DashMapReadyBuffer<u64, String> = DashMapReadyBuffer::new();
        buf.publish(1, "old".to_string());
        std::thread::sleep(Duration::from_millis(20));
        buf.publish(2, "fresh".to_string());

        // Anything older than 10ms is evicted — key 1 goes, key 2 stays.
        let evicted = buf.evict_stale(Duration::from_millis(10));
        assert_eq!(evicted, 1);
        assert_eq!(buf.peek(&1), None);
        assert_eq!(buf.peek(&2), Some("fresh".to_string()));
    }

    #[test]
    fn test_evict_stale_zero_max_age_clears_everything() {
        let buf: DashMapReadyBuffer<u64, String> = DashMapReadyBuffer::new();
        buf.publish(1, "a".to_string());
        buf.publish(2, "b".to_string());
        let evicted = buf.evict_stale(Duration::ZERO);
        assert_eq!(evicted, 2);
        assert!(buf.is_empty());
    }

    #[test]
    fn test_len_and_is_empty_reflect_state() {
        let buf: DashMapReadyBuffer<u64, String> = DashMapReadyBuffer::new();
        assert!(buf.is_empty());
        assert_eq!(buf.len(), 0);
        buf.publish(1, "x".to_string());
        assert!(!buf.is_empty());
        assert_eq!(buf.len(), 1);
    }

    #[test]
    fn test_clone_shares_underlying_storage() {
        let buf_a: DashMapReadyBuffer<u64, String> = DashMapReadyBuffer::new();
        let buf_b = buf_a.clone();
        buf_a.publish(1, "from-a".to_string());
        // Both handles see the same value — Arc-shared inner DashMap.
        assert_eq!(buf_b.peek(&1), Some("from-a".to_string()));
    }

    #[test]
    fn test_trait_object_usage() {
        // Trait is dyn-compatible for handlers that don't care about
        // the concrete type.
        let buf: Box<dyn ReadyBuffer<Key = u64, Value = String>> =
            Box::new(DashMapReadyBuffer::<u64, String>::new());
        buf.publish(1, "via-trait".to_string());
        assert_eq!(buf.peek(&1), Some("via-trait".to_string()));
    }

    #[test]
    fn test_with_capacity_constructor() {
        let buf: DashMapReadyBuffer<u64, u64> = DashMapReadyBuffer::with_capacity(64);
        buf.publish(1, 100);
        assert_eq!(buf.peek(&1), Some(100));
    }
}
