use super::*;

/// Site-wide substrate call context. Joel's framing (2026-05-31):
/// "Usually you pass around a context. Universally. Common pattern
/// from Android among others. … This is usually the first parameter
/// or you use structs. Got into big annoying parameter hell last
/// iteration because you weren't grouping things and were
/// haphazardly overloading huge lists of bullshit."
///
/// Lives here provisionally; will likely move to
/// `crate::runtime::SubstrateContext` once another cognitive module
/// (motor cortex, recall scorer, hippocampus tick) wants the same
/// shape. All substrate operations extend or wrap this — RAG via
/// `RagContext`, motor cortex via `MotorContext`, etc.
///
/// Cheap to clone (Copy-ish fields + small handles); typically
/// constructed once per cognition turn and passed by reference
/// throughout that turn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubstrateContext {
    /// Persona this operation is for. Per-persona modules MUST
    /// validate that `ctx.persona_id` matches their own binding
    /// (defense-in-depth) and MUST refuse cursors / handles from
    /// a different persona.
    pub persona_id: uuid::Uuid,

    /// Wallclock at this turn's start. Modules should read THIS
    /// instead of calling `SystemTime::now()` so turn observations
    /// are stamped consistently and deterministic replay is
    /// possible.
    pub now_ms: u64,

    /// Optional airc room the turn is happening inside. Modules
    /// that bias by current channel/room (per Algorithm 2
    /// "channel-as-bias-not-filter") read this. None when the turn
    /// has no specific room context (background consolidation,
    /// idle sleep tick, etc.).
    pub airc_room: Option<airc_core::RoomId>,

    /// Optional turn_id — the cognition tick that produced this
    /// context. Useful for cross-module telemetry correlation.
    /// None when the call isn't tied to a specific turn.
    pub turn_id: Option<uuid::Uuid>,
}

impl SubstrateContext {
    pub fn for_persona(persona_id: uuid::Uuid, now_ms: u64) -> Self {
        Self {
            persona_id,
            now_ms,
            airc_room: None,
            turn_id: None,
        }
    }

    /// Like [`for_persona`](Self::for_persona) but stamped with the CONTEXT the
    /// turn is happening inside (the WHERE axis — `Workspace::room_id`, the
    /// tick's contextId). Room-scoped sources compare it against their own bound
    /// room and abstain on mismatch, so a turn in room B (or a synthetic context
    /// like the eval fork's nil room) never receives room A's board, roster, or
    /// doctrine. `None` (the plain `for_persona`) remains "no room context
    /// claimed" — background consolidation and legacy callers keep today's
    /// behavior. See [[identity-context-session-three-axes]].
    pub fn for_persona_in_room(persona_id: uuid::Uuid, now_ms: u64, room: uuid::Uuid) -> Self {
        Self {
            persona_id,
            now_ms,
            airc_room: Some(airc_core::RoomId::from_uuid(room)),
            turn_id: None,
        }
    }
}

/// RAG-specific extension of SubstrateContext. Wraps the substrate
/// context via composition + Deref so callers write `ctx.persona_id`
/// directly without `ctx.substrate.persona_id` noise. Future RAG-
/// specific fields (target_tokenizer, assembly_strategy_hint, etc.)
/// land here without changing the substrate-wide base.
///
/// Per Joel's "rag context extends or contains a site wide context
/// (airc and persona details) and for rag has something special":
/// composition is the safer shape — we can swap substrate context
/// behind the scenes without breaking RAG callers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagContext {
    pub substrate: SubstrateContext,
    // Future RAG-specific extensions go here. Empty for now is fine —
    // the wrapper exists so future fields don't change trait
    // signatures.
}

impl std::ops::Deref for RagContext {
    type Target = SubstrateContext;
    fn deref(&self) -> &Self::Target {
        &self.substrate
    }
}

impl RagContext {
    pub fn for_persona(persona_id: uuid::Uuid, now_ms: u64) -> Self {
        Self {
            substrate: SubstrateContext::for_persona(persona_id, now_ms),
        }
    }

    /// Context-stamped variant — see [`SubstrateContext::for_persona_in_room`].
    pub fn for_persona_in_room(persona_id: uuid::Uuid, now_ms: u64, room: uuid::Uuid) -> Self {
        Self {
            substrate: SubstrateContext::for_persona_in_room(persona_id, now_ms, room),
        }
    }
}

/// The ONE room gate every room-scoped source shares: does a delivery from a
/// source bound to `bound` belong in the turn context `ctx`? `true` = deliver.
/// Abstains ONLY when both sides are known and disagree — an unbound source
/// (`None`, legacy/test construction) and an unstamped ctx (`airc_room: None`,
/// background consolidation) both keep pre-gate behavior. One logical decision,
/// one place (the compression law); every abstain emits a probe naming both
/// rooms so a mis-binding is diagnosable from the PROBE STREAM, never a silent
/// blank grounding block. [[identity-context-session-three-axes]]
///
/// `probe!`, not `tracing::info!(probe_class = …)`. Glass-boxed 2026-08-14
/// chasing the anchor_silent cascade (#346/#353/#264): this abstain WAS a bare
/// `tracing::info!` carrying a `probe_class` field, which is the "tracing
/// masquerading as a probe" move the concurrency guide forbids — it never
/// reaches `~/.continuum/probes/`, so querying the probe stream for it returned
/// a confident zero that meant nothing at all. An absence is only evidence when
/// the instrument can produce a presence; this one couldn't.
pub fn room_scope_allows(bound: Option<uuid::Uuid>, ctx: &RagContext, source_id: &str) -> bool {
    match (bound, ctx.airc_room.as_ref()) {
        (Some(b), Some(t)) if t.as_uuid() != b => {
            continuum_base::probe!(
                class = "rag.room_gate.abstain",
                source = %source_id,
                bound_room = %b,
                turn_room = %t.as_uuid(),
                persona_id = %ctx.persona_id,
                "room-scoped source abstained: turn is in a different context than its room"
            );
            false
        }
        _ => true,
    }
}

//=============================================================================
// CORE TYPES
//=============================================================================
/// One source's budget claim. Sent INTO the allocator as input.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RagSourceBudget {
    /// Stable identifier (`"conversation"`, `"memories"`, …). Owned
    /// String so the budget can be serialized into a capture trace
    /// (per `rag_capture.rs`) and deserialized for replay. Sources
    /// still expose `source_id()` as `&'static str` via the trait;
    /// the budget claim is just the wire-shape envelope.
    pub source_id: String,

    /// Priority weight 1-10, higher = more important. Used as the
    /// flex-grow share when distributing free tokens.
    pub priority: u8,

    /// UNCONDITIONAL minimum tokens. Even if other required sources
    /// can't fit their minimums, this floor is honored first or the
    /// source's allocation state escalates to `UnderProvisioned`.
    /// The recent-universal floor (per the cognition-cache-hierarchy
    /// doc) lives here on `ConversationSource`.
    pub floor_tokens: u32,

    /// Flex-basis target — desired baseline above the floor. The
    /// allocator pulls down to `floor_tokens` before dropping a
    /// required source; for required=false sources, falling below
    /// `min_tokens` triggers `AllocationState::Dropped`.
    pub min_tokens: u32,

    /// Flex-cap — never allocate more than this regardless of
    /// available budget. Stops a high-priority source from
    /// consuming the entire context window when other sources
    /// haven't asked for it.
    pub max_tokens: u32,

    /// If true, allocation FAILS when this source can't get
    /// `floor_tokens`; if false, the source may be dropped silently
    /// (its `AllocationState` shows `Dropped` for telemetry).
    pub required: bool,
}

impl RagSourceBudget {
    /// Existing retrieval appetite, resolved against this turn's capacity.
    /// The source owns its atomic floor; the shared allocator still owns fitting.
    pub fn retrieval(source_id: &str, floor: u32, window: u32, headroom: u32) -> Self {
        let max = (((window as u64) * 6 / 10) as u32).min(headroom);
        let floor = floor.min(max);
        Self {
            source_id: source_id.to_owned(),
            priority: 10,
            floor_tokens: floor,
            min_tokens: COMFORTABLE_SOURCE_TOKENS.min(max).max(floor),
            max_tokens: max,
            required: false,
        }
    }

    /// A lightweight source's existing window-relative offer. The denominator
    /// belongs to that source's content policy, never to its identifier.
    pub fn floorless_share(
        source_id: &str,
        window: u32,
        headroom: u32,
        denominator: std::num::NonZeroU32,
    ) -> Self {
        let mut claim = Self::retrieval(source_id, 0, window, headroom);
        claim.min_tokens = 0;
        claim.max_tokens = claim.max_tokens.min(window / denominator.get());
        claim
    }
}

/// Existing comfortable retrieval target (not the atomic survival floor).
/// Measured first units are 6..40 tokens; a source can survive below this target.
// context-budget-exempt: a per-source TARGET, capped by the resolved window/headroom
pub(super) const COMFORTABLE_SOURCE_TOKENS: u32 = 500;

/// Per-source outcome. Reported back from the allocator.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceAllocation {
    pub source_id: String,
    pub allocated_tokens: u32,
    pub requested_floor: u32,
    pub requested_min: u32,
    pub requested_max: u32,
    pub state: AllocationState,
}

/// What happened to a source's allocation. Telemetry-honest per the
/// substrate-is-a-good-citizen doctrine — the caller sees exactly
/// where each source landed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AllocationState {
    /// Got >= min_tokens. The source delivers its preferred content
    /// at full resolution.
    Satisfied,
    /// Got >= floor_tokens but < min_tokens. The source delivers
    /// at the floor — fewer items / compressed / pin-only — but
    /// the floor is honored.
    FloorOnly,
    /// required=false source got 0 tokens. Caller skips it; no
    /// content from this source enters the prompt this turn.
    Dropped,
    /// required=true source got < floor_tokens. Caller MUST
    /// escalate — substrate-side warning, request smaller model,
    /// or request lower-resolution content. The substrate never
    /// silently clips, so this state surfaces the operator
    /// decision.
    UnderProvisioned,
}

/// Reserved tokens — fixed costs that come off the top before any
/// source allocation. `system` is the system prompt + identity
/// header overhead; `completion` is the tokens reserved for the
/// model's output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReservedTokens {
    pub system: u32,
    pub completion: u32,
}

impl ReservedTokens {
    pub fn total(self) -> u32 {
        self.system.saturating_add(self.completion)
    }

    /// THE window-scaled reservation shape (#424 dedup — this derivation was
    /// byte-identical in `unified.rs` and `rag_inspect.rs`; one logical
    /// decision lives in one place). Reservations scale as a percentage of
    /// the window per [[intent-driven-api-not-hot-patches]]: a 2048-window
    /// persona can't reserve a flat 4000 for completion (negative headroom →
    /// all sources get 0 → cognition fires with no RAG content → LLM defaults
    /// to the grammar-shortest "will_respond=false" attractor), while an
    /// M-series 32k persona shouldn't be pinned to Compat-tier crumbs.
    ///
    /// - system: 10% of window, clamped [128, 512]
    /// - completion: 25% of window, clamped [256, 4_000]
    ///
    /// This is a FALLBACK shape, not a budgeter: the substrate's real
    /// budgeter (profile/model-characteristic driven) can override via a
    /// richer reservation API. NOTE the 4_000 completion ceiling is the RAG
    /// *reservation* only — the generation ceiling itself is deliberately
    /// uncapped (`llm_deliberation_faculty::completion_budget_for`, Joel
    /// 2026-07-13: stop choking context); whether this reservation should
    /// follow it up on large windows is an open budgeting question, not a
    /// dedup question.
    pub fn scaled_for_window(context_window: u32) -> Self {
        Self {
            system: (context_window / 10).clamp(128, 512),
            completion: (context_window / 4).clamp(256, 4_000),
        }
    }

    /// Actual tokens left for sources after this reservation. Exhausted capacity
    /// is zero, never a fabricated minimum; the allocator reports underprovisioning.
    pub fn headroom_within(self, context_window: u32) -> u32 {
        context_window.saturating_sub(self.total())
    }
}

/// Full allocation result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BudgetAllocation {
    pub context_window: u32,
    pub reserved: ReservedTokens,
    pub available_for_sources: u32,
    pub allocations: Vec<SourceAllocation>,
    pub total_allocated: u32,
    pub unallocated: u32,
    /// True if any required source ended up `UnderProvisioned`.
    /// Caller MUST handle this — escalate to operator, request
    /// lower-resolution content from sources, or switch models.
    pub escalation_needed: bool,
    /// Warnings collected during allocation — non-fatal but
    /// surfaced for operator visibility (e.g., "floors exceeded
    /// available budget; dropped required=false sources").
    pub warnings: Vec<String>,
}

//=============================================================================
// SOURCE-OWNED DELIVERY (the no-clipping mechanism)
//=============================================================================
/// What "resolution" of content the allocator wants from a source.
/// The source delivers at the resolution that fits its budget;
/// compression is a substrate-side fallback, never a clip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionPreference {
    /// Verbatim, full fidelity. L1 raw — recent messages, current
    /// engrams in their original form.
    Raw,
    /// L2-style outlined gist. Used when raw doesn't fit but the
    /// source has a compressed form available.
    Compressed,
    /// Single-sentence digest per item.
    Summarized,
    /// Metadata-only ("3 engrams from coding session, gist available
    /// on demand via cursor"). Last resort before drop.
    Placeholder,
}

/// Continuation cursor — a persona-scoped handle to "where this
/// source left off." Per Joel's "we know who is who, have to use
/// handles as we do" framing, this is shaped like the substrate's
/// existing Handle pattern (cell-processor-command-runtime memory):
/// every cursor carries its persona scope, its source scope, and
/// an opaque source-specific resume payload.
///
/// The persona_id guarantees the cursor can't be accidentally
/// applied to a different citizen's recall state. The source_id
/// guarantees the cursor can only resume the source that produced
/// it. The opaque field is the source's private resume state —
/// could be a row offset, an embedding-similarity threshold, a
/// merkle hash of what was already delivered, anything the source
/// needs to pick up where it left off.
///
/// Future substrate-side extensions may add: turn_id (which
/// cognition turn produced this), room_id (which activity scope),
/// budget_used (so the resume can decide whether more is now
/// affordable). All extensions go on this struct, NOT inside
/// `opaque` — keep substrate concerns substrate-visible.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContinuationCursor {
    /// Persona this cursor belongs to. Sources MUST validate that
    /// `deliver_continuation` is being called for the same persona
    /// that produced the cursor — substrate-side identity check.
    pub persona_id: uuid::Uuid,
    /// Source that produced the cursor. Sources MUST refuse to
    /// resume cursors from a different source_id.
    pub source_id: String,
    /// Source-private resume state. Allocator does not inspect.
    pub opaque: serde_json::Value,
}

/// One delivered item — already a complete atomic unit by the
/// source's definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RagItem {
    /// Ready-to-include text. The source has serialized, formatted,
    /// and verified structural completeness. Allocator concatenates
    /// directly into the prompt.
    pub content: String,
    /// Pre-counted by the source using the model's tokenizer.
    pub tokens: u32,
    /// For audit + provenance — engram_id, message_id, file_path,
    /// content hash. Lets prompt assembly + sentinel verifiers
    /// trace what made it in.
    pub metadata: serde_json::Value,
}

/// What a source returns when asked to deliver.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RagDelivery {
    pub source_id: String,
    /// Items already pre-validated as complete atomic units. Never
    /// partial. Sum of `items[i].tokens` <= the budget the source
    /// was given.
    pub items: Vec<RagItem>,
    /// Actual tokens consumed across all items.
    pub tokens_used: u32,
    /// Some(cursor) → the source has more than it delivered. The remainder
    /// reaches the reader as a HANDLE: the workspace marks the block
    /// `source_truncated` and its notice names the source's `expand_command`
    /// (whole-or-handle). No production allocator resumes a cursor today
    /// (card e049aa82). None → the source delivered everything it had, or
    /// doesn't support pagination.
    pub continuation: Option<ContinuationCursor>,
    /// What resolution the source actually used. May differ from
    /// the requested resolution if the source's content can't fit
    /// at the requested resolution.
    pub resolution_used: ResolutionPreference,
}

//=============================================================================
// SOURCE TRAIT
//=============================================================================
/// Which room a source describes when task and conversation are distinct.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RagRoomScope {
    Conversation,
    Activity,
}

/// A RAG content source. Implementations hold state via interior
/// mutability (DashMap, Mutex, atomics) — `deliver` takes `&self`.
///
/// Examples expected over the next slices:
/// - `ConversationSource` reads recent messages, atomic unit = one
///   message, holds a cursor for "older than T" pagination
/// - `EngramSource` reads RecallMetadata + admission_state engrams,
///   atomic unit = one engram, ranks by salience × structural
///   relevance × recency, supports compressed resolution via the
///   engram's existing summary form
/// - `CodeSource` reads file contents, atomic unit = one function
///   or snippet, supports pagination by file
/// - `ToolSource` reads available tool descriptions, atomic unit =
///   one tool description, no pagination
#[async_trait]
pub trait RagSource: Send + Sync {
    /// Conversation sources retain wake attribution; task-board adapters opt in.
    fn room_scope(&self) -> RagRoomScope {
        RagRoomScope::Conversation
    }

    fn source_id(&self) -> &'static str;

    /// The verb that yields THIS source's content in full, for when the prompt
    /// budget could only fit part of it.
    ///
    /// A truncated grounding block has to tell the reader two things: that it is
    /// truncated, and **exactly how to see the rest**. "The full list is available
    /// from the matching command" fails the second half — a citizen cannot run a
    /// description. It has to be the real verb, spelled the way she would type it,
    /// because a name she has to guess is a name she gets wrong
    /// ([[command-names-must-be-accurate]]).
    ///
    /// `None` is a legitimate answer for a source with genuinely nothing more to
    /// show (a one-shot fact, a stub). It is NOT the answer for "I didn't think
    /// about it" — which is why there is no default impl: every source decides,
    /// the same forcing function as [`super::super::room_board_source::RoomBoardReader::peer_names`].
    fn expand_command(&self) -> Option<&'static str>;

    /// The cost, in tokens, of the SMALLEST COMPLETE STATEMENT this source can
    /// make — its first atomic unit, not its comfortable size.
    ///
    /// This is the allocator's floor for the source, and the allocator's rule
    /// for a floor that doesn't fit is to drop the source ENTIRELY. So this
    /// number decides, alone, whether a citizen under budget pressure hears
    /// anything at all from this source.
    ///
    /// ### The defect this exists to kill (measured 2026-08-06)
    ///
    /// Every source was assigned a hardcoded floor of **500** tokens by
    /// `unified.rs`, a number unrelated to any of them. Their real first units,
    /// measured off a live prompt: room 6, board 26, workspace-map 32,
    /// recall 40 — about **104 tokens for one complete unit from all six**.
    /// On a node whose grounding budget measured 0..214, every source was
    /// dropped on 100% of turns (137 of 137 for one citizen, 132 of 132 for
    /// another) while asking for 12-80x more than it needed to say something
    /// true. That is what made grounding all-or-nothing instead of degrading:
    /// a source that could have delivered a complete 26-token headline was
    /// never asked for it.
    ///
    /// So: answer with what your FIRST unit actually costs. Not what you'd
    /// like. `0` is legitimate and means "I have no floor — give me whatever is
    /// left over" (the lightweight roster/doctrine shape). There is no default
    /// impl on purpose: a default would let a new source silently inherit
    /// someone else's appetite, which is exactly how the 500 got everywhere.
    fn floor_tokens(&self) -> u32;

    /// Declare this source's appetite independently of its name or decoration.
    /// This is input to the existing allocator, not a second allocation path.
    fn budget_claim(&self, window: u32, headroom: u32) -> RagSourceBudget {
        RagSourceBudget::retrieval(self.source_id(), self.floor_tokens(), window, headroom)
    }

    /// Deliver as many complete atomic units as fit within `budget`.
    /// The source decides what counts as complete; allocator only
    /// trusts that `delivery.tokens_used <= budget`.
    ///
    /// `ctx` carries the per-call substrate context (persona scope,
    /// timing, room handle). Sources MUST validate that
    /// `ctx.persona_id == self.persona_id` if they're bound to a
    /// specific persona at construction.
    ///
    /// If `resolution = Raw` doesn't fit, the source MAY automatically
    /// fall back to a lower resolution and report
    /// `delivery.resolution_used`. The source decides when fallback
    /// is preferable to delivering fewer items at higher resolution.
    async fn deliver(
        &self,
        ctx: &RagContext,
        budget: u32,
        resolution: ResolutionPreference,
    ) -> RagDelivery;

    /// Resume delivery from a prior cursor. Returns None if the
    /// cursor is stale, the source doesn't support pagination, the
    /// cursor was issued for a different persona / source, or the
    /// source has no more content.
    async fn deliver_continuation(
        &self,
        ctx: &RagContext,
        cursor: ContinuationCursor,
        budget: u32,
    ) -> Option<RagDelivery>;
}

//=============================================================================
// ADAPTER TRAIT — POLYMORPHISM RAIL
//=============================================================================
/// The allocation strategy. Ship one heuristic impl
/// (`FlexboxRagBudgetAdapter`); future learnable adapters
/// (`LearnedRagBudgetAdapter` reading telemetry from
/// `MemoryParameterAdapter`) slot in without changing callers per
/// the adapter-first methodology.
pub trait RagBudgetAdapter: Send + Sync {
    fn name(&self) -> &'static str;

    /// Allocate tokens to each source. Pure-function — no I/O, no
    /// async. Sources are CALLED later with their allocation by
    /// the prompt-assembly layer.
    ///
    /// `ctx` first per the Android Context pattern. Allocators may
    /// use it for telemetry stamping, persona-specific tuning (a
    /// future `LearnedRagBudgetAdapter` reads per-persona regret
    /// signals from `MemoryParameterAdapter`), or for stable
    /// deterministic seeds keyed on `(ctx.persona_id, ctx.turn_id)`.
    fn allocate(
        &self,
        ctx: &RagContext,
        context_window: u32,
        reserved: ReservedTokens,
        sources: &[RagSourceBudget],
    ) -> BudgetAllocation;
}

//=============================================================================
// FLEXBOX ADAPTER — THE FIRST CONCRETE IMPL
//=============================================================================
/// CSS-flexbox-inspired allocation. Algorithm (anti-clipping):
///
/// 1. Reserve system + completion off the top
/// 2. **Floor pass** — allocate `floor_tokens` to every source.
///    Floors are unconditional; if floor totals exceed available,
///    drop required=false sources by priority (lowest first) until
///    required floors fit. If even required floors can't fit, set
///    affected sources to `UnderProvisioned` + flag escalation.
/// 3. **Min pass** — top up to `min_tokens` for sources by priority.
///    If a source can't reach `min_tokens` but is at >= `floor_tokens`,
///    its state is `FloorOnly`.
/// 4. **Grow pass** — distribute remaining tokens by priority weight,
///    capped at `max_tokens` per source. Iterate until no movement
///    (capped sources release tokens to non-capped).
/// 5. Report — each source's state classifies the outcome.
pub struct FlexboxRagBudgetAdapter;

impl FlexboxRagBudgetAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for FlexboxRagBudgetAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl RagBudgetAdapter for FlexboxRagBudgetAdapter {
    fn name(&self) -> &'static str {
        "flexbox"
    }

    fn allocate(
        &self,
        _ctx: &RagContext,
        context_window: u32,
        reserved: ReservedTokens,
        sources: &[RagSourceBudget],
    ) -> BudgetAllocation {
        let available = reserved.headroom_within(context_window);

        if available == 0 {
            let warnings = vec![format!(
                "reserved tokens ({}) >= context window ({}); no budget for sources",
                reserved.total(),
                context_window
            )];
            return empty_allocation(context_window, reserved, sources, warnings, true);
        }

        let FlexAllocation {
            allocations,
            unallocated,
            escalation_needed,
            warnings,
        } = flex_allocate(available, sources);
        let mut total_allocated = 0u32;
        for allocation in &allocations {
            total_allocated = total_allocated.saturating_add(allocation.allocated_tokens);
            // The allocator decided how much of her mind each source gets, and
            // until now said NOTHING. So when a grounding block failed to
            // appear, "the source abstained" and "the source was granted zero"
            // were indistinguishable from outside — which is exactly the
            // ambiguity that survived the #331 room fix: gate passing, board
            // non-empty, block still absent, no way to tell why.
            //
            // Emitted per source per allocation: what it ASKED for and what it
            // GOT. A source at allocated=0 (or below its own floor) is a
            // faculty the persona cannot hear this turn, and that must be a
            // readable fact, not an inference. [[observability-as-substrate]]
            if allocation.allocated_tokens == 0
                || allocation.allocated_tokens < allocation.requested_floor
            {
                tracing::info!(
                    probe_class = "rag.budget.starved",
                    source = %allocation.source_id,
                    granted = allocation.allocated_tokens,
                    floor = allocation.requested_floor,
                    min = allocation.requested_min,
                    max = allocation.requested_max,
                    state = ?allocation.state,
                    context_window,
                    "source granted less than its own floor — this faculty is silent this turn"
                );
            }
        }

        BudgetAllocation {
            context_window,
            reserved,
            available_for_sources: available,
            allocations,
            total_allocated,
            unallocated,
            escalation_needed,
            warnings,
        }
    }
}

/// What [`flex_allocate`] decided: every claim's grant in INPUT order, the tokens no
/// claim could take, and the same escalation/warning receipts the adapter reports.
#[derive(Debug, Clone, PartialEq)]
pub struct FlexAllocation {
    pub allocations: Vec<SourceAllocation>,
    pub unallocated: u32,
    /// A required claim got less than its floor.
    pub escalation_needed: bool,
    pub warnings: Vec<String>,
}

impl FlexAllocation {
    /// The grant of the claim named `source_id`; 0 when no such claim was made.
    pub fn granted(&self, source_id: &str) -> u32 {
        self.allocations
            .iter()
            .find(|a| a.source_id == source_id)
            .map_or(0, |a| a.allocated_tokens)
    }
}
