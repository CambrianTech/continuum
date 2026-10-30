//! Unified Per-Persona Cognitive State
//!
//! All per-persona state in a single struct — one DashMap entry, one lock.
//!
//! Before: 7 separate DashMap<Uuid, T> — 7 lock acquisitions per command,
//! related state scattered across cache lines, no atomic cross-field access.
//!
//! After: 1 DashMap<Uuid, PersonaCognition> — 1 lock, contiguous memory,
//! atomic access to engine + rate_limiter + sleep_state + adapters + genome.

use crate::cognition::admission_state::AdmissionState;
use crate::persona::cognition::PersonaCognitionEngine;
use crate::persona::domain_classifier::DomainClassifier;
use crate::persona::evaluator::{RateLimiterState, SleepState};
use crate::persona::inbox::PersonaInbox;
use crate::persona::message_cache::{ContentDeduplicator, RecentMessageCache};
use crate::persona::model_selection::AdapterRegistry;
use crate::cognition::rag_budget::{RagContext, RagDelivery, RagSource};
use crate::persona::rag_capture::{
    NoopRagCaptureSink, RagCaptureEvent, RagCaptureSink, RecordingRagSource,
};
use crate::cognition::recall_metadata::RecallMetadataRegistry;
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
    /// Wrapped in `Arc` (slice 10.5) so recall shares the same admission store. Arc
    /// transparency means existing `cognition.admission.admit(...)` callers remain
    /// source-unchanged.
    pub admission: Arc<AdmissionState>,
    /// RecallMetadata sidecar — Algorithm 4's volatile per-engram
    /// state (salience, access_count, last_accessed_ms,
    /// protected_until_ms). Shared with AdmissionState (admit-time
    /// writes flow through there) and with the future recall scorer
    /// + decay tick (read-mostly hot paths). Per-persona because each
    /// persona's recall state is independent.
    pub recall_metadata: Arc<RecallMetadataRegistry>,
    /// The persona's live-airc RAG source, per
    /// [[source-drain-is-the-universal-pattern]] and the L1–L5 cognitive cache doctrine. Bound at supervisor
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
    /// The capture sink every bound source's RecordingRagSource records into. Default = `NoopRagCaptureSink` (zero overhead, drops
    /// events on the floor). Production callers swap in
    /// `JsonlRagCaptureSink` for on-disk traces or
    /// `InMemoryRagCaptureSink` for in-flight inspection.
    pub capture_sink: Arc<dyn RagCaptureSink>,
}

// Self-determined attention allocation (#91) does NOT live on the brain: it must be
// reachable by `persona_id` from BOTH the service loop and a self-set tool she invokes
// through the command registry (which only knows her id, never holds `cognition.lock()`).
// Its single home is `crate::cognition::focus::registry()` — see that module.

/// What [`PersonaCognition::compose_for_turn`] returns: every item of her conversation and
/// roster sources, whole, for the turn's burst. Nothing here is sized: her prompt view's one
/// allocation decides what she sees (FLOW-IMPLEMENTATION-PLAN §2d).
#[derive(Debug, Clone)]
pub struct ComposedTurn {
    /// One delivery per bound source, airc first, then the roster.
    pub deliveries: Vec<RagDelivery>,
}

impl PersonaCognition {
    /// Create a new PersonaCognition with default sub-states.
    /// Engine and inbox require persona_id; everything else uses defaults.
    /// Capture sink defaults to `NoopRagCaptureSink` (zero overhead).
    pub fn new(persona_id: Uuid, persona_name: String, rag_engine: Arc<RagEngine>) -> Self {
        let sink: Arc<dyn RagCaptureSink> = Arc::new(NoopRagCaptureSink);
        Self::with_capture_sink(persona_id, persona_name, rag_engine, sink)
    }

    /// Create with a custom capture sink — production callers swap
    /// in `JsonlRagCaptureSink` (on-disk trace) or
    /// `InMemoryRagCaptureSink` (in-flight inspection). Every bound source is wrapped in a
    /// `RecordingRagSource` decorator against this sink.
    pub fn with_capture_sink(
        persona_id: Uuid,
        persona_name: String,
        rag_engine: Arc<RagEngine>,
        capture_sink: Arc<dyn RagCaptureSink>,
    ) -> Self {
        let (_, shutdown_rx) = tokio::sync::watch::channel(false);
        let recall_metadata = Arc::new(RecallMetadataRegistry::new());
        let admission = Arc::new(AdmissionState::new(recall_metadata.clone()));
        Self {
            engine: PersonaCognitionEngine::new(persona_id, persona_name, rag_engine, shutdown_rx),
            inbox: PersonaInbox::new(persona_id),
            rate_limiter: RateLimiterState::default(),
            sleep_state: SleepState::default(),
            adapter_registry: AdapterRegistry::default(),
            domain_classifier: DomainClassifier::new(),
            message_cache: RecentMessageCache::new(),
            content_dedup: ContentDeduplicator::new(),
            admission,
            recall_metadata,
            airc_source: None,
            roster_source: None,
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
    /// Adopts its recall-metadata registry, so encoding + recall + the workspace's
    /// RecallFaculty all share the persisted store. Without this, admission is `NoopSink` — in-memory, lost on restart.
    /// Must run BEFORE the per-persona WorkspaceCycle is assembled, so its
    /// RecallFaculty binds the persisted admission.
    pub fn attach_persistent_admission(
        &mut self,
        persona_id: Uuid,
        admission: Arc<AdmissionState>,
    ) {
        let _ = persona_id; // the admission is already the persona's; kept for the boot call's shape
        self.recall_metadata = admission.recall_metadata().clone();
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

    /// Brain composition for one cognition turn: her conversation (airc) and who is present
    /// (roster), every item whole. No budget and no allocation here: her prompt view's one
    /// allocation decides what she sees (FLOW-IMPLEMENTATION-PLAN §2d). This ran a second
    /// flexbox over five sources, of which only these two were read; the engram, doctrine
    /// and awareness deliveries reached only the capture.
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
        now_ms: u64,
        room: Option<uuid::Uuid>,
    ) -> ComposedTurn {
        let persona_id = self.engine.persona_id();
        let rag_ctx = match room {
            Some(r) => RagContext::for_persona_in_room(persona_id, now_ms, r),
            None => RagContext::for_persona(persona_id, now_ms),
        };

        let sources = self.airc_source.iter().chain(self.roster_source.iter());
        let turn_id = Uuid::new_v4();
        // A turn in her mind room: no retrieval capture at all (sink 5).
        let captured = !crate::persona::rag_capture::turn_is_private(&rag_ctx);
        if captured {
            self.capture_sink.record(RagCaptureEvent::TurnStart {
                captured_at_ms: now_ms,
                persona_id,
                turn_id: Some(turn_id),
                context: rag_ctx.clone(),
            });
        }
        let mut deliveries = Vec::with_capacity(2);
        for source in sources {
            deliveries.push(source.deliver(&rag_ctx).await);
        }
        if captured {
            self.capture_sink.record(RagCaptureEvent::TurnEnd {
                captured_at_ms: now_ms,
                persona_id,
                turn_id: Some(turn_id),
            });
        }
        ComposedTurn { deliveries }
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

    async fn deliver(&self, ctx: &RagContext) -> RagDelivery {
        self.0.deliver(ctx).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cognition::rag_budget::{RagContext, RagItem, RagSource};
    use crate::persona::rag_capture::{InMemoryRagCaptureSink, RagCaptureEvent, RagCaptureSink};
    use async_trait::async_trait;

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
    }

    /// A source that holds `items` fixed items and delivers every one.
    struct CannedSource {
        id: &'static str,
        items: usize,
    }

    #[async_trait]
    impl RagSource for CannedSource {
        fn source_id(&self) -> &'static str {
            self.id
        }

        fn expand_command(&self) -> Option<&'static str> {
            None // a stub: nothing further to fetch
        }

        async fn deliver(&self, _ctx: &RagContext) -> RagDelivery {
            let items = (0..self.items)
                .map(|i| RagItem::whole(format!("{i}"), format!("{}: item {i}", self.id), serde_json::Value::Null))
                .collect();
            RagDelivery::of(self.id, items, false)
        }
    }

    // what this catches: compose_for_turn sizing her conversation or roster (it ran a second
    // flexbox, FLOW plan §2d). Every bound source delivers every item it holds, through the
    // capture decorator, airc first; nothing unbound appears.
    #[tokio::test]
    async fn compose_for_turn_delivers_every_item_of_her_bound_sources() {
        let mut pc = PersonaCognition::new(Uuid::new_v4(), "TestBot".into(), Arc::new(RagEngine::new()));
        assert!(pc.compose_for_turn(1_000_000, None).await.deliveries.is_empty(), "nothing bound, nothing delivered");
        pc.set_airc_source(Arc::new(CannedSource { id: "airc", items: 300 }));
        let (cached, _) = crate::cognition::cached_source::CachedRagSource::new(Arc::new(CannedSource { id: "roster", items: 40 }));
        pc.set_roster_source(cached);
        let composed = pc.compose_for_turn(1_000_000, None).await;
        let shape: Vec<(&str, usize)> = composed.deliveries.iter().map(|d| (d.source_id.as_str(), d.items.len())).collect();
        assert_eq!(shape, [("airc", 300), ("roster", 40)]);
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
            PersonaCognition::with_capture_sink(id, "Kimi".into(), Arc::new(RagEngine::new()), sink_dyn);
        pc.set_airc_source(Arc::new(CannedSource { id: "airc", items: 2 }));
        let mind = crate::cognition::mind_room::mind_room_id(id);
        let _ = pc.compose_for_turn(1_000_000, Some(mind)).await;
        assert!(sink.is_empty(), "her mind room: no capture events, got {}", sink.len());
        let _ = pc.compose_for_turn(1_000_000, Some(Uuid::new_v4())).await;
        assert!(!sink.is_empty(), "an ordinary room still records its turn");
    }

    // what this catches: compose_for_turn leaving the capture/replay loop. A turn records
    // its start, each source's delivery and its end, in that order.
    #[tokio::test]
    async fn compose_for_turn_emits_capture_events_for_replay() {
        let sink = Arc::new(InMemoryRagCaptureSink::new());
        let sink_dyn: Arc<dyn RagCaptureSink> = sink.clone();
        let mut pc = PersonaCognition::with_capture_sink(Uuid::new_v4(), "TestBot".into(), Arc::new(RagEngine::new()), sink_dyn);
        pc.set_airc_source(Arc::new(CannedSource { id: "airc", items: 2 }));
        let _ = pc.compose_for_turn(1_000_000, None).await;
        let kinds: Vec<&str> = sink
            .events()
            .iter()
            .map(|e| match e {
                RagCaptureEvent::TurnStart { .. } => "TurnStart",
                RagCaptureEvent::SourceDelivered { .. } => "SourceDelivered",
                RagCaptureEvent::TurnEnd { .. } => "TurnEnd",
            })
            .collect();
        assert_eq!(kinds, ["TurnStart", "SourceDelivered", "TurnEnd"]);
    }
}
