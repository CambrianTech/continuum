//! Residents an engine relaunch discards, remembered until their next turn (card 6b28a445).
//!
//! A replacement saves residents first (#5348), but three paths cannot: the engine already
//! exited, paging is quarantined (a save could not be trusted), or an adopted engine is
//! already dead. Their slots' KV is gone with the engine. The pool is rebuilt for the next
//! generation, so the record lives on the endpoint, and the resident's next turn takes it:
//! a cold turn then names the relaunch, instead of reading as a lost page or a ledger defect.

use super::*;

/// Why a relaunch discarded residents' unsaved KV.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DiscardReason {
    /// The owned engine exited before its replacement could save anything.
    EngineExited,
    /// Paging completion was uncertain, so the endpoint was quarantined and no save ran.
    PagingUncertain,
    /// The adopted engine's process was already gone when retirement looked.
    AdoptedEngineDead,
}

impl DiscardReason {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::EngineExited => "engine_exited",
            Self::PagingUncertain => "paging_uncertain",
            Self::AdoptedEngineDead => "adopted_engine_dead",
        }
    }
}

impl KvSlotPool {
    /// The minds held in a slot without a confirmed save: what a relaunch now loses. A fork
    /// dies with the core that ran it, so only minds, as in the ledger.
    pub(super) fn unsaved_residents(&self) -> Vec<PageKey> {
        // Lock order: `saved`, then `holders` (see `ledger_snapshot`).
        let saved = self.saved.lock();
        let holders = self.holders.lock();
        holders.values().filter(|key| key.as_mind().is_some() && !saved.contains(key)).copied().collect()
    }
}

impl EndpointSlots {
    /// Record that a relaunch is discarding this endpoint's unsaved residents, and say so.
    pub(crate) fn record_relaunch_discard(&self, reason: DiscardReason) {
        let pool = self.state.lock().pool.as_ref().and_then(Clone::clone);
        let lost = pool.map_or_else(Vec::new, |pool| pool.unsaved_residents());
        continuum_base::probe!(
            class = "inference.kv_page.discarded_by_relaunch",
            reason = reason.as_str(),
            residents = lost.len() as u64,
            "an engine relaunch discarded residents' unsaved KV: their next turns prefill in full"
        );
        let mut discarded = self.discarded.lock();
        for key in lost {
            discarded.insert(key, reason);
        }
    }
}

impl EndpointAdmission {
    /// Why a relaunch discarded `key`'s KV, if one did since its last turn. Taken once, so a
    /// later cold turn is never blamed on an old relaunch.
    pub(crate) fn take_relaunch_discard(&self, key: &PageKey) -> Option<DiscardReason> {
        self.endpoint.discarded.lock().remove(key)
    }
}
