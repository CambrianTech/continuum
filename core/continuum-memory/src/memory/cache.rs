//! TTL-based memory cache — per-persona caching for hot data.
//!
//! Avoids redundant SQLite queries for frequently accessed data:
//! - Core memories (importance >= 0.8) — cached 30s
//! - Consciousness context — cached 30s
//! - Embedding vectors — cached until invalidated

use parking_lot::Mutex;
use std::borrow::Borrow;
use std::collections::HashMap;
use std::hash::Hash;
use std::time::{Duration, Instant};

// ─── MemoryCache ───────────────────────────────────────────────────────────────

/// Thread-safe TTL cache with automatic expiry.
/// Clone bound on T because values are returned by clone (cache retains ownership).
///
/// The key is a TYPE, not a formatted string. It used to be `String`, and the
/// consciousness cache built its keys as `format!("{persona}:{room}")` while every
/// invalidation site passed the persona alone — so `entries.remove("<persona>")`
/// never matched `"<persona>:<room>"` and the cache was NEVER invalidated, only
/// TTL-expired. A persona could store a memory and keep reading a context that
/// omitted it for the whole 30 s window. Nothing failed; the calls compiled and
/// silently did nothing. With a key type, `invalidate(persona)` against a
/// `ConsciousnessKey` map does not compile, and removing every entry for one
/// persona has to say so ([`MemoryCache::invalidate_where`]).
pub struct MemoryCache<K: Eq + Hash + Clone, T: Clone> {
    entries: Mutex<HashMap<K, CacheEntry<T>>>,
    ttl: Duration,
}

struct CacheEntry<T> {
    value: T,
    inserted_at: Instant,
}

impl<K: Eq + Hash + Clone, T: Clone> MemoryCache<K, T> {
    pub fn new(ttl: Duration) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            ttl,
        }
    }

    /// Get a cached value if it exists and hasn't expired.
    pub fn get<Q>(&self, key: &Q) -> Option<T>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let entries = self.entries.lock();
        entries.get(key).and_then(|entry| {
            if entry.inserted_at.elapsed() < self.ttl {
                Some(entry.value.clone())
            } else {
                None
            }
        })
    }

    /// Store a value in the cache.
    pub fn set(&self, key: K, value: T) {
        let mut entries = self.entries.lock();
        entries.insert(
            key,
            CacheEntry {
                value,
                inserted_at: Instant::now(),
            },
        );
    }

    /// Remove a specific key.
    pub fn invalidate<Q>(&self, key: &Q)
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let mut entries = self.entries.lock();
        entries.remove(key);
    }

    /// Remove every entry whose key matches `pred`.
    ///
    /// The honest way to say "everything for this persona" when the key is a pair.
    /// The old code said it by passing HALF a formatted key to `invalidate` and
    /// removing nothing at all; a partial key is not a key, and this is the call
    /// that partial intent actually requires.
    pub fn invalidate_where(&self, pred: impl Fn(&K) -> bool) {
        let mut entries = self.entries.lock();
        entries.retain(|key, _| !pred(key));
    }

    /// Remove all entries.
    pub fn clear(&self) {
        let mut entries = self.entries.lock();
        entries.clear();
    }

    /// Evict expired entries (call periodically to free memory).
    pub fn evict_expired(&self) {
        let ttl = self.ttl;
        let mut entries = self.entries.lock();
        entries.retain(|_, entry| entry.inserted_at.elapsed() < ttl);
    }

    /// Number of entries (including expired ones not yet evicted).
    pub fn len(&self) -> usize {
        self.entries.lock().len()
    }

    /// Check if cache is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.lock().is_empty()
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_get_set() {
        let cache = MemoryCache::new(Duration::from_secs(60));
        cache.set("key1".to_string(), "value1".to_string());
        assert_eq!(cache.get("key1"), Some("value1".to_string()));
        assert_eq!(cache.get("key2"), None);
    }

    #[test]
    fn test_cache_expiry() {
        let cache = MemoryCache::new(Duration::from_millis(100));
        cache.set("key1".to_string(), "value1".to_string());
        assert_eq!(cache.get("key1"), Some("value1".to_string()));

        std::thread::sleep(Duration::from_millis(150));
        assert_eq!(cache.get("key1"), None);
    }

    #[test]
    fn test_cache_invalidate() {
        let cache = MemoryCache::new(Duration::from_secs(60));
        cache.set("key1".to_string(), 42);
        assert_eq!(cache.get("key1"), Some(42));

        cache.invalidate("key1");
        assert_eq!(cache.get("key1"), None);
    }

    // what this catches: the packed-string key regression. The consciousness cache
    // keyed entries as `format!("{persona}:{room}")` while all three invalidation
    // sites passed the persona ALONE, so exact-match `remove` never hit anything and
    // the cache was never invalidated — only TTL-expired. A persona could store a
    // memory and keep reading a context without it for the rest of the 30 s window.
    //
    // Both halves are asserted because the bug needs both to be caught: removing one
    // persona's entries must take EVERY room she has, and must leave other personas
    // alone. A `retain` that got the predicate backwards passes the first and fails
    // the second.
    #[test]
    fn invalidating_one_persona_takes_all_her_rooms_and_nobody_elses() {
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        struct Key {
            persona: &'static str,
            room: &'static str,
        }
        let cache = MemoryCache::new(Duration::from_secs(60));
        cache.set(
            Key {
                persona: "ada",
                room: "general",
            },
            1,
        );
        cache.set(
            Key {
                persona: "ada",
                room: "academy",
            },
            2,
        );
        cache.set(
            Key {
                persona: "grace",
                room: "general",
            },
            3,
        );

        cache.invalidate_where(|k| k.persona == "ada");

        assert_eq!(
            cache.get(&Key {
                persona: "ada",
                room: "general"
            }),
            None,
            "every room for the invalidated persona must go"
        );
        assert_eq!(
            cache.get(&Key {
                persona: "ada",
                room: "academy"
            }),
            None,
            "including the rooms the caller did not name — that is what the old \
             half-a-key `invalidate` silently failed to do"
        );
        assert_eq!(
            cache.get(&Key {
                persona: "grace",
                room: "general"
            }),
            Some(3),
            "another persona's context is not hers to evict"
        );
    }

    #[test]
    fn test_cache_evict_expired() {
        let cache = MemoryCache::new(Duration::from_millis(100));
        cache.set("key1".to_string(), 1);
        cache.set("key2".to_string(), 2);
        assert_eq!(cache.len(), 2);

        std::thread::sleep(Duration::from_millis(150));
        cache.evict_expired();
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_cache_clear() {
        let cache = MemoryCache::new(Duration::from_secs(60));
        cache.set("a".to_string(), 1);
        cache.set("b".to_string(), 2);
        cache.set("c".to_string(), 3);
        assert_eq!(cache.len(), 3);

        cache.clear();
        assert_eq!(cache.len(), 0);
    }
}
