//! Tool Executor — the verb that turns a persona's tool_use decision into
//! executed outcomes (result content + stored working-memory + media).
//!
//! Phase 0.5.3 scope (per PR #949 reshape 893580f18): thin trait surface
//! here in Rust, concrete impl deferred until 0.5.6 brings a real Rust
//! caller. The heavy universal infrastructure — `AgentToolExecutor`'s
//! loop detection, parse/strip/correct, ToolRegistry interop, and the
//! ~1000-line constellation of tool implementations (code/*, interface/*,
//! collaboration/*, data/*) — all stay TS-side. Moving them would be a
//! separate phase when tool implementations themselves have reason to
//! port.
//!
//! Layout (split for modularization — see `da61eb68f`
//! `metal_monitor::mach_ffi` pattern):
//! - `types.rs` — wire-format structs (`#[derive(TS)]` for each). Data
//!   layer kept independent of trait behavior so future impl edits don't
//!   churn type definitions and vice versa.
//! - `mod.rs` (this file) — the `ToolExecutor` trait + round-trip tests
//!   that validate the wire contract.
//! - `default_impl.rs` — future concrete impl slot, deferred until
//!   0.5.6's Rust caller materializes.
//!
//! Why trait + deferred impl:
//! - Tool implementations live in TS today; Rust can't call them without
//!   RE-homing the registry + every tool impl
//! - Persona pipeline crossing IPC for each batch of tool calls is
//!   tolerable; the path is already async and batch-shaped
//! - When the time comes to port, add the impl module in the pattern
//!   already laid here — no caller-code changes

pub mod command_executor;
pub mod spill;
pub mod types;

/// Realistic 50-persona load/profiling harness (real CodeModule, real payloads).
/// Gated `stress-tests` per the test doctrine; compiled only for profiling runs.
#[cfg(all(test, feature = "stress-tests"))]
mod load_harness;

pub use command_executor::CommandToolExecutor;
pub use types::{
    MediaItemLite, NativeBatchOutcome, ParsedToolBatch, PersonaMediaConfigLite, ToolError,
    ToolExecutionContext, ToolInvocation, ToolOutcome,
};

use async_trait::async_trait;

use crate::ai::types::ToolCall as NativeToolCall;

/// The trait callers (cognition pipeline) depend on. One impl today
/// (`TsIpcToolExecutor`, lands next commit). A future rust-native impl
/// slots in here with no caller-side changes — same method shapes.
///
/// All methods async because the TS-IPC impl is async; a rust-native
/// impl stays async-compatible trivially.
///
/// **Errors are typed** (`ToolError`, see `types.rs`) rather than
/// `String`. Rationale + variant catalog live with the type, not
/// here. Callers can pattern-match on the discriminant for retry /
/// correction / forbidden-handling logic; ts-rs exports the type so
/// TS callers get the same discriminator at the IPC boundary.
/// (continuum#1207)
#[async_trait]
pub trait ToolExecutor: Send + Sync {
    /// Execute a batch of native tool calls. Called by the agent loop
    /// after the model emits `finish_reason = tool_use`. Each call's
    /// outcome correlates back by `NativeToolCall::id`.
    ///
    /// Per-call failure modes (one bad call shouldn't fail the batch)
    /// land inside `NativeBatchOutcome`. `Err(ToolError)` is reserved
    /// for batch-level failures (e.g. the executor itself is
    /// unavailable / IPC channel down).
    async fn execute_native_batch(
        &self,
        calls: &[NativeToolCall],
        context: &ToolExecutionContext,
        max_result_chars: usize,
    ) -> Result<NativeBatchOutcome, ToolError>;

    /// The core `CommandExecutor` behind these hands, if any. A live persona's executor
    /// returns `Some` — enabling fire-and-poll `dispatch_background` for long-running
    /// commands and `message_bus()` for the async-dispatch listener that folds their
    /// results back into working memory ([[persona-async-dispatch-channel]]). Harnesses and
    /// mocks return the default `None` and simply run every command synchronously.
    fn command_executor(
        &self,
    ) -> Option<std::sync::Arc<crate::runtime::command_executor::CommandExecutor>> {
        None
    }

    /// Parse tool calls from a raw AI response string (XML-fallback path
    /// for models that don't emit native tool_use blocks). Returns
    /// extracted calls + cleaned-of-tool-blocks text + parse-time
    /// telemetry. Delegates straight to `AgentToolExecutor.parseResponse`
    /// on the TS side; Rust never does the parsing itself (the format
    /// adapter constellation lives in TS).
    ///
    /// Returns `Err(ToolError::ParseFailed { raw_preview, reason })`
    /// when the response contained no parseable tool block — distinct
    /// from `Ok` with empty tool_calls (which means "model emitted
    /// text, no tools requested" — a normal silence outcome).
    async fn parse_response(
        &self,
        response_text: &str,
        model_family: Option<&str>,
    ) -> Result<ParsedToolBatch, ToolError>;

    /// Store a tool result in working memory as a ChatMessageEntity.
    /// Returns the assigned id so the caller can reference the stored
    /// row for later recall/expansion. Fire-and-forget from the
    /// response path — caller doesn't await.
    ///
    /// `Err(ToolError::StoreFailed { tool, underlying })` is for
    /// observability — the cognition turn already produced its
    /// outcome by the time storage runs; storage failure should be
    /// LOGGED with structure, not propagated as a turn failure.
    async fn store_outcome(
        &self,
        outcome: &ToolOutcome,
        context: &ToolExecutionContext,
    ) -> Result<uuid::Uuid, ToolError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashMap;
    use uuid::Uuid;

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
                    description: None,
                },
                MediaItemLite {
                    item_type: "audio".to_string(),
                    base64: None,
                    mime_type: None,
                    description: None,
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
