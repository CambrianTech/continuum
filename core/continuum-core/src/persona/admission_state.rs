//! Per-Persona Admission State (continuum#1121 PR-4)
//!
//! Owns the per-persona admission machinery + the in-memory side-effect
//! stores that turn the stateless runner from PR-3 into a stateful loop.
//! This is the bridge between the IPC layer (`cognition/admit-inbox-message`)
//! and the pure-Rust admission gate from PRs 1-3.
//!
//! # What ships
//!
//! - [`AdmissionState`] — bundles a `InboxAdmissionRunner<HeuristicIsMemorable>`
//!   plus in-memory `SeenContentLookup` + `SeenEventLookup` impls plus a
//!   simple `Vec<Engram>` admitted-engram store. One per persona, owned by
//!   `PersonaCognition` (see `persona::unified`).
//! - `admit(message, trace)` — runs the full pipeline AND records the
//!   side-effects (admitted engram added to store, content_hash recorded
//!   for dedup, AIRC event_id recorded for replay protection).
//! - Read-only inspection: `engram_count()`, `engram_at()`,
//!   `is_content_seen()`, `is_event_seen()` — for tests + future recall
//!   surface (PR-5+).
//!
//! # What this PR does NOT ship (deferred)
//!
//! - **ORM persistence.** Engrams stay in-memory for v1. PR-5 swaps in
//!   ORM-backed lookups + the entity registry path so admitted engrams
//!   survive restarts.
//! - **Recall surface.** Reading admitted engrams back out is just
//!   `engram_at(idx)` for v1. PR-5+ adds a typed query API.
//! - **Quarantine store.** `Quarantine` decisions don't actually quarantine
//!   anywhere; the engram is dropped on the floor for now. (Replay
//!   protection still records the event_id, which is correct.) PR-5+ adds
//!   the quarantine store.
//! - **Per-persona config customization.** All personas use the same
//!   `default_v1()` runner config in this PR. Config-per-persona ships
//!   when the IPC layer needs it.
//!
//! # Concurrency
//!
//! `AdmissionState` is `Send + Sync`. Internal mutability via `Mutex` so
//! the struct can be borrowed immutably (`&AdmissionState`) and called
//! concurrently from per-persona task tasks. Same shape as `PersonaInbox`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

use uuid::Uuid;

use super::admission::{HeuristicIsMemorable, SeenContentLookup, SeenEventLookup};
use super::engram::{
    AdmissionDecision, AdmissionDropReason, AdmissionError, Engram, EngramOrigin,
};
use super::inbox_admission::{content_hash_sha256, InboxAdmissionRunner};
use super::trace::CognitionTrace;
use super::types::InboxMessage;

//=============================================================================
// IN-MEMORY ORACLES (private, used by AdmissionState)
//=============================================================================

#[derive(Default)]
struct InMemorySeenContent(Mutex<HashMap<String, Uuid>>);

impl SeenContentLookup for InMemorySeenContent {
    fn find_by_content_hash(&self, hash: &str) -> Option<Uuid> {
        self.0.lock().unwrap().get(hash).copied()
    }
}

impl InMemorySeenContent {
    fn record(&self, hash: String, engram_id: Uuid) {
        self.0.lock().unwrap().insert(hash, engram_id);
    }
    /// Snapshot the dedup map for an eval-isolation checkpoint.
    fn snapshot(&self) -> HashMap<String, Uuid> {
        self.0.lock().unwrap().clone()
    }
    /// Restore the dedup map to a prior snapshot (rewind a measurement window).
    fn restore(&self, snap: HashMap<String, Uuid>) {
        *self.0.lock().unwrap() = snap;
    }
}

#[derive(Default)]
struct InMemorySeenEvents(Mutex<HashMap<String, u64>>);

impl SeenEventLookup for InMemorySeenEvents {
    fn first_seen_ms(&self, event_id: &str) -> Option<u64> {
        self.0.lock().unwrap().get(event_id).copied()
    }
}

impl InMemorySeenEvents {
    fn record(&self, event_id: String, when_ms: u64) {
        self.0.lock().unwrap().insert(event_id, when_ms);
    }
    /// Snapshot the replay-protection map for an eval-isolation checkpoint.
    fn snapshot(&self) -> HashMap<String, u64> {
        self.0.lock().unwrap().clone()
    }
    /// Restore the replay-protection map to a prior snapshot.
    fn restore(&self, snap: HashMap<String, u64>) {
        *self.0.lock().unwrap() = snap;
    }
}

//=============================================================================
// EVAL-ISOLATION CHECKPOINT
//=============================================================================

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
    engrams: Vec<Engram>,
    recall_metadata: Vec<(Uuid, crate::persona::recall_metadata::RecallMetadata)>,
    seen_content: HashMap<String, Uuid>,
    seen_events: HashMap<String, u64>,
}

//=============================================================================
// ADMISSION STATE
//=============================================================================

/// Per-persona admission bundle. Holds the runner + in-memory oracles +
/// admitted-engram store. One per persona, lazy-initialized on first
/// admission attempt or eagerly in `PersonaCognition::with_budget()`.
///
/// In-memory only for v1. PR-5 will swap the oracle + engram store for
/// ORM-backed implementations without changing this struct's public API.
pub struct AdmissionState {
    runner: InboxAdmissionRunner<HeuristicIsMemorable>,
    seen_content: Arc<InMemorySeenContent>,
    seen_events: Arc<InMemorySeenEvents>,
    engrams: Mutex<Vec<Engram>>,
    /// RecallMetadata sidecar (slice 5+). When an Engram is admitted,
    /// its volatile recall state (salience, access_count, decay,
    /// novelty protection) lives here — separate from the Engram's
    /// durable content layer per the cognition-cache-hierarchy
    /// doctrine. Lock-free reads via DashMap; admission-time write
    /// happens inside record_admitted().
    recall_metadata: Arc<crate::persona::recall_metadata::RecallMetadataRegistry>,
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
    persistence: RwLock<Arc<dyn crate::persona::admission_persistence::AdmissionPersistenceSink>>,
}

impl Default for AdmissionState {
    fn default() -> Self {
        Self::new(Arc::new(
            crate::persona::recall_metadata::RecallMetadataRegistry::new(),
        ))
    }
}

impl AdmissionState {
    /// Construct fresh admission state with the v1 default recipe + permissive
    /// trust mapping. `recall_metadata` is the per-persona sidecar registry
    /// that tracks volatile recall state for every admitted Engram. Per the
    /// no-backwards-compat doctrine (slice 5+), the constructor now requires
    /// the registry rather than minting one internally — this lets
    /// PersonaCognition share a single registry view across admission +
    /// recall + decay tick subsystems.
    pub fn new(
        recall_metadata: Arc<crate::persona::recall_metadata::RecallMetadataRegistry>,
    ) -> Self {
        Self::new_with_persistence(
            recall_metadata,
            crate::persona::admission_persistence::NoopSink::arc(),
        )
    }

    /// Construct AdmissionState with an explicit persistence sink.
    /// Production uses `OrmPersistenceSink::arc(engram_store,
    /// metadata_store)`; tests use `RecordingSink::arc()` or
    /// `NoopSink::arc()`. The sink observes every admission +
    /// metadata update and chooses what to do with them
    /// (fire-and-forget write, buffer, no-op, etc.).
    pub fn new_with_persistence(
        recall_metadata: Arc<crate::persona::recall_metadata::RecallMetadataRegistry>,
        persistence: Arc<dyn crate::persona::admission_persistence::AdmissionPersistenceSink>,
    ) -> Self {
        Self {
            runner: InboxAdmissionRunner::default_v1(),
            seen_content: Arc::new(InMemorySeenContent::default()),
            seen_events: Arc::new(InMemorySeenEvents::default()),
            engrams: Mutex::new(Vec::new()),
            recall_metadata,
            persistence: RwLock::new(persistence),
        }
    }

    /// Construct AdmissionState pre-populated with engrams + metadata
    /// loaded from disk. Used at persona boot — rehydrates the Vec +
    /// DashMap from the previous lifetime's persistence layer.
    ///
    /// Per the substrate's continual-learning property: every admit
    /// from now on rides on top of the loaded state; recall scoring
    /// works against the rehydrated salience values; the Hebbian
    /// rehearsal loop continues across the restart boundary.
    pub fn new_rehydrated(
        recall_metadata: Arc<crate::persona::recall_metadata::RecallMetadataRegistry>,
        persistence: Arc<dyn crate::persona::admission_persistence::AdmissionPersistenceSink>,
        loaded_engrams: Vec<Engram>,
        loaded_metadata: Vec<(Uuid, crate::persona::recall_metadata::RecallMetadata)>,
    ) -> Self {
        // Populate the metadata registry with the loaded snapshots
        // BEFORE building the Vec — recall scoring later reads
        // metadata via the registry, not via Engram fields.
        for (engram_id, metadata) in loaded_metadata {
            recall_metadata.admit(engram_id, metadata);
        }
        // Phantom-engram backfill (Slice B fix for review #1519,
        // task #171): an engram on disk WITHOUT a metadata row would
        // otherwise be permanently invisible to recall — `recall_scored`
        // filter_maps it out, so `record_recall_hit` never fires for it,
        // so the metadata row never gets created. The original inline
        // comment "next decay tick will resurface this" was wrong (decay
        // walks the registry, not the engrams table). Now we seed
        // default metadata for any loaded engram that lacks a row, so
        // every engram is recall-visible after rehydration regardless
        // of what crashed mid-write in the prior lifetime.
        for engram in &loaded_engrams {
            recall_metadata.admit_with_defaults(engram.id);
        }
        Self {
            runner: InboxAdmissionRunner::default_v1(),
            seen_content: Arc::new(InMemorySeenContent::default()),
            seen_events: Arc::new(InMemorySeenEvents::default()),
            engrams: Mutex::new(loaded_engrams),
            recall_metadata,
            persistence: RwLock::new(persistence),
        }
    }

    /// Borrow the shared recall metadata registry. Recall + decay tick
    /// subsystems clone this Arc for their own reads/writes — they
    /// observe the same DashMap admission writes into.
    pub fn recall_metadata(
        &self,
    ) -> &Arc<crate::persona::recall_metadata::RecallMetadataRegistry> {
        &self.recall_metadata
    }

    /// Construct AdmissionState for a specific persona — opens the
    /// per-persona SQLite at `<home>/engrams.sqlite`, wires up
    /// `OrmStore<Engram>` + `OrmStore<EngramRecallMetadata>`, builds
    /// the production `OrmPersistenceSink`, rehydrates the in-memory
    /// Vec + DashMap from disk, and returns the configured state.
    ///
    /// **The persona-scoped entry point for the chain-of-custody
    /// architecture.** All future per-persona substrate work that
    /// needs persistence (signing keys, Merkle chain heads, future
    /// per-collection databases) hangs off the same `PersonaHome`
    /// the engrams DB lives in. One home = one citizen's complete
    /// on-disk surface.
    ///
    /// Per [[entity-chain-of-custody-vision]]: this is slice 1 of
    /// the multi-slice arc. Subsequent slices add author_peer_id +
    /// content_hash, signing, Merkle chain head caching, and
    /// airc-native entity envelope emission — all riding on top of
    /// this same PersonaHome resolution.
    pub async fn for_persona(
        home: &crate::persona::home::PersonaHome,
        recall_metadata: Arc<crate::persona::recall_metadata::RecallMetadataRegistry>,
    ) -> Result<Self, crate::orm::OrmStoreError> {
        use crate::orm::adapter::{AdapterConfig, StorageAdapter};
        use crate::orm::sqlite::SqliteAdapter;
        use crate::orm::OrmStore;
        use crate::persona::admission_persistence::{
            AdmissionPersistenceLoader, OrmLoader, OrmPersistenceSink,
        };
        use crate::persona::engram::Engram;
        use crate::persona::recall_metadata::EngramRecallMetadata;

        // Ensure the persona's home directory exists. fs::create_dir_all
        // is idempotent and safe to call on every boot.
        home.ensure_exists().map_err(|e| {
            crate::orm::OrmStoreError::AdapterFailed {
                operation: "ensure_persona_home",
                collection: "engrams".to_string(),
                detail: format!(
                    "failed to ensure persona home dir {}: {}",
                    home.root().display(),
                    e
                ),
            }
        })?;

        // Open the per-persona SQLite. The adapter handles WAL + FK
        // pragmas etc; we just hand it the path.
        let mut adapter = SqliteAdapter::new();
        let mut config = AdapterConfig::default();
        config.connection_string = home.engrams_db().to_string_lossy().into_owned();
        adapter.initialize(config).await.map_err(|e| {
            crate::orm::OrmStoreError::AdapterFailed {
                operation: "initialize",
                collection: "engrams".to_string(),
                detail: e,
            }
        })?;
        let adapter: Arc<dyn StorageAdapter> = Arc::new(adapter);

        // Build the typed stores. Each ensure_schema runs on
        // construction (idempotent).
        let engram_store = Arc::new(OrmStore::<Engram>::new(Arc::clone(&adapter)).await?);
        let metadata_store =
            Arc::new(OrmStore::<EngramRecallMetadata>::new(Arc::clone(&adapter)).await?);

        // Build the production sink as the concrete type so we can
        // prime its engram_id → row_id cache BEFORE losing it behind
        // the trait object. Without the prime, the first
        // observe_metadata_update for a rehydrated engram would log a
        // "row not found" miss; with the prime, it's an O(1) typed
        // update.
        let sink_concrete = Arc::new(OrmPersistenceSink::new(
            Arc::clone(&engram_store),
            Arc::clone(&metadata_store),
        ));
        let loader = OrmLoader::new(engram_store, metadata_store);

        // Rehydrate from disk — engrams + metadata that the previous
        // lifetime persisted come back into memory before this
        // AdmissionState handles its first new admit. The row_id
        // pairs prime the sink's cache so post-rehydration metadata
        // updates use the existing rows (no UNIQUE-race, no full
        // table scan).
        let (engrams, metadata, row_id_pairs) = loader.load_with_row_ids().await?;
        sink_concrete.prime_cache(row_id_pairs);

        Ok(Self::new_rehydrated(
            recall_metadata,
            sink_concrete as Arc<dyn crate::persona::admission_persistence::AdmissionPersistenceSink>,
            engrams,
            metadata,
        ))
    }

    /// Run the admission pipeline on one inbox message, recording all
    /// side-effects (admitted engram → store + content_hash dedup record;
    /// any signed origin → event_id replay record).
    ///
    /// Returns the typed `AdmissionDecision` (Admit/Drop/Quarantine) or a
    /// typed `AdmissionError`. Trace gets one `SEAM_ADMISSION` entry per
    /// call (success + every error path) — same forensic invariant as
    /// `AdmissionGate::admit`.
    pub fn admit(
        &self,
        message: &InboxMessage,
        trace: Option<&mut CognitionTrace>,
    ) -> Result<AdmissionDecision, AdmissionError> {
        let decision = self.runner.admit(
            message,
            self.seen_content.as_ref(),
            self.seen_events.as_ref(),
            trace,
        )?;
        self.record_side_effects(&decision);
        Ok(decision)
    }

    /// Override the recall salience of an already-admitted engram, preserving its
    /// other metadata (notably `last_decayed_ms`, so this never triggers the
    /// epoch-delta decay-collapse `admit_with_defaults` guards against).
    ///
    /// Used to DOWN-WEIGHT proprioception (#166): an action-observation receipt
    /// ("code/list(...) → ok") is admitted as an Episodic engram so the mind can
    /// remember what it did, but it is NOT durable knowledge — at neutral salience
    /// it out-competes genuine findings in recall (recency-heavy), so recall
    /// echoes the persona's own recent tool-chatter back at it instead of useful
    /// memory. A lower salience keeps the receipt recallable (for "what did I just
    /// do") without letting it dominate. Not a content heuristic that steers
    /// output — a storage-tier weight on a structurally-known kind (a receipt),
    /// the recall-side twin of the recency-vs-recall channel split in act_observe.
    pub fn set_recall_salience(&self, engram_id: uuid::Uuid, salience: f32) {
        let mut meta = self.recall_metadata.get(engram_id).unwrap_or_default();
        meta.salience = salience;
        self.recall_metadata.admit(engram_id, meta);
    }

    /// Admit a SELF-PRODUCED engram — a memory the persona generated ABOUT
    /// ITSELF (dream-consolidated `Semantic` facts, `SelfReflection`
    /// meta-cognition), NOT a message that arrived off the wire.
    ///
    /// This is the `SelfTrust` counterpart to [`admit`](Self::admit). It runs
    /// the same store side-effects — dedup record, recall-metadata seed,
    /// fire-and-forget persistence, in-memory push — but deliberately SKIPS the
    /// external admission gate (`runner.admit`'s envelope verification,
    /// trust-boundary check, and wire-replay protection). That gate exists to
    /// decide whether to trust DATA FROM ANOTHER PARTY; a fact the persona
    /// distilled from its own already-admitted episodic memories has no external
    /// envelope to verify and is, by construction, `SelfTrust`. Skipping the
    /// gate here is a DISTINCT legitimate ingestion path, not a bypass of a
    /// safety check — the path is named, not hidden
    /// (`[[fallbacks-are-illegal-fail-loud]]`).
    ///
    /// Idempotent: identical fact content (same `content_hash_sha256`) is
    /// dropped as `Duplicate`, so a dream that re-distills the same cluster on a
    /// later tick does not accumulate duplicate facts. The caller MUST set the
    /// engram's `kind`/`origin`/`trust_state_at_admission` to the self-produced
    /// shape (e.g. `Semantic` + `SelfReflection` + `SelfTrust`); this method
    /// records, it does not synthesize them.
    pub fn admit_reflection(
        &self,
        engram: Engram,
    ) -> Result<AdmissionDecision, AdmissionError> {
        let hash = content_hash_sha256(&engram.content);
        if let Some(existing_engram_id) = self.seen_content.find_by_content_hash(&hash) {
            // Idempotent dream: this exact fact is already engrammed.
            return Ok(AdmissionDecision::Drop {
                reason: AdmissionDropReason::Duplicate { existing_engram_id },
            });
        }

        // Record the dedup pointer + recall metadata BEFORE the store push, so
        // a concurrent re-admit of identical content loses the race cleanly
        // (the dedup map points at this engram). Mirrors `record_admitted` +
        // `record_side_effects` for the external path.
        self.seen_content.record(hash, engram.id);
        self.recall_metadata.admit_with_defaults(engram.id);
        let metadata = self.recall_metadata.get(engram.id).unwrap_or_default();
        self.persistence
            .read()
            .unwrap()
            .observe_admission(&engram, metadata);
        self.engrams.lock().unwrap().push(engram.clone());

        Ok(AdmissionDecision::Admit {
            engram,
            why: "self-produced reflection admitted (SelfTrust, no external envelope)"
                .to_string(),
        })
    }

    /// Apply the decision's side-effects to the stores. Pulled out so the
    /// admission path stays linear and testable.
    ///
    /// **Quarantine subtlety (claude-tab-2 review nit on #1155):** v1 has
    /// no quarantine store, so a Quarantined engram gets dropped on the
    /// floor. Recording its `content_hash` in `seen_content` would leave
    /// a dangling pointer — future dedup hits would return an
    /// `existing_engram_id` that can't be looked up. So Quarantine ONLY
    /// records the `event_id` (replay protection — the load-bearing
    /// behaviour for `AdmissionError::ReplayDetected`). Once PR-5+ adds
    /// a real quarantine store, the engram lands somewhere lookup-able
    /// and content_hash recording can come back.
    fn record_side_effects(&self, decision: &AdmissionDecision) {
        match decision {
            AdmissionDecision::Admit { engram, .. } => {
                self.record_admitted(engram);
                self.engrams.lock().unwrap().push(engram.clone());
                // Observe the admission through the persistence sink.
                // NoopSink (default) does nothing; OrmPersistenceSink
                // fires-and-forgets the disk write through tokio::spawn.
                // The metadata snapshot reflects the just-admitted
                // default state (admit_with_defaults above).
                let metadata = self
                    .recall_metadata
                    .get(engram.id)
                    .unwrap_or_default();
                self.persistence
                    .read()
                    .unwrap()
                    .observe_admission(engram, metadata);
            }
            AdmissionDecision::Quarantine { engram, .. } => {
                // Replay-only recording — see method-doc Quarantine note.
                self.record_replay_only(engram);
            }
            AdmissionDecision::Drop { .. } => {
                // Pure drop. No side-effect — by design, dropped messages
                // shouldn't bias future dedup or replay decisions.
            }
        }
    }

    /// Full recording for an admitted engram: content_hash → engram_id
    /// (dedup) PLUS, for AIRC origins, event_id → timestamp (replay).
    /// Use only when the engram is actually being stored, otherwise the
    /// dedup pointer dangles.
    fn record_admitted(&self, engram: &Engram) {
        match &engram.origin {
            EngramOrigin::Chat(r) => {
                self.seen_content.record(r.content_hash.clone(), engram.id);
            }
            EngramOrigin::Airc(r) => {
                self.seen_content.record(r.content_hash.clone(), engram.id);
                self.seen_events
                    .record(r.message_id.clone(), engram.admitted_at_ms);
            }
            EngramOrigin::Tool(_) | EngramOrigin::SelfReflection { .. } => {
                // Tool + SelfReflection origins don't carry a content_hash
                // string on a uniform field — dedup for those paths lands
                // when the tool/reflection ingestion converters land
                // (later PR). For now the admit path doesn't synthesize
                // these origins from the inbox path.
            }
        }

        // Slice 6 wiring: mirror this engram into the RecallMetadata
        // sidecar so the cache hierarchy starts tracking salience,
        // access count, decay timing, and novelty protection. Initial
        // metadata is the neutral default; slice 7+ will plug in the
        // novelty detector (embedding distance × magnitude) to set
        // scored initial salience + protection windows at this same
        // call site.
        self.recall_metadata.admit_with_defaults(engram.id);
    }

    /// Replay-only recording for a Quarantined engram: event_id → timestamp
    /// for AIRC origins (so a duplicate quarantined event doesn't re-fire
    /// admission). Skips content_hash because v1 doesn't actually store
    /// quarantined engrams; recording dedup pointers to dropped engrams
    /// would leave dangling `existing_engram_id` references in
    /// `AdmissionDropReason::Duplicate` results.
    fn record_replay_only(&self, engram: &Engram) {
        if let EngramOrigin::Airc(r) = &engram.origin {
            self.seen_events
                .record(r.message_id.clone(), engram.admitted_at_ms);
        }
        // Chat / Tool / SelfReflection origins have no replay surface
        // distinct from content dedup, so quarantine of those origins
        // records nothing here. PR-5's quarantine store will revisit.
    }

    //--- read-only inspection (for tests + future recall surface) -----------

    /// Number of admitted engrams currently in this persona's store.
    pub fn engram_count(&self) -> usize {
        self.engrams.lock().unwrap().len()
    }

    /// Borrow an admitted engram by index (for inspection / future recall).
    /// Returns None if index out of bounds. Clone is cheap in v1; PR-5+
    /// recall will return `&Engram` borrowed from a longer-lived store.
    pub fn engram_at(&self, idx: usize) -> Option<Engram> {
        self.engrams.lock().unwrap().get(idx).cloned()
    }

    /// **Test-only**: push an engram directly into the store without
    /// running the admission pipeline. Used by sibling modules' tests
    /// (e.g., `engram_source.rs`) to inject deterministic fixture
    /// engrams without constructing a full inbox-message + admission
    /// flow. Per crate-test visibility, this is callable from any
    /// test elsewhere in continuum-core but NOT from production code
    /// (the cfg gate ensures it doesn't appear in non-test builds).
    #[cfg(test)]
    pub fn push_for_test(&self, engram: Engram) {
        self.engrams.lock().unwrap().push(engram);
    }

    /// True iff `content_hash` is recorded as seen in the dedup store.
    pub fn is_content_seen(&self, content_hash: &str) -> bool {
        self.seen_content
            .find_by_content_hash(content_hash)
            .is_some()
    }

    /// True iff the AIRC event_id is recorded in the replay-protection store.
    pub fn is_event_seen(&self, event_id: &str) -> bool {
        self.seen_events.first_seen_ms(event_id).is_some()
    }

    /// Borrow the runner — useful for tests + introspection of per-persona
    /// config (recipe id, trust thresholds, etc.).
    pub fn runner(&self) -> &InboxAdmissionRunner<HeuristicIsMemorable> {
        &self.runner
    }

    //=========================================================================
    // EVAL ISOLATION (checkpoint / restore / persistence mute)
    //=========================================================================
    //
    // Lets a measurement (`cognition/eval`) run the persona's REAL admission
    // motion without leaving a trace: snapshot here, rewind between A/B arms,
    // mute persistence so nothing lands in sqlite. See
    // [[eval-mutates-persona-lift-needs-isolation]].

    /// Snapshot the full in-memory admission state — engram store, recall
    /// metadata, and both oracles — for a later [`restore`](Self::restore).
    pub fn checkpoint(&self) -> AdmissionCheckpoint {
        AdmissionCheckpoint {
            engrams: self.engrams.lock().unwrap().clone(),
            recall_metadata: self.recall_metadata.snapshot(),
            seen_content: self.seen_content.snapshot(),
            seen_events: self.seen_events.snapshot(),
        }
    }

    /// Restore the in-memory admission state to a prior checkpoint, discarding
    /// every engram / metadata / dedup-record admitted since it was taken.
    /// Does NOT touch the persistence sink — mute that separately with
    /// [`swap_persistence`](Self::swap_persistence) so a restore can't race a
    /// disk write back in.
    pub fn restore(&self, cp: &AdmissionCheckpoint) {
        *self.engrams.lock().unwrap() = cp.engrams.clone();
        self.recall_metadata.restore(cp.recall_metadata.clone());
        self.seen_content.restore(cp.seen_content.clone());
        self.seen_events.restore(cp.seen_events.clone());
    }

    /// Fork a fully DETACHED copy of this admission frame — same engrams,
    /// salience, and dedup oracles as right now, but sharing NO mutable state
    /// with the original. The copy gets a FRESH `RecallMetadataRegistry` (so the
    /// fork's Hebbian recall-hits + decay land on the copy, never bumping HER
    /// live salience) and the default `NoopSink` (so nothing the fork admits ever
    /// reaches her sqlite). The runner + seen oracles are independent instances.
    ///
    /// This is the welfare primitive for `cognition/eval`: the exam runs on the
    /// fork while the LIVING persona keeps living — heartbeat beating, present in
    /// the room, never frozen or anesthetized to be measured. See
    /// [[design-the-persona-as-a-being]] + [[eval-mutates-persona-lift-needs-isolation]].
    pub fn fork_detached(&self) -> AdmissionState {
        let cp = self.checkpoint();
        let fork = Self::new(Arc::new(
            crate::persona::recall_metadata::RecallMetadataRegistry::new(),
        ));
        fork.restore(&cp);
        fork
    }

    /// Hot-swap the persistence sink, returning the previous one. The
    /// eval-isolation window swaps in a `NoopSink` (admit still fires; nothing
    /// reaches the persona's real sqlite) and swaps the real sink back when the
    /// measurement ends. The lock is taken once per swap (twice per eval), not
    /// on any hot path.
    pub fn swap_persistence(
        &self,
        sink: Arc<dyn crate::persona::admission_persistence::AdmissionPersistenceSink>,
    ) -> Arc<dyn crate::persona::admission_persistence::AdmissionPersistenceSink> {
        let mut guard = self.persistence.write().unwrap();
        std::mem::replace(&mut *guard, sink)
    }

    //=========================================================================
    // RECALL SURFACE (continuum#1121 PR-5)
    //=========================================================================
    //
    // Read-side query API on the admitted-engram store. v1 backs against
    // the in-memory `Vec<Engram>` from PR-4; PR-6+ swaps in an ORM-backed
    // store without changing this API. Pattern is the same as how
    // `cv::Algorithm` exposes a stable interface over swappable backends.

    /// Recall the most recent N admitted engrams, newest first. Returns
    /// at most `limit` engrams. `limit == 0` returns an empty Vec.
    ///
    /// "Newest first" = reverse insertion order in the in-memory v1 store.
    /// PR-6 will swap to ORM-backed storage indexed by `admitted_at_ms`
    /// for the same ordering guarantee under restart.
    pub fn recall_recent(&self, limit: usize) -> Vec<Engram> {
        if limit == 0 {
            return Vec::new();
        }
        let engrams = self.engrams.lock().unwrap();
        engrams.iter().rev().take(limit).cloned().collect()
    }

    /// The AMNESIA FLASH (Joel's "MIB lamp"): drop every engram tagged with `context_id`,
    /// returning how many were forgotten. This is what lets a benchmark be a PROCTORED EXAM of
    /// the NATURAL living persona — she sits the exam with her full memory intact (never a
    /// stripped fork), and afterward we neuralyze JUST that exam's episode so the answer key
    /// can't leak into what she carries forward or trains on. Scoped strictly by `context_id`
    /// (the exam's own context), so her life's other memories are untouched. In-memory drop;
    /// the ORM/persistence sink deletion for the durable row is a follow-up (the eval fork uses
    /// a NoopSink, so nothing durable is written during an exam anyway).
    /// See [[benchmarks-are-proctored-exams-of-the-natural-living-persona]].
    pub fn forget_context(&self, context_id: Uuid) -> usize {
        let mut engrams = self.engrams.lock().unwrap();
        let before = engrams.len();
        engrams.retain(|e| e.context_id != Some(context_id));
        before - engrams.len()
    }

    /// The SURGICAL complement to [`forget_context`](Self::forget_context):
    /// walk every engram and scrub a policy-defined *class* of content out of
    /// its `content` and `recall_keys`, keeping the engram (its id, salience,
    /// recall history, provenance) otherwise intact. Where `forget_context`
    /// drops a whole exam episode, `redact` keeps the memory of *having been
    /// asked and having answered* and excises only the crib sheet — the held-out
    /// answer key, a leaked secret, PII on export. Returns an aggregate report
    /// of what was removed across the whole store.
    ///
    /// Durable: every rewritten engram is re-saved through the persistence sink
    /// (`observe_content_update`) so the scrub survives restart. A noop policy
    /// (no detectors) short-circuits — no store walk, no lock churn.
    ///
    /// Note on dedup: rewriting `content` changes its hash, so the
    /// `seen_content` map's old-hash→id pointer goes stale. That is harmless and
    /// arguably correct — the pre-redaction content is gone, so a future admit of
    /// the *old* text should NOT dedup against a memory that no longer holds it.
    pub fn redact(
        &self,
        policy: &crate::persona::redaction::RedactionPolicy,
    ) -> crate::persona::redaction::RedactionReport {
        let mut total = crate::persona::redaction::RedactionReport::default();
        if policy.is_noop() {
            return total;
        }

        // Rewrite in-memory under the engrams lock; collect the changed rows to
        // persist AFTER releasing it (never hold the store lock across the sink).
        let mut changed: Vec<Engram> = Vec::new();
        {
            let mut engrams = self.engrams.lock().unwrap();
            for engram in engrams.iter_mut() {
                let mut touched = false;
                let (new_content, r) = policy.redact(&engram.content);
                if !r.is_empty() {
                    engram.content = new_content;
                    total.merge(&r);
                    touched = true;
                }
                for key in engram.recall_keys.iter_mut() {
                    let (new_key, rk) = policy.redact(key);
                    if !rk.is_empty() {
                        *key = new_key;
                        total.merge(&rk);
                        touched = true;
                    }
                }
                if touched {
                    changed.push(engram.clone());
                }
            }
        }

        if !changed.is_empty() {
            let sink = self.persistence.read().unwrap().clone();
            for engram in &changed {
                sink.observe_content_update(engram);
            }
        }
        total
    }

    /// Algorithm 4 recall. Returns the top `limit` engrams ranked by
    /// `salience × recency-decay`, after applying decay to bring each
    /// engram's salience up to `now_ms`. Records a recall hit on the
    /// returned engrams (Hebbian rehearsal — use-it-keeps-it).
    ///
    /// Per [[source-drain-is-the-universal-pattern]] and the cognition-
    /// cache-hierarchy doc: salient + protected + recently-used
    /// engrams stay near the top; novel ones get the protection
    /// window; everything else drains. The substrate's continual-
    /// learning property compounds through this scoring — memory
    /// that gets used keeps coming back; memory that doesn't fades
    /// (but doesn't disappear — `apply_decay` honors SALIENCE_FLOOR).
    ///
    /// Returns engrams paired with their post-decay salience score so
    /// the caller can introspect what shaped the recall — per
    /// [[observability-is-half-the-architecture]] the cycle's L2 →
    /// prompt seam is observable, not opaque.
    pub fn recall_scored(&self, now_ms: u64, limit: usize) -> Vec<(Engram, f32)> {
        if limit == 0 {
            return Vec::new();
        }

        let engrams = self.engrams.lock().unwrap();

        // 1. Score every engram. apply_decay brings salience up to
        // `now_ms` for the engram (honors novelty protection +
        // SALIENCE_FLOOR + skips racing-tick double-decay).
        let mut scored: Vec<(Engram, f32)> = engrams
            .iter()
            .filter_map(|e| {
                self.recall_metadata.apply_decay(e.id, now_ms);
                self.recall_metadata
                    .get(e.id)
                    .map(|m| (e.clone(), m.salience))
            })
            .collect();
        drop(engrams);

        // 2. Top-N by score, descending. Stable sort so equal-score
        // engrams keep insertion order (recency tiebreak).
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit);

        // 3. Record recall hits on the returned engrams. Hebbian
        // rehearsal: each access bumps salience + access_count + the
        // last_accessed clock. Without this step the scoring would
        // be one-way: salient memories would only ever decay, never
        // climb back from being used. record_recall_hit closes the
        // use-it-keeps-it feedback loop.
        //
        // After each in-memory mutation, observe through the
        // persistence sink so the disk row keeps pace. NoopSink
        // (default) does nothing; OrmPersistenceSink upserts the
        // engram_recall_metadata row through tokio::spawn.
        let sink = self.persistence.read().unwrap().clone();
        for (engram, _) in &scored {
            self.recall_metadata.record_recall_hit(engram.id, now_ms);
            if let Some(updated) = self.recall_metadata.get(engram.id) {
                sink.observe_metadata_update(engram.id, updated);
            }
        }

        scored
    }

    /// Score the top `limit` engrams by salience-decay — but DO NOT record recall
    /// hits. The hit (Hebbian rehearsal / salience uplift) must land on what a
    /// caller actually SURFACES, not on everything it scored. A relevance
    /// re-ranker (RecallFaculty) over-fetches candidates here, narrows by cosine
    /// similarity to the burst, then calls [`record_recall_hits`] on the final
    /// surfaced set — so the loop closes on the memories the persona truly used,
    /// not on candidates that lost the re-rank. (`recall_scored` is the
    /// no-re-ranker shortcut: score + hit in one pass.)
    pub fn recall_candidates(&self, now_ms: u64, limit: usize) -> Vec<(Engram, f32)> {
        if limit == 0 {
            return Vec::new();
        }
        // How long a wanderer INNER-SPEECH thought (EngramKind::SelfReflection — the
        // historian/dreamer lenses, #145) stays eligible for AMBIENT recall. A passing
        // thought is "current inner speech" only briefly; past this it has PASSED, and
        // resurfacing it as though it were current fact is a bug, not a memory. 15 min:
        // long enough that a fresh musing can still bubble up the same activity, short
        // enough that a stale self-assessment cannot masquerade as the present.
        const INNER_SPEECH_RECALL_TTL_MS: u64 = 15 * 60 * 1000;
        let engrams = self.engrams.lock().unwrap();
        // Enumerate in insertion order: `idx` is a monotonic recency rank
        // (higher == more recently admitted), used as the tiebreaker below.
        let mut scored: Vec<(usize, Engram, f32)> = engrams
            .iter()
            .enumerate()
            .filter_map(|(idx, e)| {
                // Tool receipts — the persona's own "I ran X → result" — are
                // PROPRIOCEPTION, not durable knowledge. The recency/working-memory
                // channel carries them so she sees her own hands
                // ([[act-results-need-a-recency-channel-not-semantic-recall]]); they
                // must NOT compete in the SEMANTIC recall pool, where they were
                // drowning real memories (#166, seen live: recall surfaced "I ran
                // commands/list because Anwen is acting" as its top hits). Gate them
                // out here → recall returns knowledge (conversation, distilled
                // self-reflection), never the persona's own tool chatter. This is the
                // structural form of the earlier salience down-weight, which alone
                // couldn't stop them surfacing when knowledge was sparse.
                if matches!(e.origin, EngramOrigin::Tool(_)) {
                    return None;
                }
                // Wanderer inner-speech (EngramKind::SelfReflection) is a PASSING
                // thought, not durable knowledge. A FRESH one may bubble up as ambient
                // inner speech (the arc's intent, dream_consolidation §wanderer); a STALE
                // one must NOT resurface as current fact. Glass-boxed 2026-07-14: Atlas
                // recalled a 25m-old "[thought:historian] you keep failing to claim" AS
                // present truth minutes after his claim SUCCEEDED, and rationalized the
                // contradiction into a loop — the "feedback vs rag" incoherence. Bound
                // its ambient-recall lifetime here; introspection still queries ALL of
                // them explicitly by `thought:<lens>` recall-key (`recall_by_keyword`, a
                // separate path). Dream-DISTILLED insight is EngramKind::Semantic
                // (durable) — untouched. Sibling of the Tool-receipt gate above (#166).
                if e.kind == crate::persona::engram::EngramKind::SelfReflection
                    && now_ms.saturating_sub(e.admitted_at_ms) > INNER_SPEECH_RECALL_TTL_MS
                {
                    return None;
                }
                self.recall_metadata.apply_decay(e.id, now_ms);
                self.recall_metadata
                    .get(e.id)
                    .map(|m| (idx, e.clone(), m.salience))
            })
            .collect();
        drop(engrams);
        // Rank by salience, breaking ties by RECENCY (newest wins). Without the
        // recency tiebreak, recall-hit uplift flattens salience to a ceiling
        // (every surfaced memory rises toward 1.0), a stable sort then preserves
        // insertion order (oldest-first), and `truncate` evicts the NEWEST engram
        // — so a just-admitted act-observation never even reaches the re-ranker,
        // and the mind loops re-issuing the identical act, blind to its own hands.
        // Newest-wins-on-ties guarantees the freshest memory survives truncation;
        // the relevance re-rank (RecallFaculty) then surfaces it because it
        // literally contains what the burst is asking about.
        // See [[act-results-need-a-recency-channel-not-semantic-recall]].
        scored.sort_by(|a, b| {
            b.2.partial_cmp(&a.2)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(b.0.cmp(&a.0))
        });
        scored.truncate(limit);
        scored.into_iter().map(|(_, e, s)| (e, s)).collect()
    }

    /// Record recall hits (salience uplift + access_count + persistence observe)
    /// on a specific set of engram ids — the memories a caller actually surfaced.
    /// The write half of the [`recall_candidates`] → re-rank → record loop.
    pub fn record_recall_hits(&self, ids: &[Uuid], now_ms: u64) {
        let sink = self.persistence.read().unwrap().clone();
        for id in ids {
            self.recall_metadata.record_recall_hit(*id, now_ms);
            if let Some(updated) = self.recall_metadata.get(*id) {
                sink.observe_metadata_update(*id, updated);
            }
        }
    }

    /// Recall a specific engram by id. None if not present in the store
    /// (either never admitted, or evicted in a future GC pass).
    pub fn recall_by_id(&self, id: Uuid) -> Option<Engram> {
        let engrams = self.engrams.lock().unwrap();
        engrams.iter().find(|e| e.id == id).cloned()
    }

    /// Recall engrams whose content contains `keyword` (case-insensitive
    /// substring match). Returns matches in newest-first order, capped
    /// at `limit`. v1 = linear scan over the in-memory store; PR-6 will
    /// add an ORM-side query / index.
    ///
    /// Empty `keyword` returns an empty Vec — the caller meant to skip
    /// search. (Avoids the gotcha where every engram contains the empty
    /// string.)
    pub fn recall_by_keyword(&self, keyword: &str, limit: usize) -> Vec<Engram> {
        if keyword.is_empty() || limit == 0 {
            return Vec::new();
        }
        let needle = keyword.to_lowercase();
        let engrams = self.engrams.lock().unwrap();
        engrams
            .iter()
            .rev()
            .filter(|e| e.content.to_lowercase().contains(&needle))
            .take(limit)
            .cloned()
            .collect()
    }

    /// Recall engrams filtered by origin variant (Chat / Airc / Tool /
    /// SelfReflection). Newest first, capped at `limit`. Useful for
    /// callers that want "what did I learn from chat" vs "what did I
    /// learn from tool invocations".
    pub fn recall_by_origin_kind(&self, kind: EngramOriginKind, limit: usize) -> Vec<Engram> {
        if limit == 0 {
            return Vec::new();
        }
        let engrams = self.engrams.lock().unwrap();
        engrams
            .iter()
            .rev()
            .filter(|e| EngramOriginKind::from(&e.origin) == kind)
            .take(limit)
            .cloned()
            .collect()
    }
}

/// Discriminator over `EngramOrigin` variants. Used by `recall_by_origin_kind`
/// so callers can filter without pattern-matching the full origin (which
/// carries variant-specific reference fields they don't need for the
/// filter decision).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngramOriginKind {
    Chat,
    Airc,
    Tool,
    SelfReflection,
}

impl From<&EngramOrigin> for EngramOriginKind {
    fn from(origin: &EngramOrigin) -> Self {
        match origin {
            EngramOrigin::Chat(_) => Self::Chat,
            EngramOrigin::Airc(_) => Self::Airc,
            EngramOrigin::Tool(_) => Self::Tool,
            EngramOrigin::SelfReflection { .. } => Self::SelfReflection,
        }
    }
}

//=============================================================================
// TESTS
//=============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persona::admission::IsMemorable as _;
    use crate::persona::engram::{
        AdmissionDropReason, AircMessageRef, ChatMessageRef, EngramKind, TrustState,
    };
    use crate::persona::inbox_admission::content_hash_sha256;
    use crate::persona::types::SenderType;

    fn synthetic_human_message(content: &str) -> InboxMessage {
        InboxMessage {
            id: Uuid::new_v4(),
            room_id: Uuid::new_v4(),
            sender_id: Uuid::new_v4(),
            sender_name: "test-human".to_string(),
            sender_type: SenderType::Human,
            content: content.to_string(),
            timestamp: 1_715_625_600_000,
            priority: 0.5,
            source_modality: None,
            voice_session_id: None,
        }
    }

    /// What this catches: a clean admit records the engram in the store,
    /// records the content_hash for dedup, AND a subsequent admit of the
    /// SAME content gets dropped as Duplicate (proving the side-effect
    /// recording actually feeds back into the next call's recipe).
    #[test]
    fn admit_records_engram_and_dedup_blocks_repeat() {
        let state = AdmissionState::new(Arc::new(
            crate::persona::recall_metadata::RecallMetadataRegistry::new(),
        ));
        let mut trace = CognitionTrace::new();
        let content = "this is a non-trivial design observation worth storing";
        let msg = synthetic_human_message(content);

        let first = state.admit(&msg, Some(&mut trace)).unwrap();
        assert!(matches!(first, AdmissionDecision::Admit { .. }));
        assert_eq!(state.engram_count(), 1);
        assert!(state.is_content_seen(&content_hash_sha256(content)));

        // Second admit of identical content (different message id, same content)
        // should drop as Duplicate.
        let msg2 = synthetic_human_message(content);
        let second = state.admit(&msg2, Some(&mut trace)).unwrap();
        match second {
            AdmissionDecision::Drop {
                reason: AdmissionDropReason::Duplicate { .. },
            } => {}
            other => panic!("expected Drop Duplicate, got {other:?}"),
        }
        // No new engram was admitted.
        assert_eq!(state.engram_count(), 1);
    }

    /// What this catches: dropped messages do NOT pollute either store.
    /// A dropped message's content_hash should NOT be in seen_content
    /// (otherwise a later legit version of the same content would be
    /// blocked as duplicate against a non-existent engram).
    #[test]
    fn dropped_message_records_no_side_effect() {
        let state = AdmissionState::new(Arc::new(
            crate::persona::recall_metadata::RecallMetadataRegistry::new(),
        ));
        let mut trace = CognitionTrace::new();
        // Short content → drops with NotMemorable.
        let msg = synthetic_human_message("short");

        let decision = state.admit(&msg, Some(&mut trace)).unwrap();
        match decision {
            AdmissionDecision::Drop {
                reason: AdmissionDropReason::NotMemorable { .. },
            } => {}
            other => panic!("expected Drop NotMemorable, got {other:?}"),
        }
        assert_eq!(state.engram_count(), 0);
        assert!(!state.is_content_seen(&content_hash_sha256("short")));
    }

    /// What this catches: admitted engrams accumulate in admission order
    /// + each engram is retrievable by index. Future recall surface
    /// depends on this; missing items would silently break recall.
    #[test]
    fn admitted_engrams_accumulate_in_order_and_are_retrievable() {
        let state = AdmissionState::new(Arc::new(
            crate::persona::recall_metadata::RecallMetadataRegistry::new(),
        ));
        let mut trace = CognitionTrace::new();
        let messages = [
            "first design observation worth recording",
            "second design observation worth recording",
            "third design observation worth recording",
        ];
        for content in messages {
            let _ = state.admit(&synthetic_human_message(content), Some(&mut trace));
        }
        assert_eq!(state.engram_count(), 3);
        assert_eq!(
            state.engram_at(0).expect("first engram present").content,
            messages[0]
        );
        assert_eq!(
            state.engram_at(2).expect("third engram present").content,
            messages[2]
        );
        assert!(state.engram_at(99).is_none(), "out-of-bounds returns None");
    }

    /// What this catches: the trace seam invariant carries through the
    /// state wrapper. Every admit() call (success + drop) appends exactly
    /// one SEAM_ADMISSION to the trace. Same forensic guarantee as the
    /// underlying runner.
    #[test]
    fn admit_emits_one_seam_per_call_through_state_wrapper() {
        let state = AdmissionState::new(Arc::new(
            crate::persona::recall_metadata::RecallMetadataRegistry::new(),
        ));
        let mut trace = CognitionTrace::new();
        // Three admits with three different outcomes:
        // (1) admit, (2) drop short, (3) drop duplicate of #1.
        let msg1 = synthetic_human_message("a long enough observation worth recording");
        let msg2 = synthetic_human_message("short");
        let msg3 = synthetic_human_message("a long enough observation worth recording");
        let _ = state.admit(&msg1, Some(&mut trace));
        let _ = state.admit(&msg2, Some(&mut trace));
        let _ = state.admit(&msg3, Some(&mut trace));
        assert_eq!(trace.seam_count(), 3, "one seam per admit() call");
    }

    /// What this catches: the runner accessor returns the configured
    /// runner so callers can introspect (recipe id for trace metadata,
    /// trust thresholds for debugging). A regression in the accessor
    /// would silently hide config from observability surfaces.
    #[test]
    fn runner_accessor_exposes_default_v1_config() {
        let state = AdmissionState::new(Arc::new(
            crate::persona::recall_metadata::RecallMetadataRegistry::new(),
        ));
        assert_eq!(state.runner().recipe().id(), "heuristic.v1");
    }

    /// What this catches: AdmissionState is Send + Sync. Compile-time
    /// proof that it can live inside `PersonaCognition` (which is held in
    /// a `DashMap<Uuid, PersonaCognition>` + crossed across tokio tasks).
    /// If a future refactor drops Send/Sync, this test fails to compile.
    #[test]
    fn admission_state_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<AdmissionState>();
    }

    // ── Quarantine side-effect rule (claude-tab-2 review nit on #1155) ──
    //
    // v1 has no quarantine store, so a Quarantined engram is dropped on
    // the floor. Recording its content_hash → engram_id in the dedup
    // store would leave a dangling pointer (future Duplicate drops would
    // surface an existing_engram_id that can't be looked up). The right
    // behaviour: ONLY record event_id (replay protection still applies),
    // never record content_hash on Quarantine.
    //
    // These tests construct synthetic AdmissionDecision values + call
    // `record_side_effects` directly so they don't need a custom recipe
    // — the heuristic recipe shipped here doesn't naturally emit
    // Quarantine, but the rule is about the side-effect helper itself.

    fn synthetic_engram_with_chat_origin(content: &str) -> Engram {
        Engram {
            context_id: None,
            id: Uuid::new_v4(),
            kind: EngramKind::Episodic,
            content: content.to_string(),
            origin: EngramOrigin::Chat(ChatMessageRef {
                message_id: Uuid::new_v4(),
                room_id: Uuid::new_v4(),
                sender_id: Uuid::new_v4(),
                posted_at_ms: 1_000_000,
                content_hash: content_hash_sha256(content),
            }),
            recall_keys: vec!["test".to_string()],
            admitted_at_ms: 1_000_000,
            trust_state_at_admission: TrustState::ApprovedPeer,
            admission_trace_id: None,
        }
    }

    fn synthetic_engram_with_airc_origin(content: &str, message_id: &str) -> Engram {
        Engram {
            context_id: None,
            id: Uuid::new_v4(),
            kind: EngramKind::Episodic,
            content: content.to_string(),
            origin: EngramOrigin::Airc(AircMessageRef {
                transport: "airc".to_string(),
                room_id: "cambriantech".to_string(),
                message_id: message_id.to_string(),
                sender_id: "airc-8a5e".to_string(),
                sent_at_ms: 1_000_000,
                received_at_ms: 1_000_000,
                content_hash: content_hash_sha256(content),
                signature: "sig".to_string(),
                proof_refs: vec![],
                schema_version: "v1".to_string(),
                client_name: None,
            }),
            recall_keys: vec!["test".to_string()],
            admitted_at_ms: 1_000_000,
            trust_state_at_admission: TrustState::ApprovedPeer,
            admission_trace_id: None,
        }
    }

    /// What this catches: Quarantine of a Chat-origin engram records
    /// NEITHER content_hash NOR event_id. Chat origins have no replay
    /// surface distinct from content dedup, so quarantine on chat is a
    /// pure no-op as far as the side-effect stores are concerned.
    /// Original PR-4 code recorded content_hash here, leaving a dangling
    /// pointer.
    #[test]
    fn quarantine_chat_origin_records_no_side_effects() {
        let state = AdmissionState::new(Arc::new(
            crate::persona::recall_metadata::RecallMetadataRegistry::new(),
        ));
        let engram = synthetic_engram_with_chat_origin("borderline observation");
        let content_hash = match &engram.origin {
            EngramOrigin::Chat(r) => r.content_hash.clone(),
            _ => unreachable!(),
        };
        let decision = AdmissionDecision::Quarantine {
            engram,
            reason: "test borderline".to_string(),
            expiry_ms: 2_000_000,
        };

        state.record_side_effects(&decision);

        assert!(
            !state.is_content_seen(&content_hash),
            "chat-origin quarantine MUST NOT record content_hash (would dangle)"
        );
        assert_eq!(
            state.engram_count(),
            0,
            "quarantine MUST NOT add to engram store"
        );
    }

    /// What this catches: Quarantine of an AIRC-origin engram records
    /// the event_id (replay protection — the load-bearing behaviour) but
    /// MUST NOT record the content_hash (which would dangle since v1
    /// doesn't store quarantined engrams).
    #[test]
    fn quarantine_airc_origin_records_event_id_only_not_content_hash() {
        let state = AdmissionState::new(Arc::new(
            crate::persona::recall_metadata::RecallMetadataRegistry::new(),
        ));
        let event_id = "airc-msg-quarantine-1";
        let engram =
            synthetic_engram_with_airc_origin("borderline observation worth holding", event_id);
        let content_hash = match &engram.origin {
            EngramOrigin::Airc(r) => r.content_hash.clone(),
            _ => unreachable!(),
        };
        let decision = AdmissionDecision::Quarantine {
            engram,
            reason: "test borderline".to_string(),
            expiry_ms: 2_000_000,
        };

        state.record_side_effects(&decision);

        assert!(
            state.is_event_seen(event_id),
            "airc-origin quarantine MUST record event_id (replay protection)"
        );
        assert!(
            !state.is_content_seen(&content_hash),
            "airc-origin quarantine MUST NOT record content_hash (would dangle)"
        );
        assert_eq!(
            state.engram_count(),
            0,
            "quarantine MUST NOT add to engram store"
        );
    }

    // ── Recall surface (#1121 PR-5) ──────────────────────────────────────

    /// Helper: admit N synthetic human messages with distinct content,
    /// returning the engram ids in admission order.
    fn admit_n_distinct(state: &AdmissionState, contents: &[&str]) -> Vec<Uuid> {
        let mut trace = CognitionTrace::new();
        let mut ids = Vec::new();
        for c in contents {
            match state
                .admit(&synthetic_human_message(c), Some(&mut trace))
                .unwrap()
            {
                AdmissionDecision::Admit { engram, .. } => ids.push(engram.id),
                other => panic!("expected Admit for content {c:?}, got {other:?}"),
            }
        }
        ids
    }

    /// What this catches: recall_recent returns engrams in NEWEST-FIRST
    /// order (reverse insertion). A regression to insertion-order would
    /// silently invert what callers expect when they ask for "recent".
    #[test]
    fn recall_recent_returns_newest_first() {
        let state = AdmissionState::new(Arc::new(
            crate::persona::recall_metadata::RecallMetadataRegistry::new(),
        ));
        let ids = admit_n_distinct(
            &state,
            &[
                "first observation worth storing here",
                "second observation worth storing here",
                "third observation worth storing here",
            ],
        );
        let recent = state.recall_recent(3);
        assert_eq!(recent.len(), 3);
        // Newest first → reverse of admission order.
        assert_eq!(recent[0].id, ids[2]);
        assert_eq!(recent[1].id, ids[1]);
        assert_eq!(recent[2].id, ids[0]);
    }

    /// What this catches: the amnesia flash (`forget_context`) wipes ONLY the exam episode's
    /// engrams and leaves her other memory intact — the property that makes a natural proctored
    /// exam safe (she keeps her life's memory; only the answer-key episode is neuralyzed). A
    /// regression that widened the scope would erase real memories; one that narrowed it would
    /// leak exam answers into what she carries forward.
    /// See [[benchmarks-are-proctored-exams-of-the-natural-living-persona]].
    #[test]
    fn forget_context_wipes_only_the_tagged_episode() {
        let state = AdmissionState::new(Arc::new(
            crate::persona::recall_metadata::RecallMetadataRegistry::new(),
        ));
        admit_n_distinct(
            &state,
            &[
                "her real life memory one worth keeping",
                "exam question engram to be neuralyzed",
                "her real life memory two worth keeping",
            ],
        );
        let exam_ctx = Uuid::new_v4();
        // Stamp the middle engram as belonging to the exam episode (the acting body tags act
        // results with a context_id in the live path; here we set it directly for the test).
        {
            let mut engrams = state.engrams.lock().unwrap();
            engrams[1].context_id = Some(exam_ctx);
        }
        let forgotten = state.forget_context(exam_ctx);
        assert_eq!(forgotten, 1, "exactly the one exam engram is neuralyzed");
        let remaining = state.recall_recent(10);
        assert_eq!(remaining.len(), 2, "her two real memories survive");
        assert!(
            remaining.iter().all(|e| e.context_id != Some(exam_ctx)),
            "no exam-tagged engram remains"
        );
        // Flashing an unrelated context wipes nothing (scope safety).
        assert_eq!(state.forget_context(Uuid::new_v4()), 0);
    }

    /// What this catches: recall_recent honors the limit, never exceeds
    /// it, never panics on limit > available.
    #[test]
    fn recall_recent_respects_limit_above_and_below_count() {
        let state = AdmissionState::new(Arc::new(
            crate::persona::recall_metadata::RecallMetadataRegistry::new(),
        ));
        admit_n_distinct(
            &state,
            &[
                "alpha observation worth storing",
                "beta observation worth storing",
            ],
        );
        assert_eq!(state.recall_recent(0).len(), 0, "limit=0 returns empty");
        assert_eq!(state.recall_recent(1).len(), 1, "limit=1 returns one");
        assert_eq!(
            state.recall_recent(99).len(),
            2,
            "limit > count caps at count"
        );
    }

    // What this catches: when many engrams share the same salience (the common
    // case once recall-hit uplift flattens them to a ceiling), recall_candidates
    // must keep the NEWEST under truncation, not the oldest. A stable sort on
    // tied salience used to preserve insertion order (oldest-first), so the
    // truncate evicted the just-admitted engram — a persona acting and admitting
    // "I ran X → got Y" could never recall its own fresh result and looped
    // re-issuing the identical act. Regression for the [you just acted]-fold
    // removal (2026-06-25); see [[act-results-need-a-recency-channel-not-semantic-recall]].
    #[test]
    fn recall_candidates_keeps_newest_when_salience_ties() {
        let state = AdmissionState::new(Arc::new(
            crate::persona::recall_metadata::RecallMetadataRegistry::new(),
        ));
        // Seven engrams, all at the default salience (a tie). The freshest is ids[6].
        let ids = admit_n_distinct(
            &state,
            &[
                "first observation worth storing here",
                "second observation worth storing here",
                "third observation worth storing here",
                "fourth observation worth storing here",
                "fifth observation worth storing here",
                "sixth observation worth storing here",
                "I ran code/read(external_fingerprint) and got the function body",
            ],
        );
        let surfaced = state.recall_candidates(10_000, 5);
        assert_eq!(surfaced.len(), 5, "honors the limit");
        let surfaced_ids: Vec<Uuid> = surfaced.iter().map(|(e, _)| e.id).collect();
        // The freshest memory survives truncation and ranks first on the tie.
        assert_eq!(
            surfaced_ids[0], ids[6],
            "newest-wins-on-ties: the just-admitted act-observation is surfaced first"
        );
        // The two oldest are the ones evicted by the limit, not the newest.
        assert!(
            !surfaced_ids.contains(&ids[0]) && !surfaced_ids.contains(&ids[1]),
            "the OLDEST tied engrams are evicted, never the newest"
        );
    }

    // what this catches: a Tool-origin engram — a persona's own "I ran X → result"
    // receipt (proprioception) — must be EXCLUDED from the semantic recall pool so
    // it can never drown durable knowledge (#166). Note it's admitted NEWER than the
    // knowledge, so if the exclusion regressed, recency would surface it first. This
    // is the coverage that was missing when act_observe mis-admitted receipts as
    // non-Tool persona messages and the gate never bit.
    #[test]
    fn recall_candidates_excludes_tool_origin_receipts() {
        let state = AdmissionState::new(Arc::new(
            crate::persona::recall_metadata::RecallMetadataRegistry::new(),
        ));
        let knowledge = admit_n_distinct(&state, &["the ticket asks for a wordstats CLI here"]);
        let receipt = crate::persona::engram::Engram {
            id: Uuid::new_v4(),
            context_id: None,
            kind: crate::persona::engram::EngramKind::Episodic,
            content: "code/list(path=src) → ok".to_string(),
            origin: crate::persona::engram::EngramOrigin::Tool(
                crate::persona::engram::ToolInvocationRef {
                    invocation_id: Uuid::new_v4(),
                    tool_name: "code/list".to_string(),
                    invoked_at_ms: 1000,
                    input_hash: "sha256:in".to_string(),
                    output_hash: "sha256:out".to_string(),
                },
            ),
            recall_keys: Vec::new(),
            admitted_at_ms: 2000,
            trust_state_at_admission: crate::persona::engram::TrustState::SelfTrust,
            admission_trace_id: None,
        };
        let receipt_id = receipt.id;
        state.admit_reflection(receipt).expect("receipt admits");

        let ids: Vec<Uuid> = state
            .recall_candidates(10_000, 10)
            .into_iter()
            .map(|(e, _)| e.id)
            .collect();
        assert!(ids.contains(&knowledge[0]), "durable knowledge stays recallable");
        assert!(
            !ids.contains(&receipt_id),
            "a Tool-origin receipt is NEVER in the semantic recall pool"
        );
    }

    // what this catches: wanderer INNER-SPEECH recall is recency-bounded (#145 / the
    // 2026-07-14 "feedback vs rag" incoherence — Atlas recalled a stale historian
    // thought "you keep failing to claim" as present truth after his claim succeeded).
    // A FRESH SelfReflection-kind thought still bubbles up; a STALE one drops; a
    // dream-DISTILLED Semantic reflection is durable and ALWAYS stays.
    #[test]
    fn recall_recency_bounds_wanderer_inner_speech_but_keeps_distilled() {
        let state = AdmissionState::new(Arc::new(
            crate::persona::recall_metadata::RecallMetadataRegistry::new(),
        ));
        let now = 100 * 60 * 1000; // 100 min in
        let inner = |content: &str, admitted_at_ms: u64| Engram {
            id: Uuid::new_v4(),
            context_id: None,
            kind: EngramKind::SelfReflection, // wanderer inner speech
            content: content.to_string(),
            origin: EngramOrigin::SelfReflection { parent_engram_id: Uuid::new_v4() },
            recall_keys: vec!["thought:historian".to_string()],
            admitted_at_ms,
            trust_state_at_admission: TrustState::SelfTrust,
            admission_trace_id: None,
        };
        let fresh = inner("[thought:historian] a passing thought, moments old", now - 5 * 60 * 1000);
        let stale = inner("[thought:historian] you keep failing to claim", now - 40 * 60 * 1000);
        // A dream-distilled DURABLE insight (Semantic kind, SelfReflection origin).
        let distilled = semantic_reflection("the codebase grades via rustc exit code", Uuid::new_v4());
        let (fresh_id, stale_id, distilled_id) = (fresh.id, stale.id, distilled.id);
        for e in [fresh, stale, distilled] {
            state.admit_reflection(e).expect("admits");
        }
        let ids: Vec<Uuid> = state
            .recall_candidates(now, 10)
            .into_iter()
            .map(|(e, _)| e.id)
            .collect();
        assert!(ids.contains(&fresh_id), "fresh inner speech still bubbles up");
        assert!(!ids.contains(&stale_id), "a stale wanderer thought does NOT resurface as current fact");
        assert!(ids.contains(&distilled_id), "dream-distilled Semantic insight is durable — always recallable");
    }

    fn semantic_reflection(content: &str, parent: Uuid) -> Engram {
        Engram {
            context_id: None,
            id: Uuid::new_v4(),
            kind: EngramKind::Semantic,
            content: content.to_string(),
            origin: EngramOrigin::SelfReflection {
                parent_engram_id: parent,
            },
            recall_keys: vec!["distilled".to_string()],
            admitted_at_ms: 2_000_000,
            trust_state_at_admission: TrustState::SelfTrust,
            admission_trace_id: None,
        }
    }

    /// What this catches: a self-produced (dream-distilled) Semantic engram is
    /// ingested through `admit_reflection` WITHOUT an external envelope, lands
    /// in the store, and is recall-visible — proving the dream's output reaches
    /// the same persistence recall reads, not a side buffer.
    #[test]
    fn admit_reflection_stores_self_produced_fact() {
        let state = AdmissionState::new(Arc::new(
            crate::persona::recall_metadata::RecallMetadataRegistry::new(),
        ));
        let fact = semantic_reflection("Rust is the core; Node is the shell", Uuid::new_v4());

        let decision = state
            .admit_reflection(fact.clone())
            .expect("self-admission does not error");
        assert!(matches!(decision, AdmissionDecision::Admit { .. }));
        assert_eq!(state.engram_count(), 1);
        let recent = state.recall_recent(5);
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].content, fact.content);
        assert_eq!(recent[0].kind, EngramKind::Semantic);
    }

    /// What this catches: `redact` is the surgical complement to
    /// `forget_context` — it scrubs the held-out answer key out of a memory
    /// while KEEPING the engram and the experience of having answered. The
    /// proctored-exam integrity guarantee at the store level: she can't
    /// memorize the crib sheet, but her autobiography of struggling stays.
    #[test]
    fn redact_scrubs_exam_key_from_memory_keeping_the_experience() {
        use crate::persona::redaction::{ExamKeyDetector, RedactionClass, RedactionPolicy};
        let state = AdmissionState::new(Arc::new(
            crate::persona::recall_metadata::RecallMetadataRegistry::new(),
        ));
        let mem = semantic_reflection(
            "I was asked which file holds the loop; I answered service_loop.rs and it passed.",
            Uuid::new_v4(),
        );
        state.admit_reflection(mem).expect("admits");

        let policy = RedactionPolicy::new(vec![Box::new(ExamKeyDetector::new(
            ["service_loop.rs".to_string()],
            ExamKeyDetector::DEFAULT_MIN_LEN,
        ))]);
        let report = state.redact(&policy);
        assert_eq!(report.count(RedactionClass::ExamKey), 1);

        let recalled = state.recall_recent(1);
        assert_eq!(recalled.len(), 1);
        let content = &recalled[0].content;
        assert!(content.contains("I was asked which file holds the loop"));
        assert!(content.contains("it passed."));
        assert!(!content.contains("service_loop.rs"));
        assert!(content.contains("[redacted:exam-key]"));
    }

    /// What this catches: a noop policy (no detectors) short-circuits to an
    /// empty report and mutates nothing — the hot path when no exam is in play.
    #[test]
    fn redact_with_noop_policy_changes_nothing() {
        use crate::persona::redaction::RedactionPolicy;
        let state = AdmissionState::new(Arc::new(
            crate::persona::recall_metadata::RecallMetadataRegistry::new(),
        ));
        let mem = semantic_reflection("plain memory, nothing sensitive", Uuid::new_v4());
        state.admit_reflection(mem).expect("admits");
        let report = state.redact(&RedactionPolicy::new(vec![]));
        assert!(report.is_empty());
        assert_eq!(state.recall_recent(1)[0].content, "plain memory, nothing sensitive");
    }

    /// What this catches: re-distilling the same cluster on a later dream tick
    /// is idempotent — identical fact content is Dropped as Duplicate, never
    /// accumulated. Without this guard the store would fill with duplicate facts
    /// every idle tick.
    #[test]
    fn admit_reflection_dedups_identical_facts() {
        let state = AdmissionState::new(Arc::new(
            crate::persona::recall_metadata::RecallMetadataRegistry::new(),
        ));
        let parent = Uuid::new_v4();
        let first = semantic_reflection("Headless core, many equal clients", parent);
        let second = semantic_reflection("Headless core, many equal clients", parent);

        let d1 = state.admit_reflection(first.clone()).expect("first admits");
        assert!(matches!(d1, AdmissionDecision::Admit { .. }));

        let d2 = state.admit_reflection(second).expect("second resolves");
        match d2 {
            AdmissionDecision::Drop {
                reason: AdmissionDropReason::Duplicate { existing_engram_id },
            } => assert_eq!(existing_engram_id, first.id),
            other => panic!("expected Duplicate drop, got {other:?}"),
        }
        assert_eq!(state.engram_count(), 1, "duplicate fact not stored twice");
    }

    /// What this catches: recall_scored ranks engrams by salience desc.
    /// A regression to admission-order or insertion-order would silently
    /// return whichever was admitted last instead of whichever scored
    /// highest — defeating the whole point of Algorithm 4 driving recall.
    #[test]
    fn recall_scored_ranks_by_salience_desc() {
        let registry =
            Arc::new(crate::persona::recall_metadata::RecallMetadataRegistry::new());
        let state = AdmissionState::new(Arc::clone(&registry));
        let ids = admit_n_distinct(
            &state,
            &[
                "alpha observation worth storing",
                "beta observation worth storing",
                "gamma observation worth storing",
            ],
        );
        // Pin gamma → salience 1.0, permanent protection. Beta gets a
        // recall hit → salience uplift. Alpha is left at default (0.5).
        // Expected order after scoring: gamma > beta > alpha.
        registry.pin_permanent(ids[2]);
        registry.record_recall_hit(ids[1], 1_000);

        let now_ms = 10_000;
        let scored = state.recall_scored(now_ms, 8);
        assert_eq!(scored.len(), 3);
        assert_eq!(scored[0].0.id, ids[2], "pinned gamma ranks first");
        assert_eq!(scored[1].0.id, ids[1], "uplifted beta ranks second");
        assert_eq!(scored[2].0.id, ids[0], "untouched alpha ranks last");
        assert!(
            scored[0].1 >= scored[1].1 && scored[1].1 >= scored[2].1,
            "score values are monotonically descending: {:?}",
            scored.iter().map(|(_, s)| *s).collect::<Vec<_>>()
        );
    }

    /// What this catches: recall_scored records a recall hit on every
    /// returned engram (Hebbian rehearsal — use-it-keeps-it). Without
    /// this, scoring is one-way: salient memories only decay, never
    /// climb back from being used. The feedback loop closing here is
    /// what makes the substrate continual-learning, not just continual-
    /// remembering.
    #[test]
    fn recall_scored_records_recall_hit_on_returned_engrams() {
        let registry =
            Arc::new(crate::persona::recall_metadata::RecallMetadataRegistry::new());
        let state = AdmissionState::new(Arc::clone(&registry));
        let ids = admit_n_distinct(
            &state,
            &[
                "alpha observation worth storing",
                "beta observation worth storing",
            ],
        );
        let before_alpha = registry.get(ids[0]).expect("alpha metadata exists");
        let before_beta = registry.get(ids[1]).expect("beta metadata exists");

        // Score returns both. Both should get a recall hit.
        let scored = state.recall_scored(5_000, 8);
        assert_eq!(scored.len(), 2);

        let after_alpha = registry.get(ids[0]).expect("alpha metadata after");
        let after_beta = registry.get(ids[1]).expect("beta metadata after");
        assert!(
            after_alpha.access_count > before_alpha.access_count,
            "alpha access_count climbed: {} -> {}",
            before_alpha.access_count,
            after_alpha.access_count
        );
        assert!(
            after_beta.access_count > before_beta.access_count,
            "beta access_count climbed"
        );
        assert!(
            after_alpha.last_accessed_ms == 5_000,
            "alpha last_accessed_ms updated to now_ms (was {}, now {})",
            before_alpha.last_accessed_ms,
            after_alpha.last_accessed_ms
        );
    }

    /// What this catches: recall_scored honors the limit (caps below
    /// count, doesn't exceed available) and limit=0 returns empty
    /// without panicking + without recording spurious hits.
    #[test]
    fn recall_scored_respects_limit_and_empty() {
        let registry =
            Arc::new(crate::persona::recall_metadata::RecallMetadataRegistry::new());
        let state = AdmissionState::new(Arc::clone(&registry));
        let ids = admit_n_distinct(
            &state,
            &[
                "alpha observation worth storing",
                "beta observation worth storing",
                "gamma observation worth storing",
            ],
        );
        let before = registry.get(ids[0]).expect("alpha metadata exists");

        assert_eq!(
            state.recall_scored(1_000, 0).len(),
            0,
            "limit=0 returns empty"
        );

        let after_zero = registry.get(ids[0]).expect("alpha metadata after limit=0");
        assert_eq!(
            after_zero.access_count, before.access_count,
            "limit=0 records no recall hits"
        );

        assert_eq!(state.recall_scored(1_000, 1).len(), 1, "limit=1 returns one");
        assert_eq!(
            state.recall_scored(1_000, 99).len(),
            3,
            "limit > count caps at count"
        );
    }

    /// What this catches: recall_by_id returns the exact engram for a
    /// known id, None for an unknown id. Foundation of any future recall
    /// pipeline that walks parent/reflection links.
    #[test]
    fn recall_by_id_finds_known_returns_none_unknown() {
        let state = AdmissionState::new(Arc::new(
            crate::persona::recall_metadata::RecallMetadataRegistry::new(),
        ));
        let ids = admit_n_distinct(
            &state,
            &[
                "first observation worth storing",
                "second observation worth storing",
            ],
        );
        let found = state.recall_by_id(ids[0]).expect("known id must resolve");
        assert_eq!(found.id, ids[0]);
        assert_eq!(found.content, "first observation worth storing");
        assert!(
            state.recall_by_id(Uuid::new_v4()).is_none(),
            "unknown id is None"
        );
    }

    /// What this catches: keyword search is case-insensitive substring,
    /// returns newest-first, honors limit. Empty keyword returns empty
    /// (caller-meant-to-skip semantic, not match-everything).
    #[test]
    fn recall_by_keyword_case_insensitive_newest_first_with_limit() {
        let state = AdmissionState::new(Arc::new(
            crate::persona::recall_metadata::RecallMetadataRegistry::new(),
        ));
        admit_n_distinct(
            &state,
            &[
                "the recall ratchet design needs work",
                "not relevant to our search needle here",
                "another RECALL ratchet observation",
            ],
        );
        let hits = state.recall_by_keyword("recall", 10);
        assert_eq!(
            hits.len(),
            2,
            "two engrams contain 'recall' (case-insensitive)"
        );
        // Newest first: "another RECALL..." was admitted last.
        assert!(
            hits[0].content.contains("another RECALL"),
            "newest-first ordering: got {}",
            hits[0].content
        );
        // Empty needle = caller skipped search.
        assert!(state.recall_by_keyword("", 10).is_empty());
        // Zero limit short-circuits.
        assert!(state.recall_by_keyword("recall", 0).is_empty());
        // Limit caps result count.
        assert_eq!(state.recall_by_keyword("recall", 1).len(), 1);
    }

    /// What this catches: origin-kind filter returns only matching
    /// variants. Inbox-sourced messages currently always synthesize
    /// `Chat` origins (per PR-3 design); if someone admits via a
    /// different origin path (PR-5+ tool/reflection ingestion), the
    /// filter must still segregate cleanly.
    #[test]
    fn recall_by_origin_kind_filters_to_requested_variant() {
        let state = AdmissionState::new(Arc::new(
            crate::persona::recall_metadata::RecallMetadataRegistry::new(),
        ));
        admit_n_distinct(
            &state,
            &[
                "human observation worth storing here",
                "another human observation worth storing",
            ],
        );
        // All inbox admits are Chat-origin.
        let chat_hits = state.recall_by_origin_kind(EngramOriginKind::Chat, 10);
        assert_eq!(chat_hits.len(), 2);
        // No Airc origins admitted via the inbox path.
        let airc_hits = state.recall_by_origin_kind(EngramOriginKind::Airc, 10);
        assert!(airc_hits.is_empty());
        // Limit honored.
        assert_eq!(
            state.recall_by_origin_kind(EngramOriginKind::Chat, 1).len(),
            1
        );
        // Limit zero = empty.
        assert!(state
            .recall_by_origin_kind(EngramOriginKind::Chat, 0)
            .is_empty());
    }

    /// What this catches: EngramOriginKind::from(&EngramOrigin) covers
    /// every variant of EngramOrigin. If a future PR adds a new variant
    /// to EngramOrigin without updating the From impl, this test fails
    /// to compile (exhaustive match in From). The recall filter would
    /// otherwise silently miss the new origin variant.
    #[test]
    fn engram_origin_kind_covers_all_origin_variants() {
        // Construct one of each variant; `From` impl is exhaustive at
        // compile time. This test confirms the runtime mapping.
        let chat = synthetic_engram_with_chat_origin("x");
        let airc = synthetic_engram_with_airc_origin("y", "evt-1");
        assert_eq!(EngramOriginKind::from(&chat.origin), EngramOriginKind::Chat);
        assert_eq!(EngramOriginKind::from(&airc.origin), EngramOriginKind::Airc);
        // Tool + SelfReflection variants exist on EngramOrigin (per PR-1)
        // and are covered by the From impl's exhaustive match — no need
        // to construct them here; the compiler enforces coverage.
    }

    /// What this catches: Admit (NOT Quarantine) records BOTH content_hash
    /// AND event_id for AIRC origins. This is the regression-anchor for
    /// the refactor that split `record_engram_origin` → `record_admitted`
    /// + `record_replay_only`. If the refactor accidentally narrowed the
    /// Admit path's recording, dedup would silently break.
    #[test]
    fn admit_airc_origin_still_records_both_content_hash_and_event_id() {
        let state = AdmissionState::new(Arc::new(
            crate::persona::recall_metadata::RecallMetadataRegistry::new(),
        ));
        let event_id = "airc-msg-admit-1";
        let engram =
            synthetic_engram_with_airc_origin("valuable observation worth recalling", event_id);
        let content_hash = match &engram.origin {
            EngramOrigin::Airc(r) => r.content_hash.clone(),
            _ => unreachable!(),
        };
        let decision = AdmissionDecision::Admit {
            engram,
            why: "test admit".to_string(),
        };

        state.record_side_effects(&decision);

        assert!(
            state.is_event_seen(event_id),
            "airc-origin admit MUST record event_id"
        );
        assert!(
            state.is_content_seen(&content_hash),
            "airc-origin admit MUST record content_hash"
        );
        assert_eq!(state.engram_count(), 1, "admit MUST add to engram store");
    }

    // ── Persistence wire-up tests (#168) ──────────────────────────

    /// What this catches: admit observes through the persistence
    /// sink. The RecordingSink buffers each admission so the test
    /// can assert it landed.
    #[test]
    fn admit_observes_admission_through_persistence_sink() {
        use crate::persona::admission_persistence::RecordingSink;
        let registry = Arc::new(
            crate::persona::recall_metadata::RecallMetadataRegistry::new(),
        );
        let sink = Arc::new(RecordingSink::new());
        let state = AdmissionState::new_with_persistence(
            Arc::clone(&registry),
            Arc::clone(&sink) as Arc<dyn crate::persona::admission_persistence::AdmissionPersistenceSink>,
        );
        let msg = synthetic_human_message("watch me persist");
        state.admit(&msg, None).expect("admit");
        let seen = sink.admissions_seen();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].0.content, "watch me persist");
    }

    /// What this catches: recall_scored observes metadata updates
    /// through the persistence sink after each record_recall_hit.
    /// Hebbian rehearsal IS the disk-write trigger — every recall
    /// that lifts salience flushes the new value.
    #[test]
    fn recall_scored_observes_metadata_updates_through_sink() {
        use crate::persona::admission_persistence::RecordingSink;
        let registry = Arc::new(
            crate::persona::recall_metadata::RecallMetadataRegistry::new(),
        );
        let sink = Arc::new(RecordingSink::new());
        let state = AdmissionState::new_with_persistence(
            Arc::clone(&registry),
            Arc::clone(&sink) as Arc<dyn crate::persona::admission_persistence::AdmissionPersistenceSink>,
        );
        // Admit 2 engrams so recall has something to score.
        admit_n_distinct(
            &state,
            &[
                "first observation worth recall",
                "second observation worth recall",
            ],
        );
        // Clear the sink's admission buffer before scoring — we
        // want to assert only the metadata updates that follow.
        let _ = sink.admissions_seen(); // (just a sanity touch, no clear)
        let _scored = state.recall_scored(5_000, 8);
        let updates = sink.metadata_updates_seen();
        assert_eq!(
            updates.len(),
            2,
            "each recall hit observes a metadata update"
        );
    }

    /// What this catches: eval mutating the persona it measures. The
    /// isolation seam (checkpoint → mute persistence → admit → restore →
    /// unmute) must (1) let admit fire normally INTO the frozen frame so the
    /// measured memory motion is identical to a real turn, (2) keep every muted
    /// admit off the real sink, (3) rewind engram_count AND the dedup oracle on
    /// restore — proven by re-admitting the same content after restore and
    /// seeing it ADMIT (not dedup-drop) — and (4) restore the real sink so it
    /// observes again afterward. Regresses [[eval-mutates-persona-lift-needs-isolation]].
    #[test]
    fn eval_isolation_checkpoint_restore_leaves_no_trace() {
        use crate::persona::admission_persistence::{NoopSink, RecordingSink};
        let registry = Arc::new(
            crate::persona::recall_metadata::RecallMetadataRegistry::new(),
        );
        let real_sink = Arc::new(RecordingSink::new());
        let state = AdmissionState::new_with_persistence(
            Arc::clone(&registry),
            Arc::clone(&real_sink)
                as Arc<dyn crate::persona::admission_persistence::AdmissionPersistenceSink>,
        );

        // Non-trivial content: the admission gate drops short strings as
        // NotMemorable, so both arms use durable, distinct observations.
        let content_a = "the durable baseline observation worth keeping in memory";
        let content_b = "the eval-window observation that must never reach disk";

        // Baseline: one durable engram, observed by the real sink.
        state.admit(&synthetic_human_message(content_a), None).expect("admit A");
        assert_eq!(state.engram_count(), 1, "baseline admit lands");
        assert_eq!(real_sink.admissions_seen().len(), 1, "real sink saw A");

        // ── Begin isolation: checkpoint the frame, mute persistence. ──
        let checkpoint = state.checkpoint();
        let saved_real = state.swap_persistence(NoopSink::arc());

        // Admit INSIDE the window: admit fires (count climbs → identical
        // motion) but the real sink, now swapped out, sees nothing more.
        state.admit(&synthetic_human_message(content_b), None).expect("admit B");
        assert_eq!(state.engram_count(), 2, "muted admit still forms memory");
        assert_eq!(
            real_sink.admissions_seen().len(),
            1,
            "muted window writes NOTHING to the real sink"
        );

        // ── End isolation: rewind the frame, restore the real sink. ──
        state.restore(&checkpoint);
        state.swap_persistence(saved_real);
        assert_eq!(state.engram_count(), 1, "restore rewinds memory to baseline");

        // The dedup oracle rewound too: content_b is admissible again (had it
        // NOT rewound, this would dedup-drop and the count would stay 1).
        state.admit(&synthetic_human_message(content_b), None).expect("re-admit B");
        assert_eq!(state.engram_count(), 2, "dedup oracle rewound — B re-admits");
        assert_eq!(
            real_sink.admissions_seen().len(),
            2,
            "real sink restored — observes the post-window admit"
        );
    }

    /// What this catches: new_rehydrated populates the engram Vec
    /// + the metadata DashMap from loaded snapshots. Subsequent
    /// recall_scored sees those engrams and uses the loaded
    /// salience values. The proof that boot rehydration works.
    #[test]
    fn new_rehydrated_restores_engrams_and_metadata_for_recall() {
        let registry = Arc::new(
            crate::persona::recall_metadata::RecallMetadataRegistry::new(),
        );
        // Synthesize a couple of engrams + their metadata as if they
        // had been loaded from disk.
        let alpha_engram = synthetic_engram_with_chat_origin("alpha persisted");
        let beta_engram = synthetic_engram_with_chat_origin("beta persisted");
        let loaded_engrams = vec![alpha_engram.clone(), beta_engram.clone()];
        let high_salience = crate::persona::recall_metadata::RecallMetadata {
            salience: 0.9,
            access_count: 5,
            last_accessed_ms: 1_000,
            protected_until_ms: 0,
            last_decayed_ms: 1_000,
        };
        let low_salience = crate::persona::recall_metadata::RecallMetadata {
            salience: 0.2,
            access_count: 0,
            last_accessed_ms: 0,
            protected_until_ms: 0,
            last_decayed_ms: 1_000,
        };
        let loaded_metadata = vec![
            (alpha_engram.id, low_salience),
            (beta_engram.id, high_salience),
        ];

        let state = AdmissionState::new_rehydrated(
            Arc::clone(&registry),
            crate::persona::admission_persistence::NoopSink::arc(),
            loaded_engrams,
            loaded_metadata,
        );

        assert_eq!(state.engram_count(), 2, "engrams rehydrated into Vec");

        // recall_scored uses the loaded salience values — beta
        // (high salience) ranks before alpha (low salience).
        let scored = state.recall_scored(1_500, 8);
        assert_eq!(scored.len(), 2);
        assert_eq!(scored[0].0.id, beta_engram.id, "high-salience first");
        assert_eq!(scored[1].0.id, alpha_engram.id, "low-salience second");
    }

    /// What this catches: the phantom-engram-without-metadata case
    /// from Reviewer-2 BLOCK finding on PR #1519. If the engram-save
    /// succeeded but the metadata-save failed (the inline-comment
    /// claimed self-healing but the original code did NOT), the
    /// engram on disk would be permanently invisible to recall
    /// because `recall_scored` filter_maps engrams whose registry
    /// entry is missing. Slice B's fix: at rehydrate, seed
    /// `admit_with_defaults` for any loaded engram lacking metadata.
    ///
    /// This test simulates the crash by handing `new_rehydrated` a
    /// Vec with N engrams + a metadata vec with only K < N entries.
    /// The post-fix invariant: every loaded engram is recall-visible
    /// (with default metadata if its row was missing).
    // what this catches: set_recall_salience (#166) down-weights an admitted
    // engram's recall salience WITHOUT clobbering its other metadata — critically
    // last_decayed_ms, whose loss would trigger the epoch-delta decay collapse.
    #[test]
    fn set_recall_salience_lowers_salience_and_preserves_decay_clock() {
        let registry =
            Arc::new(crate::persona::recall_metadata::RecallMetadataRegistry::new());
        let state = AdmissionState::new(registry.clone());
        let id = Uuid::new_v4();
        // Seed as an ordinary admission does (default 0.5 salience, decay clock set).
        registry.admit_with_defaults(id);
        let before = registry.get(id).expect("seeded");
        assert_eq!(before.salience, 0.5);
        assert!(before.last_decayed_ms > 0, "decay clock initialized");

        state.set_recall_salience(id, PROPRIOCEPTION_RECALL_SALIENCE_FOR_TEST);
        let after = registry.get(id).expect("still present");
        assert_eq!(after.salience, PROPRIOCEPTION_RECALL_SALIENCE_FOR_TEST);
        assert_eq!(
            after.last_decayed_ms, before.last_decayed_ms,
            "decay clock preserved — no epoch-delta collapse"
        );
    }
    const PROPRIOCEPTION_RECALL_SALIENCE_FOR_TEST: f32 = 0.25;

    #[test]
    fn rehydrate_backfills_metadata_for_phantom_engrams() {
        let registry = Arc::new(
            crate::persona::recall_metadata::RecallMetadataRegistry::new(),
        );
        let phantom_engram = synthetic_engram_with_chat_origin("phantom no metadata row");
        let healthy_engram = synthetic_engram_with_chat_origin("healthy has metadata");
        let phantom_id = phantom_engram.id;
        let healthy_id = healthy_engram.id;

        // Only healthy has a metadata entry — simulates the crash
        // window between engram-save and metadata-save.
        let metadata_pair = (
            healthy_id,
            crate::persona::recall_metadata::RecallMetadata {
                salience: 0.8,
                access_count: 2,
                last_accessed_ms: 1_000,
                protected_until_ms: 0,
                last_decayed_ms: 1_000,
            },
        );

        let state = AdmissionState::new_rehydrated(
            Arc::clone(&registry),
            crate::persona::admission_persistence::NoopSink::arc(),
            vec![phantom_engram, healthy_engram],
            vec![metadata_pair],
        );

        assert_eq!(state.engram_count(), 2, "both engrams in Vec");

        // The phantom MUST appear in scored recall — with default
        // salience since its row was missing. Without the fix, this
        // assertion fails because the phantom is permanently
        // invisible to filter_map.
        let scored = state.recall_scored(2_000, 8);
        assert_eq!(scored.len(), 2, "both engrams recall-visible after rehydration");

        let scored_ids: std::collections::BTreeSet<Uuid> =
            scored.iter().map(|(e, _)| e.id).collect();
        assert!(
            scored_ids.contains(&phantom_id),
            "phantom must be recall-visible (default metadata seeded by backfill)"
        );
        assert!(
            scored_ids.contains(&healthy_id),
            "healthy must remain recall-visible"
        );

        // Healthy keeps its loaded high salience; phantom got the
        // default 0.5 from admit_with_defaults — healthy ranks higher.
        let healthy_score = scored.iter().find(|(e, _)| e.id == healthy_id).unwrap().1;
        let phantom_score = scored.iter().find(|(e, _)| e.id == phantom_id).unwrap().1;
        assert!(
            healthy_score > phantom_score,
            "loaded metadata wins over seeded default: healthy {} vs phantom {}",
            healthy_score,
            phantom_score
        );
    }

    /// What this catches: AdmissionState::for_persona opens a
    /// per-persona SQLite, admits land in it, and a fresh
    /// AdmissionState constructed from the SAME PersonaHome
    /// rehydrates those admissions. Round-trip via real disk +
    /// per-persona scoping in one test.
    #[tokio::test]
    async fn for_persona_round_trips_admissions_via_per_persona_sqlite() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let home = crate::persona::home::PersonaHome::for_persona(tmp.path(), "Paige");

        // ── Lifetime 1: admit through the persona's home ────────
        let original_ids: Vec<Uuid> = {
            let registry = Arc::new(
                crate::persona::recall_metadata::RecallMetadataRegistry::new(),
            );
            let state = AdmissionState::for_persona(&home, Arc::clone(&registry))
                .await
                .expect("for_persona setup");

            let messages = [
                "paige learns alpha for real",
                "paige learns beta for real",
            ];
            let mut ids = Vec::new();
            for content in &messages {
                let decision = state
                    .admit(&synthetic_human_message(content), None)
                    .expect("admit");
                match decision {
                    AdmissionDecision::Admit { engram, .. } => ids.push(engram.id),
                    other => panic!("expected Admit got {other:?}"),
                }
            }
            ids
            // state dropped here
        };

        // Wait for fire-and-forget writes to land.
        let mut tries = 0;
        let registry2 = Arc::new(
            crate::persona::recall_metadata::RecallMetadataRegistry::new(),
        );
        let state2 = loop {
            let st = AdmissionState::for_persona(&home, Arc::clone(&registry2))
                .await
                .expect("for_persona lifetime 2");
            if st.engram_count() == 2 {
                break st;
            }
            tries += 1;
            if tries > 100 {
                panic!("persistent engrams never rehydrated after 100 yields");
            }
            tokio::task::yield_now().await;
        };

        let scored = state2.recall_scored(10_000, 8);
        let scored_ids: std::collections::BTreeSet<Uuid> =
            scored.iter().map(|(e, _)| e.id).collect();
        let original_set: std::collections::BTreeSet<Uuid> =
            original_ids.iter().copied().collect();
        assert_eq!(scored_ids, original_set);

    }

    /// What this catches: two personas under the same continuum_root
    /// get fully isolated stores. An engram admitted by Paige does
    /// not appear in Niko's recall, and vice-versa. The first
    /// defense for the chain-of-custody design's per-citizen scoping.
    #[tokio::test]
    async fn for_persona_isolates_two_personas_at_the_storage_layer() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let paige_home =
            crate::persona::home::PersonaHome::for_persona(tmp.path(), "Paige");
        let niko_home =
            crate::persona::home::PersonaHome::for_persona(tmp.path(), "Niko");

        // Admit through Paige's home only.
        let paige_id = {
            let registry = Arc::new(
                crate::persona::recall_metadata::RecallMetadataRegistry::new(),
            );
            let state = AdmissionState::for_persona(&paige_home, registry)
                .await
                .expect("paige setup");
            let decision = state
                .admit(
                    &synthetic_human_message("only paige sees this engram"),
                    None,
                )
                .expect("paige admit");
            match decision {
                AdmissionDecision::Admit { engram, .. } => engram.id,
                other => panic!("expected Admit got {other:?}"),
            }
        };

        // Wait for Paige's fire-and-forget write to land.
        let mut tries = 0;
        loop {
            let paige_registry = Arc::new(
                crate::persona::recall_metadata::RecallMetadataRegistry::new(),
            );
            let paige_state =
                AdmissionState::for_persona(&paige_home, paige_registry)
                    .await
                    .expect("paige reload");
            if paige_state.engram_count() == 1 {
                break;
            }
            tries += 1;
            if tries > 100 {
                panic!("paige's engram never landed");
            }
            tokio::task::yield_now().await;
        }

        // Niko's fresh state must NOT see Paige's engram.
        let niko_registry = Arc::new(
            crate::persona::recall_metadata::RecallMetadataRegistry::new(),
        );
        let niko_state = AdmissionState::for_persona(&niko_home, niko_registry)
            .await
            .expect("niko setup");
        assert_eq!(
            niko_state.engram_count(),
            0,
            "Niko's home is independent of Paige's — no cross-persona engram leak"
        );

        // And Niko's scored recall returns nothing.
        let scored = niko_state.recall_scored(10_000, 8);
        assert!(
            scored.is_empty(),
            "Niko's recall is empty: paige's engram {paige_id} stayed scoped to her home"
        );

    }

    // What this catches: the per-task eval rewind invariant (cognition/eval.rs
    // run_pass calls `isolation.rewind()` before EVERY task). An engram admitted
    // while measuring task N — e.g. the act->observe self-observation from a
    // code/search she ran — must NOT survive into task N+1's recall, or the exam
    // bleeds ("based on my earlier code search, SELF_TICK_MS is in…" leaking into
    // an unrelated task, observed live 2026-07-02). checkpoint()/restore() is the
    // mechanism the rewind uses: restoring the pre-task checkpoint discards every
    // engram admitted since, while PRESERVING the baseline the fork was born with
    // (her real engrams via fork_detached) — mirror her reality, drop only the
    // cross-task bleed. See [[mirror-and-challenge-during-training-and-dream]].
    #[test]
    fn restore_drops_post_checkpoint_engrams_but_keeps_baseline() {
        let state = AdmissionState::new(Arc::new(
            crate::persona::recall_metadata::RecallMetadataRegistry::new(),
        ));
        let mut trace = CognitionTrace::new();

        // Baseline: one engram present when the "task" begins (stands in for the
        // real engrams the eval fork is born carrying).
        let baseline = "a durable baseline observation the fork was born carrying";
        state
            .admit(&synthetic_human_message(baseline), Some(&mut trace))
            .unwrap();
        assert_eq!(state.engram_count(), 1);

        // Checkpoint = the pre-task frame the rewind restores to.
        let cp = state.checkpoint();

        // Task N admits a fresh engram (the act->observe result-as-engram).
        let task_n = "task N just ran code/search and admitted this result engram";
        state
            .admit(&synthetic_human_message(task_n), Some(&mut trace))
            .unwrap();
        assert_eq!(state.engram_count(), 2);
        assert!(
            state.recall_recent(8).iter().any(|e| e.content == task_n),
            "task N's engram is recallable within task N (sanity)"
        );

        // Rewind before task N+1.
        state.restore(&cp);

        // Task N's engram is gone; the baseline reality survives untouched.
        assert_eq!(state.engram_count(), 1, "rewind drops the post-checkpoint engram");
        let recalled = state.recall_recent(8);
        assert!(
            !recalled.iter().any(|e| e.content == task_n),
            "task N's engram must NOT bleed into task N+1"
        );
        assert!(
            recalled.iter().any(|e| e.content == baseline),
            "the pre-eval baseline reality is preserved (mirror, not sterilize)"
        );
    }
}
