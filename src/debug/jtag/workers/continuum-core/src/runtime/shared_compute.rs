//! SharedCompute — lazy-compute-once, share-many cache.
//!
//! Like CBAR_VideoFrame's lazy getters: getRGBImage() computes once on first access,
//! subsequent accesses return the cached result. Thread-safe via OnceCell.
//!
//! Usage:
//! ```ignore
//! let embedding = compute.get_or_compute(
//!     "persona-123", "query_embedding",
//!     embed_model.embed(&text)
//! ).await;
//! // Second call returns cached result instantly
//! let same_embedding = compute.get_or_compute(
//!     "persona-123", "query_embedding",
//!     embed_model.embed(&text)  // Never called — cached
//! ).await;
//! ```

use dashmap::DashMap;
use std::any::Any;
use std::sync::Arc;
use tokio::sync::OnceCell;

/// A lazy-computed value. Computed once, shared via Arc.
/// Thread-safe: OnceCell guarantees only one computation runs.
struct LazyValue {
    cell: OnceCell<Arc<dyn Any + Send + Sync>>,
}

impl LazyValue {
    fn new() -> Self {
        Self {
            cell: OnceCell::new(),
        }
    }
}

/// Shared compute cache with two-level scoping: scope -> key -> value.
///
/// Scopes isolate caches per context (e.g., per persona, per session).
/// Keys identify specific computed values within a scope.
///
/// Like a texture pool: lightweight handles (scope+key) reference expensive
/// computed results that are shared via Arc (zero-copy).
pub struct SharedCompute {
    /// scope -> (key -> LazyValue)
    cache: DashMap<String, DashMap<String, LazyValue>>,
}

impl SharedCompute {
    pub fn new() -> Self {
        Self {
            cache: DashMap::new(),
        }
    }

    /// Get a cached value, or compute and cache it.
    ///
    /// The compute future runs at most once per (scope, key) pair.
    /// Concurrent callers with the same key will wait for the first computation
    /// to complete, then all receive the same Arc<T>.
    ///
    /// # Type Safety
    /// The caller must use the same type T for the same (scope, key) pair.
    /// Mismatched types will return None (not panic).
    pub async fn get_or_compute<T, F>(
        &self,
        scope: &str,
        key: &str,
        compute: F,
    ) -> Arc<T>
    where
        T: Any + Send + Sync + 'static,
        F: std::future::Future<Output = T>,
    {
        let scope_map = self.cache
            .entry(scope.to_string())
            .or_insert_with(DashMap::new);

        let lazy = scope_map
            .entry(key.to_string())
            .or_insert_with(LazyValue::new);

        let any_arc = lazy.cell.get_or_init(|| async {
            let val = compute.await;
            Arc::new(val) as Arc<dyn Any + Send + Sync>
        }).await;

        any_arc
            .clone()
            .downcast::<T>()
            .expect("SharedCompute type mismatch: same (scope, key) used with different types")
    }

    /// Get a cached value without computing.
    /// Returns None if not yet computed.
    pub fn get<T: Any + Send + Sync + 'static>(
        &self,
        scope: &str,
        key: &str,
    ) -> Option<Arc<T>> {
        self.cache.get(scope).and_then(|scope_map| {
            scope_map.get(key).and_then(|lazy| {
                lazy.cell.get().and_then(|any_arc| {
                    any_arc.clone().downcast::<T>().ok()
                })
            })
        })
    }

    /// Invalidate a specific cached value.
    pub fn invalidate(&self, scope: &str, key: &str) {
        if let Some(scope_map) = self.cache.get(scope) {
            scope_map.remove(key);
        }
    }

    /// Invalidate all cached values for a scope.
    /// Call when a persona disconnects, a session ends, etc.
    pub fn invalidate_scope(&self, scope: &str) {
        self.cache.remove(scope);
    }

    /// Clear the entire cache.
    pub fn clear(&self) {
        self.cache.clear();
    }

    /// Number of scopes in the cache.
    pub fn scope_count(&self) -> usize {
        self.cache.len()
    }

    /// Number of cached values in a scope.
    pub fn key_count(&self, scope: &str) -> usize {
        self.cache.get(scope).map(|m| m.len()).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn test_compute_once() {
        let compute = SharedCompute::new();
        let call_count = Arc::new(AtomicUsize::new(0));

        let count = call_count.clone();
        let result1: Arc<String> = compute.get_or_compute("scope1", "key1", async {
            count.fetch_add(1, Ordering::SeqCst);
            "hello".to_string()
        }).await;

        let count = call_count.clone();
        let result2: Arc<String> = compute.get_or_compute("scope1", "key1", async {
            count.fetch_add(1, Ordering::SeqCst);
            "should not run".to_string()
        }).await;

        assert_eq!(*result1, "hello");
        assert_eq!(*result2, "hello"); // Same cached value
        assert_eq!(call_count.load(Ordering::SeqCst), 1); // Computed only once
    }

    #[tokio::test]
    async fn test_different_keys() {
        let compute = SharedCompute::new();

        let r1: Arc<i32> = compute.get_or_compute("s", "a", async { 1 }).await;
        let r2: Arc<i32> = compute.get_or_compute("s", "b", async { 2 }).await;

        assert_eq!(*r1, 1);
        assert_eq!(*r2, 2);
    }

    #[tokio::test]
    async fn test_different_scopes() {
        let compute = SharedCompute::new();

        let r1: Arc<i32> = compute.get_or_compute("s1", "key", async { 10 }).await;
        let r2: Arc<i32> = compute.get_or_compute("s2", "key", async { 20 }).await;

        assert_eq!(*r1, 10);
        assert_eq!(*r2, 20);
    }

    #[tokio::test]
    async fn test_invalidate_key() {
        let compute = SharedCompute::new();

        let _: Arc<i32> = compute.get_or_compute("s", "k", async { 1 }).await;
        assert!(compute.get::<i32>("s", "k").is_some());

        compute.invalidate("s", "k");
        assert!(compute.get::<i32>("s", "k").is_none());

        // Can recompute after invalidation
        let r: Arc<i32> = compute.get_or_compute("s", "k", async { 2 }).await;
        assert_eq!(*r, 2);
    }

    #[tokio::test]
    async fn test_invalidate_scope() {
        let compute = SharedCompute::new();

        let _: Arc<i32> = compute.get_or_compute("s", "a", async { 1 }).await;
        let _: Arc<i32> = compute.get_or_compute("s", "b", async { 2 }).await;
        assert_eq!(compute.key_count("s"), 2);

        compute.invalidate_scope("s");
        assert_eq!(compute.key_count("s"), 0);
    }
}
