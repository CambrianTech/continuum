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

#[async_trait]
impl<K, V, E> ConcurrencyPolicy<K, V, E> for TokioConcurrencyPolicy<K, V, E>
where
    K: Eq + Hash + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
    E: Clone + Send + Sync + 'static,
{
    async fn single_flight(&self, key: K, work: BoxFuture<'static, Result<V, E>>) -> Result<V, E> {
        let shared = {
            let mut in_flight = self.in_flight.lock();
            if let Some(existing) = in_flight.get(&key) {
                existing.clone()
            } else {
                let shared = work.shared();
                in_flight.insert(key.clone(), shared.clone());
                self.in_flight_count.fetch_add(1, Ordering::AcqRel);
                shared
            }
        };

        let result = shared.await;

        let mut in_flight = self.in_flight.lock();
        if in_flight.remove(&key).is_some() {
            self.in_flight_count.fetch_sub(1, Ordering::AcqRel);
        }

        result
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
