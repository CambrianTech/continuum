//! RagBudgetManager — flexbox-style token allocation across RAG
//! sources, with the no-clipping doctrine baked in.
//!
//! ### What this module solves
//!
//! Every LLM has a different context window — local Qwen 1.7B at 4k,
//! Qwen 3-30B at 128k, Claude Sonnet at 200k, future models at 1M+.
//! Plus per-channel constraints (video real-time is bandwidth-bound,
//! coding sessions can afford bigger working sets) and per-LoRA-stack
//! overhead. The L1 RAG working memory has to share that budget
//! across multiple content sources (recent conversation, salience-
//! scored engrams, code context, tool descriptions, …) WITHOUT
//! truncating anyone mid-content. Clipping breaks HTML, code, JSON,
//! mid-sentence semantics — it's never acceptable.
//!
//! Per `RAGBudgetManager.ts` (the production TS prior art) +
//! `docs/architecture/COGNITION-CACHE-HIERARCHY.md` (the L1 budget
//! math + recent-universal floor doctrine), this module implements
//! a CSS-flexbox-inspired allocator that gives each source a token
//! budget; sources are responsible for delivering COMPLETE atomic
//! units within that budget.
//!
//! ### Doctrine — no clipping
//!
//! When budget is tight, sources are dropped WHOLE in priority
//! order (required=false first). A source that can't satisfy its
//! `floor_tokens` (the unconditional minimum) returns
//! `AllocationState::UnderProvisioned` and the caller escalates —
//! the substrate never silently clips content mid-unit.
//!
//! The source-owned-unit model means each source decides what
//! counts as "complete":
//! - `ConversationSource`: one message
//! - `EngramSource`: one engram
//! - `CodeSource`: one function / one snippet
//! - `ToolSource`: one tool description
//! The allocator never knows what a "complete unit" looks like —
//! it only deals in token counts.
//!
//! ### Doctrine — sources own state
//!
//! Joel, 2026-05-31: "And to maintain state if necessary."
//!
//! Implementations use interior mutability (DashMap, Mutex, atomics)
//! to hold per-source state — cursor positions, recently-served
//! sets, computation caches, telemetry. The `RagSource::deliver`
//! method takes `&self`; state lives inside via the same pattern
//! `PersonaAircRuntimeRegistry`, `RecallMetadataRegistry`, etc.
//! already use across the substrate.
//!
//! ### Variability is intrinsic
//!
//! Context window sizes vary by 250×. Allocation must scale
//! continuously (no `if context > 32k` branches inside the
//! algorithm). The `RagBudgetAdapter` trait + per-profile presets
//! handle the variability cleanly; the math doesn't care.

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

//=============================================================================
// CONTEXT — Android-style first-parameter pattern
//=============================================================================

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
    pub fn from_substrate(substrate: SubstrateContext) -> Self {
        Self { substrate }
    }
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
            crate::probe!(
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

    /// Tokens left for sources after this reservation, floored at 512 so a
    /// tiny window still delivers *some* context instead of zeroing every
    /// source. Sibling of [`Self::scaled_for_window`] — both call sites
    /// computed this identically too.
    pub fn headroom_within(self, context_window: u32) -> u32 {
        context_window.saturating_sub(self.total()).max(512)
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
    /// Some(cursor) → source has more available; allocator may
    /// resume in a future turn. None → source delivered everything
    /// it had OR doesn't support pagination.
    pub continuation: Option<ContinuationCursor>,
    /// What resolution the source actually used. May differ from
    /// the requested resolution if the source's content can't fit
    /// at the requested resolution.
    pub resolution_used: ResolutionPreference,
}

//=============================================================================
// SOURCE TRAIT
//=============================================================================

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
    /// the same forcing function as [`super::room_board_source::RoomBoardReader::peer_names`].
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
        let mut warnings = Vec::new();
        let available = context_window.saturating_sub(reserved.total());

        if available == 0 {
            warnings.push(format!(
                "reserved tokens ({}) >= context window ({}); no budget for sources",
                reserved.total(),
                context_window
            ));
            return empty_allocation(context_window, reserved, sources, warnings, true);
        }

        // Stable sort by priority desc, then by source_id for
        // deterministic tie-break — the boot-time output should
        // not depend on slice ordering or hashmap iteration.
        let mut sorted: Vec<&RagSourceBudget> = sources.iter().collect();
        sorted.sort_by(|a, b| {
            b.priority
                .cmp(&a.priority)
                .then(a.source_id.cmp(&b.source_id))
        });

        // Working allocation: source_id -> tokens. Use a Vec parallel
        // to sorted for cache-locality + deterministic iteration.
        let mut alloc: Vec<u32> = vec![0; sorted.len()];
        let mut state: Vec<AllocationState> = vec![AllocationState::Dropped; sorted.len()];
        let mut remaining: u32 = available;
        let mut escalation_needed = false;

        // ---- Pass 1: floors (unconditional) ----
        // Pre-flight: do all required floors fit?
        let required_floor_sum: u32 = sorted
            .iter()
            .filter(|s| s.required)
            .map(|s| s.floor_tokens)
            .sum();

        if required_floor_sum > available {
            warnings.push(format!(
                "required floor sum ({}) exceeds available ({}); some required sources UnderProvisioned",
                required_floor_sum, available
            ));
        }

        // Allocate floors in priority order. required first, then
        // optional. If we can't honor a required floor, set
        // UnderProvisioned (the floor itself becomes whatever
        // remains, or 0).
        for (i, source) in sorted.iter().enumerate() {
            if !source.required {
                continue;
            }
            if source.floor_tokens <= remaining {
                alloc[i] = source.floor_tokens;
                remaining -= source.floor_tokens;
                state[i] = AllocationState::FloorOnly;
            } else {
                // required source can't get its floor — escalate.
                alloc[i] = remaining;
                remaining = 0;
                state[i] = AllocationState::UnderProvisioned;
                escalation_needed = true;
            }
        }
        for (i, source) in sorted.iter().enumerate() {
            if source.required {
                continue;
            }
            if source.floor_tokens == 0 {
                // optional source with floor 0 — floor is trivially
                // satisfied; mark FloorOnly so pass 2 + pass 3 see it
                // as eligible for grow. (If we left state as Dropped
                // here, the source would be permanently skipped — bug
                // surfaced by the max_caps_distribution test.)
                state[i] = AllocationState::FloorOnly;
                continue;
            }
            if source.floor_tokens <= remaining {
                alloc[i] = source.floor_tokens;
                remaining -= source.floor_tokens;
                state[i] = AllocationState::FloorOnly;
            } else {
                // optional source can't get its floor — drop entirely.
                // alloc[i] stays 0, state stays Dropped.
                warnings.push(format!(
                    "optional source `{}` dropped — floor {} > remaining {}",
                    source.source_id, source.floor_tokens, remaining
                ));
            }
        }

        // ---- Pass 2: min — top up to min_tokens for sources we
        // haven't dropped, in priority order ----
        for (i, source) in sorted.iter().enumerate() {
            if matches!(
                state[i],
                AllocationState::Dropped | AllocationState::UnderProvisioned
            ) {
                continue;
            }
            let needed = source.min_tokens.saturating_sub(alloc[i]);
            let granted = needed
                .min(remaining)
                .min(source.max_tokens.saturating_sub(alloc[i]));
            alloc[i] += granted;
            remaining -= granted;
            if alloc[i] >= source.min_tokens {
                state[i] = AllocationState::Satisfied;
            }
            // else stays FloorOnly
        }

        // ---- Pass 3: grow — distribute remaining by priority weight,
        // capped at max_tokens ----
        // Iterate until no movement (capped sources stop being
        // candidates and free tokens flow to others).
        loop {
            let active: Vec<usize> = sorted
                .iter()
                .enumerate()
                .filter(|(i, s)| {
                    !matches!(
                        state[*i],
                        AllocationState::Dropped | AllocationState::UnderProvisioned
                    ) && alloc[*i] < s.max_tokens
                })
                .map(|(i, _)| i)
                .collect();
            if active.is_empty() || remaining == 0 {
                break;
            }
            let priority_sum: u32 = active.iter().map(|&i| sorted[i].priority as u32).sum();
            if priority_sum == 0 {
                break;
            }
            let mut moved = 0u32;
            for &i in &active {
                let share = ((remaining as u64) * (sorted[i].priority as u64)
                    / (priority_sum as u64)) as u32;
                let headroom = sorted[i].max_tokens - alloc[i];
                let grant = share.min(headroom);
                if grant > 0 {
                    alloc[i] += grant;
                    moved += grant;
                }
            }
            if moved == 0 {
                // No grant could move (e.g., remaining/priority_sum = 0).
                // Give the single highest-priority active source 1 token
                // to break the loop deterministically.
                let i = active[0];
                let headroom = sorted[i].max_tokens - alloc[i];
                if headroom > 0 && remaining > 0 {
                    alloc[i] += 1;
                    moved = 1;
                } else {
                    break;
                }
            }
            remaining = remaining.saturating_sub(moved);
        }

        // Build result in input order (NOT sorted order) for caller
        // ergonomics.
        let mut allocations_by_id: std::collections::HashMap<
            String,
            (u32, AllocationState, &RagSourceBudget),
        > = std::collections::HashMap::new();
        for (i, source) in sorted.iter().enumerate() {
            allocations_by_id.insert(source.source_id.clone(), (alloc[i], state[i], *source));
        }
        let mut allocations = Vec::with_capacity(sources.len());
        let mut total_allocated = 0u32;
        for src in sources {
            let (tokens, st, _) = allocations_by_id
                .remove(&src.source_id)
                .expect("every source must appear in the working alloc");
            total_allocated = total_allocated.saturating_add(tokens);
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
            if tokens == 0 || tokens < src.floor_tokens {
                tracing::info!(
                    probe_class = "rag.budget.starved",
                    source = %src.source_id,
                    granted = tokens,
                    floor = src.floor_tokens,
                    min = src.min_tokens,
                    max = src.max_tokens,
                    state = ?st,
                    context_window,
                    "source granted less than its own floor — this faculty is silent this turn"
                );
            }
            allocations.push(SourceAllocation {
                source_id: src.source_id.to_string(),
                allocated_tokens: tokens,
                requested_floor: src.floor_tokens,
                requested_min: src.min_tokens,
                requested_max: src.max_tokens,
                state: st,
            });
        }

        BudgetAllocation {
            context_window,
            reserved,
            available_for_sources: available,
            allocations,
            total_allocated,
            unallocated: available.saturating_sub(total_allocated),
            escalation_needed,
            warnings,
        }
    }
}

fn empty_allocation(
    context_window: u32,
    reserved: ReservedTokens,
    sources: &[RagSourceBudget],
    warnings: Vec<String>,
    escalation_needed: bool,
) -> BudgetAllocation {
    BudgetAllocation {
        context_window,
        reserved,
        available_for_sources: 0,
        allocations: sources
            .iter()
            .map(|s| SourceAllocation {
                source_id: s.source_id.to_string(),
                allocated_tokens: 0,
                requested_floor: s.floor_tokens,
                requested_min: s.min_tokens,
                requested_max: s.max_tokens,
                state: if s.required {
                    AllocationState::UnderProvisioned
                } else {
                    AllocationState::Dropped
                },
            })
            .collect(),
        total_allocated: 0,
        unallocated: 0,
        escalation_needed,
        warnings,
    }
}

//=============================================================================
// TEST STUB SOURCE — proves the trait shape compiles + composes
//=============================================================================

/// Stub source for tests. Holds a Vec of pre-built RagItems and
/// delivers as many as fit. Demonstrates the interior-mutability
/// pattern (Mutex<usize> cursor) without dragging in real engram
/// store dependencies. Also demonstrates persona-scoped handles —
/// cursors carry the persona_id this source was constructed for.
pub struct StubRagSource {
    source_id: &'static str,
    persona_id: uuid::Uuid,
    items: Vec<RagItem>,
    cursor: std::sync::Mutex<usize>,
}

impl StubRagSource {
    pub fn new(source_id: &'static str, persona_id: uuid::Uuid, items: Vec<RagItem>) -> Self {
        Self {
            source_id,
            persona_id,
            items,
            cursor: std::sync::Mutex::new(0),
        }
    }
}

#[async_trait]
impl RagSource for StubRagSource {
    fn source_id(&self) -> &'static str {
        self.source_id
    }

    fn expand_command(&self) -> Option<&'static str> {
        // Test/stub source — nothing further to fetch.
        None
    }

    /// Test/stub source — floorless, so allocator tests exercise the grow pass
    /// rather than accidentally encoding a production floor.
    fn floor_tokens(&self) -> u32 {
        0
    }

    async fn deliver(
        &self,
        ctx: &RagContext,
        budget: u32,
        _resolution: ResolutionPreference,
    ) -> RagDelivery {
        // Defense-in-depth identity check: this source is bound to
        // a specific persona at construction; refuse calls from a
        // different ctx.persona_id by returning empty (no panics,
        // no half-state — graceful degradation).
        if ctx.persona_id != self.persona_id {
            return RagDelivery {
                source_id: self.source_id.to_string(),
                items: Vec::new(),
                tokens_used: 0,
                continuation: None,
                resolution_used: ResolutionPreference::Placeholder,
            };
        }

        let mut taken = Vec::new();
        let mut used: u32 = 0;
        let start = *self.cursor.lock().unwrap();
        let mut end = start;
        for item in &self.items[start..] {
            if used.saturating_add(item.tokens) > budget {
                break;
            }
            used += item.tokens;
            taken.push(item.clone());
            end += 1;
        }
        let continuation = if end < self.items.len() {
            Some(ContinuationCursor {
                persona_id: self.persona_id,
                source_id: self.source_id.to_string(),
                opaque: serde_json::json!({ "next": end }),
            })
        } else {
            None
        };
        // Update cursor so subsequent deliver() calls resume — this
        // is the state-maintenance pattern Joel asked about.
        *self.cursor.lock().unwrap() = end;
        RagDelivery {
            source_id: self.source_id.to_string(),
            items: taken,
            tokens_used: used,
            continuation,
            resolution_used: ResolutionPreference::Raw,
        }
    }

    async fn deliver_continuation(
        &self,
        ctx: &RagContext,
        cursor: ContinuationCursor,
        budget: u32,
    ) -> Option<RagDelivery> {
        // Defense-in-depth identity checks: refuse cursors not
        // scoped to this persona / this source, and refuse calls
        // from a context for a different persona.
        if ctx.persona_id != self.persona_id {
            return None;
        }
        if cursor.persona_id != self.persona_id {
            return None;
        }
        if cursor.source_id != self.source_id {
            return None;
        }
        let next: usize = cursor.opaque.get("next")?.as_u64()? as usize;
        if next >= self.items.len() {
            return None;
        }
        *self.cursor.lock().unwrap() = next;
        Some(self.deliver(ctx, budget, ResolutionPreference::Raw).await)
    }
}

//=============================================================================
// TESTS
//=============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn budget(
        source_id: &'static str,
        priority: u8,
        floor: u32,
        min: u32,
        max: u32,
        required: bool,
    ) -> RagSourceBudget {
        RagSourceBudget {
            source_id: source_id.to_string(),
            priority,
            floor_tokens: floor,
            min_tokens: min,
            max_tokens: max,
            required,
        }
    }

    fn reserved(system: u32, completion: u32) -> ReservedTokens {
        ReservedTokens { system, completion }
    }

    fn alloc_for<'a>(result: &'a BudgetAllocation, id: &str) -> &'a SourceAllocation {
        result
            .allocations
            .iter()
            .find(|a| a.source_id == id)
            .unwrap()
    }

    fn ctx() -> RagContext {
        RagContext::for_persona(
            uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap(),
            1_000_000,
        )
    }

    #[test]
    fn empty_context_window_under_provisions_required() {
        let adapter = FlexboxRagBudgetAdapter::new();
        let result = adapter.allocate(
            &ctx(),
            500,
            reserved(400, 200),
            &[budget("conversation", 10, 100, 200, 1000, true)],
        );
        assert_eq!(result.available_for_sources, 0);
        assert!(result.escalation_needed);
        assert_eq!(
            alloc_for(&result, "conversation").state,
            AllocationState::UnderProvisioned
        );
    }

    #[test]
    fn single_required_source_satisfied() {
        let adapter = FlexboxRagBudgetAdapter::new();
        let result = adapter.allocate(
            &ctx(),
            10_000,
            reserved(500, 2000),
            &[budget("conversation", 10, 200, 500, 5000, true)],
        );
        let conv = alloc_for(&result, "conversation");
        assert!(conv.allocated_tokens >= 500);
        assert_eq!(conv.state, AllocationState::Satisfied);
        assert!(!result.escalation_needed);
    }

    #[test]
    fn priority_distributes_remaining_proportionally() {
        let adapter = FlexboxRagBudgetAdapter::new();
        // max well above expected share so neither caps before the
        // priority ratio gets to express.
        let result = adapter.allocate(
            &ctx(),
            10_000,
            reserved(0, 0),
            &[
                budget("conversation", 10, 100, 500, 50_000, true),
                budget("memories", 5, 100, 500, 50_000, true),
            ],
        );
        let conv = alloc_for(&result, "conversation");
        let mem = alloc_for(&result, "memories");
        // Both got their mins (500). Remaining 9000 distributed by
        // priority 10 vs 5 → conv should get roughly 2× memories.
        assert!(
            conv.allocated_tokens > mem.allocated_tokens,
            "conv {} mem {}",
            conv.allocated_tokens,
            mem.allocated_tokens
        );
    }

    #[test]
    fn optional_source_drops_when_floor_cant_fit() {
        let adapter = FlexboxRagBudgetAdapter::new();
        let result = adapter.allocate(
            &ctx(),
            1_000,
            reserved(500, 200),
            &[
                budget("conversation", 10, 200, 200, 500, true),
                budget("artifacts", 3, 200, 200, 500, false),
            ],
        );
        // Conversation required, gets its 200 floor. Remaining 100 <
        // artifacts floor 200, so artifacts is Dropped.
        let conv = alloc_for(&result, "conversation");
        let art = alloc_for(&result, "artifacts");
        assert!(conv.allocated_tokens >= 200);
        assert_ne!(conv.state, AllocationState::Dropped);
        assert_eq!(art.allocated_tokens, 0);
        assert_eq!(art.state, AllocationState::Dropped);
        assert!(!result.escalation_needed); // optional drop is fine
    }

    #[test]
    fn required_under_provisions_when_floor_cant_fit() {
        let adapter = FlexboxRagBudgetAdapter::new();
        let result = adapter.allocate(
            &ctx(),
            300,
            reserved(100, 100),
            &[
                budget("conversation", 10, 200, 200, 500, true),
                budget("memories", 5, 200, 200, 500, true),
            ],
        );
        // Available = 100; conv floor 200 takes it all; memories floor
        // 200 can't fit → UnderProvisioned + escalate.
        assert!(result.escalation_needed);
        assert_eq!(
            alloc_for(&result, "memories").state,
            AllocationState::UnderProvisioned
        );
    }

    #[test]
    fn floor_is_honored_above_min() {
        // Joel's recent-universal floor doctrine: even if min is
        // squeezed, floor is unconditional. Here floor == min so the
        // test verifies the floor lands BEFORE the min pass.
        let adapter = FlexboxRagBudgetAdapter::new();
        let result = adapter.allocate(
            &ctx(),
            2_000,
            reserved(0, 0),
            &[
                budget("conversation", 10, 500, 500, 1000, true),
                budget("memories", 5, 200, 600, 1500, false),
            ],
        );
        let conv = alloc_for(&result, "conversation");
        let mem = alloc_for(&result, "memories");
        assert!(conv.allocated_tokens >= 500);
        assert!(mem.allocated_tokens >= 200);
    }

    #[test]
    fn max_caps_distribution() {
        let adapter = FlexboxRagBudgetAdapter::new();
        let result = adapter.allocate(
            &ctx(),
            10_000,
            reserved(0, 0),
            &[
                budget("tiny", 10, 0, 0, 100, false),
                budget("big", 5, 0, 0, 9_000, false),
            ],
        );
        let tiny = alloc_for(&result, "tiny");
        let big = alloc_for(&result, "big");
        assert_eq!(tiny.allocated_tokens, 100); // capped
                                                // Big should absorb whatever the priority-10 cap left behind.
        assert!(big.allocated_tokens >= 5000);
        assert!(big.allocated_tokens <= 9_000);
    }

    #[test]
    fn deterministic_priority_tiebreak() {
        // Two sources at same priority must allocate identically across
        // runs. Use source_id alpha order.
        let adapter = FlexboxRagBudgetAdapter::new();
        let result_a = adapter.allocate(
            &ctx(),
            10_000,
            reserved(0, 0),
            &[
                budget("a", 5, 0, 500, 2000, false),
                budget("b", 5, 0, 500, 2000, false),
            ],
        );
        let result_b = adapter.allocate(
            &ctx(),
            10_000,
            reserved(0, 0),
            &[
                budget("b", 5, 0, 500, 2000, false),
                budget("a", 5, 0, 500, 2000, false),
            ],
        );
        let a_in_a = alloc_for(&result_a, "a").allocated_tokens;
        let a_in_b = alloc_for(&result_b, "a").allocated_tokens;
        assert_eq!(a_in_a, a_in_b, "allocation must be input-order-independent");
    }

    // ---- Source trait + stub tests ----

    fn item(text: &str, tokens: u32) -> RagItem {
        RagItem {
            content: text.to_string(),
            tokens,
            metadata: serde_json::json!({}),
        }
    }

    fn persona() -> uuid::Uuid {
        uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap()
    }

    #[tokio::test]
    async fn stub_source_delivers_what_fits() {
        let source = StubRagSource::new(
            "stub",
            persona(),
            vec![item("a", 10), item("b", 20), item("c", 100)],
        );
        let delivery = source.deliver(&ctx(), 50, ResolutionPreference::Raw).await;
        // a (10) + b (20) = 30 fits, c (100) doesn't.
        assert_eq!(delivery.items.len(), 2);
        assert_eq!(delivery.tokens_used, 30);
        assert!(delivery.continuation.is_some());
        assert_eq!(delivery.continuation.unwrap().persona_id, persona());
    }

    #[tokio::test]
    async fn stub_source_continuation_resumes() {
        let source = StubRagSource::new(
            "stub",
            persona(),
            vec![item("a", 10), item("b", 10), item("c", 10), item("d", 10)],
        );
        let first = source.deliver(&ctx(), 20, ResolutionPreference::Raw).await;
        assert_eq!(first.items.len(), 2);
        let cursor = first.continuation.unwrap();
        let second = source
            .deliver_continuation(&ctx(), cursor, 100)
            .await
            .unwrap();
        assert_eq!(second.items.len(), 2);
        assert!(second.continuation.is_none());
    }

    #[tokio::test]
    async fn stub_source_returns_none_when_exhausted() {
        let source = StubRagSource::new("stub", persona(), vec![item("a", 10)]);
        let first = source.deliver(&ctx(), 100, ResolutionPreference::Raw).await;
        assert_eq!(first.items.len(), 1);
        assert!(first.continuation.is_none());

        let stale = ContinuationCursor {
            persona_id: persona(),
            source_id: "stub".to_string(),
            opaque: serde_json::json!({ "next": 99 }),
        };
        let exhausted = source.deliver_continuation(&ctx(), stale, 100).await;
        assert!(exhausted.is_none());
    }

    #[tokio::test]
    async fn stub_source_never_partial_includes() {
        // The no-clipping invariant: even with budget mid-item, the
        // source skips the over-budget item rather than partial-include.
        let source = StubRagSource::new("stub", persona(), vec![item("huge", 500)]);
        let delivery = source.deliver(&ctx(), 100, ResolutionPreference::Raw).await;
        assert_eq!(delivery.items.len(), 0);
        assert_eq!(delivery.tokens_used, 0);
        // Continuation set because the item still exists, just didn't
        // fit at this budget.
        assert!(delivery.continuation.is_some());
    }

    #[tokio::test]
    async fn stub_source_refuses_cross_persona_cursor() {
        // Joel's substrate-side identity check: cursors from another
        // citizen MUST be refused. "We know who is who, have to use
        // handles" — handles enforce persona scoping.
        let pax = uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000abc").unwrap();
        let maya = uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000def").unwrap();
        let pax_ctx = RagContext::for_persona(pax, 1_000_000);
        let maya_ctx = RagContext::for_persona(maya, 1_000_000);

        let pax_source = StubRagSource::new("stub", pax, vec![item("a", 10), item("b", 10)]);
        let pax_first = pax_source
            .deliver(&pax_ctx, 15, ResolutionPreference::Raw)
            .await;
        let pax_cursor = pax_first.continuation.unwrap();
        assert_eq!(pax_cursor.persona_id, pax);

        // Maya's source must refuse Pax's cursor — both because the
        // cursor's persona_id doesn't match Maya's binding AND because
        // the source verifies its own persona_id against ctx.persona_id.
        let maya_source = StubRagSource::new("stub", maya, vec![item("x", 10), item("y", 10)]);
        let cross = maya_source
            .deliver_continuation(&maya_ctx, pax_cursor, 100)
            .await;
        assert!(cross.is_none(), "cross-persona cursor must be refused");
    }

    #[tokio::test]
    async fn stub_source_refuses_wrong_source_id_cursor() {
        let source = StubRagSource::new("conversation", persona(), vec![item("a", 10)]);
        let alien_cursor = ContinuationCursor {
            persona_id: persona(),
            source_id: "memories".to_string(),
            opaque: serde_json::json!({ "next": 0 }),
        };
        let cross = source.deliver_continuation(&ctx(), alien_cursor, 100).await;
        assert!(cross.is_none(), "wrong-source cursor must be refused");
    }

    #[tokio::test]
    async fn stub_source_refuses_wrong_persona_ctx() {
        // The defense-in-depth check: source bound to persona A,
        // called with ctx for persona B — must return empty rather
        // than serve B's caller with A's content.
        let pax = uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000abc").unwrap();
        let maya = uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000def").unwrap();
        let pax_source = StubRagSource::new("stub", pax, vec![item("a", 10)]);
        let maya_ctx = RagContext::for_persona(maya, 1_000_000);
        let delivery = pax_source
            .deliver(&maya_ctx, 100, ResolutionPreference::Raw)
            .await;
        assert_eq!(delivery.items.len(), 0);
        assert_eq!(delivery.resolution_used, ResolutionPreference::Placeholder);
    }

    // what this catches (#338, measured live 2026-08-06): a source being dropped
    // WHOLE because its declared floor was its COMFORTABLE size rather than the
    // cost of its first complete unit.
    //
    // Every grounding source declared floor=min=500 (a hardcoded number in
    // unified.rs, unrelated to any of them) while their real first units measured
    // 6..40 tokens — about 104 for one unit from all six. On a node whose
    // grounding budget measured 0..214, that made the allocator drop EVERY source
    // on 100% of turns (137/137 and 132/132 for two citizens), so a citizen who
    // could have been told "you hold 0 cards; 59 claimable" in 26 tokens was told
    // nothing and reported she had no work. Grounding was all-or-nothing when it
    // should have degraded.
    //
    // The invariant: at a budget that fits the FLOORS but not the comfortable
    // sizes, every source still speaks. Floor is survival; min is appetite.
    #[test]
    fn sources_survive_at_their_first_unit_when_budget_wont_fit_their_appetite() {
        let adapter = FlexboxRagBudgetAdapter::new();
        // Real measured first units; 500 is the comfortable target each would
        // like. Total floors = 104, total appetites = 2000.
        let sources = [
            budget("room", 10, 6, 500, 2000, false),
            budget("room-kanban", 10, 26, 500, 2000, false),
            budget("workspace-map", 10, 32, 500, 2000, false),
            budget("recall", 10, 40, 500, 2000, false),
        ];
        // 214 tokens — the live starved budget. Fits all four floors (104),
        // fits not even ONE comfortable size.
        let alloc = adapter.allocate(
            &RagContext::for_persona(uuid::Uuid::nil(), 0),
            214,
            ReservedTokens {
                system: 0,
                completion: 0,
            },
            &sources,
        );
        for (i, s) in sources.iter().enumerate() {
            assert!(
                alloc.allocations[i].allocated_tokens >= s.floor_tokens,
                "`{}` must survive at its first unit ({} tokens) on a starved budget — \
                 got {}. A source dropped here is a citizen told nothing at all.",
                s.source_id,
                s.floor_tokens,
                alloc.allocations[i].allocated_tokens,
            );
            assert!(
                !matches!(alloc.allocations[i].state, AllocationState::Dropped),
                "`{}` was DROPPED at a budget that fits its floor — this is the #338 defect",
                s.source_id,
            );
        }
    }

    /// What this catches: the window-scaled reservation stays ONE derivation
    /// with the documented shape — a second hand-copied variant (the exact
    /// duplication this replaced across unified.rs / rag_inspect.rs) shows up
    /// as a drift here. Also pins the headroom floor: a tiny window must
    /// still deliver ≥512 tokens to sources, never zero. regression for #424
    #[test]
    fn reserved_tokens_scaled_shape_is_the_one_derivation() {
        // Compat-tier 2048: percentages, floors active.
        let small = ReservedTokens::scaled_for_window(2048);
        assert_eq!(small.system, 204); // 10%, inside [128, 512]
        assert_eq!(small.completion, 512); // 25%, inside [256, 4_000]
        assert_eq!(small.headroom_within(2048), 2048 - 204 - 512);

        // M-series 32k: both ceilings engage.
        let big = ReservedTokens::scaled_for_window(32_768);
        assert_eq!(big.system, 512);
        assert_eq!(big.completion, 4_000);
        assert_eq!(big.headroom_within(32_768), 32_768 - 4_512);

        // Degenerate window: floors dominate, headroom floor holds — sources
        // still get 512, never zero (the zero-budget annihilation class, #259).
        let tiny = ReservedTokens::scaled_for_window(256);
        assert_eq!((tiny.system, tiny.completion), (128, 256));
        assert_eq!(tiny.headroom_within(256), 512);
    }
}

// Silence unused-Arc-import warning on builds where the type isn't
// referenced outside docs. The Arc pattern is the expected runtime
// shape for sharing sources across modules.
#[allow(dead_code)]
fn _doc_arc_pattern_unused() -> Option<Arc<dyn RagSource>> {
    None
}
