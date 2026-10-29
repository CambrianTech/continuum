//! What of her results' bodies stays in RAM: the one memory owner decides.
//!
//! A result's body stays resident until the memory pressure monitor asks the mind-results
//! reporter to shed (`persona_workspace::mind_results_reporter`), never by a size cap at
//! receipt or restore: RAM decides, not a number (Joel, 2026-10-29). A shed moves the oldest
//! saved bodies to their handles; each keeps its measured length, so her view still prices its
//! whole level and loads it when the allocation grants it (FOCUS-CONTRACT rule 7,
//! `continuum_context::StoredWhole`). The newest result always stays, and so does any body
//! whose saved output is not there to reload: it is the last copy.

use super::*;

/// One resident body a shed may move to its handle.
#[derive(Debug, Clone)]
pub(crate) struct EvictableResult {
    /// When it was received; `None` (an older build's record) sheds first.
    pub received_ms: Option<u64>,
    pub seq: u64,
    pub bytes: u64,
}

/// One mind's resident result bodies, as the memory owner accounts them.
#[derive(Debug, Clone)]
pub(crate) struct Residency {
    /// Bytes of every resident body.
    pub resident_bytes: u64,
    /// Bytes no shed may move: the newest result, and every body whose saved output is not
    /// there to reload (unsaved, or its output missing).
    pub pinned_bytes: u64,
    /// What a shed may move, oldest first.
    pub evictable: Vec<EvictableResult>,
}

impl WorkingMemory {
    /// The RAM her resident result bodies hold, in bytes. Cheap: one pass, no allocation.
    pub(crate) fn resident_result_bytes(&self) -> u64 {
        self.actions.lock().resident_bytes()
    }

    /// Her resident bodies split into what a shed may move and what it may not. `saved` says a
    /// handle's output is there to reload; the caller owns that check (it knows her persona).
    pub(crate) fn residency(&self, saved: &dyn Fn(&str) -> bool) -> Residency {
        let actions = self.actions.lock();
        let resident_bytes = actions.resident_bytes();
        let evictable: Vec<EvictableResult> = actions
            .evictable(saved)
            .map(|record| EvictableResult {
                received_ms: record.received_ms,
                seq: record.seq,
                bytes: record.resident_bytes(),
            })
            .collect();
        let movable: u64 = evictable.iter().map(|result| result.bytes).sum();
        Residency {
            resident_bytes,
            pinned_bytes: resident_bytes - movable,
            evictable,
        }
    }

    /// THE eviction verb: move the oldest saved bodies to their handles until at most `budget`
    /// bytes stay resident. Only the memory owner's shed calls it in production. A body whose
    /// output `saved` cannot vouch for stays, and so does the newest; past the budget after
    /// that, the overrun is reported, never forced.
    pub(crate) fn evict_resident_results_over(&self, budget: u64, saved: &dyn Fn(&str) -> bool) {
        let total = self.actions.lock().collapse_over(budget, saved).total;
        self.publish_live_handles();
        if total > budget {
            continuum_base::probe!(
                class = "wm.results.last_copies_over_allocation",
                total = %total,
                budget = %budget,
                "results with no saved output to reload stay resident over the shed's allocation"
            );
        }
    }
}
