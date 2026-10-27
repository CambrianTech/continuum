//! The hold on serving, published (card a18452d6): a node whose engine hosts live work says
//! what holds it and since when, instead of reading as an unexplained empty node.
use super::*;
use crate::inference::engine_residency::Occupancy;
use crate::inference::llama_server::serving_hold::ServingHold;

impl ServingDaemonModule {
    /// Publish the hold `occupancy` puts on serving: set, kept (its start preserved) or cleared
    /// on the snapshot, and emitted only when it changes. Called inside the reconcile's
    /// `ServingOperation`, the one gate every publish of a ready snapshot runs in.
    pub(super) fn publish_hold(&self, occupancy: &Occupancy) {
        let next = {
            let mut held = self.serving_hold.lock();
            let next = ServingHold::of(occupancy, held.as_ref(), EpochMs::now().millis());
            held.clone_from(&next);
            next
        };
        let changed = self.serving_tx.send_if_modified(|snapshot| {
            if snapshot.held_for == next {
                return false;
            }
            snapshot.held_for = next;
            true
        });
        if changed {
            let snapshot = self.serving_tx.borrow().clone();
            Self::emit_serving(self.bus.get(), &snapshot);
        }
    }
}
