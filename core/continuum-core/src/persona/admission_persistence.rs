//! `AdmissionPersistenceSink` — the adapter-first persistence rail
//! for AdmissionState.
//!
//! Per [[organization-purity-as-we-migrate]] + Joel's adapter-first
//! methodology ("code the adapters even if there's just ONE to start,
//! that is how I do it"): persistence is a separate concern from
//! admission. AdmissionState observes admissions + metadata updates;
//! impls of this trait choose what to do with those observations.
//!
//! ### Why a trait
//!
//! - `NoopSink` — current substrate behavior. In-memory only. Test
//!   fixtures + replay paths that don't care about durability use
//!   this.
//! - `OrmPersistenceSink` — production. Fire-and-forget writes
//!   through `OrmStore<Engram>` + `OrmStore<EngramRecallMetadata>`
//!   via `tokio::spawn`. Admit/recall_scored return immediately;
//!   the spawned task does the disk I/O.
//! - `RecordingSink` — tests that need to assert "this engram was
//!   observed for persistence" without involving real disk.
//! - Future: `BatchingSink` that buffers writes for N ms before
//!   flushing — the obvious next adapter when latency telemetry
//!   asks for it.
//!
//! ### Fire-and-forget vs synchronous semantics
//!
//! `observe_admission` + `observe_metadata_update` are sync (return
//! `()`). AdmissionState's hot path stays sync. The trait's impls
//! handle async dispatch internally — production fires through
//! `tokio::spawn`, tests buffer in memory.
//!
//! The durability tradeoff: under tokio runtime shutdown, fire-and-
//! forget writes may not complete. For substrate-correctness this is
//! acceptable for v1 — the cost of a crash during the brief window
//! between admit and disk-write is bounded (a few engrams, no worse
//! than restart-amnesia of today). A future BatchingSink with
//! explicit drain-on-shutdown semantics removes even that window.

use std::sync::Arc;

use async_trait::async_trait;
use dashmap::DashMap;
use uuid::Uuid;

use crate::orm::{OrmStore, OrmStoreError};
use crate::persona::engram::Engram;
use crate::persona::recall_metadata::{EngramRecallMetadata, RecallMetadata};

/// Adapter-first persistence rail. Impls choose synchronously /
/// fire-and-forget / batched / no-op.
pub trait AdmissionPersistenceSink: Send + Sync {
    /// Human-readable impl name. Useful for telemetry + log lines.
    fn name(&self) -> &'static str;

    /// Called from `AdmissionState::admit` after the in-memory write
    /// succeeds. The metadata snapshot reflects the engram's initial
    /// salience + protection window.
    fn observe_admission(&self, engram: &Engram, metadata: RecallMetadata);

    /// Called from `AdmissionState::recall_scored` after each
    /// `record_recall_hit` mutates the in-memory DashMap. The
    /// metadata snapshot reflects the post-mutation state.
    fn observe_metadata_update(&self, engram_id: Uuid, metadata: RecallMetadata);

    /// Called from `AdmissionState::redact` after an already-admitted engram's
    /// `content`/`recall_keys` are rewritten in place (a policy scrubbed an
    /// answer key / secret / PII out of it). This is a CONTENT update, not a new
    /// admission and not a metadata update — the engram keeps its id, salience,
    /// and recall history; only its text changed. The durable row must be
    /// re-saved so the scrub survives restart (else the un-redacted content
    /// rehydrates from disk).
    ///
    /// Default is a no-op so in-memory-only sinks (Noop) and any future adapter
    /// stay correct without change; persistence-backed sinks override it.
    fn observe_content_update(&self, _engram: &Engram) {}
}

// ─── NoopSink ──────────────────────────────────────────────────────────

/// In-memory-only sink. Does nothing. The default + the current
/// substrate behavior preserved for tests + replay paths that don't
/// involve persistence.
pub struct NoopSink;

impl AdmissionPersistenceSink for NoopSink {
    fn name(&self) -> &'static str {
        "noop"
    }
    fn observe_admission(&self, _engram: &Engram, _metadata: RecallMetadata) {}
    fn observe_metadata_update(&self, _engram_id: Uuid, _metadata: RecallMetadata) {}
}

impl NoopSink {
    pub fn arc() -> Arc<dyn AdmissionPersistenceSink> {
        Arc::new(NoopSink)
    }
}

// ─── OrmPersistenceSink — production ───────────────────────────────────

/// Production sink. Fire-and-forget writes through OrmStore<Engram>
/// + OrmStore<EngramRecallMetadata> via `tokio::spawn`. The hot
/// path stays sync; the disk write happens on a background task.
///
/// **Failure-handling:** spawned tasks that fail log the error via
/// `tracing::warn` and proceed. Per [[every-error-is-an-opportunity-
/// to-battle-harden]], we surface the failure visibly rather than
/// swallow — operators see "engram X failed to persist" in logs and
/// can intervene. Later impls (BatchingSink with retry) close the
/// gap further.
pub struct OrmPersistenceSink {
    engram_store: Arc<OrmStore<Engram>>,
    metadata_store: Arc<OrmStore<EngramRecallMetadata>>,
    /// Maps `engram_id → metadata row_id`. Built on admission (when we
    /// know both) + populated from `OrmLoader::load_admission_state` at
    /// boot. Lets `observe_metadata_update` do an O(1) lookup +
    /// targeted `update()` instead of the O(N) `find_all` scan the
    /// first iteration used. Fixes the UNIQUE-race the reviewer flagged
    /// (concurrent recall hits no longer both decide to INSERT).
    row_id_by_engram: DashMap<Uuid, Uuid>,
}

impl OrmPersistenceSink {
    pub fn new(
        engram_store: Arc<OrmStore<Engram>>,
        metadata_store: Arc<OrmStore<EngramRecallMetadata>>,
    ) -> Self {
        Self {
            engram_store,
            metadata_store,
            row_id_by_engram: DashMap::new(),
        }
    }

    pub fn arc(
        engram_store: Arc<OrmStore<Engram>>,
        metadata_store: Arc<OrmStore<EngramRecallMetadata>>,
    ) -> Arc<dyn AdmissionPersistenceSink> {
        Arc::new(Self::new(engram_store, metadata_store))
    }

    /// Pre-seed the engram_id → row_id cache from a loaded snapshot.
    /// Called by `OrmLoader::load_admission_state` (or any boot path
    /// reading existing rows) so the first `observe_metadata_update`
    /// after boot already knows which row to update.
    pub fn prime_cache(&self, pairs: impl IntoIterator<Item = (Uuid, Uuid)>) {
        for (engram_id, row_id) in pairs {
            self.row_id_by_engram.insert(engram_id, row_id);
        }
    }
}

impl AdmissionPersistenceSink for OrmPersistenceSink {
    fn name(&self) -> &'static str {
        "orm"
    }

    fn observe_admission(&self, engram: &Engram, metadata: RecallMetadata) {
        let engram = engram.clone();
        let metadata_row = EngramRecallMetadata::for_new_row(engram.id, metadata);
        let row_id = uuid::Uuid::parse_str(&metadata_row.base.id)
            .expect("BaseEntity::for_new_record always produces a valid UUID");
        let engram_id = engram.id;
        // Cache the engram_id → row_id mapping NOW (before spawn) so
        // any concurrent `observe_metadata_update` for this engram_id
        // finds the cached row_id and does a targeted update instead
        // of racing on insert. Even if the disk write fails later,
        // the cache reflects "we intend this row_id"; failure logs
        // surface the inconsistency.
        self.row_id_by_engram.insert(engram_id, row_id);
        let engram_store = Arc::clone(&self.engram_store);
        let metadata_store = Arc::clone(&self.metadata_store);
        tokio::spawn(async move {
            if let Err(e) = engram_store.save(engram_id, &engram).await {
                tracing::warn!(
                    engram_id = %engram_id,
                    error = %e,
                    "OrmPersistenceSink: engram save failed"
                );
                return;
            }
            if let Err(e) = metadata_store.save(row_id, &metadata_row).await {
                tracing::warn!(
                    engram_id = %engram_id,
                    row_id = %row_id,
                    error = %e,
                    "OrmPersistenceSink: metadata save failed — engram persisted but its metadata didn't; \
                     boot rehydration's phantom-engram backfill will seed default metadata on next load"
                );
            }
        });
    }

    fn observe_metadata_update(&self, engram_id: Uuid, metadata: RecallMetadata) {
        // O(1) lookup via the cache populated on admission +
        // prime_cache. The UNIQUE-race the first iteration had
        // (two concurrent updates both inserting on the same
        // engram_id) is gone — we ALWAYS know the row_id, so the
        // operation is a deterministic `update` against that row_id.
        // If the cache somehow doesn't have the row_id (e.g.,
        // observe_metadata_update called before observe_admission
        // for this engram, which shouldn't happen but we surface
        // rather than swallow), we log and skip — admit will
        // eventually fire and seed the cache.
        let Some(row_id_entry) = self.row_id_by_engram.get(&engram_id) else {
            tracing::debug!(
                engram_id = %engram_id,
                "OrmPersistenceSink: observe_metadata_update before observe_admission — \
                 skipping; the metadata row will be created when admission fires"
            );
            return;
        };
        let row_id = *row_id_entry.value();
        drop(row_id_entry);
        let metadata_store = Arc::clone(&self.metadata_store);
        tokio::spawn(async move {
            let updated_row = EngramRecallMetadataPatch {
                base_id: row_id,
                engram_id,
                metadata,
            };
            if let Err(e) = update_metadata_row(metadata_store.as_ref(), updated_row).await {
                tracing::warn!(
                    engram_id = %engram_id,
                    row_id = %row_id,
                    error = %e,
                    "OrmPersistenceSink: metadata update failed"
                );
            }
        });
    }

    fn observe_content_update(&self, engram: &Engram) {
        // Re-save the engram row under its EXISTING id (OrmStore::save is an
        // upsert keyed by engram_id, same call the admission path uses). Only
        // the content/recall_keys changed; metadata is untouched, so we do not
        // touch the metadata store or the row_id cache. Fire-and-forget, same
        // as admission — the redaction command's report reflects the in-memory
        // rewrite; this makes it durable.
        let engram = engram.clone();
        let engram_id = engram.id;
        let engram_store = Arc::clone(&self.engram_store);
        tokio::spawn(async move {
            if let Err(e) = engram_store.save(engram_id, &engram).await {
                tracing::warn!(
                    engram_id = %engram_id,
                    error = %e,
                    "OrmPersistenceSink: redaction content-update save failed — \
                     the un-redacted content will rehydrate on next boot; re-run redaction"
                );
            }
        });
    }
}

/// Helper struct so the async task can take a tidy bundle of values
/// without re-deriving them inside the closure.
struct EngramRecallMetadataPatch {
    base_id: Uuid,
    engram_id: Uuid,
    metadata: RecallMetadata,
}

/// Update a known metadata row in place. The row_id is the
/// BaseEntity.id; we look up the existing record to preserve its
/// `createdAt` (lookup is O(1) via find_by_id), then update with
/// the new metadata fields. If the row went missing between the
/// cache insert and this update — unexpected but possible if a
/// concurrent delete fires — we fall back to creating a fresh row
/// with the same row_id and engram_id.
async fn update_metadata_row(
    store: &OrmStore<EngramRecallMetadata>,
    patch: EngramRecallMetadataPatch,
) -> Result<(), OrmStoreError> {
    let existing = store.find_by_id(patch.base_id).await?;
    match existing {
        Some(mut row) => {
            row.salience = patch.metadata.salience;
            row.access_count = patch.metadata.access_count;
            row.last_accessed_ms = patch.metadata.last_accessed_ms;
            row.protected_until_ms = patch.metadata.protected_until_ms;
            row.last_decayed_ms = patch.metadata.last_decayed_ms;
            store.update(patch.base_id, &row).await
        }
        None => {
            // Row not found — concurrent delete or never created.
            // Recreate at the cached row_id to preserve the
            // engram_id → row_id mapping (otherwise the next update
            // would orphan the new row).
            let mut row = EngramRecallMetadata::for_new_row(patch.engram_id, patch.metadata);
            row.base.id = patch.base_id.to_string();
            store.save(patch.base_id, &row).await
        }
    }
}

// ─── RecordingSink — tests ──────────────────────────────────────────────

/// Test-only sink that buffers observations in memory. Lets tests
/// assert "this engram was observed for persistence" without
/// involving real disk or a tokio runtime.
///
/// Per [[test-fixtures-are-system-primitives]]: this lives at the
/// system level, ubiquitous across every test in the substrate,
/// not bespoke per test module.
pub struct RecordingSink {
    admissions: std::sync::Mutex<Vec<(Engram, RecallMetadata)>>,
    metadata_updates: std::sync::Mutex<Vec<(Uuid, RecallMetadata)>>,
    content_updates: std::sync::Mutex<Vec<Engram>>,
}

impl RecordingSink {
    pub fn new() -> Self {
        Self {
            admissions: std::sync::Mutex::new(Vec::new()),
            metadata_updates: std::sync::Mutex::new(Vec::new()),
            content_updates: std::sync::Mutex::new(Vec::new()),
        }
    }

    pub fn arc() -> Arc<dyn AdmissionPersistenceSink> {
        Arc::new(Self::new())
    }

    pub fn admissions_seen(&self) -> Vec<(Engram, RecallMetadata)> {
        self.admissions.lock().unwrap().clone()
    }

    pub fn metadata_updates_seen(&self) -> Vec<(Uuid, RecallMetadata)> {
        self.metadata_updates.lock().unwrap().clone()
    }

    /// Engrams whose content was re-saved via `observe_content_update`
    /// (redaction rewrote them). Lets a test assert the durable-rewrite fired.
    pub fn content_updates_seen(&self) -> Vec<Engram> {
        self.content_updates.lock().unwrap().clone()
    }
}

impl Default for RecordingSink {
    fn default() -> Self {
        Self::new()
    }
}

impl AdmissionPersistenceSink for RecordingSink {
    fn name(&self) -> &'static str {
        "recording"
    }
    fn observe_admission(&self, engram: &Engram, metadata: RecallMetadata) {
        self.admissions
            .lock()
            .unwrap()
            .push((engram.clone(), metadata));
    }
    fn observe_metadata_update(&self, engram_id: Uuid, metadata: RecallMetadata) {
        self.metadata_updates
            .lock()
            .unwrap()
            .push((engram_id, metadata));
    }
    fn observe_content_update(&self, engram: &Engram) {
        self.content_updates.lock().unwrap().push(engram.clone());
    }
}

// ─── Boot rehydration ──────────────────────────────────────────────────

/// Load all engrams + recall metadata from disk. Called at persona
/// boot to rehydrate AdmissionState's in-memory Vec + DashMap.
///
/// The result returns engrams in admission order (the adapter's
/// query without ORDER BY uses insertion order for SQLite, which
/// matches our admit() insertion path) + metadata as (engram_id,
/// RecallMetadata) pairs.
///
/// Missing-metadata case: if an engram exists but its metadata row
/// doesn't, the engram is loaded with default RecallMetadata. This
/// can happen if persistence crashed between the engram save and the
/// metadata save in OrmPersistenceSink — the decay tick will
/// resurface the engram with default metadata, no data loss at the
/// engram layer.
#[async_trait]
pub trait AdmissionPersistenceLoader: Send + Sync {
    async fn load_admission_state(
        &self,
    ) -> Result<(Vec<Engram>, Vec<(Uuid, RecallMetadata)>), OrmStoreError>;
}

pub struct OrmLoader {
    engram_store: Arc<OrmStore<Engram>>,
    metadata_store: Arc<OrmStore<EngramRecallMetadata>>,
}

impl OrmLoader {
    pub fn new(
        engram_store: Arc<OrmStore<Engram>>,
        metadata_store: Arc<OrmStore<EngramRecallMetadata>>,
    ) -> Self {
        Self {
            engram_store,
            metadata_store,
        }
    }
}

#[async_trait]
impl AdmissionPersistenceLoader for OrmLoader {
    async fn load_admission_state(
        &self,
    ) -> Result<(Vec<Engram>, Vec<(Uuid, RecallMetadata)>), OrmStoreError> {
        let (engrams, metadata, _) = self.load_with_row_ids().await?;
        Ok((engrams, metadata))
    }
}

impl OrmLoader {
    /// Same as `load_admission_state` but also returns the
    /// `engram_id → row_id` pairs so callers can prime the
    /// OrmPersistenceSink's cache. Skipping this prime would force the
    /// first observe_metadata_update per engram after boot to log a
    /// "row not found" miss (now that observe_metadata_update no
    /// longer scans the whole table).
    pub async fn load_with_row_ids(
        &self,
    ) -> Result<(Vec<Engram>, Vec<(Uuid, RecallMetadata)>, Vec<(Uuid, Uuid)>), OrmStoreError> {
        let engrams_with_ids = self.engram_store.find_all().await?;
        let metadata_with_ids = self.metadata_store.find_all().await?;
        let engrams: Vec<Engram> = engrams_with_ids.into_iter().map(|(_, e)| e).collect();
        let mut metadata: Vec<(Uuid, RecallMetadata)> = Vec::with_capacity(metadata_with_ids.len());
        let mut row_id_pairs: Vec<(Uuid, Uuid)> = Vec::with_capacity(metadata_with_ids.len());
        for (row_id, row) in metadata_with_ids {
            row_id_pairs.push((row.engram_id, row_id));
            metadata.push(row.into());
        }
        Ok((engrams, metadata, row_id_pairs))
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persona::engram::{AircMessageRef, EngramKind, EngramOrigin, TrustState};

    fn sample_engram(content: &str) -> Engram {
        Engram {
            context_id: None,
            id: Uuid::new_v4(),
            kind: EngramKind::Episodic,
            content: content.to_string(),
            origin: EngramOrigin::Airc(AircMessageRef {
                transport: "airc".to_string(),
                room_id: "general".to_string(),
                message_id: format!("msg-{}", Uuid::new_v4()),
                sender_id: "airc-test".to_string(),
                sent_at_ms: 1_000,
                received_at_ms: 1_000,
                content_hash: "sha256:test".to_string(),
                signature: "sig-test".to_string(),
                proof_refs: vec![],
                schema_version: "v1".to_string(),
                client_name: Some("test".to_string()),
            }),
            recall_keys: vec![],
            admitted_at_ms: 1_000,
            trust_state_at_admission: TrustState::ApprovedPeer,
            admission_trace_id: None,
        }
    }

    fn sample_metadata() -> RecallMetadata {
        RecallMetadata {
            salience: 0.5,
            access_count: 0,
            last_accessed_ms: 0,
            protected_until_ms: 0,
            last_decayed_ms: 1_000,
        }
    }

    /// What this catches: NoopSink does nothing (the safety net for
    /// substrate paths that don't want persistence).
    #[test]
    fn noop_sink_observes_silently() {
        let sink = NoopSink;
        sink.observe_admission(&sample_engram("noop"), sample_metadata());
        sink.observe_metadata_update(Uuid::new_v4(), sample_metadata());
        assert_eq!(sink.name(), "noop");
    }

    /// What this catches: RecordingSink buffers admissions in order
    /// and exposes them for assertion. Foundation for any test that
    /// needs to verify "admission produced a persistence observation."
    #[test]
    fn recording_sink_buffers_admissions_in_order() {
        let sink = RecordingSink::new();
        let a = sample_engram("alpha");
        let b = sample_engram("beta");
        sink.observe_admission(&a, sample_metadata());
        sink.observe_admission(&b, sample_metadata());
        let seen = sink.admissions_seen();
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[0].0.id, a.id);
        assert_eq!(seen[1].0.id, b.id);
    }

    /// What this catches: RecordingSink buffers metadata updates
    /// separately from admissions.
    #[test]
    fn recording_sink_buffers_metadata_updates_separately() {
        let sink = RecordingSink::new();
        let engram_id = Uuid::new_v4();
        sink.observe_metadata_update(engram_id, sample_metadata());
        assert_eq!(sink.metadata_updates_seen().len(), 1);
        assert_eq!(sink.metadata_updates_seen()[0].0, engram_id);
        assert!(sink.admissions_seen().is_empty());
    }

    /// What this catches: **engrams survive process restart**. The
    /// proof point this whole multi-slice arc was building toward.
    ///
    /// Flow: build AdmissionState with OrmPersistenceSink against a
    /// real SQLite db; admit two engrams; wait for fire-and-forget
    /// writes to land; drop the AdmissionState (simulates restart);
    /// load engrams + metadata from disk via OrmLoader; build a
    /// fresh AdmissionState via `new_rehydrated`; verify recall sees
    /// the original engrams.
    ///
    /// When this test passes, the substrate's continual-learning
    /// property compounds across process boundaries for the first
    /// time. The "every pair-programming session starts amnesic"
    /// problem is structurally solved.
    #[tokio::test]
    async fn engrams_survive_process_restart_via_orm_persistence() {
        use crate::orm::adapter::{AdapterConfig, StorageAdapter};
        use crate::orm::sqlite::SqliteAdapter;
        use crate::orm::OrmStore;
        use crate::persona::admission_state::AdmissionState;
        use crate::persona::recall_metadata::RecallMetadataRegistry;
        use crate::persona::types::InboxMessage;

        // Set up SQLite + OrmStores + the production sink + loader.
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("persona-home.sqlite");
        let mut adapter = SqliteAdapter::new();
        let mut config = AdapterConfig::default();
        config.connection_string = path.to_string_lossy().into_owned();
        adapter.initialize(config).await.expect("adapter init");
        let adapter: Arc<dyn StorageAdapter> = Arc::new(adapter);
        let engrams = Arc::new(OrmStore::<Engram>::new(Arc::clone(&adapter)).await.unwrap());
        let metadata = Arc::new(
            OrmStore::<EngramRecallMetadata>::new(Arc::clone(&adapter))
                .await
                .unwrap(),
        );
        let sink: Arc<dyn AdmissionPersistenceSink> =
            OrmPersistenceSink::arc(Arc::clone(&engrams), Arc::clone(&metadata));
        let loader = OrmLoader::new(Arc::clone(&engrams), Arc::clone(&metadata));

        // ── Lifetime 1: admit two engrams ─────────────────────────
        let original_engram_ids: Vec<Uuid> = {
            let registry = Arc::new(RecallMetadataRegistry::new());
            let state =
                AdmissionState::new_with_persistence(Arc::clone(&registry), Arc::clone(&sink));
            // Admit two distinct messages.
            let m1 = InboxMessage {
                id: Uuid::new_v4(),
                room_id: Uuid::new_v4(),
                sender_id: Uuid::new_v4(),
                sender_name: "operator".to_string(),
                sender_type: crate::persona::types::SenderType::Human,
                content: "persistence proof engram alpha".to_string(),
                timestamp: 1_000,
                priority: 0.5,
                source_modality: None,
                voice_session_id: None,
            };
            let m2 = InboxMessage {
                id: Uuid::new_v4(),
                room_id: Uuid::new_v4(),
                sender_id: Uuid::new_v4(),
                sender_name: "operator".to_string(),
                sender_type: crate::persona::types::SenderType::Human,
                content: "persistence proof engram beta".to_string(),
                timestamp: 2_000,
                priority: 0.5,
                source_modality: None,
                voice_session_id: None,
            };
            let d1 = state.admit(&m1, None).expect("admit m1");
            let d2 = state.admit(&m2, None).expect("admit m2");
            let id1 = match d1 {
                crate::persona::engram::AdmissionDecision::Admit { engram, .. } => engram.id,
                other => panic!("expected Admit got {other:?}"),
            };
            let id2 = match d2 {
                crate::persona::engram::AdmissionDecision::Admit { engram, .. } => engram.id,
                other => panic!("expected Admit got {other:?}"),
            };
            vec![id1, id2]
            // state dropped here — simulates process exit
        };

        // Wait for fire-and-forget writes to land. Yield until the
        // loader sees both engrams, capped to avoid runaway.
        let mut tries = 0;
        let (loaded_engrams, loaded_metadata) = loop {
            let (es, ms) = loader.load_admission_state().await.expect("load");
            if es.len() == 2 && ms.len() == 2 {
                break (es, ms);
            }
            tries += 1;
            if tries > 100 {
                panic!(
                    "writes never landed: engrams={} metadata={} after 100 yields",
                    es.len(),
                    ms.len()
                );
            }
            tokio::task::yield_now().await;
        };

        // ── Lifetime 2: rehydrate ─────────────────────────────────
        let registry2 = Arc::new(RecallMetadataRegistry::new());
        let state2 = AdmissionState::new_rehydrated(
            Arc::clone(&registry2),
            // Noop sink for the rehydrate-then-test path — the test
            // is asserting "what was on disk is now in memory", not
            // exercising another round-trip.
            NoopSink::arc(),
            loaded_engrams.clone(),
            loaded_metadata,
        );
        assert_eq!(state2.engram_count(), 2, "rehydrated 2 engrams");

        // Recall must see the originals.
        let scored = state2.recall_scored(10_000, 8);
        let scored_ids: std::collections::BTreeSet<Uuid> =
            scored.iter().map(|(e, _)| e.id).collect();
        let original_ids: std::collections::BTreeSet<Uuid> =
            original_engram_ids.iter().copied().collect();
        assert_eq!(
            scored_ids, original_ids,
            "recall after restart returns the originally-admitted engram ids"
        );
    }

    /// What this catches: the PORTABILITY invariant for a persona's
    /// context-keyed memory — "move the home, keep the self." A persona's
    /// `(token, engram_db)` lives in one home; its identity resolves from that
    /// home (airc-lib's `attach_as`), so moving the home to another node and
    /// re-spawning yields the same individual with the same memory. This proves
    /// the memory half locally: write an engram tagged with its room
    /// (contextId), drop the store (the home goes quiet / moves), re-open
    /// `OrmStore<Engram>` from the SAME path (spawn-from-the-moved-home), and
    /// confirm the engram returns with its room intact. This is the exact shape
    /// of the eventual cross-node M5⇄BigMama persona move. See
    /// docs/architecture/IDENTITY-SCOPE-PEER-LIVENESS-MODEL.md Part A.
    #[tokio::test]
    async fn engram_context_survives_home_reopen_portability_proof() {
        use crate::orm::adapter::{AdapterConfig, StorageAdapter};
        use crate::orm::sqlite::SqliteAdapter;
        use crate::orm::OrmStore;

        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("engrams.sqlite");
        let room = Uuid::new_v4();

        // ── Lifetime 1: persona alive on "node A" — admit a room-keyed engram ──
        let engram_id = {
            let mut adapter = SqliteAdapter::new();
            let mut config = AdapterConfig::default();
            config.connection_string = path.to_string_lossy().into_owned();
            adapter.initialize(config).await.expect("adapter init");
            let adapter: Arc<dyn StorageAdapter> = Arc::new(adapter);
            let store = OrmStore::<Engram>::new(adapter).await.unwrap();

            let mut engram = sample_engram("portable memory");
            engram.context_id = Some(room);
            let id = engram.id;
            store.save(id, &engram).await.expect("save engram");
            id
            // store + adapter dropped here — the home goes quiet / moves nodes
        };

        // ── Lifetime 2: spawn from the SAME home on "node B" ──
        let mut adapter2 = SqliteAdapter::new();
        let mut config2 = AdapterConfig::default();
        config2.connection_string = path.to_string_lossy().into_owned();
        adapter2.initialize(config2).await.expect("adapter init 2");
        let adapter2: Arc<dyn StorageAdapter> = Arc::new(adapter2);
        let store2 = OrmStore::<Engram>::new(adapter2).await.unwrap();

        let loaded = store2
            .find_by_id(engram_id)
            .await
            .expect("find_by_id")
            .expect("engram present after re-opening the moved home");
        assert_eq!(
            loaded.context_id,
            Some(room),
            "the engram's room (contextId) must survive the home move — \
             memory arrives keyed by the same conversation"
        );
        assert_eq!(loaded.content, "portable memory");
    }

    /// What this catches: OrmPersistenceSink + OrmLoader form a
    /// complete persistence cycle. Admission observation lands on
    /// disk via the OrmStore; the loader reads it back. The proof
    /// point of the production sink — when this passes, the
    /// AdmissionState wire-up has a working persistence rail to
    /// plug into.
    #[tokio::test]
    async fn orm_persistence_sink_writes_then_loader_reads_back() {
        use crate::orm::adapter::{AdapterConfig, StorageAdapter};
        use crate::orm::sqlite::SqliteAdapter;
        use crate::orm::OrmStore;

        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("admission.sqlite");
        let mut adapter = SqliteAdapter::new();
        let mut config = AdapterConfig::default();
        config.connection_string = path.to_string_lossy().into_owned();
        adapter.initialize(config).await.expect("adapter init");
        let adapter: Arc<dyn StorageAdapter> = Arc::new(adapter);
        let engrams = Arc::new(OrmStore::<Engram>::new(Arc::clone(&adapter)).await.unwrap());
        let metadata = Arc::new(
            OrmStore::<EngramRecallMetadata>::new(Arc::clone(&adapter))
                .await
                .unwrap(),
        );

        let sink = OrmPersistenceSink::new(Arc::clone(&engrams), Arc::clone(&metadata));
        let engram = sample_engram("persist-me");
        let original_id = engram.id;
        sink.observe_admission(&engram, sample_metadata());

        // Fire-and-forget needs a moment for the spawned task to
        // complete its writes. Yield repeatedly until the load sees
        // the row, capped to avoid a runaway loop on real failure.
        let loader = OrmLoader::new(engrams, metadata);
        let mut tries = 0;
        loop {
            let (loaded_engrams, _) = loader.load_admission_state().await.unwrap();
            if !loaded_engrams.is_empty() {
                assert_eq!(loaded_engrams.len(), 1);
                assert_eq!(loaded_engrams[0].id, original_id);
                break;
            }
            tries += 1;
            if tries > 50 {
                panic!("spawned write never landed after 50 yields");
            }
            tokio::task::yield_now().await;
        }
    }
}
