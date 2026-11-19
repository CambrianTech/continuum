//! The aggregate decode admission of ONE endpoint (card 23069702 part 2): the permit every pooled
//! admission takes first (`turn_admission::admit`, step 1). Its size is the execution policy, not the
//! slot count: ONE generation at a time whenever the endpoint serves any slot ([`active_decodes`]),
//! re-applied when a new engine generation installs. Slots are residency (her KV kept warm); this is
//! who runs.
//!
//! It was each adapter's own semaphore, seeded from the GLOBAL serving snapshot and resized by a
//! reconciler per adapter: N adapters on one server meant N independent bounds, a learner holding no
//! adapter had none to take, and an endpoint other than the gateway was sized from the gateway.
//! One owner per endpoint now, shared by every adapter and the learner (Codex on #5898, option 2).
//! Cloud and other pool-less endpoints keep the adapter's own semaphore, as a [`DecodeBound`]
//! with nothing to retire.

use parking_lot::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, LazyLock};
use tokio::sync::{Notify, OwnedSemaphorePermit, Semaphore, TryAcquireError};

/// What a waiter for a decode permit waits on: the next return, registered BEFORE the try that
/// found none, so a return between the try and the wait is never missed.
pub type PermitReturn<'a> = std::pin::Pin<Box<tokio::sync::futures::Notified<'a>>>;

/// The return signal of every adapter's own semaphore (cloud and other pool-less endpoints): that
/// semaphore carries no state of its own, so one signal serves them all, and each woken waiter
/// re-tries its own semaphore. A wake is still one real return, never a spin.
static ADAPTER_PERMIT_RETURNED: LazyLock<Arc<Notify>> = LazyLock::new(|| Arc::new(Notify::new()));

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
    /// Raised when a permit returns to circulation or the size grows: what a waiter waits on.
    returned: Arc<Notify>,
    /// Turns waiting for a permit: while any waits, background work does not take one.
    turns_waiting: Arc<AtomicUsize>,
}

/// How many generations an endpoint runs at once, whatever its resident slots: ONE. Every slot keeps its
/// persona's KV resident; only one of them is on the engine. Measured on the M5 (2026-11-17, Qwen3.6-35B-A3B,
/// `--parallel 5`): four minds generating together each decoded at 2.1-2.4 tok/s (the engine's own
/// `decodeTokensPerSecond`, captures 20:29-21:29Z), and every other mind's turn waited behind them
/// (`delib.window.waiting` BehindActs { acting: 3 }); one stream on the same engine is ~80 tok/s. So a mind
/// takes the engine whole for her turn and the next waits for it, the way a GPU renders one view at a time.
pub(super) fn active_decodes(slots: u32) -> usize {
    usize::from(slots > 0) // one generation when the endpoint serves any slot, none when it serves none
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
                returned: Arc::new(Notify::new()),
                turns_waiting: Arc::new(AtomicUsize::new(0)),
            }),
        }
    }

    /// The bound `turn_admission::admit_turn` takes this endpoint's decode permits from.
    pub fn bound(&self) -> DecodeBound {
        DecodeBound {
            permits: Arc::clone(&self.circulation.permits),
            returned: Arc::clone(&self.circulation.returned),
            turns_waiting: Arc::clone(&self.circulation.turns_waiting),
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
            drop(counts);
            circulation.returned.notify_waiters(); // the boot's waiters had no permit to wait for
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
///
/// NO WAITER IS EVER HANDED A PERMIT (card 6742aad1). A waiter waits for a RETURN and then tries,
/// never in the semaphore's own queue: Tokio hands a released permit straight to its oldest queued
/// waiter, so a caller that waited there only to let the permit go again (a lane holder must not
/// keep a lane across the wait) passed it to the next such waiter while its own retry found none.
/// Two dreams did that forever on the M5 (2026-11-13, canary 0ed0d231b): 35,000 lane grants a
/// second at 180% CPU, no generation. With no queue, no permit can be assigned to a waiter that is
/// then cancelled, and a free permit while over size cannot exist (a shrink retires the free ones
/// under the lock that a return retires under).
#[derive(Clone)]
pub struct DecodeBound {
    permits: Arc<Semaphore>,
    returned: Arc<Notify>,
    /// Turns waiting for this endpoint's permit. A TURN OUTRANKS BACKGROUND WORK (Codex, BigMama R21,
    /// 2026-11-19 13:24-13:34Z): a turn holding a lane lets it go to wait, then must take its lane
    /// again before it retries, while a dream retries at once; so each returned permit went to the
    /// next dream and Kimi's admitted act waited 610 s behind four of them. While a turn waits,
    /// [`DecodeBound::acquire`] (background, dreams) does not take a returned permit.
    turns_waiting: Arc<AtomicUsize>,
    retire: Option<Arc<Circulation>>,
}

impl From<&Arc<Semaphore>> for DecodeBound {
    fn from(permits: &Arc<Semaphore>) -> Self {
        Self {
            permits: Arc::clone(permits),
            returned: Arc::clone(&ADAPTER_PERMIT_RETURNED),
            // a pool-less adapter's own semaphore ranks its own waiters only: a turn waiting on one
            // never holds back another adapter's work (Codex on #6159)
            turns_waiting: Arc::new(AtomicUsize::new(0)),
            retire: None,
        }
    }
}

/// A turn waiting for a decode permit, counted from its first refused try until it holds one; the
/// last to leave wakes background work held back for it. Timed: how long her page waited for the
/// permit is told apart from the lane, gate and slot waits on the same act (Codex, BigMama R21).
pub struct TurnWaiting {
    turns_waiting: Arc<AtomicUsize>,
    returned: Arc<Notify>,
    page: super::PageKey,
    since: std::time::Instant,
}

impl Drop for TurnWaiting {
    fn drop(&mut self) {
        continuum_base::probe!(class = "inference.decode_permit.turn_waited", page = %self.page, ms = self.since.elapsed().as_millis() as u64,
            waiting = self.turns_waiting.load(Ordering::Acquire) as u64, "a turn's wait for the decode permit ended");
        if self.turns_waiting.fetch_sub(1, Ordering::AcqRel) == 1 {
            self.returned.notify_waiters();
        }
    }
}

impl DecodeBound {
    /// A free permit within the endpoint's size, or `NoPermits`.
    pub fn try_acquire(&self) -> Result<DecodePermit, TryAcquireError> {
        let permit = Arc::clone(&self.permits).try_acquire_owned()?;
        Ok(DecodePermit { permit: Some(permit), returned: Arc::clone(&self.returned), retire: self.retire.clone() })
    }

    /// A free permit now, or the wait for the next return, registered before the try. The caller
    /// lets go of whatever it must not hold across the wait, awaits it, and tries again.
    pub fn try_or_return(&self) -> Result<DecodePermit, PermitReturn<'_>> {
        let mut returned = Box::pin(self.returned.notified());
        returned.as_mut().enable();
        self.try_acquire().map_err(|_| returned)
    }

    /// [`Self::try_or_return`] for a caller that may be a turn: a refused turn is counted waiting in
    /// `waiting` (kept across its retries, its lane released and taken again) until it holds a
    /// permit, so background work does not take the permits it waits for.
    pub fn try_or_return_as(&self, turn: Option<super::PageKey>, waiting: &mut Option<TurnWaiting>) -> Result<DecodePermit, PermitReturn<'_>> {
        // Background work holding a lane (a keyed dream, a dream on a pool without scratch) takes
        // the endpoint's priority decision too: while a turn waits it gets the wait, never the
        // permit, and lets its lane go as any refused try does (Codex on #6159 at 411773eb20).
        if turn.is_none() && self.turns_waiting.load(Ordering::Acquire) > 0 {
            let mut returned = Box::pin(self.returned.notified());
            returned.as_mut().enable();
            return Err(returned);
        }
        let tried = self.try_or_return();
        match (&tried, turn) {
            (Ok(_), _) => *waiting = None,
            (Err(_), Some(page)) if waiting.is_none() => {
                self.turns_waiting.fetch_add(1, Ordering::AcqRel);
                *waiting = Some(TurnWaiting {
                    turns_waiting: Arc::clone(&self.turns_waiting),
                    returned: Arc::clone(&self.returned),
                    page,
                    since: std::time::Instant::now(),
                });
            }
            (Err(_), _) => {}
        }
        tried
    }

    /// Waits for a permit within the endpoint's size: each return wakes it to try once. Background
    /// work: while a turn waits it does not take a returned permit (a turn outranks it), and the last
    /// waiting turn's leaving wakes it.
    pub async fn acquire(&self) -> DecodePermit {
        loop {
            let mut returned = Box::pin(self.returned.notified());
            returned.as_mut().enable();
            if self.turns_waiting.load(Ordering::Acquire) == 0 {
                if let Ok(permit) = self.try_acquire() {
                    return permit;
                }
            }
            returned.await;
        }
    }
}

/// One decode admission. Dropped while its endpoint is over size, it retires instead of returning;
/// returned, it wakes every waiter to try.
pub struct DecodePermit {
    permit: Option<OwnedSemaphorePermit>,
    returned: Arc<Notify>,
    retire: Option<Arc<Circulation>>,
}

impl Drop for DecodePermit {
    fn drop(&mut self) {
        let Some(permit) = self.permit.take() else { return };
        if let Some(circulation) = &self.retire {
            let mut counts = circulation.counts.lock();
            if counts.issued > counts.target {
                permit.forget();
                counts.issued -= 1;
                continuum_base::probe!(class = "inference.permits.retired", issued = counts.issued as u64, target = counts.target as u64,
                    "a returning decode permit retired toward the endpoint's smaller generation");
                return;
            }
            drop(permit); // returned under the lock, so a concurrent shrink counts it
        } else {
            drop(permit);
        }
        self.returned.notify_waiters();
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

    // what this catches (card 6742aad1): a waiter handed a permit by the semaphore's FIFO queue. Two
    // callers that each waited only to let the permit go passed it between them forever while each
    // one's retry found none (35,000 lane grants a second on the M5). A waiter now waits for a
    // return and is assigned nothing: the returned permit stays free until a try takes it.
    #[tokio::test]
    async fn a_waiter_is_woken_by_a_return_and_never_handed_the_permit() {
        let endpoint = EndpointConcurrency::new();
        endpoint.retarget(1);
        let bound = endpoint.bound();
        let held = bound.try_acquire().expect("test: a free permit");
        let returned = bound.try_or_return().err().expect("test: none free while it is held");
        let waiter = bound.acquire();
        tokio::pin!(waiter);
        assert!(futures::poll!(&mut waiter).is_pending(), "queued behind the held permit");
        drop(held);
        returned.await; // registered before the return: never missed
        assert_eq!(available(&endpoint), 1, "returned to the semaphore, not assigned to the waiter");
        let tried = bound.try_acquire().expect("test: a try takes the returned permit");
        assert!(futures::poll!(&mut waiter).is_pending(), "the waiter woke, found it taken, and waits again");
        drop(tried);
        drop(waiter.await);
    }

    // what this catches: a waiter at boot (no permits yet) that only a RETURN wakes would wait
    // forever, because the first permits arrive by growth, not by a return.
    #[tokio::test]
    async fn growth_wakes_a_waiter_that_found_no_permits() {
        let endpoint = EndpointConcurrency::new();
        let bound = endpoint.bound();
        let waiter = bound.acquire();
        tokio::pin!(waiter);
        assert!(futures::poll!(&mut waiter).is_pending(), "no permits before a generation");
        endpoint.retarget(1);
        drop(waiter.await);
        assert_eq!(issued(&endpoint), 1);
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

    // what this catches: an adapter's semaphore whose return wakes nobody, so a waiter on a full
    // cloud adapter would wait forever (its signal is shared by every adapter's semaphore).
    #[tokio::test]
    async fn an_adapter_return_wakes_its_waiter() {
        let adapter = Arc::new(Semaphore::new(1));
        let bound = DecodeBound::from(&adapter);
        let held = bound.try_acquire().expect("test: a free permit");
        let waiter = bound.acquire();
        tokio::pin!(waiter);
        assert!(futures::poll!(&mut waiter).is_pending(), "queued behind the held permit");
        drop(held);
        drop(waiter.await);
        assert_eq!(adapter.available_permits(), 1);
    }

    // what this catches (Codex, BigMama R21, 2026-11-19 13:24-13:34Z): an admitted turn waiting 610 s
    // while four dreams took the one decode permit in turn. A turn that must retake its lane before it
    // retries lost every return to a background acquire that retries at once. While a turn waits,
    // background work leaves a returned permit for it, and goes once no turn waits.
    #[tokio::test]
    async fn a_returned_permit_waits_for_the_turn_not_the_dream() {
        let endpoint = EndpointConcurrency::new();
        endpoint.retarget(1);
        let bound = endpoint.bound();
        let held = bound.try_acquire().expect("test: the one permit"); // expect: the test's own precondition
        let mut turn = None;
        let page = super::super::PageKey::mind(uuid::Uuid::new_v4()).expect("test: a mind page"); // expect: the test's own precondition
        assert!(bound.try_or_return_as(Some(page), &mut turn).is_err(), "the turn finds none");
        assert!(turn.is_some(), "and is counted waiting");
        let dream = bound.acquire();
        tokio::pin!(dream);
        assert!(futures::poll!(&mut dream).is_pending());
        drop(held);
        assert!(futures::poll!(&mut dream).is_pending(), "the returned permit is left for the turn");
        let admitted = bound.try_or_return_as(Some(page), &mut turn).ok().expect("test: the turn takes it"); // expect: the test's own precondition
        assert!(turn.is_none(), "holding a permit, she no longer waits");
        assert!(futures::poll!(&mut dream).is_pending(), "the turn holds it");
        drop(admitted);
        drop(dream.await);
        assert_eq!(available(&endpoint), 1);
    }

    // what this catches (Codex on #6159 at 411773eb20): background work that holds a lane (a keyed
    // dream, a dream on a pool without scratch) tries through try_or_return_as, and took a returned
    // permit while a turn waited. While a turn waits it is given the wait, not the permit, so it lets
    // its lane go; it takes a permit again once no turn waits.
    #[test]
    fn a_laned_dream_is_given_the_wait_while_a_turn_waits() {
        let endpoint = EndpointConcurrency::new();
        endpoint.retarget(1);
        let bound = endpoint.bound();
        let held = bound.try_acquire().expect("test: the one permit"); // expect: the test's own precondition
        let page = super::super::PageKey::mind(uuid::Uuid::new_v4()).expect("test: a mind page"); // expect: the test's own precondition
        let mut turn = None;
        assert!(bound.try_or_return_as(Some(page), &mut turn).is_err(), "the turn finds none and waits");
        drop(held);
        let mut dream = None;
        assert!(bound.try_or_return_as(None, &mut dream).is_err(), "the free permit is not the dream's while the turn waits");
        assert!(dream.is_none(), "a dream is never counted as a waiting turn");
        let admitted = bound.try_or_return_as(Some(page), &mut turn).ok().expect("test: the turn takes it"); // expect: the test's own precondition
        drop(admitted);
        assert!(bound.try_or_return_as(None, &mut dream).is_ok(), "no turn waits: the dream goes");
    }

    // what this catches (Codex on #6159): pool-less adapters sharing one waiting-turn count, so a turn
    // waiting on one adapter's semaphore held back another adapter's work.
    #[test]
    fn a_pool_less_adapter_ranks_only_its_own_waiters() {
        let mine = Arc::new(Semaphore::new(0));
        let theirs = Arc::new(Semaphore::new(1));
        let page = super::super::PageKey::mind(uuid::Uuid::new_v4()).expect("test: a mind page"); // expect: the test's own precondition
        let mut turn = None;
        assert!(DecodeBound::from(&mine).try_or_return_as(Some(page), &mut turn).is_err(), "a turn waits on mine");
        let mut other = None;
        assert!(DecodeBound::from(&theirs).try_or_return_as(None, &mut other).is_ok(), "their work is not held back by my turn");
    }
}
