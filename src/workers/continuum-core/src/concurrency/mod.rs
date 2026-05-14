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

    /// What this catches: regression in the #1235 fix. The previous
    /// "only the analyzer holds a Drop guard" model removed the
    /// in_flight entry as soon as the analyzer cancelled, even if
    /// awaiters were still holding the Shared. A NEW caller arriving
    /// after the analyzer drop but before the awaiter completed would
    /// find no entry and start duplicate work for the same key.
    ///
    /// With the refcount fix, the entry survives analyzer cancellation
    /// for as long as ANY caller still holds a guard. A new caller
    /// arriving in that window joins the existing Shared instead of
    /// kicking off a duplicate.
    ///
    /// Test shape:
    ///   1. Analyzer.single_flight("k") starts long-running work, then
    ///      its hosting task is dropped (cancellation).
    ///   2. While the analyzer task is dropping, an awaiter holds a
    ///      clone of the Shared via its own single_flight call.
    ///   3. After analyzer drop, a NEW caller arrives for "k".
    ///   4. The new caller MUST join the same Shared (work executes
    ///      ONCE total across all three callers), not start fresh.
    ///
    /// This test would FAIL on pre-#1235 code because step (1)'s drop
    /// would have removed the in_flight entry, and step (3) would have
    /// triggered a fresh `work` future. After #1235 the analyzer's
    /// guard drop only decrements the refcount; the awaiter's guard
    /// keeps the entry alive.
    #[tokio::test]
    async fn analyzer_cancellation_does_not_evict_entry_while_awaiters_hold_it() {
        let policy = Arc::new(TokioConcurrencyPolicy::<String, usize, String>::new());
        let producers = Arc::new(AtomicUsize::new(0));
        let key = "k".to_string();

        // Start the work-future producer with a release-on-signal handle
        // so the test can hold it open until we're ready.
        let release = Arc::new(tokio::sync::Notify::new());

        // (1) Analyzer task: starts the work, awaits indefinitely until
        // we drop its handle to simulate cancellation.
        let analyzer_handle = {
            let policy = Arc::clone(&policy);
            let producers = Arc::clone(&producers);
            let release = Arc::clone(&release);
            let key = key.clone();
            tokio::spawn(async move {
                policy
                    .single_flight(
                        key,
                        async move {
                            producers.fetch_add(1, Ordering::AcqRel);
                            // Block until released so the test can stage
                            // cancellation + new-caller arrival.
                            release.notified().await;
                            Ok::<usize, String>(7)
                        }
                        .boxed(),
                    )
                    .await
            })
        };

        // (2) Awaiter task: joins the same key. Hold this open across
        // analyzer cancellation so the entry refcount stays >= 1.
        let awaiter_handle = {
            let policy = Arc::clone(&policy);
            let release = Arc::clone(&release);
            let key = key.clone();
            tokio::spawn(async move {
                // Yield so analyzer registers first.
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                let result = policy
                    .single_flight(
                        key,
                        async move {
                            // Should NEVER run: awaiter joins existing
                            // Shared, doesn't create its own work.
                            release.notified().await;
                            Ok::<usize, String>(999)
                        }
                        .boxed(),
                    )
                    .await;
                result
            })
        };

        // Give both tasks time to register / clone the Shared.
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        assert_eq!(
            policy.in_flight_count(),
            1,
            "after analyzer + awaiter, exactly one in-flight key"
        );

        // (3) Cancel the analyzer task. With the old model, this would
        // remove the in_flight entry. With #1235 the awaiter's
        // refcount keeps it alive.
        analyzer_handle.abort();
        let _ = analyzer_handle.await; // observe the cancellation

        // The entry MUST still be in the map because the awaiter holds
        // a guard. Pre-#1235 this assertion failed.
        assert_eq!(
            policy.in_flight_count(),
            1,
            "analyzer cancellation must NOT evict the entry — \
             awaiter still holds the Shared (#1235)"
        );

        // (4) NEW caller arrives. With #1235 it joins the awaiter's
        // Shared. Pre-#1235 it would have started fresh work.
        let new_caller_handle = {
            let policy = Arc::clone(&policy);
            let key = key.clone();
            tokio::spawn(async move {
                policy
                    .single_flight(
                        key,
                        async move {
                            // Should NEVER run: joins existing Shared.
                            Ok::<usize, String>(999)
                        }
                        .boxed(),
                    )
                    .await
            })
        };

        // Give new caller time to enter single_flight + bump refcount.
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;

        // Release the original work future. Awaiter + new caller both
        // observe its result via the same Shared.
        release.notify_waiters();

        let awaiter_result = awaiter_handle.await.unwrap();
        let new_caller_result = new_caller_handle.await.unwrap();

        assert_eq!(
            awaiter_result,
            Ok(7),
            "awaiter should see the original work's result"
        );
        assert_eq!(
            new_caller_result,
            Ok(7),
            "NEW caller MUST see the SAME shared result, not a fresh \
             work-future's value (would be 999 if duplicate work ran)"
        );
        assert_eq!(
            producers.load(Ordering::Acquire),
            1,
            "work-future producer body must have run EXACTLY ONCE \
             across analyzer + awaiter + new-caller (the contract \
             #1235 enforces). Pre-#1235 this would have been 2 \
             because the new caller started a duplicate after the \
             analyzer's guard evicted the entry."
        );
        assert_eq!(
            policy.in_flight_count(),
            0,
            "all callers complete → refcount → 0 → entry evicted"
        );
    }

    /// What this catches: regression in the all-callers-cancelled path.
    /// If every holder drops without completing, the entry should be
    /// removed (refcount → 0) and a brand-new caller for the same key
    /// should correctly start fresh — the prior abandoned work is
    /// no longer of interest to anyone.
    #[tokio::test]
    async fn all_callers_cancelled_evicts_entry_for_fresh_start() {
        let policy = Arc::new(TokioConcurrencyPolicy::<String, usize, String>::new());
        let producers = Arc::new(AtomicUsize::new(0));
        let key = "k".to_string();

        // Two cancellable callers, both holding the same key.
        let release_never = Arc::new(tokio::sync::Notify::new());
        let make_caller = || {
            let policy = Arc::clone(&policy);
            let producers = Arc::clone(&producers);
            let release = Arc::clone(&release_never);
            let key = key.clone();
            tokio::spawn(async move {
                policy
                    .single_flight(
                        key,
                        async move {
                            producers.fetch_add(1, Ordering::AcqRel);
                            release.notified().await;
                            Ok::<usize, String>(1)
                        }
                        .boxed(),
                    )
                    .await
            })
        };

        let a = make_caller();
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        let b = make_caller();
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        assert_eq!(policy.in_flight_count(), 1);

        // Cancel both — entry should evict cleanly.
        a.abort();
        b.abort();
        let _ = a.await;
        let _ = b.await;
        // Yield so the abort drops + Drop chain run.
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        assert_eq!(
            policy.in_flight_count(),
            0,
            "all guards dropped → entry evicted"
        );

        // Fresh caller for the same key: starts fresh work (the prior
        // abandoned work is gone).
        let result = policy
            .single_flight(key, async move { Ok::<usize, String>(42) }.boxed())
            .await;
        assert_eq!(result, Ok(42), "fresh caller after eviction succeeds");
        assert_eq!(policy.in_flight_count(), 0);
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
