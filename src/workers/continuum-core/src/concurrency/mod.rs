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

/// Per-key in-flight entry: the shared future + a refcount of how many
/// callers (analyzer + awaiters) currently hold a `RefCountGuard` for
/// this key. The entry is removed when the refcount drops to zero
/// (#1235 — replaces the previous "only-analyzer-cleans-up" model so
/// analyzer cancellation can no longer remove the entry while awaiters
/// still hold the Shared, which previously let a brand-new caller race
/// in and start duplicate work for the same key).
struct KeyEntry<V, E>
where
    V: Clone + Send + Sync + 'static,
    E: Clone + Send + Sync + 'static,
{
    shared: SharedResult<V, E>,
    /// Number of `single_flight` calls currently holding a guard for
    /// this key. Bumped under the in_flight mutex on every entry path
    /// (analyzer + awaiter), decremented on every guard drop.
    refcount: Arc<AtomicUsize>,
}

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
    in_flight: Mutex<HashMap<K, KeyEntry<V, E>>>,
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

/// RAII refcount guard for an in-flight entry (#1232 + #1235).
///
/// **Every** caller — the analyzer (first caller for this key) AND each
/// awaiter — holds a `RefCountGuard` for the duration of its
/// `single_flight` call. The entry's `Arc<AtomicUsize>` is bumped under
/// the in_flight mutex when the guard is constructed, and decremented
/// when the guard drops. The map entry is removed only when the
/// refcount hits zero (under the lock, double-checked to handle a new
/// caller racing in between fetch_sub and the lock acquisition).
///
/// # Why every caller holds one (not just the analyzer)
///
/// Pre-#1235 only the analyzer held a Drop guard. That correctly fixed
/// the panic-cleanup case (#1232) but left a window during analyzer
/// cancellation:
///
/// ```text
///   T0: analyzer.single_flight("k") → creates entry, holds guard
///   T1: awaiter1.single_flight("k") → clones Shared, no guard
///   T2: analyzer task is dropped (cancellation)
///   T3: analyzer's guard.drop fires → removes entry from in_flight
///   T4: NEW caller.single_flight("k") → finds no entry → starts a
///       FRESH `work` future for "k" — duplicate work, contract
///       violated. awaiter1 still completes the original Shared, but
///       there are now two concurrent inferences for the same key.
/// ```
///
/// With per-caller refcounts, the entry stays alive as long as ANY
/// caller (analyzer or awaiter) is still holding the Shared. Only when
/// the last holder drops does cleanup fire — at which point any future
/// caller correctly starts fresh (no one is waiting for the old
/// result).
///
/// # Panic behavior preserved
///
/// If the work future panics, the panic unwinds through `shared.await`
/// in every caller (Shared re-raises to clones). All guards drop during
/// unwind, refcount → 0, entry removed. Same end state as #1232.
struct RefCountGuard<'a, K, V, E>
where
    K: Eq + Hash + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
    E: Clone + Send + Sync + 'static,
{
    in_flight: &'a Mutex<HashMap<K, KeyEntry<V, E>>>,
    in_flight_count: &'a AtomicUsize,
    /// Same Arc the entry holds — pre-bumped under the in_flight lock
    /// when this guard was constructed.
    refcount: Arc<AtomicUsize>,
    /// Wrapped in Option so Drop can take() it. Always Some until
    /// drop fires.
    key: Option<K>,
}

impl<K, V, E> Drop for RefCountGuard<'_, K, V, E>
where
    K: Eq + Hash + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
    E: Clone + Send + Sync + 'static,
{
    fn drop(&mut self) {
        let Some(key) = self.key.take() else { return };

        // Decrement first; this is the contract that as long as ANY
        // refcount > 0 the entry MUST be in the map. The decrement is
        // unconditional — every guard pre-incremented in single_flight
        // under the lock, so every drop must match it exactly once.
        let prev = self.refcount.fetch_sub(1, Ordering::AcqRel);
        if prev != 1 {
            // Other callers are still holding the entry; nothing to
            // clean up. The entry stays in the map for them.
            return;
        }

        // We were the last holder (refcount went 1 → 0). Acquire the
        // lock and DOUBLE-CHECK the per-key refcount under the lock —
        // a brand-new single_flight call may have raced in between our
        // fetch_sub and our lock acquisition, found the entry, bumped
        // refcount back to 1, and we'd erroneously remove the entry
        // with that fresh caller still expecting it.
        //
        // parking_lot::Mutex::lock is poison-free (vs std::sync) so a
        // previously-panicking future cannot poison this lock.
        let mut in_flight = self.in_flight.lock();
        if let Some(entry) = in_flight.get(&key) {
            if entry.refcount.load(Ordering::Acquire) == 0 {
                in_flight.remove(&key);
                self.in_flight_count.fetch_sub(1, Ordering::AcqRel);
            }
            // else: a new caller raced in and bumped the refcount under
            // the lock. Leave the entry — it now belongs to them.
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
        // EVERY caller (analyzer + awaiters) gets a RefCountGuard so
        // the entry's lifetime is tied to all outstanding holders, not
        // just the first caller (#1235). The two paths differ only in
        // whether they create a fresh entry or join an existing one;
        // both increment the per-key refcount under the in_flight lock.
        let (shared, _guard) = {
            let mut in_flight = self.in_flight.lock();
            if let Some(entry) = in_flight.get(&key) {
                // Awaiter path: bump existing refcount, clone Shared.
                entry.refcount.fetch_add(1, Ordering::AcqRel);
                (
                    entry.shared.clone(),
                    RefCountGuard {
                        in_flight: &self.in_flight,
                        in_flight_count: &self.in_flight_count,
                        refcount: entry.refcount.clone(),
                        key: Some(key),
                    },
                )
            } else {
                // Analyzer path: create fresh entry with refcount=1.
                let shared = work.shared();
                let refcount = Arc::new(AtomicUsize::new(1));
                in_flight.insert(
                    key.clone(),
                    KeyEntry {
                        shared: shared.clone(),
                        refcount: refcount.clone(),
                    },
                );
                self.in_flight_count.fetch_add(1, Ordering::AcqRel);
                (
                    shared,
                    RefCountGuard {
                        in_flight: &self.in_flight,
                        in_flight_count: &self.in_flight_count,
                        refcount,
                        key: Some(key),
                    },
                )
            }
        };

        // Every caller awaits the SAME Shared future. The Shared keeps
        // the underlying BoxFuture alive across analyzer cancellation
        // (Arc internal); whichever awaiter polls drives it forward.
        // If work panics, panic re-raises through every clone; the
        // guards drop on the way out, refcount → 0, entry removed.
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
