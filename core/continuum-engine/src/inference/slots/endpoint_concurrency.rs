//! The aggregate decode admission of ONE endpoint (card 23069702 part 2): the permit every pooled
//! admission takes first (`turn_admission::admit`, step 1), sized from that endpoint's own engine
//! generation (its pool's slot count) and resized when a new generation installs.
//!
//! It was each adapter's own semaphore, seeded from the GLOBAL serving snapshot and resized by a
//! reconciler per adapter: N adapters on one server meant N independent bounds, a learner holding no
//! adapter had none to take, and an endpoint other than the gateway was sized from the gateway.
//! One owner per endpoint now, shared by every adapter and the learner (Codex on #5898, option 2).
//! Cloud and other pool-less endpoints keep the adapter's own semaphore, as a [`DecodeBound`]
//! with nothing to retire.

use parking_lot::Mutex;
use std::sync::Arc;
use tokio::sync::{OwnedSemaphorePermit, Semaphore, TryAcquireError};

/// Permits in circulation (held or free) and the size the installed generation asks for.
struct Counts {
    issued: usize,
    target: usize,
}

/// The semaphore and its counts under one lock: a shrink and a returning permit decide against
/// the same numbers, so a surplus permit is retired where it is found, never handed on.
struct Circulation {
    permits: Arc<Semaphore>,
    counts: Mutex<Counts>,
}

pub struct EndpointConcurrency {
    circulation: Arc<Circulation>,
}

impl EndpointConcurrency {
    /// No permits until the endpoint's first engine generation names its slot count.
    pub(super) fn new() -> Self {
        Self {
            circulation: Arc::new(Circulation {
                permits: Arc::new(Semaphore::new(0)),
                counts: Mutex::new(Counts { issued: 0, target: 0 }),
            }),
        }
    }

    /// The bound `turn_admission::admit_turn` takes this endpoint's decode permits from.
    pub fn bound(&self) -> DecodeBound {
        DecodeBound {
            permits: Arc::clone(&self.circulation.permits),
            retire: Some(Arc::clone(&self.circulation)),
        }
    }

    /// Free permits, for the slots tests that drive a generation through the directory.
    #[cfg(test)]
    pub(super) fn available_permits(&self) -> usize {
        self.circulation.permits.available_permits()
    }

    /// Generations decoding beside one more, for the speculation draft limit.
    pub fn decoding_others(&self) -> usize {
        let counts = self.circulation.counts.lock();
        counts.issued.saturating_sub(self.circulation.permits.available_permits() + 1)
    }

    /// Size the endpoint to `slots` (its installed generation's pool), at once in both directions:
    /// growth adds the permits; a shrink retires every free surplus permit before it returns, and
    /// each held one as it comes back (a held permit is never revoked, and no waiter receives one
    /// while the endpoint is over its size). The caller holds the endpoint's state lock, so the
    /// target follows the installed pool and never a stale one.
    pub(super) fn retarget(&self, slots: usize) {
        let circulation = &self.circulation;
        let mut counts = circulation.counts.lock();
        counts.target = slots;
        if slots > counts.issued {
            circulation.permits.add_permits(slots - counts.issued);
            continuum_base::probe!(class = "inference.permits.grown", from = counts.issued as u64, to = slots as u64,
                "the endpoint's decode permits grew to its generation's slot count");
            counts.issued = slots;
            return;
        }
        while counts.issued > counts.target {
            match circulation.permits.try_acquire() {
                Ok(free) => {
                    free.forget();
                    counts.issued -= 1;
                }
                Err(_) => break, // the rest are held: each retires as it returns
            }
        }
        continuum_base::probe!(class = "inference.permits.shrunk", issued = counts.issued as u64, target = slots as u64,
            "the endpoint's free surplus permits retired; held ones retire as they return");
    }
}

/// Where an admission takes its decode permit: an endpoint's own circulation, or an adapter's
/// semaphore (cloud and other pool-less endpoints), which never shrinks.
#[derive(Clone)]
pub struct DecodeBound {
    permits: Arc<Semaphore>,
    retire: Option<Arc<Circulation>>,
}

impl From<&Arc<Semaphore>> for DecodeBound {
    fn from(permits: &Arc<Semaphore>) -> Self {
        Self { permits: Arc::clone(permits), retire: None }
    }
}

impl DecodeBound {
    pub fn try_acquire(&self) -> Result<DecodePermit, TryAcquireError> {
        Arc::clone(&self.permits).try_acquire_owned().map(|permit| self.held(permit))
    }

    pub async fn acquire(&self) -> DecodePermit {
        let permit = Arc::clone(&self.permits).acquire_owned().await
            .expect("decode semaphore never closed"); // expect: no path closes it; it lives as long as its endpoint or adapter
        self.held(permit)
    }

    fn held(&self, permit: OwnedSemaphorePermit) -> DecodePermit {
        DecodePermit { permit: Some(permit), retire: self.retire.clone() }
    }
}

/// One decode admission. Dropped while its endpoint is over size, it retires instead of returning.
pub struct DecodePermit {
    permit: Option<OwnedSemaphorePermit>,
    retire: Option<Arc<Circulation>>,
}

impl Drop for DecodePermit {
    fn drop(&mut self) {
        let (Some(circulation), Some(permit)) = (&self.retire, self.permit.take()) else {
            return; // an adapter's permit returns through its own drop
        };
        let mut counts = circulation.counts.lock();
        if counts.issued > counts.target {
            permit.forget();
            counts.issued -= 1;
            continuum_base::probe!(class = "inference.permits.retired", issued = counts.issued as u64, target = counts.target as u64,
                "a returning decode permit retired toward the endpoint's smaller generation");
        } else {
            drop(permit); // returned under the lock, so a concurrent shrink counts it
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn available(endpoint: &EndpointConcurrency) -> usize {
        endpoint.circulation.permits.available_permits()
    }

    fn issued(endpoint: &EndpointConcurrency) -> usize {
        endpoint.circulation.counts.lock().issued
    }

    // what this catches (Codex on #5902 at 9e0ea85a8): a shrink that only scheduled a drain left the
    // surplus free, so `retarget(4); retarget(1)` still admitted four before the drain ran. With no
    // runtime and no yield, a shrink admits exactly the new size.
    #[test]
    fn a_shrink_admits_only_the_new_size_before_anything_else_runs() {
        let endpoint = EndpointConcurrency::new();
        assert_eq!(available(&endpoint), 0, "no permits before a generation");
        endpoint.retarget(4);
        endpoint.retarget(1);
        let bound = endpoint.bound();
        let admitted: Vec<_> = (0..4).filter_map(|_| bound.try_acquire().ok()).collect();
        assert_eq!(admitted.len(), 1, "the smaller generation admits its one slot, at once");
        assert_eq!(issued(&endpoint), 1);
    }

    // what this catches: a returned permit handed to the oldest waiter while the endpoint is over its
    // size (the semaphore is FIFO, so a drain task queued behind waiters loses every race), and a
    // held permit revoked by the shrink. Held ones retire as they return; the waiter is admitted only
    // by the permit that brings the endpoint within its size.
    #[tokio::test]
    async fn a_queued_waiter_gets_no_permit_until_the_endpoint_is_within_its_size() {
        let endpoint = EndpointConcurrency::new();
        endpoint.retarget(4);
        let bound = endpoint.bound();
        let mut held: Vec<_> = (0..4).map(|_| bound.try_acquire().expect("test: a free permit")).collect();
        let waiter = bound.acquire();
        tokio::pin!(waiter);
        assert!(futures::poll!(&mut waiter).is_pending(), "queued behind four held permits");
        endpoint.retarget(1);
        for returned in 1..=3 {
            drop(held.pop());
            assert!(futures::poll!(&mut waiter).is_pending(), "returned permit {returned} retired, not handed on");
            assert_eq!(issued(&endpoint), 4 - returned);
        }
        drop(held.pop());
        let admitted = waiter.await;
        assert_eq!(issued(&endpoint), 1, "the last held permit was the one slot, and it went to the waiter");
        drop(admitted);
        assert_eq!(available(&endpoint), 1);
    }

    // what this catches: growth that waits for something (an admission right after discovery would
    // block on a permit on its way), and a regrowth during a shrink that loses permits the larger
    // generation needs.
    #[test]
    fn growth_is_immediate_and_a_regrowth_mid_shrink_keeps_every_permit() {
        let endpoint = EndpointConcurrency::new();
        endpoint.retarget(2);
        let bound = endpoint.bound();
        let held = bound.try_acquire().expect("test: a free permit");
        endpoint.retarget(0);
        assert_eq!((issued(&endpoint), available(&endpoint)), (1, 0), "the free one retired, the held one kept");
        endpoint.retarget(3);
        drop(held);
        assert_eq!((issued(&endpoint), available(&endpoint)), (3, 3), "the held permit returned into the larger size");
    }

    // what this catches: an adapter's semaphore (cloud) treated as an endpoint's, retiring its permits.
    #[test]
    fn an_adapter_semaphore_never_retires() {
        let adapter = Arc::new(Semaphore::new(2));
        let bound = DecodeBound::from(&adapter);
        drop(bound.try_acquire().expect("test: a free permit"));
        assert_eq!(adapter.available_permits(), 2);
    }
}
