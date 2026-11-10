//! The aggregate decode admission of ONE endpoint (card 23069702 part 2): the permit every pooled
//! admission takes first (`turn_admission::admit`, step 1), sized from that endpoint's own engine
//! generation (its pool's slot count) and resized when a new generation installs.
//!
//! It was each adapter's own semaphore, seeded from the GLOBAL serving snapshot and resized by a
//! reconciler per adapter: N adapters on one server meant N independent bounds, a learner holding no
//! adapter had none to take, and an endpoint other than the gateway was sized from the gateway.
//! One owner per endpoint now, shared by every adapter and the learner (Codex on #5898, option 2).
//! Cloud and other pool-less endpoints keep the adapter's own semaphore.

use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{watch, Semaphore};

pub struct EndpointConcurrency {
    permits: Arc<Semaphore>,
    /// Permits in circulation (held or available): grows at once, falls as a retired permit
    /// comes back.
    issued: Arc<Mutex<usize>>,
    target: watch::Sender<usize>,
    drain_started: AtomicBool,
}

impl EndpointConcurrency {
    /// No permits until the endpoint's first engine generation names its slot count.
    pub(super) fn new() -> Self {
        Self {
            permits: Arc::new(Semaphore::new(0)),
            issued: Arc::new(Mutex::new(0)),
            target: watch::channel(0).0,
            drain_started: AtomicBool::new(false),
        }
    }

    /// The endpoint's decode permits, for `turn_admission::admit_turn`.
    pub fn permits(&self) -> Arc<Semaphore> {
        Arc::clone(&self.permits)
    }

    /// Generations decoding beside one more, for the speculation draft limit.
    pub fn decoding_others(&self) -> usize {
        self.issued.lock().saturating_sub(self.permits.available_permits() + 1)
    }

    /// Size the endpoint to `slots` (its installed generation's pool). Growth is immediate. A
    /// shrink retires the surplus as in-flight generations release it, so a smaller relaunch is
    /// never over-admitted, and a growth during the drain keeps every permit it still needs.
    pub(super) fn retarget(&self, slots: usize) {
        let mut issued = self.issued.lock();
        self.target.send_replace(slots);
        if slots > *issued {
            self.permits.add_permits(slots - *issued);
            continuum_base::probe!(class = "inference.permits.grown", from = *issued as u64, to = slots as u64,
                "the endpoint's decode permits grew to its generation's slot count");
            *issued = slots;
        } else if slots < *issued {
            drop(issued);
            self.ensure_drain();
        }
    }

    /// One drain task per endpoint, started at its first shrink, retiring permits while more are
    /// issued than the latest target. It needs a runtime; outside one (a synchronous test that only
    /// grows) there is nothing to drain.
    fn ensure_drain(&self) {
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            continuum_base::probe!(class = "inference.permits.shrink_unscheduled", "a shrink outside a runtime: retired at the next shrink inside one");
            return;
        };
        if self.drain_started.swap(true, Ordering::SeqCst) {
            return;
        }
        let permits = Arc::clone(&self.permits);
        let issued = Arc::clone(&self.issued);
        let mut target = self.target.subscribe();
        runtime.spawn(async move {
            loop {
                while *issued.lock() > *target.borrow_and_update() {
                    let Ok(permit) = permits.acquire().await else { return };
                    let mut held = issued.lock();
                    let goal = *target.borrow();
                    if *held > goal {
                        permit.forget();
                        *held -= 1;
                        continuum_base::probe!(class = "inference.permits.shrunk", to = *held as u64, target = goal as u64,
                            "a decode permit retired toward the endpoint's smaller generation");
                    }
                    // else: the target grew back while this permit was in hand; it returns on drop.
                }
                if target.changed().await.is_err() {
                    return; // the endpoint is gone
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: growth that waits for a task (an admission right after discovery would
    // block on a permit that is on its way), and a shrink that over-admits a smaller generation or,
    // when the target grows back mid-drain, loses permits it still needs.
    #[tokio::test]
    async fn grows_at_once_shrinks_as_permits_return_and_never_loses_one_on_regrowth() {
        let endpoint = EndpointConcurrency::new();
        assert_eq!(endpoint.permits().available_permits(), 0, "no permits before a generation");
        endpoint.retarget(2);
        assert_eq!(endpoint.permits().available_permits(), 2, "growth is immediate");
        endpoint.retarget(4);
        assert_eq!(endpoint.permits().available_permits(), 4);

        let held: Vec<_> = (0..3).map(|_| endpoint.permits().try_acquire_owned().expect("test: a free permit")).collect();
        endpoint.retarget(1);
        for _ in 0..50 {
            if *endpoint.issued.lock() == 3 {
                break;
            }
            tokio::task::yield_now().await;
        }
        assert_eq!(endpoint.permits().available_permits(), 0, "the free permit retires first; held ones are never revoked");
        drop(held);
        for _ in 0..50 {
            if *endpoint.issued.lock() == 1 {
                break;
            }
            tokio::task::yield_now().await;
        }
        assert_eq!(*endpoint.issued.lock(), 1, "returned permits retire down to the smaller generation");
        assert_eq!(endpoint.permits().available_permits(), 1);

        // Regrowth during a drain: hold the one permit, shrink to 0, grow to 3 before it returns.
        let only = endpoint.permits().try_acquire_owned().expect("test: the one permit");
        endpoint.retarget(0);
        endpoint.retarget(3);
        drop(only);
        tokio::task::yield_now().await;
        assert_eq!(endpoint.permits().available_permits(), 3, "every permit the larger generation needs is in circulation");
        assert_eq!(*endpoint.issued.lock(), 3);
    }
}
