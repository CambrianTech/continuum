//! Command lifecycle events emitted on the kernel `MessageBus`.
//!
//! Per [docs/planning/PERSONA-AS-DEVELOPER-GAP.md](../../../../../../../docs/planning/PERSONA-AS-DEVELOPER-GAP.md)
//! Priority 3: the substrate must emit completion events on the bus
//! so the autonomous persona loop can stay reactive. Polling violates
//! the RTOS-brain doctrine ("handlers read pre-staged results, never
//! block on recall/embedding/planning") — a persona that has to
//! `code/shell/watch` in a poll loop freezes its inbox cadence.
//!
//! # The event
//!
//! Every command dispatched through [`CommandExecutor::execute`]
//! emits ONE [`CommandCompletedEvent`] on the bus, regardless of
//! whether the command succeeded, errored, or routed through an
//! interceptor. The event's `success` field distinguishes — a single
//! topic + a boolean is simpler than two parallel topics and lets
//! subscribers filter by predicate.
//!
//! # Topic
//!
//! Published on `command:completed`. Follows the bus's
//! `<namespace>:<action>` convention (matching `data:<collection>:<action>`
//! and `chat:<verb>` patterns elsewhere). Subscribers register via
//! `bus.subscribe("command:completed", ...)` or via a glob like
//! `command:*` for forward-compat with future events
//! (e.g. `command:queued`, `command:dispatching`).
//!
//! # Compositional value (per the alignment-via-substrate-economics memory)
//!
//! Once every dispatch emits a structured completion event, attribution
//! becomes substrate-observable in real time. A persona on machine A
//! authoring a module + running `cargo/test` against it emits a
//! `command:completed` event that peers on B/C/etc. subscribed to the
//! room see — turning "I built this" into "the grid knows I built this"
//! without any new protocol.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Lifecycle event emitted on the kernel bus when a command completes
/// (successfully or with an error).
///
/// Wire shape is intentionally small and stable: command name,
/// outcome, duration, optional error message. Subscribers that want
/// richer detail can call the command themselves or read the
/// per-module log streams.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/runtime/CommandCompletedEvent.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct CommandCompletedEvent {
    /// The full command name as dispatched (e.g. `"chat/send"`,
    /// `"data/query-next"`, `"cargo/build"`). NOT the routed/local
    /// variant — what the caller asked for.
    pub command_name: String,

    /// Wall-clock time the dispatch took, in milliseconds. Includes
    /// interceptor chain traversal, local module handling, and any
    /// TS bridge IPC. Excludes time spent waiting for the bus
    /// publish to settle (the publish is fire-and-forget).
    #[ts(type = "number")]
    pub duration_ms: u64,

    /// `true` when the command's handler returned `Ok(_)`; `false`
    /// when it returned `Err(_)`. Note: this is COMMAND-level
    /// success, not result-level — a command that returns
    /// `CommandResponse::err(...)` (e.g. chat/send with airc-fail
    /// returning `Ok(result with warning)`) is `success: true` here
    /// because the dispatch itself succeeded.
    pub success: bool,

    /// The error message when `success == false`. Mirrors the
    /// `Err(String)` value that bubbled out of the dispatch chain.
    /// Absent on success.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<String>,

    /// The dispatch handle (UUID) when this command was fired as a TRACKED background
    /// dispatch (`CommandExecutor::dispatch_background`). Absent for ordinary synchronous
    /// commands, which stay thin. Lets a subscriber — e.g. a persona that sent a sentinel
    /// away — match this completion to the exact call it dispatched, and reuse the same
    /// handle in a follow-up command (cancel/query).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string")]
    pub handle: Option<uuid::Uuid>,

    /// The command's JSON result, included ONLY for tracked background dispatches — so the
    /// dispatcher gets the outcome from the event itself, no second call. Absent for
    /// synchronous commands (the caller already holds the return value).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "unknown")]
    pub result: Option<serde_json::Value>,
}

/// The canonical bus topic for command-completion events.
/// Centralized so subscribers, publishers, and tests reference one
/// truth.
pub const COMMAND_COMPLETED_TOPIC: &str = "command:completed";

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn event_round_trips_through_wire_with_camel_case() {
        let original = CommandCompletedEvent {
            command_name: "chat/send".to_string(),
            duration_ms: 42,
            success: true,
            error: None,
            handle: None,
            result: None,
        };
        let wire = serde_json::to_value(&original).expect("serialize");
        assert_eq!(wire["commandName"], "chat/send");
        assert_eq!(wire["durationMs"], 42);
        assert_eq!(wire["success"], true);
        assert!(
            !wire.as_object().unwrap().contains_key("error"),
            "error elided when None"
        );
        assert!(
            !wire.as_object().unwrap().contains_key("handle"),
            "handle elided when None — sync commands stay thin"
        );
        assert!(
            !wire.as_object().unwrap().contains_key("result"),
            "result elided when None"
        );

        let parsed: CommandCompletedEvent =
            serde_json::from_value(wire).expect("deserialize round-trip");
        assert_eq!(parsed, original);
    }

    #[test]
    fn event_with_error_includes_error_on_wire() {
        let original = CommandCompletedEvent {
            command_name: "data/query-next".to_string(),
            duration_ms: 7,
            success: false,
            error: Some("handle not found".to_string()),
            handle: None,
            result: None,
        };
        let wire = serde_json::to_value(&original).expect("serialize");
        assert_eq!(wire["success"], false);
        assert_eq!(wire["error"], "handle not found");
    }

    #[test]
    fn event_parses_from_wire_shape_subscribers_will_see() {
        // Subscribers receiving the event via the bus see this exact
        // JSON shape. Pin it by parsing from a hand-crafted JSON
        // object — locks the wire contract for downstream consumers.
        let wire = json!({
            "commandName": "cargo/build",
            "durationMs": 12345,
            "success": false,
            "error": "cargo timed out after 300000ms"
        });
        let parsed: CommandCompletedEvent = serde_json::from_value(wire).unwrap();
        assert_eq!(parsed.command_name, "cargo/build");
        assert_eq!(parsed.duration_ms, 12345);
        assert!(!parsed.success);
        assert_eq!(
            parsed.error.as_deref(),
            Some("cargo timed out after 300000ms")
        );
    }

    #[test]
    fn topic_constant_is_namespaced_action_format() {
        // Bus convention is `<namespace>:<action>`. Pinning the
        // constant keeps tests + publishers + subscribers in sync.
        assert_eq!(COMMAND_COMPLETED_TOPIC, "command:completed");
        assert!(COMMAND_COMPLETED_TOPIC.contains(':'));
    }
}
