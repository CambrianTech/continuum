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
use crate::persona::inference_profile::PersonaInferenceProfile;
use crate::persona::message_cache::{ContentDeduplicator, RecentMessageCache};
use crate::persona::model_selection::AdapterRegistry;
use crate::cognition::rag_budget::{
    BudgetAllocation, FlexboxRagBudgetAdapter, RagBudgetAdapter, RagContext, RagDelivery,
    RagSource, RagSourceBudget, ReservedTokens, ResolutionPreference,
};
use crate::persona::rag_capture::{
    NoopRagCaptureSink, RagCaptureEvent, RagCaptureSink, RecordingRagSource,
};
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
    /// The persona's live-airc RAG source — paired with
    /// `engram_source` per [[source-drain-is-the-universal-pattern]]
    /// and the L1–L5 cognitive cache doctrine. Bound at supervisor
    /// boot (task #148, "RAG source pre-binding — cache source set
    /// at boot, lease per inspection") once the persona's
    /// `AircCitizen` is attached to the grid. `None` during
    /// pre-attach / unit tests that don't stand up the airc daemon;
    /// `Some` in production. Construction is decoupled from
    /// PersonaCognition::new because the airc reader (from
    /// `runtime.transcript_reader()`) only becomes available after
    /// PersonaAircRuntime bootstraps.
    pub airc_source: Option<Arc<dyn RagSource>>,
    /// The persona's room-roster RAG source — "who else is present in
    /// this room right now", read from airc `active_agents`. Bound at
    /// supervisor boot alongside `airc_source` (same `Airc` handle, which
    /// satisfies `AircRosterReader`). `None` pre-attach / in unit tests
    /// without a daemon; `Some` in production. Its delivery is routed by
    /// the service loop into system-prompt GROUNDING (a `[Present in
    /// this room]` block), not conversation history — the fix for a
    /// persona confabulating other citizens' turns. See
    /// docs/grid/AIRC-NATIVE-IDENTITY-ROOMS-SECURITY.md §5 slice 1.
    pub roster_source: Option<Arc<dyn RagSource>>,
    /// Her awareness strip across all activities (`AwarenessRagSource`); turn churn.
    pub awareness_source: Option<Arc<dyn RagSource>>,
    /// The persona's room-doctrine RAG source — "what KIND of room is
    /// this" (the airc-published operating contract via
    /// `Airc::room_doctrine`). Bound at supervisor boot from the same
    /// `Airc` handle (satisfies `AircDoctrineReader`). `None` pre-attach
    /// / in tests. Routed by the service loop into system-prompt
    /// grounding (a `[Room operating doctrine]` block) so a persona
    /// calibrates participation to the activity (slice 2). See
    /// docs/grid/AIRC-NATIVE-IDENTITY-ROOMS-SECURITY.md §5 slice 2.
    pub doctrine_source: Option<Arc<dyn RagSource>>,
    /// The capture sink the RecordingRagSource wraps engram_source
    /// against. Default = `NoopRagCaptureSink` (zero overhead, drops
    /// events on the floor). Production callers swap in
    /// `JsonlRagCaptureSink` for on-disk traces or
    /// `InMemoryRagCaptureSink` for in-flight inspection.
    pub capture_sink: Arc<dyn RagCaptureSink>,
}

// Self-determined attention allocation (#91) does NOT live on the brain: it must be
// reachable by `persona_id` from BOTH the service loop and a self-set tool she invokes
// through the command registry (which only knows her id, never holds `cognition.lock()`).
// Its single home is `crate::cognition::focus::registry()` — see that module.

/// What [`PersonaCognition::compose_for_turn`] returns — the
/// substrate's structured handoff between "brain composed a budgeted
/// multi-source prompt context" and "inference adapter generates a
/// response." Per the brain doctrine ([[no-fallbacks-ever]],
/// [[no-if-statements-use-llms-for-cognition]]) this struct carries
/// the budget allocator's verdict alongside the deliveries, so the
/// caller (service_loop / introspection) can see exactly what landed
/// — Satisfied / FloorOnly / Dropped / UnderProvisioned — instead of
/// having to re-derive that from item counts.
#[derive(Debug, Clone)]
pub struct ComposedTurn {
    /// The budget allocator's per-source verdict, with the rich
    /// AllocationState telemetry the substrate-is-a-good-citizen
    /// doctrine requires. Caller can inspect `escalation_needed` to
    /// know if any required source got under-provisioned.
    pub allocation: BudgetAllocation,
    /// One delivery per source the brain composed. Ordering matches
    /// the order the brain assembled budgets in (engram first, then
    /// airc, then any future sources). The caller threads these into
    /// the prompt in the order presented.
    pub deliveries: Vec<RagDelivery>,
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
            airc_source: None,
            roster_source: None,
            awareness_source: None,
            doctrine_source: None,
            capture_sink,
        }
    }

    /// Bind the brain's live-airc RAG source. Called by the
    /// supervisor once the persona's `AircCitizen` is attached to
    /// the grid and a `Arc<dyn AircTranscriptReader>` is available.
    /// Per [[init-once-handle-then-lease-zero-copy-refs]] this is a
    /// boot-time wire, NOT a per-turn allocation.
    ///
    /// Decorating with the brain's existing `capture_sink` keeps
    /// airc deliveries flowing through the same capture/replay
    /// pipeline as engrams (per
    /// [[persona-record-replay-is-a-product-requirement]]).
    /// Swap the in-memory admission for a disk-backed, rehydrated one
    /// (per-persona `engrams.sqlite`, via `AdmissionState::for_persona`). Called
    /// by the supervisor at boot once the persona's `PersonaHome` is known.
    /// Rebuilds `engram_source` against the new admission (decorated with the
    /// brain's `capture_sink`) and adopts its recall-metadata registry, so
    /// encoding + recall + the workspace's RecallFaculty all share the persisted
    /// store. Without this, admission is `NoopSink` — in-memory, lost on restart.
    /// Must run BEFORE the per-persona WorkspaceCycle is assembled, so its
    /// RecallFaculty binds the persisted admission.
    pub fn attach_persistent_admission(
        &mut self,
        persona_id: Uuid,
        admission: Arc<AdmissionState>,
    ) {
        self.recall_metadata = admission.recall_metadata().clone();
        self.engram_source = Arc::new(RecordingRagSource::new(
            EngramSource::new(persona_id, admission.clone()),
            self.capture_sink.clone(),
        ));
        self.admission = admission;
    }

    pub fn set_airc_source(&mut self, raw_source: Arc<dyn RagSource>) {
        let decorated: Arc<dyn RagSource> = Arc::new(RecordingRagSource::new(
            ArcRagSource::new(raw_source),
            self.capture_sink.clone(),
        ));
        self.airc_source = Some(decorated);
    }

    /// Bind the brain's room-roster RAG source (`RoomRosterSource`).
    /// Called by the supervisor at boot with the same `Airc` handle that
    /// backs `airc_source` (it satisfies `AircRosterReader`). Decorated
    /// with the brain's `capture_sink` so roster deliveries are recorded
    /// + replayable on the same wire as engrams and airc transcript
    /// (per [[persona-record-replay-is-a-product-requirement]]). Boot-
    /// time wire, not a per-turn allocation.
    pub fn set_roster_source(&mut self, raw_source: Arc<dyn RagSource>) {
        let decorated: Arc<dyn RagSource> = Arc::new(RecordingRagSource::new(
            ArcRagSource::new(raw_source),
            self.capture_sink.clone(),
        ));
        self.roster_source = Some(decorated);
    }

    /// Bind the brain's awareness-strip source (`AwarenessRagSource`): what she
    /// perceives across ALL her activities, loudest first, with her continuation. Same
    /// boot-time wire and capture decoration as the roster.
    pub fn set_awareness_source(&mut self, raw_source: Arc<dyn RagSource>) {
        let decorated: Arc<dyn RagSource> = Arc::new(RecordingRagSource::new(
            ArcRagSource::new(raw_source),
            self.capture_sink.clone(),
        ));
        self.awareness_source = Some(decorated);
    }

    /// Bind the brain's room-doctrine RAG source (`RoomDoctrineSource`).
    /// Same boot-time wire as `set_roster_source`, from the same `Airc`
    /// handle (satisfies `AircDoctrineReader`), decorated with the
    /// `capture_sink` so doctrine deliveries are recorded + replayable.
    pub fn set_doctrine_source(&mut self, raw_source: Arc<dyn RagSource>) {
        let decorated: Arc<dyn RagSource> = Arc::new(RecordingRagSource::new(
            ArcRagSource::new(raw_source),
            self.capture_sink.clone(),
        ));
        self.doctrine_source = Some(decorated);
    }

    /// Brain composition for one cognition turn. Walks the brain's
    /// own bound sources (engram + airc + future) through the
    /// `FlexboxRagBudgetAdapter` Joel wrote in PR #8 / task #93 —
    /// the no-clipping doctrine, source-owned-units, full
    /// telemetry. NOT a separate budget. NOT a parallel allocator.
    /// THE substrate budgeter, called the way the substrate
    /// expects.
    ///
    /// Budget reservations scale with `profile.context_length` so a
    /// Compat-tier 2048-window persona AND an M-series 32k+ persona
    /// both call this same method — the answer differs because the
    /// profile differs, per
    /// [[context-is-the-client-airc-token-is-identity]] and
    /// [[intent-driven-api-not-hot-patches]]. The substrate does
    /// NOT bake in clamps that handicap capable peers on the grid.
    ///
    /// `now_ms` is passed in (not read from `SystemTime`) so the
    /// brain's composition is replay-deterministic per
    /// [[persona-record-replay-is-a-product-requirement]].
    /// `room` is the WHERE axis — the context this turn is happening inside.
    /// Room-scoped sources (`room-kanban`, `room-roster`, `room-doctrine`,
    /// `room-board`) compare it against their own bound room and ABSTAIN when it
    /// is absent, so passing `None` here makes the persona blind to the board,
    /// the roster, the room's doctrine and its wall — all four at once.
    ///
    /// That is not hypothetical: this parameter did not exist until 2026-08-06,
    /// and the probe (`rag.room_gate.abstain`) recorded 504 abstains with
    /// `turn_room = NIL` — 89% of live turns — while six citizens across two
    /// machines spent a night correctly reporting "there are no open tasks
    /// available" from a window that had no room content in it. `#127` built the
    /// gate, the constructor, and the probe; this caller was never switched over.
    ///
    /// `None` remains legitimate for genuinely room-less work (background
    /// consolidation, dreams) — it means "no room context claimed", not "unknown".
    pub async fn compose_for_turn(
        &self,
        profile: &PersonaInferenceProfile,
        now_ms: u64,
        room: Option<uuid::Uuid>,
    ) -> ComposedTurn {
        let persona_id = self.engine.persona_id();
        let rag_ctx = match room {
            Some(r) => RagContext::for_persona_in_room(persona_id, now_ms, r),
            None => RagContext::for_persona(persona_id, now_ms),
        };

        // Window-scaled reservation — ONE derivation, owned by
        // ReservedTokens::scaled_for_window (#424 dedup; rag_inspect
        // shares it). Doctrine lives on the constructor.
        let context_window = profile.context_length;
        let reserved = ReservedTokens::scaled_for_window(context_window);
        let headroom = reserved.headroom_within(context_window);

        // Collect the brain's bound sources in deterministic order:
        // engram first (long-term memory, the L2+ recall layer),
        // then airc (the L1 conversational floor). Future sources
        // (code, tool descriptions, identity card) extend this list
        // in order of long-term-to-immediate.
        let mut sources: Vec<Arc<dyn RagSource>> = Vec::with_capacity(4);
        sources.push(self.engram_source.clone());
        if let Some(ref airc) = self.airc_source {
            sources.push(airc.clone());
        }
        // The "identity card" sources the original author reserved this
        // list for: WHO is present (roster) and WHAT KIND of room this is
        // (doctrine). Both routed by the service loop into system-prompt
        // grounding, not history.
        if let Some(ref roster) = self.roster_source {
            sources.push(roster.clone());
        }
        if let Some(ref doctrine) = self.doctrine_source {
            sources.push(doctrine.clone());
        }
        // Her awareness across every activity (EVENT-MIND §6): the strip. Lightweight,
        // loudest first, fitted by its own source; the one block that says where else
        // she is needed while she works here.
        if let Some(ref awareness) = self.awareness_source {
            sources.push(awareness.clone());
        }

        // Sources declare appetite through the shared contract. Names identify
        // capture rows; renaming or decorating a source cannot change its share.
        let budgets: Vec<RagSourceBudget> = sources
            .iter()
            .map(|source| source.budget_claim(context_window, headroom))
            .collect();

        // Emit the TurnStart capture so audit/replay sees the
        // budget the brain actually asked for, not what landed.
        let turn_id = Uuid::new_v4();
        // A turn in her mind room: no retrieval capture at all (sink 5).
        let captured = !crate::persona::rag_capture::turn_is_private(&rag_ctx);
        if captured {
            self.capture_sink.record(RagCaptureEvent::TurnStart {
                captured_at_ms: now_ms,
                persona_id,
                turn_id: Some(turn_id),
                context_window,
                reserved,
                source_budgets: budgets.clone(),
                context: rag_ctx.clone(),
            });
        }

        let adapter = FlexboxRagBudgetAdapter::new();
        let allocation = adapter.allocate(&rag_ctx, context_window, reserved, &budgets);

        if captured {
            self.capture_sink.record(RagCaptureEvent::BudgetAllocated {
                captured_at_ms: now_ms,
                persona_id,
                turn_id: Some(turn_id),
                allocation: allocation.clone(),
            });
        }

        let mut deliveries = Vec::with_capacity(sources.len());
        for (source, source_alloc) in sources.iter().zip(allocation.allocations.iter()) {
            let delivery = source
                .deliver(
                    &rag_ctx,
                    source_alloc.allocated_tokens,
                    ResolutionPreference::Raw,
                )
                .await;
            deliveries.push(delivery);
        }

        if captured {
            self.capture_sink.record(RagCaptureEvent::TurnEnd {
                captured_at_ms: now_ms,
                persona_id,
                turn_id: Some(turn_id),
            });
        }

        ComposedTurn {
            allocation,
            deliveries,
        }
    }
}

/// Adapter that re-wraps an `Arc<dyn RagSource>` so it can be passed
/// into `RecordingRagSource::new` (which takes any `RagSource` by
/// value, not by `Arc`). Trivial delegating wrapper; the underlying
/// source's `&self` deliver path is unchanged.
struct ArcRagSource(Arc<dyn RagSource>);

impl ArcRagSource {
    fn new(inner: Arc<dyn RagSource>) -> Self {
        Self(inner)
    }
}

#[async_trait::async_trait]
impl RagSource for ArcRagSource {
    fn room_scope(&self) -> crate::cognition::rag_budget::RagRoomScope {
        self.0.room_scope()
    }

    fn source_id(&self) -> &'static str {
        self.0.source_id()
    }

    fn expand_command(&self) -> Option<&'static str> {
        // Delegates to the inner source; expansion is that source's to declare.
        // It returned None, so every source wrapped for compose_for_turn lost the
        // verb its truncation notice names (card e049aa82).
        self.0.expand_command()
    }

    /// Delegates to the inner source — the floor is that source's to declare.
    fn floor_tokens(&self) -> u32 {
        self.0.floor_tokens()
    }

    fn budget_claim(&self, window: u32, headroom: u32) -> crate::cognition::rag_budget::RagSourceBudget {
        self.0.budget_claim(window, headroom)
    }

    async fn deliver(
        &self,
        ctx: &RagContext,
        budget: u32,
        resolution: ResolutionPreference,
    ) -> RagDelivery {
        self.0.deliver(ctx, budget, resolution).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persona::engram::{ChatMessageRef, Engram, EngramKind, EngramOrigin, TrustState};
    use crate::cognition::rag_budget::{RagContext, RagSource, ResolutionPreference};
    use crate::persona::rag_capture::{
        InMemoryRagCaptureSink, RagCaptureEvent, RagCaptureSink,
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
            context_id: None,
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
        let pc = PersonaCognition::with_capture_sink(id, "TestBot".into(), rag, 200.0, sink_dyn);

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

    // ---- Slice A (task #148): brain composes via FlexboxRagBudgetAdapter
    //      over engram + airc, not via inspect_persona_rag's ad-hoc seam.

    use crate::persona::inference_profile::PersonaInferenceProfile;
    use crate::cognition::rag_budget::{AllocationState, RagDelivery, RagItem};
    use async_trait::async_trait;

    /// Test source that returns a fixed budget-aware payload — proves
    /// the brain wires the budget through to per-source deliver.
    struct CannedSource {
        id: &'static str,
        tokens_per_item: u32,
        items_offered: usize,
        share: Option<std::num::NonZeroU32>,
    }

    #[async_trait]
    impl RagSource for CannedSource {
        fn source_id(&self) -> &'static str {
            self.id
        }

        fn expand_command(&self) -> Option<&'static str> {
            // Test/stub source — nothing further to fetch.
            None
        }

        /// Test/stub source — floorless, so it never encodes a production floor.
        fn floor_tokens(&self) -> u32 {
            0
        }
        fn budget_claim(&self, window: u32, headroom: u32) -> RagSourceBudget {
            match self.share {
                Some(share) => RagSourceBudget::floorless_share(self.id, window, headroom, share),
                None => RagSourceBudget::retrieval(self.id, self.floor_tokens(), window, headroom),
            }
        }
        async fn deliver(
            &self,
            _ctx: &RagContext,
            budget: u32,
            _resolution: ResolutionPreference,
        ) -> RagDelivery {
            let mut items = Vec::new();
            let mut tokens_used = 0u32;
            for i in 0..self.items_offered {
                if tokens_used + self.tokens_per_item > budget {
                    break;
                }
                items.push(RagItem {
                    content: format!("{}: item {}", self.id, i),
                    tokens: self.tokens_per_item,
                    metadata: serde_json::Value::Null,
                });
                tokens_used += self.tokens_per_item;
            }
            RagDelivery {
                source_id: self.id.to_string(),
                items,
                tokens_used,
                truncated: false,
                resolution_used: ResolutionPreference::Raw,
            }
        }
    }

    fn lcd_profile() -> PersonaInferenceProfile {
        use crate::persona::hw_tier_descriptor::HwTierCategory;
        use crate::persona::inference_profile::SamplingProfile;
        PersonaInferenceProfile {
            persona_id: Uuid::nil(),
            persona_name: "TestBot".to_string(),
            model_id: "test/qwen-0.5b".to_string(),
            gguf_local_path: None,
            tier_category: HwTierCategory::Compat,
            tier_id: "test_compat".to_string(),
            context_length: 2048,
            n_ubatch: 512,
            n_batch: 2048,
            n_seq_max: 1,
            n_gpu_layers: 0,
            sampling: SamplingProfile::chat_defaults(),
            chat_template: None,
            stop_sequences: Vec::new(),
        }
    }

    /// With no airc_source bound the brain still composes — just the
    /// engram source. Proves engram is always-present per the
    /// substrate-managed-vs-citizen-managed split.
    #[tokio::test]
    async fn compose_for_turn_uses_engram_when_airc_unbound() {
        let id = Uuid::new_v4();
        let rag = Arc::new(RagEngine::new());
        let pc = PersonaCognition::new(id, "TestBot".into(), rag);

        let composed = pc.compose_for_turn(&lcd_profile(), 1_000_000, None).await;
        assert_eq!(composed.deliveries.len(), 1);
        assert_eq!(composed.deliveries[0].source_id, "engrams");
    }

    /// With airc_source bound the brain composes BOTH sources via the
    /// FlexboxRagBudgetAdapter — the same budgeter (no parallel
    /// allocator, no second compose path).
    #[tokio::test]
    async fn compose_for_turn_threads_airc_through_budgeter() {
        let id = Uuid::new_v4();
        let rag = Arc::new(RagEngine::new());
        let mut pc = PersonaCognition::new(id, "TestBot".into(), rag);

        let airc = Arc::new(CannedSource {
            id: "airc",
            tokens_per_item: 50,
            items_offered: 3,
            share: None,
        });
        pc.set_airc_source(airc);

        let composed = pc.compose_for_turn(&lcd_profile(), 1_000_000, None).await;
        assert_eq!(composed.deliveries.len(), 2);
        assert_eq!(composed.deliveries[0].source_id, "engrams");
        assert_eq!(composed.deliveries[1].source_id, "airc");

        // The flex allocator gave airc enough budget to deliver its
        // 3 canned items (50 tokens each = 150 total, well under
        // the per-source max for a 2048-window profile).
        assert_eq!(composed.deliveries[1].items.len(), 3);

        // Allocation telemetry surfaces — caller can read state per
        // source. With airc empty of content past the canned items,
        // there's no UnderProvisioned (required=false on both).
        assert!(!composed.allocation.escalation_needed);
        for alloc in &composed.allocation.allocations {
            assert!(
                matches!(
                    alloc.state,
                    AllocationState::Satisfied | AllocationState::FloorOnly
                ),
                "expected Satisfied or FloorOnly, got {:?} for {}",
                alloc.state,
                alloc.source_id
            );
        }
    }

    // Regression for c44d535a: a new source name keeps its declared appetite
    // through every decorator and the real composer; no ID allow-list is needed.
    #[tokio::test]
    async fn decorated_source_claim_survives_new_name_and_composition() {
        let source = Arc::new(CannedSource {
            id: "new-standing-kind",
            tokens_per_item: 1,
            items_offered: 3,
            share: std::num::NonZeroU32::new(64),
        });
        let (cached, _) = crate::cognition::cached_source::CachedRagSource::new(source.clone());
        let mut pc = PersonaCognition::new(Uuid::new_v4(), "TestBot".into(), Arc::new(RagEngine::new()));
        pc.set_roster_source(cached);
        let wrapped = pc.roster_source.as_ref().unwrap();
        for window in [0, 256, 2048, 51_712] {
            let headroom = ReservedTokens::scaled_for_window(window).headroom_within(window);
            assert_eq!(wrapped.budget_claim(window, headroom), source.budget_claim(window, headroom));
        }
        let composed = pc.compose_for_turn(&lcd_profile(), 1_000_000, None).await;
        let allocation = composed.allocation.allocations.iter()
            .find(|a| a.source_id == source.id).unwrap();
        assert_eq!(allocation.requested_max, lcd_profile().context_length / 64);
        assert_eq!(allocation.requested_min, 0);
        assert_eq!(allocation.requested_floor, 0);
        assert!(composed.deliveries.iter().any(|d| d.source_id == source.id && d.items.len() == 3));
    }

    // what this catches: a turn in her mind room landing in the RAG capture (sink 5 of
    // PRIVACY-OF-THOUGHT.md), through either the turn brackets or a recording source's
    // delivery. Zero events for her mind room; an ordinary room still records its turn.
    #[tokio::test]
    async fn a_turn_in_her_mind_room_captures_no_retrieval() {
        let id = Uuid::new_v4();
        let sink = Arc::new(InMemoryRagCaptureSink::new());
        let sink_dyn: Arc<dyn RagCaptureSink> = sink.clone();
        let mut pc =
            PersonaCognition::with_capture_sink(id, "Kimi".into(), Arc::new(RagEngine::new()), 200.0, sink_dyn);
        pc.set_airc_source(Arc::new(CannedSource { id: "airc", tokens_per_item: 50, items_offered: 2, share: None }));
        let mind = crate::persona::mind_room::mind_room_id(id);
        let _ = pc.compose_for_turn(&lcd_profile(), 1_000_000, Some(mind)).await;
        assert!(sink.is_empty(), "her mind room: no capture events, got {}", sink.len());
        let _ = pc.compose_for_turn(&lcd_profile(), 1_000_000, Some(Uuid::new_v4())).await;
        assert!(!sink.is_empty(), "an ordinary room still records its turn");
    }

    /// The brain's capture sink records the TurnStart / BudgetAllocated
    /// / TurnEnd events the substrate-replay pipeline expects. Proves
    /// compose_for_turn participates in the same capture/replay loop
    /// engram_source already does.
    #[tokio::test]
    async fn compose_for_turn_emits_capture_events_for_replay() {
        let id = Uuid::new_v4();
        let rag = Arc::new(RagEngine::new());
        let sink = Arc::new(InMemoryRagCaptureSink::new());
        let sink_dyn: Arc<dyn RagCaptureSink> = sink.clone();
        let mut pc =
            PersonaCognition::with_capture_sink(id, "TestBot".into(), rag, 200.0, sink_dyn);

        let airc = Arc::new(CannedSource {
            id: "airc",
            tokens_per_item: 50,
            items_offered: 2,
            share: None,
        });
        pc.set_airc_source(airc);

        let _composed = pc.compose_for_turn(&lcd_profile(), 1_000_000, None).await;

        let events = sink.events();
        let kinds: Vec<&str> = events
            .iter()
            .map(|e| match e {
                RagCaptureEvent::TurnStart { .. } => "TurnStart",
                RagCaptureEvent::BudgetAllocated { .. } => "BudgetAllocated",
                RagCaptureEvent::SourceDelivered { .. } => "SourceDelivered",
                RagCaptureEvent::TurnEnd { .. } => "TurnEnd",
            })
            .collect();
        assert!(kinds.contains(&"TurnStart"), "kinds: {kinds:?}");
        assert!(kinds.contains(&"BudgetAllocated"), "kinds: {kinds:?}");
        assert!(kinds.contains(&"TurnEnd"), "kinds: {kinds:?}");
    }
}
