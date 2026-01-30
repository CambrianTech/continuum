//! TTL-based memory cache — per-persona caching for hot data.
//!
//! Avoids redundant SQLite queries for frequently accessed data:
//! - Core memories (importance >= 0.8) — cached 30s
//! - Consciousness context — cached 30s
//! - Embedding vectors — cached until invalidated

use parking_lot::Mutex;
use std::collections::HashMap;
use std::time::{Duration, Instant};

// ─── MemoryCache ───────────────────────────────────────────────────────────────

/// Thread-safe TTL cache with automatic expiry.
/// Clone bound on T because values are returned by clone (cache retains ownership).
pub struct MemoryCache<T: Clone> {
    entries: Mutex<HashMap<String, CacheEntry<T>>>,
    ttl: Duration,
}

struct CacheEntry<T> {
    value: T,
    inserted_at: Instant,
}

impl<T: Clone> MemoryCache<T> {
    pub fn new(ttl: Duration) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            ttl,
        }
    }

    /// Get a cached value if it exists and hasn't expired.
    pub fn get(&self, key: &str) -> Option<T> {
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
    pub fn set(&self, key: String, value: T) {
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
    pub fn invalidate(&self, key: &str) {
        let mut entries = self.entries.lock();
        entries.remove(key);
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
