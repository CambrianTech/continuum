//! Command protocol — typed wire envelopes for substrate command dispatch
//! over airc. See module-level docs on `routing::airc_command_protocol`
//! in continuum-core for the full transport flow + envelope rationale.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

// ─── Wire-stable route kind constants ────────────────────────────────
//
// The wire-shape of a `RouteDecision` boils down to a kind string. The
// substrate's typed `RouteKind` projects to one of these literals; the
// client side hardcodes the same string. Hoisting here prevents drift
// on rename — both ends import from the protocol crate.

/// Substrate-local dispatch. Should never reach the wire (substrate
/// dispatchers route Local inline), but defined for completeness.
pub const KIND_LOCAL: &str = "local";

/// A specific peer dispatch (`airc://<peer>/...`).
pub const KIND_PEER: &str = "peer";

/// A room broadcast (`airc://room:<id>/...`).
pub const KIND_ROOM: &str = "room";

/// An env-wildcard broadcast (`airc://<peer>:*/...`).
pub const KIND_BROADCAST: &str = "broadcast";

// ─── Default round-trip deadline ─────────────────────────────────────

/// Default deadline both the substrate's cross-grid `AircTransport`
/// and the client's `AircIpcTransport` apply when the caller didn't
/// specify one. Lives here so client and server agree on the
/// budget; if either bumps it, the other must agree (or override).
pub const DEFAULT_COMMAND_DEADLINE: Duration = Duration::from_secs(30);

// ─── Header constants ────────────────────────────────────────────────

/// airc header naming the URI path being dispatched.
pub const HEADER_COMMAND_PATH: &str = "continuum.command.path";

/// The command path of a generation request. A seat serves each one concurrently: it
/// holds a serving lane for minutes, and the pump must keep reading meanwhile (card
/// b3e5b582). One owner for the literal on the wire; other hand-written sites migrate.
pub const COMMAND_PATH_AI_GENERATE: &str = "ai/generate";

/// The control command a requester sends when it abandons a generation it asked a seat
/// for (its deadline passed, or its turn was dropped): the seat aborts that generation
/// instead of finishing it for no one (card 83f774fb). Sent only to a seat that
/// advertises the capability; the seat honors it only from the original requester.
pub const COMMAND_PATH_AI_GENERATE_CANCEL: &str = "ai/generate/cancel";

/// Params of [`COMMAND_PATH_AI_GENERATE_CANCEL`]: the requester's generation id
/// ([`HEADER_GENERATE_ID`]) of the abandoned `ai/generate` request, and that request's own
/// validity ([`HEADER_GENERATE_VALID_UNTIL_MS`]): the seat remembers a cancel that overtook
/// its request exactly as long as the request could still be admitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenerateCancel {
    pub generation: uuid::Uuid,
    pub valid_until_ms: u64,
}

/// airc header on an `ai/generate` request: the REQUESTER's id for this generation, chosen
/// before the request is published. The seat keys the served generation by it, so a cancel
/// can name it even when the requester abandons the request before airc hands back its own
/// correlation id (card 83f774fb: airc publishes before `request_in` returns).
pub const HEADER_GENERATE_ID: &str = "continuum.generate.id";

/// airc header on an `ai/generate` request, sent with [`HEADER_GENERATE_ID`]: the epoch ms
/// past which the requester no longer waits for it (publication time plus its deadline).
/// The seat refuses a request that arrives after it, so a delayed frame can never start
/// work nobody waits for, and an early cancel need be remembered only until it.
pub const HEADER_GENERATE_VALID_UNTIL_MS: &str = "continuum.generate.valid_until_ms";

/// airc header naming the `RouteKind` of the dispatch.
pub const HEADER_COMMAND_KIND: &str = "continuum.command.kind";

/// airc header carrying the optional env constraint (e.g. `"vr"`).
pub const HEADER_COMMAND_ENV: &str = "continuum.command.env";

/// airc header on the reply side: `"ok"` or `"error"`.
pub const HEADER_COMMAND_STATUS: &str = "continuum.command.status";

/// airc header naming the consumer-namespaced body hint (the event protocol's frames).
/// Commands carry no hint: a command request and its result are airc's own typed
/// frame kinds (`Command` / `CommandResult`) on the addressed command bus.
pub const HEADER_CONTINUUM_BODY_HINT: &str = "continuum.body_hint";

// ─── Typed envelopes ─────────────────────────────────────────────────

/// Wire-out envelope: the substrate's "dispatch this command on your
/// peer" shape. Serialized as JSON in the airc frame body.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export, export_to = "../../../protocol/typescript/transport/AircCommandRequest.ts")]
pub struct AircCommandRequest {
    pub path: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    pub env: Option<String>,
    #[serde(default)]
    #[ts(type = "unknown")]
    pub params: Value,
}

impl AircCommandRequest {
    /// Construct a request envelope. Direct constructor for client-side
    /// callers (e.g. continuum-client's `AircIpcTransport`); substrate-
    /// side callers typically go through `command_request_from_route_decision`
    /// in continuum-core which packages a typed `RouteDecision`.
    pub fn new(path: String, kind: String, env: Option<String>, params: Value) -> Self {
        Self {
            path,
            kind,
            env,
            params,
        }
    }
}

/// Wire-back envelope: typed success/error.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export, export_to = "../../../protocol/typescript/transport/AircCommandResponse.ts")]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum AircCommandResponse {
    Ok {
        #[ts(type = "unknown")]
        result: Value,
    },
    Error {
        message: String,
        // Set when a seat refused an `ai/generate` at admission, so the requester acts
        // on the reason instead of parsing `message` (card 03660a77). Absent on every
        // other error, and from a seat that predates it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        refusal: Option<GenerateRefusal>,
    },
}

/// Why a seat refused an `ai/generate` before running it. The seat's admission owner
/// (continuum-core `served_generations::Refusal`) projects onto this; the requester maps
/// it to a typed error, so a full seat sends her mind home instead of being asked again.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/transport/GenerateRefusal.ts")]
#[serde(rename_all = "snake_case")]
pub enum GenerateRefusal {
    /// Every served lane already holds a generation.
    Overloaded,
    /// A generation with this id is already outstanding.
    Duplicate,
    /// The requester cancelled it before it arrived.
    CancelledByRequester,
    /// The requester overflowed the seat's early-cancel memory and is fenced.
    Fenced,
    /// It arrived after its own stated validity.
    Expired,
}

impl AircCommandResponse {
    pub fn ok(result: Value) -> Self {
        Self::Ok { result }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self::Error {
            message: message.into(),
            refusal: None,
        }
    }

    /// A seat's admission refusal of an `ai/generate`, with its typed reason.
    pub fn refused(message: impl Into<String>, refusal: GenerateRefusal) -> Self {
        Self::Error {
            message: message.into(),
            refusal: Some(refusal),
        }
    }

    /// The typed admission refusal this reply carries, if any.
    pub fn refusal(&self) -> Option<GenerateRefusal> {
        match self {
            Self::Error { refusal, .. } => *refusal,
            Self::Ok { .. } => None,
        }
    }

    /// Collapse to `Result<Value, String>`.
    pub fn into_result(self) -> Result<Value, String> {
        match self {
            Self::Ok { result } => Ok(result),
            Self::Error { message, .. } => Err(message),
        }
    }

    /// Header value for [`HEADER_COMMAND_STATUS`].
    pub fn status_header_value(&self) -> &'static str {
        match self {
            Self::Ok { .. } => "ok",
            Self::Error { .. } => "error",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_round_trips_json() {
        let req = AircCommandRequest::new(
            "inference/llm/generate".into(),
            "peer".into(),
            Some("vr".into()),
            serde_json::json!({"model": "qwen30b", "tokens": 256}),
        );
        let json = serde_json::to_string(&req).expect("serialize");
        let back: AircCommandRequest = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, req);
    }

    #[test]
    fn request_omits_env_when_none() {
        let req = AircCommandRequest::new(
            "data/list".into(),
            "peer".into(),
            None,
            serde_json::json!({"collection": "users"}),
        );
        let json = serde_json::to_string(&req).expect("serialize");
        assert!(
            !json.contains("\"env\""),
            "None env should be skipped on the wire, got: {json}"
        );
        let back: AircCommandRequest = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.env, None);
    }

    #[test]
    fn response_round_trips_ok() {
        let resp = AircCommandResponse::ok(serde_json::json!({"text": "hello"}));
        let json = serde_json::to_string(&resp).expect("serialize");
        let back: AircCommandResponse = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, resp);
        assert_eq!(resp.status_header_value(), "ok");
    }

    #[test]
    fn response_round_trips_error() {
        let resp = AircCommandResponse::error("policy denied: unknown peer");
        let json = serde_json::to_string(&resp).expect("serialize");
        let back: AircCommandResponse = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, resp);
        assert_eq!(resp.status_header_value(), "error");
    }

    #[test]
    fn response_into_result_ok_returns_value() {
        let resp = AircCommandResponse::ok(serde_json::json!(42));
        assert_eq!(resp.into_result(), Ok(serde_json::json!(42)));
    }

    #[test]
    fn response_into_result_error_returns_string() {
        let resp = AircCommandResponse::error("nope");
        assert_eq!(resp.into_result(), Err("nope".to_string()));
    }

    /// Header names are wire-stable strings. Renaming any here breaks
    /// peer-side filtering middleware on the other end of the socket.
    #[test]
    fn header_names_are_stable_strings() {
        assert_eq!(HEADER_COMMAND_PATH, "continuum.command.path");
        assert_eq!(HEADER_COMMAND_KIND, "continuum.command.kind");
        assert_eq!(HEADER_COMMAND_ENV, "continuum.command.env");
        assert_eq!(HEADER_COMMAND_STATUS, "continuum.command.status");
        assert_eq!(HEADER_CONTINUUM_BODY_HINT, "continuum.body_hint");
    }
}
