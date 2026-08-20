//! Wire-format types for the `ToolExecutor` trait.
//!
//! Source-of-truth structs with `#[derive(TS)]` so TypeScript consumers
//! import from `protocol/typescript/cognition/` instead of re-declaring.
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
    export_to = "../../../protocol/typescript/cognition/ToolInvocation.ts"
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
    export_to = "../../../protocol/typescript/cognition/ToolExecutionContext.ts"
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
    export_to = "../../../protocol/typescript/cognition/PersonaMediaConfigLite.ts"
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
    export_to = "../../../protocol/typescript/cognition/ToolOutcome.ts"
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
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/MediaItemLite.ts"
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
    export_to = "../../../protocol/typescript/cognition/NativeBatchOutcome.ts"
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
    export_to = "../../../protocol/typescript/cognition/ParsedToolBatch.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct ParsedToolBatch {
    pub tool_calls: Vec<ToolInvocation>,
    pub cleaned_text: String,
    pub parse_time_us: u64,
}

// ─── Typed error surface for the ToolExecutor trait (continuum#1207) ──
//
// Before: every `ToolExecutor` method returned `Result<T, String>`. TS
// callers seeing an error from execute_native_batch / parse_response /
// store_outcome had to substring-match on `error: "some string"` to
// distinguish "tool not found" (user typo) from "execution failed"
// (legitimate runtime failure) from "forbidden" (auth/policy). That
// violates Joel's standing typed-error rule
// (feedback_two_ironclad_rules_tests_and_fallbacks.md): error variants
// must preserve the discriminant so callers can pattern-match.
//
// `ToolError` is the typed replacement. Same shape pattern as
// `AdmissionError` (#1129), `NoLocalModelLoadable` (#1089),
// `NoMultimodalBase` (#1074): a tagged enum with structured `detail`.
// ts-rs exports the type so TS callers can `switch (err.error)` on the
// discriminant and read the structured fields directly.
//
// Variant catalog (see issue #1207 + tool_executor/mod.rs trait doc):
// - `ToolNotFound` — caller named a tool the registry doesn't know.
//   Carries the requested name so retry/correction logic can suggest
//   alternatives.
// - `InvalidArgs` — tool exists, but the params didn't satisfy its
//   schema (missing required field, wrong type, out-of-range value).
//   Carries the tool name + an actionable reason.
// - `ExecutionFailed` — tool ran and threw / returned an error
//   (filesystem error, HTTP failure, etc.). Carries the tool name +
//   the underlying error string. This is the one variant where the
//   inner cause is a free-form string — the underlying systems
//   (shell, fetch, db) emit unstructured errors and we preserve them
//   verbatim rather than discarding information.
// - `Forbidden` — policy / auth check rejected the call (persona
//   doesn't have the capability, sandbox denial, rate-limit hit).
//   Carries tool name + reason so the persona can either skip or
//   request the capability.
// - `ParseFailed` — XML-fallback parsing of `parse_response` couldn't
//   extract any valid tool call from the model output. Carries a
//   bounded preview of the raw text + the parser's reason so the
//   persona's prompt can be tightened on retry.
// - `StoreFailed` — `store_outcome` couldn't persist the outcome to
//   working memory (DB error, disk full, foreign-key violation).
//   The cognition turn already succeeded by the time storage runs;
//   storage failure is observability, not user-facing failure, so
//   the variant exists to be LOGGED with structure, not to gate
//   behavior. Carries the tool name + the underlying error.
//
// All variants use `tag = "error"` for the discriminant key so TS
// can `if (err.error === 'ToolNotFound')` directly. `data` holds
// the structured fields. Same pattern as `AdmissionDecision`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/ToolError.ts"
)]
#[serde(tag = "error", content = "data")]
pub enum ToolError {
    /// Caller named a tool that isn't in the registry.
    ToolNotFound { name: String },
    /// Tool exists but the supplied params didn't satisfy its schema.
    InvalidArgs { tool: String, reason: String },
    /// Tool ran and produced a runtime failure. `underlying` is the
    /// raw error message from the tool's own system — not stringly-
    /// typed by choice, but by upstream constraint (shell exit
    /// status, HTTP body, DB driver string). The variant + tool
    /// name preserve enough structure for retry / correction logic.
    ExecutionFailed { tool: String, underlying: String },
    /// Policy / auth check rejected the call.
    Forbidden { tool: String, reason: String },
    /// `parse_response` couldn't extract a tool call from the model
    /// output. `raw_preview` is bounded (first ~200 chars) so the
    /// error can be logged without spamming the trace with the full
    /// model output.
    ParseFailed { raw_preview: String, reason: String },
    /// `store_outcome` failed to persist. Recorded for observability;
    /// caller should NOT propagate as a turn failure.
    StoreFailed { tool: String, underlying: String },
}

impl std::fmt::Display for ToolError {
    /// Human-readable rendering for log lines + std::error::Error
    /// compatibility. JSON wire format (used by IPC + ts-rs callers)
    /// always carries the structured form via serde — `Display` is
    /// only for log scrapes / panic messages where the discriminant
    /// is enough.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToolError::ToolNotFound { name } => {
                write!(f, "tool not found: '{name}'")
            }
            ToolError::InvalidArgs { tool, reason } => {
                write!(f, "invalid args for tool '{tool}': {reason}")
            }
            ToolError::ExecutionFailed { tool, underlying } => {
                write!(f, "tool '{tool}' execution failed: {underlying}")
            }
            ToolError::Forbidden { tool, reason } => {
                write!(f, "tool '{tool}' forbidden: {reason}")
            }
            ToolError::ParseFailed {
                raw_preview,
                reason,
            } => {
                write!(
                    f,
                    "tool parse failed ({reason}); raw preview: {raw_preview}"
                )
            }
            ToolError::StoreFailed { tool, underlying } => {
                write!(f, "tool '{tool}' store failed: {underlying}")
            }
        }
    }
}

impl std::error::Error for ToolError {}

#[cfg(test)]
mod tool_error_tests {
    use super::*;

    /// What this catches: ts-rs serde tagging stays `error` /
    /// `data`. If a future serde rename slips, TS callers'
    /// `switch (err.error)` discriminator silently breaks (every
    /// case becomes `default`). Round-trip + key inspection guards
    /// the wire contract.
    #[test]
    fn tool_error_serializes_with_typed_discriminant() {
        let err = ToolError::ToolNotFound {
            name: "code/nonexistent".to_string(),
        };
        let wire = serde_json::to_value(&err).expect("serialize");
        assert_eq!(wire["error"], "ToolNotFound");
        assert_eq!(wire["data"]["name"], "code/nonexistent");

        let back: ToolError = serde_json::from_value(wire).expect("round-trip");
        assert!(matches!(back, ToolError::ToolNotFound { name } if name == "code/nonexistent"));
    }

    /// What this catches: every variant carries the structured
    /// fields the trait promises. If a variant ever drops a field
    /// (e.g. `Forbidden { reason }` becomes `Forbidden { }`), the
    /// constructor call here fails to compile. Compile-time
    /// enforcement of the variant shape contract.
    #[test]
    fn every_variant_constructs_with_documented_fields() {
        let _ = ToolError::ToolNotFound { name: "x".into() };
        let _ = ToolError::InvalidArgs {
            tool: "x".into(),
            reason: "missing 'path'".into(),
        };
        let _ = ToolError::ExecutionFailed {
            tool: "x".into(),
            underlying: "ENOENT".into(),
        };
        let _ = ToolError::Forbidden {
            tool: "x".into(),
            reason: "no capability".into(),
        };
        let _ = ToolError::ParseFailed {
            raw_preview: "<<garbage>>".into(),
            reason: "no tool block".into(),
        };
        let _ = ToolError::StoreFailed {
            tool: "x".into(),
            underlying: "DB constraint".into(),
        };
    }

    /// What this catches: Display impl renders the discriminant +
    /// key context for every variant. Log scrapes / panic outputs
    /// stay grep-able by tool name + error class even when the
    /// JSON form isn't reachable.
    #[test]
    fn display_rendering_includes_variant_and_tool() {
        let cases = [
            (
                ToolError::ToolNotFound { name: "x".into() },
                "tool not found: 'x'",
            ),
            (
                ToolError::InvalidArgs {
                    tool: "y".into(),
                    reason: "missing field".into(),
                },
                "invalid args for tool 'y': missing field",
            ),
            (
                ToolError::ExecutionFailed {
                    tool: "z".into(),
                    underlying: "boom".into(),
                },
                "tool 'z' execution failed: boom",
            ),
        ];
        for (err, expected) in cases {
            assert_eq!(format!("{err}"), expected);
        }
    }
}
