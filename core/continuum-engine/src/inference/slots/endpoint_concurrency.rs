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
use std::sync::Arc;
use tokio::sync::{Notify, OwnedSemaphorePermit, Semaphore, TryAcquireError};

/// What a waiter for a decode permit waits on: the next return, registered BEFORE the try that
/// found none, so a return between the try and the wait is never missed.
pub type PermitReturn<'a> = std::pin::Pin<Box<tokio::sync::futures::Notified<'a>>>;

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

impl From<&DecodeBound> for DecodeBound {
    fn from(bound: &DecodeBound) -> Self {
        bound.clone()
    }
}

/// How a request ranks for the decode permit. A foreground request (an act, or a call riding one)
/// outranks background work: while one waits, background work does not take a returned permit.
/// The one map from a request's class is [`super::SlotClass::decode_rank`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeRank {
    Foreground,
    Background,
}

/// One request's claim on its endpoint's decode permit: its class, and while it is refused, its
/// counted wait. Made once per admission and kept across its retries (its lane let go and taken
/// again), so a waiting foreground request is counted once, from its first refused try until it
/// holds a permit.
pub struct DecodeClaim {
    class: super::SlotClass,
    /// absent until a foreground claim is first refused, and again once it holds a permit
    waiting: Option<ForegroundWaiting>,
}

impl DecodeClaim {
    pub fn new(class: super::SlotClass) -> Self {
        Self { class, waiting: None }
    }
}

/// A foreground claim counted waiting. The last to leave wakes the background work held back for
/// it. Timed: the decode-permit wait told apart from the lane, gate and slot waits on the same act
/// (Codex, BigMama R21).
struct ForegroundWaiting {
    turns_waiting: Arc<AtomicUsize>,
    returned: Arc<Notify>,
    class: super::SlotClass,
    since: std::time::Instant,
}

impl Drop for ForegroundWaiting {
    fn drop(&mut self) {
        continuum_base::probe!(class = "inference.decode_permit.turn_waited", request = self.class.as_str(), ms = self.since.elapsed().as_millis() as u64,
            waiting = self.turns_waiting.load(Ordering::Acquire) as u64, "a foreground request's wait for the decode permit ended");
        if self.turns_waiting.fetch_sub(1, Ordering::AcqRel) == 1 {
            self.returned.notify_waiters();
        }
    }
}

impl DecodeBound {
    /// A pool-less adapter's own bound of `permits`, built ONCE by its owner and cloned to every
    /// caller: its waiting count and return notice are shared by every clone, so priority holds
    /// across all of them (Codex on #6159: a bound made per conversion ranked nobody).
    pub fn with_permits(permits: usize) -> Self {
        Self {
            permits: Arc::new(Semaphore::new(permits)),
            returned: Arc::new(Notify::new()),
            turns_waiting: Arc::new(AtomicUsize::new(0)),
            retire: None,
        }
    }

    /// Free permits now (an adapter's own bound; a pooled endpoint's circulation may hold more).
    pub fn available_permits(&self) -> usize {
        self.permits.available_permits()
    }

    /// A free permit within the endpoint's size, or `NoPermits`.
    pub fn try_acquire(&self) -> Result<DecodePermit, TryAcquireError> {
        let permit = Arc::clone(&self.permits).try_acquire_owned()?;
        Ok(DecodePermit { permit: Some(permit), returned: Arc::clone(&self.returned), retire: self.retire.clone() })
    }

    /// The ONE priority decision, for a caller holding something it must not keep across a wait (a
    /// lane): a permit now, or the wait for the next return, registered before the try. A
    /// background claim is given the wait while any foreground claim waits; a refused foreground
    /// claim is counted waiting until it holds a permit. On a wait the caller lets go of what it
    /// holds, awaits, and tries again with the same claim.
    pub fn try_claim(&self, claim: &mut DecodeClaim) -> Result<DecodePermit, PermitReturn<'_>> {
        let mut returned = Box::pin(self.returned.notified());
        returned.as_mut().enable();
        let rank = claim.class.decode_rank();
        if rank == DecodeRank::Background && self.turns_waiting.load(Ordering::Acquire) > 0 {
            return Err(returned);
        }
        match self.try_acquire() {
            Ok(permit) => {
                claim.waiting = None;
                Ok(permit)
            }
            Err(_) => {
                if rank == DecodeRank::Foreground && claim.waiting.is_none() {
                    self.turns_waiting.fetch_add(1, Ordering::AcqRel);
                    claim.waiting = Some(ForegroundWaiting {
                        turns_waiting: Arc::clone(&self.turns_waiting),
                        returned: Arc::clone(&self.returned),
                        class: claim.class,
                        since: std::time::Instant::now(),
                    });
                }
                Err(returned)
            }
        }
    }

    /// [`Self::try_claim`] until it holds a permit, for a caller holding nothing across the wait:
    /// each return wakes it to try once.
    pub async fn claim(&self, claim: &mut DecodeClaim) -> DecodePermit {
        loop {
            match self.try_claim(claim) {
                Ok(permit) => return permit,
                Err(returned) => returned.await,
            }
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
    use super::super::SlotClass;

    /// A caller of `class` waiting for a permit, holding nothing.
    async fn waiting(bound: &DecodeBound, class: SlotClass) -> DecodePermit {
        bound.claim(&mut DecodeClaim::new(class)).await
    }

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
        let waiter = waiting(&bound, SlotClass::Background);
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
        let returned = bound.try_claim(&mut DecodeClaim::new(SlotClass::Background)).err().expect("test: none free while it is held");
        let waiter = waiting(&bound, SlotClass::Background);
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
        let waiter = waiting(&bound, SlotClass::Background);
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

    // what this catches: an adapter's own bound (cloud) treated as an endpoint's, retiring its permits.
    #[test]
    fn an_adapter_bound_never_retires() {
        let bound = DecodeBound::with_permits(2);
        drop(bound.try_acquire().expect("test: a free permit")); // expect: the test's own precondition
        assert_eq!(bound.available_permits(), 2);
    }

    // what this catches: an adapter's bound whose return wakes nobody, so a waiter on a full cloud
    // adapter would wait forever.
    #[tokio::test]
    async fn an_adapter_return_wakes_its_waiter() {
        let bound = DecodeBound::with_permits(1);
        let held = bound.try_acquire().expect("test: a free permit"); // expect: the test's own precondition
        let waiter = waiting(&bound, SlotClass::Background);
        tokio::pin!(waiter);
        assert!(futures::poll!(&mut waiter).is_pending(), "queued behind the held permit");
        drop(held);
        drop(waiter.await);
        assert_eq!(bound.available_permits(), 1);
    }

    // what this catches (Codex, BigMama R21, 2026-11-19 13:24-13:34Z): an admitted turn waiting 610 s
    // while four dreams took the one decode permit in turn. A turn that must retake its lane before it
    // retries lost every return to background work that retries at once. While a foreground claim
    // waits, background work leaves a returned permit for it, and goes once none waits.
    #[tokio::test]
    async fn a_returned_permit_waits_for_the_turn_not_the_dream() {
        let endpoint = EndpointConcurrency::new();
        endpoint.retarget(1);
        let bound = endpoint.bound();
        let held = bound.try_acquire().expect("test: the one permit"); // expect: the test's own precondition
        let mut turn = DecodeClaim::new(SlotClass::Turn);
        assert!(bound.try_claim(&mut turn).is_err(), "the turn finds none");
        assert!(turn.waiting.is_some(), "and is counted waiting");
        let dream = waiting(&bound, SlotClass::Dream);
        tokio::pin!(dream);
        assert!(futures::poll!(&mut dream).is_pending());
        drop(held);
        assert!(futures::poll!(&mut dream).is_pending(), "the returned permit is left for the turn");
        let admitted = bound.try_claim(&mut turn).ok().expect("test: the turn takes it"); // expect: the test's own precondition
        assert!(turn.waiting.is_none(), "holding a permit, she no longer waits");
        assert!(futures::poll!(&mut dream).is_pending(), "the turn holds it");
        drop(admitted);
        drop(dream.await);
        assert_eq!(available(&endpoint), 1);
    }

    // what this catches (Codex on #6159 at 411773eb20): background work holding a lane (a keyed
    // dream, a dream on a pool without scratch) tried as a turn would and took a returned permit
    // while a turn waited; and a turn that named no page ranked as background. Rank comes from the
    // request's class: the laned dream is given the wait (so it lets its lane go), an unpaged turn
    // still outranks it, and the dream goes once no foreground claim waits.
    #[test]
    fn a_laned_dream_is_given_the_wait_while_any_foreground_claim_waits() {
        let endpoint = EndpointConcurrency::new();
        endpoint.retarget(1);
        let bound = endpoint.bound();
        let held = bound.try_acquire().expect("test: the one permit"); // expect: the test's own precondition
        let mut turn = DecodeClaim::new(SlotClass::Turn); // a turn, whether or not it names a page
        assert!(bound.try_claim(&mut turn).is_err(), "the turn finds none and waits");
        drop(held);
        let mut dream = DecodeClaim::new(SlotClass::Dream);
        assert!(bound.try_claim(&mut dream).is_err(), "the free permit is not the dream's while the turn waits");
        assert!(dream.waiting.is_none(), "background work is never counted waiting");
        drop(bound.try_claim(&mut turn).ok().expect("test: the turn takes it")); // expect: the test's own precondition
        assert!(bound.try_claim(&mut dream).is_ok(), "no foreground claim waits: the dream goes");
    }

    // what this catches (Codex on #6159): one adapter's priority split across conversions of its
    // semaphore (a turn waiting through one ranked nothing against a dream through another), and
    // unrelated adapters holding each other back. Every clone of one bound shares its rank; another
    // adapter's bound does not.
    #[test]
    fn an_adapter_bound_ranks_every_clone_and_no_other_adapter() {
        let mine = DecodeBound::with_permits(1);
        let held = mine.try_acquire().expect("test: the one permit"); // expect: the test's own precondition
        let mut turn = DecodeClaim::new(SlotClass::Turn);
        assert!(DecodeBound::from(&mine).try_claim(&mut turn).is_err(), "a turn waits on mine");
        drop(held);
        assert!(mine.clone().try_claim(&mut DecodeClaim::new(SlotClass::Dream)).is_err(), "a dream through another clone defers to it");
        let theirs = DecodeBound::with_permits(1);
        assert!(theirs.try_claim(&mut DecodeClaim::new(SlotClass::Dream)).is_ok(), "their work is not held back by my turn");
    }
}
