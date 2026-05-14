//! Shared concurrency primitives for hot-path coordination.
//!
//! Domain modules should not each invent their own single-flight maps,
//! semaphores, or waiter loops. Put those mechanics here, then inject the
//! policy where orchestration needs concurrency control.

use async_trait::async_trait;
use futures::future::{BoxFuture, FutureExt, Shared};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::hash::Hash;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::Semaphore;

type SharedResult<V, E> = Shared<BoxFuture<'static, Result<V, E>>>;

#[async_trait]
pub trait ConcurrencyPolicy<K, V, E>: Send + Sync
where
    K: Eq + Hash + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
    E: Clone + Send + Sync + 'static,
{
    /// Run `work` if no call for `key` is in flight; otherwise await the
    /// already-running call and return the same result to every waiter.
    async fn single_flight(&self, key: K, work: BoxFuture<'static, Result<V, E>>) -> Result<V, E>;

    fn in_flight_count(&self) -> usize;
}

/// Tokio-backed default policy.
///
/// The trait keeps single-flight object-safe by accepting a boxed future.
/// Bounded concurrency stays as an inherent generic method because the output
/// type varies by caller and does not belong behind `dyn ConcurrencyPolicy`.
pub struct TokioConcurrencyPolicy<K, V, E>
where
    K: Eq + Hash + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
    E: Clone + Send + Sync + 'static,
{
    in_flight: Mutex<HashMap<K, SharedResult<V, E>>>,
    in_flight_count: AtomicUsize,
    limiter: Option<Arc<Semaphore>>,
}

impl<K, V, E> TokioConcurrencyPolicy<K, V, E>
where
    K: Eq + Hash + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
    E: Clone + Send + Sync + 'static,
{
    pub fn new() -> Self {
        Self {
            in_flight: Mutex::new(HashMap::new()),
            in_flight_count: AtomicUsize::new(0),
            limiter: None,
        }
    }

    pub fn with_limit(max_concurrent: usize) -> Self {
        Self {
            in_flight: Mutex::new(HashMap::new()),
            in_flight_count: AtomicUsize::new(0),
            limiter: Some(Arc::new(Semaphore::new(max_concurrent.max(1)))),
        }
    }

    pub async fn bounded<T>(&self, work: BoxFuture<'static, T>) -> T
    where
        T: Send + 'static,
    {
        if let Some(limiter) = &self.limiter {
            let _permit = limiter
                .acquire()
                .await
                .expect("concurrency limiter should not be closed");
            work.await
        } else {
            work.await
        }
    }
}

impl<K, V, E> Default for TokioConcurrencyPolicy<K, V, E>
where
    K: Eq + Hash + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
    E: Clone + Send + Sync + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

/// RAII guard for the analyzer's in-flight entry (#1232).
///
/// Owns cleanup of `in_flight[key]` regardless of whether the work
/// future returns normally OR unwinds via panic. Without this guard,
/// a panic inside the work future skips the post-await cleanup and
/// the in_flight entry stays in the map forever — every subsequent
/// call for the same key sees the poisoned shared future + tries to
/// await it again, hanging or replaying the panic.
///
/// Only the **analyzer** holds the guard. Awaiters hold `None` because
/// the analyzer owns the lifecycle; if the analyzer's work panics,
/// awaiters of the same Shared get a cancellation, the analyzer's
/// guard cleans up the entry, and the next caller for the same key
/// starts a fresh inference instead of finding the broken entry.
struct InFlightGuard<'a, K, V, E>
where
    K: Eq + Hash + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
    E: Clone + Send + Sync + 'static,
{
    in_flight: &'a Mutex<HashMap<K, SharedResult<V, E>>>,
    in_flight_count: &'a AtomicUsize,
    /// Wrapped in Option so Drop can take() it. Always Some until
    /// drop fires; a None here would mean the guard already cleaned
    /// up (used as a no-double-cleanup guard if we add `complete()`
    /// later).
    key: Option<K>,
}

impl<K, V, E> Drop for InFlightGuard<'_, K, V, E>
where
    K: Eq + Hash + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
    E: Clone + Send + Sync + 'static,
{
    fn drop(&mut self) {
        if let Some(key) = self.key.take() {
            // parking_lot::Mutex::lock is poison-free (vs std::sync) so
            // a previously-panicking future cannot poison this lock.
            // The cleanup runs in BOTH the normal-return path (drop
            // at scope end) and the panic-unwind path (drop during
            // unwind). Atomic decrement matches the analyzer's
            // earlier increment exactly once.
            let mut in_flight = self.in_flight.lock();
            if in_flight.remove(&key).is_some() {
                self.in_flight_count.fetch_sub(1, Ordering::AcqRel);
            }
        }
    }
}

#[async_trait]
impl<K, V, E> ConcurrencyPolicy<K, V, E> for TokioConcurrencyPolicy<K, V, E>
where
    K: Eq + Hash + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
    E: Clone + Send + Sync + 'static,
{
    async fn single_flight(&self, key: K, work: BoxFuture<'static, Result<V, E>>) -> Result<V, E> {
        // Two paths:
        //   - Analyzer (first caller for this key): registers a fresh
        //     Shared future + holds an InFlightGuard. The guard owns
        //     cleanup via RAII — fires on normal return AND on panic
        //     unwind (#1232).
        //   - Awaiter (subsequent callers): clones the registered
        //     Shared future + holds NO guard. The analyzer owns the
        //     lifecycle.
        let (shared, _guard) = {
            let mut in_flight = self.in_flight.lock();
            if let Some(existing) = in_flight.get(&key) {
                // Awaiter path: no guard. Analyzer's guard runs cleanup.
                (existing.clone(), None)
            } else {
                let shared = work.shared();
                in_flight.insert(key.clone(), shared.clone());
                self.in_flight_count.fetch_add(1, Ordering::AcqRel);
                // Analyzer path: hold the RAII guard so cleanup fires
                // even if shared.await panics or the task is cancelled.
                (
                    shared,
                    Some(InFlightGuard {
                        in_flight: &self.in_flight,
                        in_flight_count: &self.in_flight_count,
                        key: Some(key),
                    }),
                )
            }
        };

        // Both arms await the SAME Shared future. If the work panics,
        // the panic unwinds OUT of this .await — and the analyzer's
        // _guard drops on the way out, cleaning up the in_flight entry.
        // Awaiters get the panic re-raised by Shared (they didn't run
        // it); their _guard is None so they don't try to clean up.
        shared.await
    }

    fn in_flight_count(&self) -> usize {
        self.in_flight_count.load(Ordering::Acquire)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn single_flight_runs_one_producer_for_many_waiters() {
        let policy = Arc::new(TokioConcurrencyPolicy::<String, usize, String>::new());
        let producers = Arc::new(AtomicUsize::new(0));

        let mut tasks = Vec::new();
        for _ in 0..16 {
            let policy = Arc::clone(&policy);
            let producers = Arc::clone(&producers);
            tasks.push(tokio::spawn(async move {
                policy
                    .single_flight(
                        "same-key".to_string(),
                        async move {
                            producers.fetch_add(1, Ordering::AcqRel);
                            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                            Ok(42usize)
                        }
                        .boxed(),
                    )
                    .await
            }));
        }

        for task in tasks {
            assert_eq!(task.await.unwrap().unwrap(), 42);
        }
        assert_eq!(producers.load(Ordering::Acquire), 1);
        assert_eq!(policy.in_flight_count(), 0);
    }

    /// What this catches: a panicking work future no longer poisons
    /// the in_flight map (#1232). Before the Drop-guard, the panic
    /// unwound past the post-await cleanup, leaving the entry +
    /// counter stuck. After the guard, the entry clears on panic
    /// unwind exactly the same way it does on normal return.
    ///
    /// The test:
    ///   1. First call panics inside the work future
    ///   2. Catch the panic via `tokio::spawn`'s JoinError-on-panic
    ///   3. Assert in_flight_count is 0 (NOT 1) after the panic
    ///   4. Second call succeeds — proving the key isn't poisoned
    #[tokio::test]
    async fn single_flight_drop_guard_clears_in_flight_on_panic() {
        let policy = Arc::new(TokioConcurrencyPolicy::<String, usize, String>::new());
        let key = "panic-key".to_string();

        // First call: panics inside the work future. tokio::spawn
        // catches the panic so the test process survives; we assert
        // the policy's in-flight state recovered.
        let policy_p = Arc::clone(&policy);
        let key_p = key.clone();
        let panic_handle = tokio::spawn(async move {
            policy_p
                .single_flight(
                    key_p,
                    async move {
                        panic!("simulated work-future panic");
                    }
                    .boxed(),
                )
                .await
        });
        let panic_outcome = panic_handle.await;
        assert!(
            panic_outcome.is_err() && panic_outcome.unwrap_err().is_panic(),
            "first call should have observed the panic"
        );

        // Drop-guard invariant: in_flight count went back to 0.
        // Without the guard this would be 1 (entry never removed).
        assert_eq!(
            policy.in_flight_count(),
            0,
            "Drop-guard should clear in_flight entry on panic; \
             a non-zero count means the panic poisoned the map"
        );

        // Second call for the SAME key: succeeds. Without the guard,
        // it would either hang on the dead Shared future or replay
        // the panic. With the guard, the key is fresh and the new
        // work runs cleanly.
        let result = policy
            .single_flight(
                key.clone(),
                async move { Ok::<usize, String>(99) }.boxed(),
            )
            .await;
        assert_eq!(result, Ok(99), "second call after panic should succeed cleanly");
        assert_eq!(policy.in_flight_count(), 0, "second call should also clean up");
    }

    #[tokio::test]
    async fn bounded_caps_concurrent_work() {
        let policy = Arc::new(TokioConcurrencyPolicy::<String, (), ()>::with_limit(2));
        let active = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));

        let mut tasks = Vec::new();
        for _ in 0..8 {
            let policy = Arc::clone(&policy);
            let active = Arc::clone(&active);
            let peak = Arc::clone(&peak);
            tasks.push(tokio::spawn(async move {
                policy
                    .bounded(
                        async move {
                            let current = active.fetch_add(1, Ordering::AcqRel) + 1;
                            peak.fetch_max(current, Ordering::AcqRel);
                            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                            active.fetch_sub(1, Ordering::AcqRel);
                        }
                        .boxed(),
                    )
                    .await;
            }));
        }

        for task in tasks {
            task.await.unwrap();
        }
        assert_eq!(peak.load(Ordering::Acquire), 2);
    }
}
