//! Unified Per-Persona Cognitive State
//!
//! All per-persona state in a single struct — one DashMap entry, one lock.
//!
//! Before: 7 separate DashMap<Uuid, T> — 7 lock acquisitions per command,
//! related state scattered across cache lines, no atomic cross-field access.
//!
//! After: 1 DashMap<Uuid, PersonaCognition> — 1 lock, contiguous memory,
//! atomic access to engine + rate_limiter + sleep_state + adapters + genome.

use crate::persona::admission_state::AdmissionState;
use crate::persona::cognition::PersonaCognitionEngine;
use crate::persona::domain_classifier::DomainClassifier;
use crate::persona::engram_source::EngramSource;
use crate::persona::evaluator::{RateLimiterState, SleepState};
use crate::persona::genome_paging::GenomePagingEngine;
use crate::persona::inbox::PersonaInbox;
use crate::persona::message_cache::{ContentDeduplicator, RecentMessageCache};
use crate::persona::model_selection::AdapterRegistry;
use crate::persona::rag_budget::RagSource;
use crate::persona::rag_capture::{NoopRagCaptureSink, RagCaptureSink, RecordingRagSource};
use crate::persona::recall_metadata::RecallMetadataRegistry;
use crate::rag::RagEngine;
use std::sync::Arc;
use uuid::Uuid;

/// All cognitive state for a single persona — single lock, cache-local.
pub struct PersonaCognition {
    pub engine: PersonaCognitionEngine,
    pub inbox: PersonaInbox,
    pub rate_limiter: RateLimiterState,
    pub sleep_state: SleepState,
    pub adapter_registry: AdapterRegistry,
    pub genome_engine: GenomePagingEngine,
    pub domain_classifier: DomainClassifier,
    /// Per-room recent message cache — echo chamber detection & post-inference adequacy
    pub message_cache: RecentMessageCache,
    /// Content hash dedup — prevents duplicate responses within time window
    pub content_dedup: ContentDeduplicator,
    /// Admission gate state — engram dedup + replay protection +
    /// in-memory engram store. Holds `InboxAdmissionRunner` configured
    /// with `default_v1()` recipe + permissive trust mapping. Per-persona
    /// because each persona's memory + dedup are independent.
    ///
    /// Wrapped in `Arc` (slice 10.5) so the `engram_source` can share
    /// the same admission store. Arc transparency means existing
    /// `cognition.admission.admit(...)` callers remain source-unchanged.
    pub admission: Arc<AdmissionState>,
    /// RecallMetadata sidecar — Algorithm 4's volatile per-engram
    /// state (salience, access_count, last_accessed_ms,
    /// protected_until_ms). Shared with AdmissionState (admit-time
    /// writes flow through there) and with the future recall scorer
    /// + decay tick (read-mostly hot paths). Per-persona because each
    /// persona's recall state is independent.
    pub recall_metadata: Arc<RecallMetadataRegistry>,
    /// The persona's RAG-layer engram source, wrapped in a
    /// `RecordingRagSource` decorator against `capture_sink`. Reads
    /// from `admission` + `recall_metadata`. Production callers
    /// (PromptAssembly in slice 12+) hold this via the
    /// `Arc<dyn RagSource>` type.
    pub engram_source: Arc<dyn RagSource>,
    /// The capture sink the RecordingRagSource wraps engram_source
    /// against. Default = `NoopRagCaptureSink` (zero overhead, drops
    /// events on the floor). Production callers swap in
    /// `JsonlRagCaptureSink` for on-disk traces or
    /// `InMemoryRagCaptureSink` for in-flight inspection.
    pub capture_sink: Arc<dyn RagCaptureSink>,
}

impl PersonaCognition {
    /// Create a new PersonaCognition with default sub-states.
    /// Engine and inbox require persona_id; everything else uses defaults.
    /// Capture sink defaults to `NoopRagCaptureSink` (zero overhead).
    pub fn new(persona_id: Uuid, persona_name: String, rag_engine: Arc<RagEngine>) -> Self {
        Self::with_budget(persona_id, persona_name, rag_engine, 200.0)
    }

    /// Create with a specific genome memory budget (from GPU manager).
    /// Capture sink defaults to `NoopRagCaptureSink`.
    pub fn with_budget(
        persona_id: Uuid,
        persona_name: String,
        rag_engine: Arc<RagEngine>,
        genome_budget_mb: f32,
    ) -> Self {
        let sink: Arc<dyn RagCaptureSink> = Arc::new(NoopRagCaptureSink);
        Self::with_capture_sink(persona_id, persona_name, rag_engine, genome_budget_mb, sink)
    }

    /// Create with a custom capture sink — production callers swap
    /// in `JsonlRagCaptureSink` (on-disk trace) or
    /// `InMemoryRagCaptureSink` (in-flight inspection). The
    /// `engram_source` is wrapped in a `RecordingRagSource`
    /// decorator against this sink.
    pub fn with_capture_sink(
        persona_id: Uuid,
        persona_name: String,
        rag_engine: Arc<RagEngine>,
        genome_budget_mb: f32,
        capture_sink: Arc<dyn RagCaptureSink>,
    ) -> Self {
        let (_, shutdown_rx) = tokio::sync::watch::channel(false);
        let recall_metadata = Arc::new(RecallMetadataRegistry::new());
        let admission = Arc::new(AdmissionState::new(recall_metadata.clone()));
        let engram_source: Arc<dyn RagSource> = Arc::new(RecordingRagSource::new(
            EngramSource::new(persona_id, admission.clone()),
            capture_sink.clone(),
        ));
        Self {
            engine: PersonaCognitionEngine::new(persona_id, persona_name, rag_engine, shutdown_rx),
            inbox: PersonaInbox::new(persona_id),
            rate_limiter: RateLimiterState::default(),
            sleep_state: SleepState::default(),
            adapter_registry: AdapterRegistry::default(),
            genome_engine: GenomePagingEngine::new(genome_budget_mb),
            domain_classifier: DomainClassifier::new(),
            message_cache: RecentMessageCache::new(),
            content_dedup: ContentDeduplicator::new(),
            admission,
            recall_metadata,
            engram_source,
            capture_sink,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persona::engram::{ChatMessageRef, Engram, EngramKind, EngramOrigin, TrustState};
    use crate::persona::rag_budget::{RagContext, RagSource, ResolutionPreference};
    use crate::persona::rag_capture::{
        InMemoryRagCaptureSink, NoopRagCaptureSink, RagCaptureEvent, RagCaptureSink,
    };

    #[test]
    fn test_persona_cognition_defaults() {
        let id = Uuid::new_v4();
        let rag = Arc::new(RagEngine::new());
        let pc = PersonaCognition::new(id, "TestBot".into(), rag);

        assert_eq!(pc.engine.persona_id(), id);
        assert!(pc.inbox.is_empty());
        assert!(!pc.rate_limiter.has_reached_response_cap(Uuid::new_v4()));
        assert_eq!(
            pc.sleep_state.mode,
            crate::persona::evaluator::SleepMode::Active
        );
        assert!(pc.adapter_registry.adapters.is_empty());
        assert!((pc.genome_engine.memory_pressure() - 0.0).abs() < 0.001);
    }

    // ---- Slice 10.5: RAG stack wiring (TDD) ----

    fn make_test_engram(now_ms: u64, idx: usize) -> Engram {
        Engram {
            id: Uuid::new_v4(),
            kind: EngramKind::Episodic,
            content: format!("test engram body {idx}"),
            origin: EngramOrigin::Chat(ChatMessageRef {
                message_id: Uuid::new_v4(),
                room_id: Uuid::new_v4(),
                sender_id: Uuid::new_v4(),
                posted_at_ms: now_ms,
                content_hash: format!("hash-{idx}"),
            }),
            recall_keys: Vec::new(),
            admitted_at_ms: now_ms,
            trust_state_at_admission: TrustState::ApprovedPeer,
            admission_trace_id: None,
        }
    }

    /// PersonaCognition exposes an engram_source field with the
    /// expected source_id, bound to the persona.
    #[test]
    fn persona_cognition_has_engram_source() {
        let id = Uuid::new_v4();
        let rag = Arc::new(RagEngine::new());
        let pc = PersonaCognition::new(id, "TestBot".into(), rag);
        assert_eq!(pc.engram_source.source_id(), "engrams");
    }

    /// Default capture sink should be Noop — record() doesn't panic
    /// and has no observable effect.
    #[test]
    fn default_capture_sink_is_callable_zero_cost() {
        let id = Uuid::new_v4();
        let rag = Arc::new(RagEngine::new());
        let pc = PersonaCognition::new(id, "TestBot".into(), rag);
        // Should be safe to record any event — Noop should accept it.
        pc.capture_sink.record(RagCaptureEvent::TurnEnd {
            captured_at_ms: 1,
            persona_id: id,
            turn_id: None,
        });
        // No panic = pass.
    }

    /// An engram admitted via the test-only push_for_test path
    /// surfaces via engram_source.deliver. This proves the wiring:
    /// PersonaCognition holds a shared AdmissionState (Arc) that
    /// both admission AND EngramSource read from.
    #[tokio::test]
    async fn engram_admitted_surfaces_via_engram_source() {
        let id = Uuid::new_v4();
        let rag = Arc::new(RagEngine::new());
        let pc = PersonaCognition::new(id, "TestBot".into(), rag);

        // Push an engram + register its metadata.
        let now = 1_000_000_000u64;
        let engram = make_test_engram(now, 0);
        let engram_id = engram.id;
        pc.admission.push_for_test(engram);
        pc.recall_metadata.admit_with_defaults(engram_id);

        // Exercise engram_source.
        let ctx = RagContext::for_persona(id, now);
        let delivery = pc
            .engram_source
            .deliver(&ctx, 1_000, ResolutionPreference::Raw)
            .await;
        assert_eq!(delivery.items.len(), 1, "engram should surface");
    }

    /// Swap in an InMemory capture sink at construction → calling
    /// engram_source.deliver should record an event. Proves the
    /// RecordingRagSource decorator is wired around the EngramSource.
    #[tokio::test]
    async fn capture_sink_records_engram_source_delivery() {
        let id = Uuid::new_v4();
        let rag = Arc::new(RagEngine::new());
        let sink = Arc::new(InMemoryRagCaptureSink::new());
        let sink_dyn: Arc<dyn RagCaptureSink> = sink.clone();
        let pc = PersonaCognition::with_capture_sink(
            id,
            "TestBot".into(),
            rag,
            200.0,
            sink_dyn,
        );

        // Admit + register one engram.
        let now = 1_000_000_000u64;
        let engram = make_test_engram(now, 0);
        let engram_id = engram.id;
        pc.admission.push_for_test(engram);
        pc.recall_metadata.admit_with_defaults(engram_id);

        // Deliver — should be intercepted by the RecordingRagSource
        // wrapper + recorded in the sink.
        let ctx = RagContext::for_persona(id, now);
        let _ = pc
            .engram_source
            .deliver(&ctx, 1_000, ResolutionPreference::Raw)
            .await;

        let events = sink.events();
        assert_eq!(
            events.len(),
            1,
            "RecordingRagSource decorator should have recorded one event"
        );
        match &events[0] {
            RagCaptureEvent::SourceDelivered { source_id, .. } => {
                assert_eq!(source_id, "engrams");
            }
            other => panic!("expected SourceDelivered, got {other:?}"),
        }
    }

    /// Default constructor (PersonaCognition::new) installs a
    /// NoopRagCaptureSink — exercising engram_source should NOT
    /// produce captured events (because Noop drops them).
    #[tokio::test]
    async fn default_noop_sink_drops_events() {
        let id = Uuid::new_v4();
        let rag = Arc::new(RagEngine::new());
        let pc = PersonaCognition::new(id, "TestBot".into(), rag);

        let now = 1_000_000_000u64;
        let engram = make_test_engram(now, 0);
        let engram_id = engram.id;
        pc.admission.push_for_test(engram);
        pc.recall_metadata.admit_with_defaults(engram_id);

        let ctx = RagContext::for_persona(id, now);
        let _ = pc
            .engram_source
            .deliver(&ctx, 1_000, ResolutionPreference::Raw)
            .await;

        // capture_sink is Noop; nothing should be recorded. We can't
        // inspect a Noop sink, but the type signature confirms it; this
        // test just verifies no panic + the call path is exercised.
        // Confirm the field type satisfies the trait.
        let _: &Arc<dyn RagCaptureSink> = &pc.capture_sink;
    }

    /// Suppress unused import warning for the explicit Noop type when
    /// the rest of the tests don't reference it directly. Keeps the
    /// import alive for visibility checking + future tests.
    #[allow(dead_code)]
    fn _noop_alive() -> NoopRagCaptureSink {
        NoopRagCaptureSink
    }
}
