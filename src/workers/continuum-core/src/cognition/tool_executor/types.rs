//! Wire-format types for the `ToolExecutor` trait.
//!
//! Source-of-truth structs with `#[derive(TS)]` so TypeScript consumers
//! import from `shared/generated/cognition/` instead of re-declaring.
//! Split out of `mod.rs` to keep the data layer independent of the
//! trait's behavior surface — matches the `metal_monitor::mach_ffi`
//! split (`da61eb68f`) where the wire-level types earn their own file
//! so future impls in a sibling module don't drag trait semantics
//! through a types edit and vice versa.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use ts_rs::TS;
use uuid::Uuid;

use crate::ai::types::ToolResult as NativeToolResult;

/// A tool invocation in the executor-internal shape: name + parameters
/// (not the native `{id, name, input}` shape used for the provider API
/// exchange). Distinct type because:
/// - `parameters` is `Record<string, string>` in the TS executor
///   (values pre-stringified for XML/registry), not `Value`
/// - `id` is absent — it's a native-exchange concern, irrelevant once
///   the call reaches the executor
///
/// Kept as a single source of truth for the executor boundary; TS
/// consumers import the generated type instead of re-declaring.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../shared/generated/cognition/ToolInvocation.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct ToolInvocation {
    pub tool_name: String,
    #[ts(type = "Record<string, string>")]
    pub parameters: HashMap<String, String>,
}

/// Context handed to every tool execution — identifies the persona, the
/// session, the chat room (contextId), and the persona's media-handling
/// preferences. Mirrors the TS `ToolExecutionContext` shape.
///
/// `caller_context` is intentionally opaque here — its concrete type
/// (`JTAGContext`) is a TS concern; Rust treats it as pass-through
/// JSON that the TS-IPC impl forwards along with the call.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../shared/generated/cognition/ToolExecutionContext.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct ToolExecutionContext {
    #[ts(type = "string")]
    pub persona_id: Uuid,
    pub persona_name: String,
    #[ts(type = "string")]
    pub session_id: Uuid,
    #[ts(type = "string")]
    pub context_id: Uuid,
    /// Opaque JTAGContext passed through to the TS-IPC layer. Rust
    /// never interprets this — the TS executor owns its schema.
    #[ts(type = "Record<string, unknown>")]
    pub caller_context: Value,
    pub persona_config: PersonaMediaConfigLite,
}

/// Subset of the TS `PersonaMediaConfig` the executor actually reads:
/// auto-load flag + supported-type filter. Full config has more knobs
/// but those are consumed upstream (at RAG / prompt-assembly time), not
/// at tool-execution time.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../shared/generated/cognition/PersonaMediaConfigLite.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct PersonaMediaConfigLite {
    pub auto_load_media: bool,
    pub supported_media_types: Vec<String>,
}

/// Outcome of a single tool call — success/failure + content + any
/// collected media items. `media` lands here (rather than only in the
/// per-batch aggregate) so callers that care about per-tool attribution
/// can walk the outcomes without re-correlating.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../shared/generated/cognition/ToolOutcome.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct ToolOutcome {
    pub tool_name: String,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<String>,
    /// Media items collected from this tool's result (post-filter per
    /// `persona_config`). Always present; empty vec when no media.
    pub media: Vec<MediaItemLite>,
    /// ChatMessageEntity id where the tool result was stored in working
    /// memory. Caller tracks this for later recall / expand-on-demand.
    #[ts(type = "string")]
    pub stored_id: Uuid,
}

/// Minimal `MediaItem` shape the executor needs to pass around. Full
/// type lives in TS `ChatMessageEntity`; Rust doesn't need every field,
/// just enough to route the item through the pipeline.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../shared/generated/cognition/MediaItemLite.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct MediaItemLite {
    /// "image" | "audio" | "video" etc. — echoing the TS union; not
    /// enumified here because the executor doesn't dispatch on it, it
    /// passes through.
    pub item_type: String,
    /// Base64 payload when inline. Absent when referenced by URL/ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub base64: Option<String>,
    /// MIME type hint for downstream sensory-bridge routing.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub mime_type: Option<String>,
    /// Pre-computed text description of this media item, populated by
    /// the TS-side `VisionDescriptionService` before the message
    /// crosses IPC into Rust. The persona response path uses this to
    /// give text-only personas a real description of attached media —
    /// without it they get a "[no description available]" marker
    /// instead of silently hallucinating from prompt context.
    ///
    /// NOTE: deliberately does NOT include filename/path. The 2026-04-21
    /// methodology rule (Joel): "never give AIs an image whose name
    /// indicates what it is" — filenames are a cheat surface for
    /// non-vision models to fake answers, so they're stripped at this
    /// IPC boundary on principle, not just incidentally.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub description: Option<String>,
}

/// Result of executing a batch of native tool calls. Shape matches the
/// TS `executeNativeToolCalls` return: per-tool `NativeToolResult` for
/// feeding back into the provider API, aggregated media, and the set
/// of working-memory ids so the caller can emit follow-up events.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../shared/generated/cognition/NativeBatchOutcome.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct NativeBatchOutcome {
    pub results: Vec<NativeToolResult>,
    pub media: Vec<MediaItemLite>,
    #[ts(type = "Array<string>")]
    pub stored_ids: Vec<Uuid>,
}

/// Output of `parse_response` — tool calls extracted, clean text the
/// model emitted outside tool blocks, and parse cost for telemetry.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../shared/generated/cognition/ParsedToolBatch.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct ParsedToolBatch {
    pub tool_calls: Vec<ToolInvocation>,
    pub cleaned_text: String,
    pub parse_time_us: u64,
}
