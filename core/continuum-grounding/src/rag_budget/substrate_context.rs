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

//=============================================================================
// SOURCE-OWNED DELIVERY (the no-clipping mechanism)
//=============================================================================
/// One delivered item — already a complete atomic unit by the
/// source's definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RagItem {
    /// Stable for the item's life within its source (a card id, a post id, a peer id, an
    /// engram id): what her view keys the item by, so it appends only when this item
    /// changes. Empty in captures written before keys.
    #[serde(default)]
    pub key: String,
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
    /// A coarser level between whole and pointer (a post's category and first line, a
    /// memory's gist), the view's choice when the whole does not fit (FLOW plan §2d).
    /// Absent in captures written before levels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// What the item is and how to reopen it (an id and the verb): its cost once collapsed.
    /// `None` when a collapsed item leaves her view entirely.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pointer: Option<String>,
}

impl RagItem {
    /// An item at its whole level, priced by the one estimator, with no coarser levels yet.
    pub fn whole(key: impl Into<String>, content: impl Into<String>, metadata: serde_json::Value) -> Self {
        let content = content.into();
        let tokens = continuum_base::token_estimate::estimate_prompt_tokens(&content);
        Self { key: key.into(), content, tokens, metadata, summary: None, pointer: None }
    }

    /// With a summary level.
    pub fn with_summary(mut self, summary: impl Into<String>) -> Self {
        self.summary = Some(summary.into());
        self
    }

    /// With a pointer level.
    pub fn with_pointer(mut self, pointer: impl Into<String>) -> Self {
        self.pointer = Some(pointer.into());
        self
    }
}

/// What a source returns when asked to deliver.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RagDelivery {
    pub source_id: String,
    /// Every item the source holds, each a complete unit at its levels. Never partial and
    /// never sized: the one allocation in her prompt view chooses each item's level.
    pub items: Vec<RagItem>,
    /// Actual tokens consumed across all items.
    pub tokens_used: u32,
    /// True → the source holds more than it delivered (a room's scrollback beyond its
    /// fetch); the rest reaches the reader through the source's `expand_command`. Never a
    /// budget cut: sources no longer take one.
    pub truncated: bool,
}

impl RagDelivery {
    /// Every item a source holds, `tokens_used` summed from them.
    pub fn of(source_id: &str, items: Vec<RagItem>, truncated: bool) -> Self {
        let tokens_used = items.iter().fold(0u32, |sum, item| sum.saturating_add(item.tokens));
        Self { source_id: source_id.to_owned(), items, tokens_used, truncated }
    }

    /// A source with nothing to say this turn.
    pub fn empty(source_id: &str) -> Self {
        Self::of(source_id, Vec::new(), false)
    }
}

/// Deserializes both wire forms of a delivered RAG pack. Current captures carry
/// `truncated`; pre-ruling-a JSONL (still on disk) carries legacy
/// `continuation` instead — `null` means the source delivered everything, an
/// object means it stopped short and more remained. Decoding recovers that
/// truth rather than dropping the line or defaulting it away: old captures must
/// replay with their source-cut notice intact (ruling a deletes the resumable
/// cursor, not the record). An explicit `truncated` always wins if both keys
/// are ever present.
impl<'de> Deserialize<'de> for RagDelivery {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Wire {
            source_id: String,
            items: Vec<RagItem>,
            tokens_used: u32,
            /// Current form; absent on pre-ruling-a lines.
            truncated: Option<bool>,
            /// Legacy form; never emitted today — read only for old JSONL.
            continuation: Option<serde_json::Value>,
        }

        let w = Wire::deserialize(deserializer)?;
        let truncated = match (w.truncated, w.continuation) {
            (Some(t), _) => t,
            // No explicit flag: a non-null legacy object is the source saying
            // "more remained"; null or absence means it did not.
            (None, Some(_)) => true,
            (None, None) => false,
        };
        Ok(Self {
            source_id: w.source_id,
            items: w.items,
            tokens_used: w.tokens_used,
            truncated,
        })
    }
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
    /// the same forcing function as persona's `room_board_source::RoomBoardReader::peer_names`.
    fn expand_command(&self) -> Option<&'static str>;

    /// Every item this source holds, each at its levels (whole, `summary`, `pointer`). A
    /// source never takes a size: the one allocation in her prompt view decides which level
    /// each item gets (FLOW-IMPLEMENTATION-PLAN §2d). Before this each source was handed its
    /// own budget (a quarter of the window per grounding faculty, a second flexbox in
    /// `compose_for_turn`) and cut or head-trimmed to it before the view ever saw it.
    ///
    /// `ctx` carries the per-call substrate context (persona scope, timing, room handle).
    /// Sources MUST validate that `ctx.persona_id == self.persona_id` if they're bound to a
    /// specific persona at construction.
    async fn deliver(&self, ctx: &RagContext) -> RagDelivery;
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
