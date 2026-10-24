//! Residents an engine relaunch discards, remembered until their next turn (card 6b28a445).
//!
//! A replacement saves residents first (#5348), but three paths cannot: the engine already
//! exited, paging is quarantined (a save could not be trusted), or an adopted engine is
//! already dead. Their slots' KV goes with the engine. The pool is rebuilt for the next
//! generation, so the record lives on the endpoint, and the resident's next turn takes it:
//! a cold turn then names the relaunch, instead of reading as a lost page or a ledger defect.
//!
//! Two steps, so a retirement that is superseded, refused, or still waiting never reports a
//! loss: the candidates are captured while the old pool is still installed
//! ([`PendingDiscard`]), and recorded only when the engine's death is OBSERVED. A sent
//! signal is not a death. An owned engine's capture waits on the endpoint for that
//! generation's [`EngineGeneration::observed_exit`]; an adopted engine's waits in the
//! process owner's retirement custody until its exact process is seen gone.

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

/// The residents one relaunch loses, and why.
#[derive(Debug)]
struct Capture {
    keys: Vec<PageKey>,
    retain: usize,
    reason: DiscardReason,
}

impl Capture {
    fn report(&self) {
        continuum_base::probe!(
            class = "inference.kv_page.discarded_by_relaunch",
            reason = self.reason.as_str(),
            residents = self.keys.len() as u64,
            "an engine relaunch discarded residents' unsaved KV: their next turns prefill in full"
        );
    }
}

/// An owned generation's capture: waiting for its exit, or already recorded.
#[derive(Debug)]
enum Watch {
    Awaiting(Capture),
    Settled,
}

/// The endpoint's discard records, oldest first. Retention is at most one pool's worth of
/// slots (the most recent discarding pool's count): one relaunch can discard no more minds
/// than it had slots, so the latest relaunch is always kept whole, and an older record is
/// evicted, oldest first, only by newer ones. An evicted mind's cold turn then reads as the
/// page store's answer, never as a relaunch it cannot be tied to.
#[derive(Debug, Default)]
pub(super) struct DiscardLog {
    entries: std::collections::VecDeque<(PageKey, DiscardReason)>,
    /// The owned generation whose relaunch captured residents, until its exit is observed.
    /// One per endpoint: the next generation cannot start before this one's exit is verified.
    watch: Option<(Uuid, Watch)>,
}

impl DiscardLog {
    fn record(&mut self, capture: &Capture) {
        // Nothing lost records nothing: an empty capture never evicts older records.
        if capture.keys.is_empty() {
            return;
        }
        for key in &capture.keys {
            self.entries.retain(|(held, _)| held != key);
            self.entries.push_back((*key, capture.reason));
        }
        while self.entries.len() > capture.retain.max(capture.keys.len()) {
            self.entries.pop_front();
        }
    }

    pub(super) fn take(&mut self, key: &PageKey) -> Option<DiscardReason> {
        let at = self.entries.iter().position(|(held, _)| held == key)?;
        self.entries.remove(at).map(|(_, reason)| reason)
    }

    /// Hold `capture` until `generation`'s exit is observed. A generation already watched
    /// (awaiting or settled) keeps its first capture, so a re-entered retirement never
    /// records twice.
    fn await_exit(&mut self, generation: Uuid, capture: Capture) {
        if self.watch.as_ref().is_some_and(|(watched, _)| *watched == generation) {
            return;
        }
        self.watch = Some((generation, Watch::Awaiting(capture)));
    }

    /// `generation` was seen gone: record what its relaunch captured, once.
    fn exit_observed(&mut self, generation: Uuid) -> Option<Capture> {
        match self.watch.take() {
            Some((watched, Watch::Awaiting(capture))) if watched == generation => {
                self.record(&capture);
                self.watch = Some((generation, Watch::Settled));
                Some(capture)
            }
            other => {
                self.watch = other;
                None
            }
        }
    }
}

/// Residents a relaunch is about to discard, captured from the pool being replaced. Dropped
/// without being recorded (a superseded or refused retirement), it records nothing.
#[must_use = "a discard is recorded only when the engine's death is observed"]
pub(crate) struct PendingDiscard {
    endpoint: Arc<EndpointSlots>,
    capture: Capture,
}

impl PendingDiscard {
    /// The same residents, lost for a different reason (the engine died before the signal).
    pub(crate) fn because(mut self, reason: DiscardReason) -> Self {
        self.capture.reason = reason;
        self
    }

    /// The engine's death was observed: record the loss and say so.
    pub(crate) fn commit(self) {
        self.capture.report();
        self.endpoint.discarded.lock().record(&self.capture);
    }

    /// An owned engine: record once `generation`'s exit is observed, now if it already was.
    pub(crate) fn on_exit_of(self, generation: &EngineGeneration) {
        self.endpoint.discarded.lock().await_exit(generation.id, self.capture);
        if generation.has_exited() {
            self.endpoint.relaunch_exit_observed(generation.id);
        }
    }

    /// A test's capture of `keys` without a pool.
    #[cfg(test)]
    pub(crate) fn for_test(endpoint: Arc<EndpointSlots>, keys: Vec<PageKey>, reason: DiscardReason) -> Self {
        let retain = keys.len();
        Self { endpoint, capture: Capture { keys, retain, reason } }
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
    /// Capture the residents the installed pool would lose if its engine went now. Nothing is
    /// recorded until the caller commits, once the engine is actually gone.
    pub(crate) fn pending_discard(self: &Arc<Self>, reason: DiscardReason) -> PendingDiscard {
        let pool = self.state.lock().pool.as_ref().and_then(Clone::clone);
        let (keys, retain) = pool.map_or((Vec::new(), 0), |pool| (pool.unsaved_residents(), pool.n_slots() as usize));
        PendingDiscard { endpoint: Arc::clone(self), capture: Capture { keys, retain, reason } }
    }

    /// An owned generation's exit was observed: record what its relaunch captured, if any.
    pub(super) fn relaunch_exit_observed(&self, generation: Uuid) {
        let recorded = self.discarded.lock().exit_observed(generation);
        if let Some(capture) = recorded {
            capture.report();
        }
    }
}

impl EndpointAdmission {
    /// Why a relaunch discarded `key`'s KV, if one did since its last turn. Taken once, so a
    /// later cold turn is never blamed on an old relaunch.
    pub(crate) fn take_relaunch_discard(&self, key: &PageKey) -> Option<DiscardReason> {
        self.endpoint.discarded.lock().take(key)
    }
}
