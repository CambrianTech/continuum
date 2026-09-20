//! Shared Cognition types — Rust source-of-truth, ts-rs auto-emit.
//!
//! TypeScript callers import from `protocol/typescript/cognition/`. Nobody
//! hand-writes the TS shape — it's projected from these definitions.
//!
//! Per the noun/verb split: these types are VERB OUTPUTS (the data
//! produced by `analyze`, `orchestrate-responders`, etc.), not nouns
//! stored via ORM. Rust owns them; TS gets the generated projection.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use ts_rs::TS;
use uuid::Uuid;

// =============================================================================
// SharedAnalysis — output of cognition/analyze
// =============================================================================

/// What kind of message this is. Drives orchestration policy: a 'social'
/// greeting may not need 4 specialists weighing in; a 'task' often does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/SharedAnalysisIntent.ts"
)]
pub enum SharedAnalysisIntent {
    Question,
    Request,
    Statement,
    Task,
    Social,
    Other,
}

impl SharedAnalysisIntent {
    /// Parse from a model-output string. Unknown values collapse to
    /// `Other` rather than failing — model variation on intent
    /// classification shouldn't blow up the analysis.
    pub fn parse_lenient(raw: &str) -> Self {
        match raw.trim().to_lowercase().as_str() {
            "question" => Self::Question,
            "request" => Self::Request,
            "statement" => Self::Statement,
            "task" => Self::Task,
            "social" => Self::Social,
            _ => Self::Other,
        }
    }
}

/// The objective layer of cognition. ONE shared analysis per message,
/// computed once on the base model (no LoRA), used by every responding
/// persona as the foundation for their specialty render.
///
/// Cached by `cache_key` (content-addressable) so repeated analysis of
/// the same message + conversation state hits the cache.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/SharedAnalysis.ts"
)]
pub struct SharedAnalysis {
    // ─── Identity / cache key ─────────────────────────────────────────
    /// The chat message this analysis is FOR.
    #[ts(type = "string")]
    pub message_id: Uuid,
    #[ts(type = "string")]
    pub room_id: Uuid,
    /// Stable hash of (room + message + recent-history-fingerprint +
    /// known-specialties). Identical inputs → identical key → cache hit.
    pub cache_key: String,
    /// Unix epoch ms — when this analysis was generated.
    #[ts(type = "number")]
    pub generated_at_ms: u64,

    // ─── Objective reading ────────────────────────────────────────────
    /// Concise summary of what the message is saying / asking.
    pub summary: String,
    /// Concept tags the message touches — for downstream specialty matching.
    pub key_concepts: Vec<String>,
    /// What kind of message this is.
    pub intent: SharedAnalysisIntent,
    /// Optional one-word tone (frustrated, curious, urgent, etc.). Personas
    /// can color their voice with this; the architecture is agnostic.
    #[ts(optional)]
    pub emotional_tone: Option<String>,

    // ─── Orchestration hints (read by ResponseOrchestrator) ───────────
    /// For each known specialty, why this specialty's perspective would
    /// matter on this message. Empty value = "no signal here, stay silent
    /// by default." Keys are stable specialty identifiers (e.g.
    /// 'code', 'education', 'general'). Values are short prose enough
    /// to ground the persona's render prompt in a specific angle.
    pub suggested_angles: HashMap<String, String>,

    /// Compact distillation of the conversation context. Per-persona
    /// renders consume this without re-loading RAG.
    #[ts(optional)]
    pub relevant_context: Option<String>,

    // ─── Diagnostic / observability ───────────────────────────────────
    #[ts(type = "number")]
    pub duration_ms: u64,
    pub model_used: String,
    /// `true` if returned from cache; `false` if fresh inference.
    pub from_cache: bool,
}

// =============================================================================
// ResponderDecision — output of cognition/orchestrate-responders
// =============================================================================

/// Per-persona orchestration decision. The orchestrator produces one
/// of these for each persona in the room based on the SharedAnalysis +
/// persona specialty + (eventually) lever calls + recent contribution
/// history.
///
/// `should_respond=false` is a first-class outcome — silence-with-reason
/// is the architecture's preferred answer when the persona has nothing
/// additive. The reason is stored for trainability + the persona's own
/// meta-cognitive trace.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/ResponderDecision.ts"
)]
pub struct ResponderDecision {
    #[ts(type = "string")]
    pub persona_id: Uuid,
    pub should_respond: bool,

    /// 0.0..1.0. How relevant this persona's specialty is to the message
    /// + analysis. Above the orchestrator's threshold = respond; below
    /// = silent.
    pub relevance_score: f32,

    /// Which keys from `SharedAnalysis.suggested_angles` matched this
    /// persona's specialty. Becomes part of the render prompt so the
    /// contribution is grounded in a specific angle. Empty when
    /// `should_respond=false`.
    pub matched_angles: Vec<String>,

    /// Human-readable explanation of the decision. Always populated
    /// — for both selection and skip cases. Observable in logs +
    /// the coordination stream.
    pub explanation: String,

    /// Phase B: which persona leads the streaming chain-of-thought
    /// (others see the lead's render in flight and build on it).
    /// Phase A: the highest-relevance responder is is_lead=true; rest
    /// are parallel renders.
    #[ts(optional)]
    pub is_lead: Option<bool>,
}

// =============================================================================
// PersonaRenderRequest — input to PRG's shared-cognition render path
// =============================================================================

/// What `PRG.respondFromSharedAnalysis` consumes (over IPC). The render
/// uses `analysis` as the foundation — it doesn't rederive the
/// objective picture. Its job is to render this persona's specialty
/// perspective on what's already been objectively analyzed.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/PersonaRenderRequest.ts"
)]
pub struct PersonaRenderRequest {
    pub analysis: SharedAnalysis,
    pub decision: ResponderDecision,
    /// Phase B streaming: prior contributions in this turn the persona has
    /// seen. Lets non-lead personas build on the lead's reasoning rather
    /// than rederive it. Empty in Phase A.
    pub prior_contributions: Vec<PriorContribution>,
}

/// A contribution another persona has made this turn that the current
/// persona can see + build on. Phase B streaming primitive.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/PriorContribution.ts"
)]
pub struct PriorContribution {
    #[ts(type = "string")]
    pub persona_id: Uuid,
    pub text: String,
    /// `false` = streaming partial; `true` = posted/final.
    pub is_complete: bool,
    /// Unix epoch ms.
    #[ts(type = "number")]
    pub posted_at_ms: u64,
}

// =============================================================================
// LeverCall — A.5 (separate PR): cognition/* lever surface personas pull
// =============================================================================

/// The 9 levers personas can call to override default orchestration
/// policy. See SHARED-COGNITION.md "Levers personas pull" section for
/// semantics. Stable string identifier so command tooling + telemetry
/// can dispatch on a canonical enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/LeverName.ts"
)]
pub enum LeverName {
    RequestDeeperAnalysis,
    EscalateToOwnThinkPass,
    CedeFloorTo,
    ClaimLead,
    RequestThinkBudget,
    InviteSpecialist,
    SeekDisagreement,
    WithholdContribution,
    RequestCrossDomainAdapter,
}

/// A persona's lever invocation. Recorded in the chat coordination
/// stream as an observable event. Args are lever-specific (typed as
/// `serde_json::Value` here so the schema stays narrow; per-lever
/// helper structs in `lever_evaluator.rs` cast to the right shape).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/LeverCall.ts"
)]
pub struct LeverCall {
    #[ts(type = "string")]
    pub persona_id: Uuid,
    pub lever: LeverName,
    /// Lever-specific arguments. Per-lever shapes are documented in the
    /// architecture doc + enforced by helpers in `lever_evaluator.rs`.
    /// Wide here to keep the contract narrow.
    #[ts(type = "Record<string, unknown>")]
    pub args: serde_json::Value,
    /// Why the persona invoked the lever. Optional but strongly
    /// encouraged — the trace is what makes the lever surface trainable.
    #[ts(optional)]
    pub reason: Option<String>,
    /// Unix epoch ms.
    #[ts(type = "number")]
    pub timestamp_ms: u64,
}
