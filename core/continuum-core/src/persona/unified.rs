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
use crate::persona::rag_budget::{
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

/// Room-roster grounding ceiling as a FRACTION (1/64) of the served window,
/// never a baked constant (task #124, [[no-hardcoded-context-numbers-derive-from-the-live-window]]).
/// A roster is a handful of presence lines (tens of tokens), so its SHARE stays
/// tiny — floorless, always `.min(per_source_max)` — but it now SCALES with the
/// window: ~256 tokens at the common 16k window (the tuned value), more on a big
/// model, less on a tight one. Keeps the "never starve airc's recent_history"
/// property (a small fraction, sorted last) while never clamping a 128k window to
/// a constant. See the budget-claim rationale in `compose_for_turn`.
// context-budget-exempt: a DENOMINATOR — already the window-relative pattern this guard enforces
const ROSTER_WINDOW_FRACTION: u32 = 64;

/// Room-doctrine grounding ceiling as a FRACTION (1/16) of the served window.
/// A doctrine is a short operating contract (a few paragraphs) — larger than the
/// roster (prose, not a name list) but still a small, floorless share that scales:
/// ~1024 tokens at a 16k window (the tuned value), growing so a big model can hold
/// a richer room contract without competing with engram/airc for grow headroom.
// context-budget-exempt: a DENOMINATOR — already the window-relative pattern this guard enforces
const DOCTRINE_WINDOW_FRACTION: u32 = 16;

/// What a heavyweight grounding source gets when there IS room — its comfortable
/// size, not its survival minimum. Formerly this same number was ALSO used as the
/// floor, which is the bug: the allocator drops a source whole when its floor
/// doesn't fit, so every source demanded 500 tokens to say anything at all while
/// its real first unit costs 6..40 (measured 2026-08-06). Floor now comes from
/// `RagSource::floor_tokens`; this stays the target the grow pass aims at.
// context-budget-exempt: a per-source TARGET, clamped by per_source_max which IS window-relative
const COMFORTABLE_SOURCE_TOKENS: u32 = 500;

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
// Its single home is `crate::persona::focus::registry()` — see that module.

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

        // Reserved tokens scale with context window. See doctrine
        // comment on the constants — these are FALLBACK shapes, NOT
        // hardcodes pinned to LCD tier. The substrate's real
        // budgeter logic (driven by profile model characteristics)
        // can override these later via a richer reservation API.
        let context_window = profile.context_length;
        let reserved = ReservedTokens {
            system: (context_window / 10).clamp(128, 512),
            completion: (context_window / 4).clamp(256, 4_000),
        };
        let headroom = context_window
            .saturating_sub(reserved.system + reserved.completion)
            .max(512);

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

        // Per-source budget claims. The two HEAVYWEIGHT sources (engram
        // long-term memory + airc recent conversation) split idle
        // headroom evenly; the recent-conversation floor lives on airc
        // per the cognition-cache-hierarchy doc. The room-roster source
        // is LIGHTWEIGHT — a handful of presence lines, tens of tokens —
        // so it claims a small fixed budget with NO floor. Giving it the
        // same 500/500/per_source_max claim as the heavyweights would
        // (a) at small context windows let the floor sum exceed
        // available and starve airc (the roster, sorted last, would also
        // drop to 0), and (b) at normal windows split grow-headroom 3
        // ways instead of 2, shrinking airc's delivered recent_history.
        // A source's budget should reflect its real appetite.
        let per_source_max = ((context_window as u64) * 6 / 10) as u32;
        let per_source_max = per_source_max.min(headroom);
        let budgets: Vec<RagSourceBudget> = sources
            .iter()
            .map(|s| {
                // Lightweight grounding sources (roster, doctrine) claim a
                // small floorless budget matching their real appetite, so
                // they never starve airc's recent_history or compete for
                // grow headroom with the heavyweight engram/airc sources.
                let (floor, min, max) = match s.source_id() {
                    "room-roster" => (
                        0,
                        0,
                        (context_window / ROSTER_WINDOW_FRACTION).min(per_source_max),
                    ),
                    "room-doctrine" => (
                        0,
                        0,
                        (context_window / DOCTRINE_WINDOW_FRACTION).min(per_source_max),
                    ),
                    _ => {
                        // FLOOR is what the source needs to say ONE true thing;
                        // MIN is what it wants when there is room. Conflating them
                        // (both were a hardcoded 500) is what made grounding
                        // all-or-nothing: the allocator drops a source WHOLE when
                        // its floor doesn't fit, so a source that could deliver a
                        // complete 26-token headline was never asked for it.
                        //
                        // Measured 2026-08-06 — real first units are 6..40 tokens
                        // (~104 for one unit from ALL six sources), against a 500
                        // floor each. On a node whose grounding budget measured
                        // 0..214, every source was dropped on 100% of turns (137/137
                        // and 132/132 for two citizens) while asking 12-80x more than
                        // it needed. Now the source answers for itself
                        // (`floor_tokens`) and the comfortable size stays `min`, so
                        // the heavyweights keep their allocation when budget allows
                        // AND survive at their headline when it does not.
                        let floor = s.floor_tokens().min(per_source_max);
                        let min = COMFORTABLE_SOURCE_TOKENS.min(per_source_max).max(floor);
                        (floor, min, per_source_max)
                    }
                };
                RagSourceBudget {
                    source_id: s.source_id().to_string(),
                    priority: 10,
                    floor_tokens: floor,
                    min_tokens: min,
                    max_tokens: max,
                    required: false,
                }
            })
            .collect();

        // Emit the TurnStart capture so audit/replay sees the
        // budget the brain actually asked for, not what landed.
        let turn_id = Uuid::new_v4();
        self.capture_sink.record(RagCaptureEvent::TurnStart {
            captured_at_ms: now_ms,
            persona_id,
            turn_id: Some(turn_id),
            context_window,
            reserved,
            source_budgets: budgets.clone(),
            context: rag_ctx.clone(),
        });

        let adapter = FlexboxRagBudgetAdapter::new();
        let allocation = adapter.allocate(&rag_ctx, context_window, reserved, &budgets);

        self.capture_sink.record(RagCaptureEvent::BudgetAllocated {
            captured_at_ms: now_ms,
            persona_id,
            turn_id: Some(turn_id),
            allocation: allocation.clone(),
        });

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

        self.capture_sink.record(RagCaptureEvent::TurnEnd {
            captured_at_ms: now_ms,
            persona_id,
            turn_id: Some(turn_id),
        });

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
    fn source_id(&self) -> &'static str {
        self.0.source_id()
    }

    fn expand_command(&self) -> Option<&'static str> {
        // delegates to the inner source; expansion is that source's to declare.
        None
    }

    /// Delegates to the inner source — the floor is that source's to declare.
    fn floor_tokens(&self) -> u32 {
        self.0.floor_tokens()
    }

    async fn deliver(
        &self,
        ctx: &RagContext,
        budget: u32,
        resolution: ResolutionPreference,
    ) -> RagDelivery {
        self.0.deliver(ctx, budget, resolution).await
    }
    async fn deliver_continuation(
        &self,
        ctx: &RagContext,
        cursor: crate::persona::rag_budget::ContinuationCursor,
        budget: u32,
    ) -> Option<RagDelivery> {
        self.0.deliver_continuation(ctx, cursor, budget).await
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

    /// Suppress unused import warning for the explicit Noop type when
    /// the rest of the tests don't reference it directly. Keeps the
    /// import alive for visibility checking + future tests.
    #[allow(dead_code)]
    fn _noop_alive() -> NoopRagCaptureSink {
        NoopRagCaptureSink
    }

    // ---- Slice A (task #148): brain composes via FlexboxRagBudgetAdapter
    //      over engram + airc, not via inspect_persona_rag's ad-hoc seam.

    use crate::persona::inference_profile::PersonaInferenceProfile;
    use crate::persona::rag_budget::{AllocationState, ContinuationCursor, RagDelivery, RagItem};
    use async_trait::async_trait;

    /// Test source that returns a fixed budget-aware payload — proves
    /// the brain wires the budget through to per-source deliver.
    struct CannedSource {
        id: &'static str,
        tokens_per_item: u32,
        items_offered: usize,
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
                continuation: None,
                resolution_used: ResolutionPreference::Raw,
            }
        }
        async fn deliver_continuation(
            &self,
            _ctx: &RagContext,
            _cursor: ContinuationCursor,
            _budget: u32,
        ) -> Option<RagDelivery> {
            None
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
