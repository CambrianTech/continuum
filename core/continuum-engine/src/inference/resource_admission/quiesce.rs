//! Quiescing admission: the one place a relaunch makes the engine idle instead of waiting to
//! sample it idle (card 2540194e). The pool's one upward relaunch (S2 debt, `pool_growth`) needs
//! an engine with nothing generating; #5742 waited for a `/slots` read showing that, and on the M5
//! on 2026-11-03 five residents on 409-token windows acted without pause, so the read never came
//! and the 2,048-token launch pool was kept for good.
//!
//! The shape: every lane request ENTERS first (one count, before either semaphore), then checks
//! the flag; a quiesce SETS the flag first, then waits for the count to reach zero. Both orders are
//! SeqCst, so no request slips between them. A request that finds admission quiescing leaves and
//! waits for the quiesce to end; one already entered runs to its end. Both waits are events (a
//! `Notify` the last leaver and the ending quiesce fire), never a poll. The entry is held by the
//! request's permit, so it ends with the call or its cancellation.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::SeqCst};

/// Admission's quiesce state, one per gate.
pub(super) struct Quiescer {
    quiescing: AtomicBool,
    /// Requests that entered admission and have not ended: queued for a lane or holding one.
    entered: AtomicUsize,
    /// Fires when a quiesce ends: a request held at entry re-enters.
    resumed: tokio::sync::Notify,
    /// Fires when the last entered request leaves while a quiesce waits.
    drained: tokio::sync::Notify,
}

impl Quiescer {
    pub(super) const fn new() -> Self {
        Self {
            quiescing: AtomicBool::new(false),
            entered: AtomicUsize::new(0),
            resumed: tokio::sync::Notify::const_new(),
            drained: tokio::sync::Notify::const_new(),
        }
    }

    /// Enter admission, held while a quiesce runs. The entry counts until it drops.
    pub(super) async fn enter(&'static self) -> AdmissionEntry {
        loop {
            // Registered before the check, so an end of the quiesce between them still wakes us.
            let resumed = self.resumed.notified();
            tokio::pin!(resumed);
            resumed.as_mut().enable();
            self.entered.fetch_add(1, SeqCst);
            if !self.quiescing.load(SeqCst) {
                return AdmissionEntry(self);
            }
            self.leave();
            resumed.await;
        }
    }

    fn leave(&self) {
        if self.entered.fetch_sub(1, SeqCst) == 1 && self.quiescing.load(SeqCst) {
            self.drained.notify_waiters();
        }
    }

    /// Hold every new admission and wait until every entered request has ended. `None` when a
    /// quiesce already runs. Dropping the result (or this future before it resolves) reopens
    /// admission.
    pub(super) async fn quiesce(&'static self) -> Option<Quiesced> {
        if self
            .quiescing
            .compare_exchange(false, true, SeqCst, SeqCst)
            .is_err()
        {
            return None;
        }
        let quiesced = Quiesced {
            owner: self,
            started: std::time::Instant::now(),
        };
        loop {
            let drained = self.drained.notified();
            tokio::pin!(drained);
            drained.as_mut().enable();
            if self.entered.load(SeqCst) == 0 {
                return Some(quiesced);
            }
            drained.await;
        }
    }

    #[cfg(test)]
    pub(super) fn entered(&self) -> usize {
        self.entered.load(SeqCst)
    }
}

/// One request's place in admission, from entry until its permit (or its cancelled wait) drops.
#[derive(Debug)]
pub struct AdmissionEntry(&'static Quiescer);

impl Drop for AdmissionEntry {
    fn drop(&mut self) {
        self.0.leave();
    }
}

impl std::fmt::Debug for Quiescer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Quiescer")
            .field("quiescing", &self.quiescing.load(SeqCst))
            .field("entered", &self.entered.load(SeqCst))
            .finish()
    }
}

/// Admission held, with nothing entered: the engine generates nothing until this drops. Its
/// owner holds it across the engine's relaunch until the new engine answers, because a request
/// released into a relaunching engine just fails (Fable on card 2540194e).
#[derive(Debug)]
pub struct Quiesced {
    owner: &'static Quiescer,
    started: std::time::Instant,
}

impl Quiesced {
    /// How long admission has been held: the measured cost of the relaunch it bought.
    pub fn held_ms(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }
}

impl Drop for Quiesced {
    fn drop(&mut self) {
        self.owner.quiescing.store(false, SeqCst);
        self.owner.resumed.notify_waiters();
        continuum_base::probe!(
            class = "admission.quiesce.ended",
            held_ms = self.held_ms(),
            "admission reopened after a quiesce"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaked() -> &'static Quiescer {
        Box::leak(Box::new(Quiescer::new()))
    }

    // what this catches (the M5, 2026-11-03: the pool's one relaunch waited for an idle sample
    // that continuous acts never gave): a quiesce that never completes under load, or one that
    // lets a request in beside it. It completes when the entered requests end, holds every new
    // one meanwhile, and reopens on drop.
    #[tokio::test]
    async fn a_quiesce_waits_out_entered_requests_and_holds_new_ones_until_it_ends() {
        let gate = leaked();
        let running = gate.enter().await;
        let mut quiesce = Box::pin(gate.quiesce());
        assert!(
            futures::poll!(quiesce.as_mut()).is_pending(),
            "a request is running: not idle yet"
        );
        let mut late = Box::pin(gate.enter());
        assert!(
            futures::poll!(late.as_mut()).is_pending(),
            "a new request is held"
        );
        assert_eq!(gate.entered(), 1, "the held request does not count");
        drop(running);
        let quiesced = quiesce.await.expect("the only quiesce");
        assert!(gate.quiesce().await.is_none(), "one quiesce at a time");
        assert!(
            futures::poll!(late.as_mut()).is_pending(),
            "still held while quiesced"
        );
        drop(quiesced);
        let entry = late.await;
        assert_eq!(gate.entered(), 1, "reopened: the held request entered");
        drop(entry);
        assert_eq!(gate.entered(), 0);
    }

    // what this catches: a cancelled wait that leaves its count behind, so a quiesce never
    // completes. A request cancelled while held, or after entering, leaves nothing counted.
    #[tokio::test]
    async fn a_cancelled_request_leaves_nothing_counted() {
        let gate = leaked();
        let entered = gate.enter().await;
        let quiesce = gate.quiesce();
        tokio::pin!(quiesce);
        assert!(futures::poll!(quiesce.as_mut()).is_pending());
        let mut held = Box::pin(gate.enter());
        assert!(futures::poll!(held.as_mut()).is_pending());
        drop(held);
        drop(entered);
        assert!(
            quiesce.await.is_some(),
            "the quiesce completes once the entered request is gone"
        );
        assert_eq!(gate.entered(), 0);
    }
}
