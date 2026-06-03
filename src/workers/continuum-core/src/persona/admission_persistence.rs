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
}

impl OrmPersistenceSink {
    pub fn new(
        engram_store: Arc<OrmStore<Engram>>,
        metadata_store: Arc<OrmStore<EngramRecallMetadata>>,
    ) -> Self {
        Self {
            engram_store,
            metadata_store,
        }
    }

    pub fn arc(
        engram_store: Arc<OrmStore<Engram>>,
        metadata_store: Arc<OrmStore<EngramRecallMetadata>>,
    ) -> Arc<dyn AdmissionPersistenceSink> {
        Arc::new(Self::new(engram_store, metadata_store))
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
                    "OrmPersistenceSink: metadata save failed (engram saved, metadata lost — next decay tick will resurface this)"
                );
            }
        });
    }

    fn observe_metadata_update(&self, engram_id: Uuid, metadata: RecallMetadata) {
        // For metadata updates we don't know the row's BaseEntity id
        // up front — record_recall_hit / apply_decay mutate the
        // DashMap by engram_id, not by metadata-row id. Strategy:
        // find_by_filter on engram_id, update if present.
        //
        // The substrate's typical pattern for this is "find or
        // insert" — but find_by_id-style lookup on a non-PK field
        // requires a query path. For v1 we'll re-issue a full save
        // (INSERT OR REPLACE semantics aren't exposed via OrmStore
        // yet); when the wire-up needs efficiency, we'll add a
        // typed `upsert` method on OrmStore.
        let metadata_store = Arc::clone(&self.metadata_store);
        tokio::spawn(async move {
            if let Err(e) =
                upsert_metadata_by_engram_id(metadata_store.as_ref(), engram_id, metadata).await
            {
                tracing::warn!(
                    engram_id = %engram_id,
                    error = %e,
                    "OrmPersistenceSink: metadata update failed"
                );
            }
        });
    }
}

/// Upsert RecallMetadata for a given engram_id. Looks up the
/// existing row by engram_id (via query), updates if present,
/// inserts if absent.
///
/// Inefficient for v1 (full query per update). A future
/// `OrmStore::upsert_by_field` method or a typed cache would
/// eliminate the lookup. Documented here so the next iteration
/// has a clear target.
async fn upsert_metadata_by_engram_id(
    store: &OrmStore<EngramRecallMetadata>,
    engram_id: Uuid,
    metadata: RecallMetadata,
) -> Result<(), OrmStoreError> {
    let all = store.find_all().await?;
    let existing = all.into_iter().find(|(_, row)| row.engram_id == engram_id);
    match existing {
        Some((row_id, mut row)) => {
            row.salience = metadata.salience;
            row.access_count = metadata.access_count;
            row.last_accessed_ms = metadata.last_accessed_ms;
            row.protected_until_ms = metadata.protected_until_ms;
            row.last_decayed_ms = metadata.last_decayed_ms;
            store.update(row_id, &row).await
        }
        None => {
            let row = EngramRecallMetadata::for_new_row(engram_id, metadata);
            let row_id = Uuid::parse_str(&row.base.id)
                .expect("BaseEntity::for_new_record always produces a valid UUID");
            store.save(row_id, &row).await
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
}

impl RecordingSink {
    pub fn new() -> Self {
        Self {
            admissions: std::sync::Mutex::new(Vec::new()),
            metadata_updates: std::sync::Mutex::new(Vec::new()),
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
        self.admissions.lock().unwrap().push((engram.clone(), metadata));
    }
    fn observe_metadata_update(&self, engram_id: Uuid, metadata: RecallMetadata) {
        self.metadata_updates.lock().unwrap().push((engram_id, metadata));
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
        let engrams_with_ids = self.engram_store.find_all().await?;
        let metadata_with_ids = self.metadata_store.find_all().await?;
        let engrams: Vec<Engram> = engrams_with_ids.into_iter().map(|(_, e)| e).collect();
        let metadata: Vec<(Uuid, RecallMetadata)> = metadata_with_ids
            .into_iter()
            .map(|(_, row)| row.into())
            .collect();
        Ok((engrams, metadata))
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persona::engram::{AircMessageRef, EngramKind, EngramOrigin, TrustState};

    fn sample_engram(content: &str) -> Engram {
        Engram {
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
                sender_name: "joel".to_string(),
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
                sender_name: "joel".to_string(),
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

        std::mem::forget(tmp);
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

        std::mem::forget(tmp);
    }
}
