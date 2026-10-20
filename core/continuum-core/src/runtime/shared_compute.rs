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
use std::sync::{Arc, OnceLock};
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

/// A failed initializer is retryable; a type mismatch is a caller contract violation.
#[derive(Debug, thiserror::Error)]
pub enum ComputeError<E> {
    #[error("shared computation failed: {0}")]
    Initialize(E),
    #[error("shared computation type mismatch for {scope}/{key}")]
    TypeMismatch { scope: String, key: String },
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
    cache: DashMap<String, DashMap<String, Arc<LazyValue>>>,
}

impl Default for SharedCompute {
    fn default() -> Self {
        Self::new()
    }
}

impl SharedCompute {
    pub fn new() -> Self {
        Self {
            cache: DashMap::new(),
        }
    }

    /// Get a cached value, or compute and cache it.
    ///
    /// Each cell retains one successful computation. A cancelled initializer
    /// leaves the cell available for another caller to initialize.
    /// Concurrent callers with the same key will wait for the first computation
    /// to complete, then all receive the same Arc<T>.
    ///
    /// # Type Safety
    /// The caller must use the same type T for the same (scope, key) pair.
    /// Mismatched types panic.
    pub async fn get_or_compute<T, F>(&self, scope: &str, key: &str, compute: F) -> Arc<T>
    where
        T: Any + Send + Sync + 'static,
        F: std::future::Future<Output = T>,
    {
        let lazy = self.cell(scope, key);

        let any_arc = lazy
            .cell
            .get_or_init(|| async {
                let val = compute.await;
                Arc::new(val) as Arc<dyn Any + Send + Sync>
            })
            .await;

        any_arc
            .clone()
            .downcast::<T>()
            .expect("SharedCompute type mismatch: same (scope, key) used with different types")
    }

    fn cell(&self, scope: &str, key: &str) -> Arc<LazyValue> {
        let scope_map = self.cache.entry(scope.to_string()).or_default();
        let entry = scope_map
            .entry(key.to_string())
            .or_insert_with(|| Arc::new(LazyValue::new()));
        Arc::clone(entry.value())
        // Both map guards end here; initialization may depend on another key.
    }

    /// Share a successful value without retaining initialization failures.
    /// Cancellation or an error leaves the cell available for another caller.
    /// Invalidation detaches this cell just as for `get_or_compute`.
    pub async fn get_or_try_compute<T, E, F>(
        &self,
        scope: &str,
        key: &str,
        compute: F,
    ) -> Result<Arc<T>, ComputeError<E>>
    where
        T: Any + Send + Sync + 'static,
        F: std::future::Future<Output = Result<T, E>>,
    {
        let lazy = self.cell(scope, key);
        let value = lazy
            .cell
            .get_or_try_init(|| async {
                compute
                    .await
                    .map(|value| Arc::new(value) as Arc<dyn Any + Send + Sync>)
            })
            .await
            .map_err(ComputeError::Initialize)?;
        Arc::clone(value)
            .downcast::<T>()
            .map_err(|_| ComputeError::TypeMismatch {
                scope: scope.to_owned(),
                key: key.to_owned(),
            })
    }

    /// Get a cached value without computing.
    /// Returns None if not yet computed.
    pub fn get<T: Any + Send + Sync + 'static>(&self, scope: &str, key: &str) -> Option<Arc<T>> {
        self.cache.get(scope).and_then(|scope_map| {
            scope_map.get(key).and_then(|lazy| {
                lazy.cell
                    .get()
                    .and_then(|any_arc| any_arc.clone().downcast::<T>().ok())
            })
        })
    }

    /// Invalidate a specific cached value.
    /// Existing readers may finish with the detached cell; subsequent lookups
    /// use a new cell. An old completion cannot repopulate the cache.
    pub fn invalidate(&self, scope: &str, key: &str) -> bool {
        self.cache
            .get(scope)
            .is_some_and(|scope_map| scope_map.remove(key).is_some())
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

/// The ONE process-global compute-once/share-many cache.
///
/// Media derivatives (a frame's scaled thumbnails, its text description, its
/// vision-encode KV) are keyed by content hash, so two personas looking at the
/// same frame must share ALL of its derivatives — the compute-once/share-many
/// property that IS the multi-persona vision moat. A *parallel* cache would
/// silently double every describe/scale, so the substrate keeps exactly one
/// instance ([[media-is-compute-once-zero-copy-hardware-grade]],
/// [[vision-replication-is-the-multipersona-moat-vs-cloud]]).
///
/// [`Runtime::new`](crate::runtime::Runtime::new) ADOPTS this instance for its own
/// `compute` handle, so a module reaching it via `ModuleContext.compute` and a
/// persona reaching it via this accessor see the SAME cache — the LiveKit media
/// ingest that warms a frame's cells and the `MediaPerceptionSource` that reads
/// them cannot diverge. Lazily created; the cache is content-addressed and pure,
/// so sharing across every consumer (including the substrate `Runtime`) is
/// correct by construction.
pub fn global() -> Arc<SharedCompute> {
    static GLOBAL: OnceLock<Arc<SharedCompute>> = OnceLock::new();
    GLOBAL
        .get_or_init(|| Arc::new(SharedCompute::new()))
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // What this catches: a transient source failure must not poison a shared artifact forever.
    #[tokio::test]
    async fn failed_initialization_retries_and_type_errors_reach_the_caller() {
        let compute = SharedCompute::new();
        let failed = compute
            .get_or_try_compute::<u32, _, _>("s", "k", async { Err("offline") })
            .await;
        assert!(matches!(failed, Err(ComputeError::Initialize("offline"))));
        assert!(compute.get::<u32>("s", "k").is_none());
        let value = compute
            .get_or_try_compute("s", "k", async { Ok::<_, &str>(7u32) })
            .await;
        assert!(matches!(value.as_deref(), Ok(7)));
        let cached = compute
            .get_or_try_compute::<u32, _, _>("s", "k", async { Err("must not run") })
            .await;
        assert!(matches!((&value, &cached), (Ok(a), Ok(b)) if Arc::ptr_eq(a, b)));
        let wrong = compute
            .get_or_try_compute("s", "k", async { Ok::<_, &str>("different type") })
            .await;
        assert!(matches!(wrong, Err(ComputeError::TypeMismatch { .. })));
        compute.invalidate("s", "k");
        let fresh = compute
            .get_or_try_compute("s", "k", async { Ok::<_, &str>(9u32) })
            .await;
        assert!(matches!(fresh.as_deref(), Ok(9)));
    }

    #[tokio::test]
    async fn test_compute_once() {
        let compute = SharedCompute::new();
        let call_count = Arc::new(AtomicUsize::new(0));

        let count = call_count.clone();
        let result1: Arc<String> = compute
            .get_or_compute("scope1", "key1", async {
                count.fetch_add(1, Ordering::SeqCst);
                "hello".to_string()
            })
            .await;

        let count = call_count.clone();
        let result2: Arc<String> = compute
            .get_or_compute("scope1", "key1", async {
                count.fetch_add(1, Ordering::SeqCst);
                "should not run".to_string()
            })
            .await;

        assert_eq!(*result1, "hello");
        assert_eq!(*result2, "hello"); // Same cached value
        assert_eq!(call_count.load(Ordering::SeqCst), 1); // Computed only once
    }

    #[tokio::test]
    async fn test_different_keys() {
        let compute = SharedCompute::new();

        // What this catches: a derived artifact must be able to request another
        // artifact in its own scope without holding a map lock across await.
        let r1: Arc<i32> = compute
            .get_or_compute("s", "a", async {
                // Fail without hanging on the old implementation's blocking lock.
                assert!(matches!(
                    compute.cache.try_get_mut("s"),
                    dashmap::try_result::TryResult::Present(_)
                ));
                *compute.get_or_compute("s", "b", async { 2 }).await - 1
            })
            .await;
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

    // What this catches: suspended computation cannot block independent work,
    // and same-key readers still share one initialized value after cancellation.
    #[tokio::test]
    async fn suspended_initialization_releases_maps_and_can_be_retried() {
        let compute = SharedCompute::new();
        let mut first = Box::pin(compute.get_or_compute("s", "a", std::future::pending::<i32>()));
        assert!(futures::poll!(&mut first).is_pending());
        {
            let scope = compute.cache.try_get_mut("s");
            let dashmap::try_result::TryResult::Present(scope) = scope else {
                panic!("initializer retains the scope lock");
            };
            assert!(matches!(
                scope.try_get_mut("a"),
                dashmap::try_result::TryResult::Present(_)
            ));
        }
        assert_eq!(*compute.get_or_compute("s", "b", async { 2 }).await, 2);
        let mut second = Box::pin(compute.get_or_compute("s", "a", async { 3 }));
        assert!(futures::poll!(&mut second).is_pending());
        drop(first);
        let result = second.await;
        let cached = compute
            .get_or_compute::<i32, _>("s", "a", async {
                panic!("computed twice");
            })
            .await;
        assert_eq!(*result, 3);
        assert!(Arc::ptr_eq(&result, &cached));
    }

    // What this catches: invalidating in-flight work detaches its cell without
    // cancelling existing readers or letting its late result overwrite new work.
    #[tokio::test]
    async fn invalidation_detaches_inflight_readers() {
        for invalidate_scope in [false, true] {
            let compute = SharedCompute::new();
            let (send, receive) = tokio::sync::oneshot::channel::<i32>();
            let mut first = Box::pin(compute.get_or_try_compute("s", "a", async {
                match receive.await {
                    Ok(value) => Ok::<_, &str>(value),
                    Err(error) => panic!("test sender dropped: {error}"),
                }
            }));
            assert!(futures::poll!(&mut first).is_pending());
            assert!(matches!(
                compute.cache.try_get_mut("s"),
                dashmap::try_result::TryResult::Present(_)
            ));
            let mut reader =
                Box::pin(compute.get_or_try_compute("s", "a", async { Ok::<_, &str>(99) }));
            assert!(futures::poll!(&mut reader).is_pending());
            if invalidate_scope {
                compute.invalidate_scope("s");
            } else {
                compute.invalidate("s", "a");
            }
            let fresh = compute
                .get_or_try_compute("s", "a", async { Ok::<_, &str>(2) })
                .await;
            assert!(send.send(1).is_ok());
            let old = first.await;
            assert!(matches!(old.as_deref(), Ok(1)));
            let reader = reader.await;
            assert!(matches!((&old, &reader), (Ok(a), Ok(b)) if Arc::ptr_eq(a, b)));
            let cached = compute
                .get_or_try_compute("s", "a", async { Ok::<_, &str>(3) })
                .await;
            assert!(matches!((&fresh, &cached), (Ok(a), Ok(b)) if Arc::ptr_eq(a, b)));
        }
    }
}
