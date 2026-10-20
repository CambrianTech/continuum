use super::*;

#[derive(Default)]
pub(super) struct InMemorySeenContent(pub(super) Mutex<HashMap<String, Uuid>>);

impl SeenContentLookup for InMemorySeenContent {
    fn find_by_content_hash(&self, hash: &str) -> Option<Uuid> {
        self.0.lock().unwrap().get(hash).copied()
    }
}

impl InMemorySeenContent {
    pub(super) fn record(&self, hash: String, engram_id: Uuid) {
        self.0.lock().unwrap().insert(hash, engram_id);
    }
    /// Snapshot the dedup map for an eval-isolation checkpoint.
    pub(super) fn snapshot(&self) -> HashMap<String, Uuid> {
        self.0.lock().unwrap().clone()
    }
    /// Restore the dedup map to a prior snapshot (rewind a measurement window).
    pub(super) fn restore(&self, snap: HashMap<String, Uuid>) {
        *self.0.lock().unwrap() = snap;
    }
}

#[derive(Default)]
pub(super) struct InMemorySeenEvents(pub(super) Mutex<HashMap<String, u64>>);

impl SeenEventLookup for InMemorySeenEvents {
    fn first_seen_ms(&self, event_id: &str) -> Option<u64> {
        self.0.lock().unwrap().get(event_id).copied()
    }
}

impl InMemorySeenEvents {
    pub(super) fn record(&self, event_id: String, when_ms: u64) {
        self.0.lock().unwrap().insert(event_id, when_ms);
    }
    /// Snapshot the replay-protection map for an eval-isolation checkpoint.
    pub(super) fn snapshot(&self) -> HashMap<String, u64> {
        self.0.lock().unwrap().clone()
    }
    /// Restore the replay-protection map to a prior snapshot.
    pub(super) fn restore(&self, snap: HashMap<String, u64>) {
        *self.0.lock().unwrap() = snap;
    }
}

/// An opaque snapshot of a persona's entire in-memory admission state — the
/// engram store, the recall-metadata sidecar, and both the dedup + replay
/// oracles. Produced by [`AdmissionState::checkpoint`] and consumed by
/// [`AdmissionState::restore`].
///
/// Why it exists: `cognition/eval` drives the persona's REAL cognition, which
/// admits act-observation engrams as it measures her. Without a checkpoint
/// those writes (1) make absolute scores non-reproducible run-to-run, (2)
/// order-bias a paired A/B (the second arm inherits the first arm's writes),
/// and (3) pollute her durable memory. The fix is NOT to skip admission (that
/// would make the measured motion differ from a real turn — see
/// PERSONA-COGNITION-PIPELINE.md "every turn that skips admit forms no
/// memory"); it is to admit normally into a frozen frame that gets rewound
/// between arms and discarded at the end, with persistence muted so nothing
/// reaches sqlite. See [[eval-mutates-persona-lift-needs-isolation]].
pub struct AdmissionCheckpoint {
    pub(super) engrams: Vec<Engram>,
    pub(super) recall_metadata: Vec<(Uuid, crate::persona::recall_metadata::RecallMetadata)>,
    pub(super) seen_content: HashMap<String, Uuid>,
    pub(super) seen_events: HashMap<String, u64>,
}

/// Per-persona admission bundle. Holds the runner + in-memory oracles +
/// admitted-engram store. One per persona, lazy-initialized on first
/// admission attempt or eagerly in `PersonaCognition::with_budget()`.
///
/// In-memory only for v1. PR-5 will swap the oracle + engram store for
/// ORM-backed implementations without changing this struct's public API.
pub struct AdmissionState {
    pub(super) runner: InboxAdmissionRunner<HeuristicIsMemorable>,
    pub(super) seen_content: Arc<InMemorySeenContent>,
    pub(super) seen_events: Arc<InMemorySeenEvents>,
    pub(super) engrams: Mutex<Vec<Engram>>,
    /// RecallMetadata sidecar (slice 5+). When an Engram is admitted,
    /// its volatile recall state (salience, access_count, decay,
    /// novelty protection) lives here — separate from the Engram's
    /// durable content layer per the cognition-cache-hierarchy
    /// doctrine. Lock-free reads via DashMap; admission-time write
    /// happens inside record_admitted().
    pub(super) recall_metadata: Arc<crate::persona::recall_metadata::RecallMetadataRegistry>,
    /// Persistence sink — observes admissions + metadata updates.
    /// `NoopSink` by default (preserves test + replay paths); the
    /// production path uses `OrmPersistenceSink` to fire-and-forget
    /// writes through OrmStore<Engram> + OrmStore<EngramRecallMetadata>.
    /// Per [[organization-purity-as-we-migrate]] + adapter-first
    /// methodology: AdmissionState observes, sink impls choose what
    /// to do with the observations.
    ///
    /// Behind a `RwLock` so a measurement window (`cognition/eval`) can
    /// hot-swap it to a `NoopSink` and back — admit still fires (memory
    /// motion stays identical → the measurement is valid) but nothing
    /// reaches the persona's real sqlite. The lock is taken once per
    /// admit / once per recall-batch (human-message cadence, never
    /// per-token), so the cost is nil. See
    /// [[eval-mutates-persona-lift-needs-isolation]].
    pub(super) persistence: RwLock<Arc<dyn crate::persona::admission_persistence::AdmissionPersistenceSink>>,
    /// The engram edge graph — causal + associative structure over the
    /// engrams this store admits (docs/cognition/CAUSAL-MEMORY-GRAPH.md,
    /// docs/cognition/BELIEF-JUSTIFICATION-GRAPH.md). Edges are FACTS about
    /// what happened, recorded at the write site; the graph never decides —
    /// retrieval surfaces walk it, cognition judges. In-memory (DashMap);
    /// sidecar durability is a follow-on slice.
    pub(super) graph: crate::persona::engram_graph::EngramGraph,
    /// The persona this store BELONGS to (its own user id). Set once at spawn via
    /// [`set_owner_id`](Self::set_owner_id); `None` for bare/test states. Used to
    /// recognize the persona's OWN authored chat engrams (`Chat` origin whose
    /// `sender_id == owner_id`) so they can be gated out of AMBIENT semantic
    /// recall (#166): a persona's own broadcasts are proprioception-like — she
    /// said them, she knows she said them — not external knowledge, and left in
    /// the recall pool they DROWN real memory (glass-boxed 2026-07-17: 271
    /// self-authored "I ran X because Y is acting" receipts recalling at
    /// salience 0.78-0.85, teaching the survey-spin). Still explicitly
    /// queryable by keyword (`recall_by_keyword`) — this gates AMBIENT recall
    /// only, the same treatment as Tool receipts and stale inner-speech.
    pub(super) owner_id: RwLock<Option<Uuid>>,
}

impl Default for AdmissionState {
    fn default() -> Self {
        Self::new(Arc::new(
            crate::persona::recall_metadata::RecallMetadataRegistry::new(),
        ))
    }
}
