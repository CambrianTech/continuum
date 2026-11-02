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

/// One resident body a shed may move to its handle, if its saved output is there to reload
/// (the caller checks that outside her lock).
#[derive(Debug, Clone)]
pub(crate) struct ShedCandidate {
    /// When it was received; `None` (an older build's record) sheds first.
    pub received_ms: Option<u64>,
    pub seq: u64,
    pub handle: String,
    pub bytes: u64,
}

/// One mind's resident result bodies, read under her lock with no filesystem access.
#[derive(Debug, Clone)]
pub(crate) struct ResidentResults {
    /// Bytes of every record: resident bodies, plus the lines and summaries no shed releases.
    pub resident_bytes: u64,
    /// Saved bodies other than the newest, oldest first: what a shed may move once each
    /// candidate's output is vouched for.
    pub candidates: Vec<ShedCandidate>,
}

impl WorkingMemory {
    /// The RAM her action records hold, in bytes: bodies, lines and summaries. Cheap: one pass, no allocation.
    pub(crate) fn resident_result_bytes(&self) -> u64 {
        self.actions.lock().resident_bytes()
    }

    /// Her resident bodies and the saved ones a shed could move. Only memory is read under the
    /// lock: whether each handle's output is there is the caller's check, made after it drops.
    pub(crate) fn resident_results(&self) -> ResidentResults {
        let actions = self.actions.lock();
        let candidates = actions
            .evictable(&|_, _| true)
            .filter_map(|record| {
                Some(ShedCandidate {
                    received_ms: record.received_ms,
                    seq: record.seq,
                    handle: record.handle.clone()?,
                    bytes: record.body_bytes(),
                })
            })
            .collect();
        ResidentResults {
            resident_bytes: actions.resident_bytes(),
            candidates,
        }
    }

    /// A copy of one resident body, if its record still names `handle`: what the memory owner
    /// proves its saved copy against, outside the lock (the lock is held only for the copy).
    pub(crate) fn resident_body(&self, seq: u64, handle: &str) -> Option<String> {
        let actions = self.actions.lock();
        let record = actions.get(seq)?;
        if record.handle.as_deref() != Some(handle) {
            return None;
        }
        record.result.clone()
    }

    /// THE eviction verb: move the oldest saved bodies to their handles until at most `budget`
    /// bytes stay resident. Only the memory owner's shed calls it in production, with
    /// `saved(seq, handle)` answering from the copies it proved outside the lock, so a record
    /// that changed since (a different handle, or gone) is not moved. No filesystem access
    /// happens under the lock. The newest stays, and
    /// past the budget after that the overrun is reported, never forced.
    pub(crate) fn evict_resident_results_over(
        &self,
        budget: u64,
        saved: &dyn Fn(u64, &str) -> bool,
    ) {
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
