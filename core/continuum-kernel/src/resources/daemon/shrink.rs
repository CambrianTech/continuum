//! Delivering the law's shrink asks to the holders of the authority's elastic leases (UNIFIED-PAGING.md
//! "Room by priority", step 3 part 6). A holder registers with the lease it may give back from; after each
//! tick the daemon collects the asks due under the governor lock, releases it, and asks each holder once.
//! A holder answers at once and never waits, with the reclaim outcome every consumer answers (one shrink
//! path, Codex): Released or Partial with the bytes freed now (applied as the lease's shrink), Deferred while
//! its fence runs (it completes through that same lease's own shrink or release, so a late or repeated answer
//! has nothing left to settle), or Refused.

use std::sync::Arc;

use super::super::consumer::ReclaimOutcome;
use super::super::ledger::{Elastic, ShrinkDue};
use super::super::lease::LeaseError;
use super::{LeaseGuard, ResourceDaemon};

/// The holder of an elastic lease: asked to give back, answering at once.
pub trait ShrinkHolder: Send + Sync {
    fn shrink(&self, due: &ShrinkDue) -> ReclaimOutcome;
}

impl ResourceDaemon {
    /// Mark `guard`'s lease elastic and register `holder` for it: the law may now ask it to give back.
    pub fn hold_elastic(&self, guard: &LeaseGuard, elastic: Elastic, holder: Arc<dyn ShrinkHolder>) -> Result<(), LeaseError> {
        let mut governor = self.governor.lock();
        governor.make_elastic(guard.lease_id(), elastic)?;
        let ask = governor.ask_of(guard.lease_id()).ok_or_else(|| LeaseError::MissingLease { lease_id: guard.lease_id().to_owned() })?;
        self.publish_locked(&governor);
        drop(governor);
        self.holders.lock().insert(ask, holder);
        Ok(())
    }

    /// Ask each holder the law has asked of, once, outside the governor lock. Bytes given back now shrink the
    /// lease at once; holders whose leases are gone leave the registry.
    pub(super) fn deliver_shrinks(&self) {
        let mut due = Vec::new();
        {
            let mut governor = self.governor.lock();
            governor.shrinks_due(&mut due);
            self.holders.lock().retain(|ask, _| governor.holds_ask(*ask));
        }
        for ask in due {
            let Some(holder) = self.holders.lock().get(&ask.ask).cloned() else {
                continuum_base::probe!(class = "resources.shrink.unheld", lease = %ask.lease_id, to = ask.to, "the law asked a lease no holder is registered for");
                continue;
            };
            let answer = holder.shrink(&ask);
            continuum_base::probe!(class = "resources.shrink.asked", lease = %ask.lease_id, kind = ?ask.kind, held = ask.held, to = ask.to, status = ?answer.status, freed = answer.freed_bytes,
                "the law asked an elastic lease to give back, and its holder answered");
            if answer.freed_bytes == 0 {
                continue; // deferred or refused: nothing given back now
            }
            let mut governor = self.governor.lock();
            if let Err(gone) = governor.apply_reclaim_outcome(&ask.lease_id, &answer) {
                continuum_base::probe!(class = "resources.shrink.stale", lease = %ask.lease_id, error = ?gone, "a given-back lease was already gone");
            }
            self.publish_locked(&governor);
        }
    }
}
