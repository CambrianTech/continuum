//! Tool Executor — the verb that turns a persona's tool_use decision into
//! executed outcomes (result content + stored working-memory + media).
//!
//! Phase 0.5.3 scope (per PR #949 reshape 893580f18): thin trait surface
//! here in Rust, TS-IPC impl in the first concrete type. The heavy
//! universal infrastructure — `AgentToolExecutor`'s loop detection,
//! parse/strip/correct, ToolRegistry interop, and the ~1000-line
//! constellation of tool implementations (code/*, interface/*,
//! collaboration/*, data/*) — all stay TS-side. Moving them would be a
//! separate phase when tool implementations themselves have reason to
//! port.
//!
//! What this module owns:
//! - Source-of-truth types (ts-rs exported to `shared/generated/cognition/`)
//! - The `ToolExecutor` trait that the cognition pipeline calls
//! - Concrete `TsIpcToolExecutor` impl that shells out to the existing
//!   TS `PersonaToolExecutor` via a command IPC round-trip (defined in
//!   a follow-up commit alongside the TS command handler)
//!
//! Why trait + IPC impl instead of rust-native port:
//! - Tool implementations live in TS today; Rust can't call them without
//!   RE-homing the registry + every tool impl
//! - Persona pipeline crossing IPC for each batch of tool calls is
//!   tolerable; the path is already async and batch-shaped
//! - When the time comes to port individual tools (or the whole thing)
//!   we add a second impl of the same trait and flip the factory — no
//!   caller-code changes
//!
//! What this module DOES NOT own:
//! - XML format construction (`formatToolResult` in TS) — specific to
//!   the XML-fallback codepath; format logic stays wherever the fallback
//!   path ultimately lives (may move when the fallback retires entirely)
//! - Tool registry lookup + execution — TS `ToolRegistry`
//! - Loop detection + correction — TS `AgentToolExecutor`

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use ts_rs::TS;
use uuid::Uuid;

use crate::ai::types::{ToolCall as NativeToolCall, ToolResult as NativeToolResult};

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

/// The trait callers (cognition pipeline) depend on. One impl today
/// (`TsIpcToolExecutor`, lands next commit). A future rust-native impl
/// slots in here with no caller-side changes — same method shapes.
///
/// All methods async because the TS-IPC impl is async; a rust-native
/// impl stays async-compatible trivially.
#[async_trait]
pub trait ToolExecutor: Send + Sync {
    /// Execute a batch of native tool calls. Called by the agent loop
    /// after the model emits `finish_reason = tool_use`. Each call's
    /// outcome correlates back by `NativeToolCall::id`.
    async fn execute_native_batch(
        &self,
        calls: &[NativeToolCall],
        context: &ToolExecutionContext,
        max_result_chars: usize,
    ) -> Result<NativeBatchOutcome, String>;

    /// Parse tool calls from a raw AI response string (XML-fallback path
    /// for models that don't emit native tool_use blocks). Returns
    /// extracted calls + cleaned-of-tool-blocks text + parse-time
    /// telemetry. Delegates straight to `AgentToolExecutor.parseResponse`
    /// on the TS side; Rust never does the parsing itself (the format
    /// adapter constellation lives in TS).
    async fn parse_response(
        &self,
        response_text: &str,
        model_family: Option<&str>,
    ) -> Result<ParsedToolBatch, String>;

    /// Store a tool result in working memory as a ChatMessageEntity.
    /// Returns the assigned id so the caller can reference the stored
    /// row for later recall/expansion. Fire-and-forget from the
    /// response path — caller doesn't await.
    async fn store_outcome(
        &self,
        outcome: &ToolOutcome,
        context: &ToolExecutionContext,
    ) -> Result<Uuid, String>;
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tool_invocation_round_trips_camel_case() {
        // What this catches: the `#[serde(rename_all = "camelCase")]`
        // attribute on ToolInvocation. TS consumers read `toolName` from
        // the JSON wire; snake_case "tool_name" would silently break the
        // persona→executor command shape (TS handler sees undefined, calls
        // the wrong tool or no tool at all). Round-tripping through a
        // pre-shaped camelCase object proves Rust emits and re-parses the
        // same keys TS generates via ts-rs.
        //
        // Validated 2026-04-21: mutation = change
        // `#[serde(rename_all = "camelCase")]` to `"snake_case"` →
        // deserialization of the camelCase fixture below fails with
        // "missing field `tool_name`"; test panics. Reverted.
        let mut params = HashMap::new();
        params.insert("path".to_string(), "/tmp/x".to_string());
        params.insert("mode".to_string(), "read".to_string());

        let original = ToolInvocation {
            tool_name: "code/read".to_string(),
            parameters: params.clone(),
        };

        let wire = serde_json::to_value(&original).expect("serialize");
        assert_eq!(wire["toolName"], "code/read");
        assert_eq!(wire["parameters"]["path"], "/tmp/x");

        let back: ToolInvocation =
            serde_json::from_value(wire).expect("deserialize camelCase wire");
        assert_eq!(back.tool_name, "code/read");
        assert_eq!(back.parameters, params);
    }

    #[test]
    fn tool_outcome_preserves_media_order_and_optionals() {
        // What this catches: (a) field-name contract on `content` — the
        // TS consumer reads `wire.content` directly; a serde rename (or
        // Some other well-meaning "use `result` for consistency" edit)
        // would silently break that. (b) Vec ordering of media — per-tool
        // attribution (caller treats "first image is the screenshot,
        // second is the diff") desyncs if serde ever reorders.
        //
        // Validated 2026-04-21: mutation = add
        // `#[serde(rename = "result")]` to the `content` field → the
        // assertion `wire["content"] == "{\"ok\":true}"` panics because
        // wire now carries `result` instead. Reverted.
        let outcome = ToolOutcome {
            tool_name: "interface/screenshot".to_string(),
            success: true,
            content: Some("{\"ok\":true}".to_string()),
            error: None,
            media: vec![
                MediaItemLite {
                    item_type: "image".to_string(),
                    base64: Some("aGVsbG8=".to_string()),
                    mime_type: Some("image/png".to_string()),
                },
                MediaItemLite {
                    item_type: "audio".to_string(),
                    base64: None,
                    mime_type: None,
                },
            ],
            stored_id: Uuid::nil(),
        };

        let wire = serde_json::to_value(&outcome).expect("serialize");
        assert_eq!(wire["media"][0]["itemType"], "image");
        assert_eq!(wire["media"][1]["itemType"], "audio");
        assert_eq!(wire["content"], "{\"ok\":true}");
        assert!(
            wire.get("error").is_none() || wire["error"].is_null(),
            "error field should be skipped when None, got: {}",
            wire
        );

        let back: ToolOutcome = serde_json::from_value(wire).expect("deserialize");
        assert_eq!(back.media[0].item_type, "image");
        assert_eq!(back.media[1].item_type, "audio");
        assert_eq!(back.content.as_deref(), Some("{\"ok\":true}"));
        assert!(back.error.is_none());
    }

    #[test]
    fn tool_execution_context_passes_nested_caller_context_through() {
        // What this catches: the `caller_context: Value` field must
        // preserve ARBITRARY JSON structure, not stringify it. The
        // TS-IPC impl forwards JTAGContext as an opaque blob; if Rust
        // serde ever tried to "helpfully" flatten or stringify it, the
        // TS handler would receive malformed context and tool calls
        // would execute under the wrong session/auth.
        //
        // Validated 2026-04-21: mutation = change
        // `caller_context: Value` to `caller_context: String` → the
        // test's struct literal `caller_context: nested.clone()` fails
        // to compile with E0308 "mismatched types: expected String,
        // found Value". The contract is enforced statically; the
        // nested-JSON assertion below is the runtime check for future
        // serde-layer mutations (e.g. adding a `#[serde(with = ...)]`
        // that re-stringifies). Reverted.
        let nested = json!({
            "user": { "id": "u-42", "role": "persona" },
            "trace": ["a", "b", "c"],
            "flags": { "debug": true, "count": 7 }
        });

        let ctx = ToolExecutionContext {
            persona_id: Uuid::nil(),
            persona_name: "Helper".to_string(),
            session_id: Uuid::nil(),
            context_id: Uuid::nil(),
            caller_context: nested.clone(),
            persona_config: PersonaMediaConfigLite {
                auto_load_media: true,
                supported_media_types: vec!["image".to_string(), "audio".to_string()],
            },
        };

        let wire = serde_json::to_value(&ctx).expect("serialize");
        assert_eq!(wire["callerContext"]["user"]["id"], "u-42");
        assert_eq!(wire["callerContext"]["trace"][1], "b");
        assert_eq!(wire["callerContext"]["flags"]["count"], 7);

        let back: ToolExecutionContext = serde_json::from_value(wire).expect("deserialize");
        assert_eq!(back.caller_context, nested);
        assert_eq!(back.persona_name, "Helper");
        assert!(back.persona_config.auto_load_media);
    }
}
