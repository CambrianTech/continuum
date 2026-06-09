//! Command protocol — typed wire envelopes for substrate command dispatch
//! over airc. See module-level docs on `routing::airc_command_protocol`
//! in continuum-core for the full transport flow + envelope rationale.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

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

/// airc header naming the `RouteKind` of the dispatch.
pub const HEADER_COMMAND_KIND: &str = "continuum.command.kind";

/// airc header carrying the optional env constraint (e.g. `"vr"`).
pub const HEADER_COMMAND_ENV: &str = "continuum.command.env";

/// airc header on the reply side: `"ok"` or `"error"`.
pub const HEADER_COMMAND_STATUS: &str = "continuum.command.status";

/// airc header naming the consumer-namespaced body hint.
pub const HEADER_CONTINUUM_BODY_HINT: &str = "continuum.body_hint";

/// Body-hint value for request envelopes.
pub const COMMAND_REQUEST_BODY_HINT: &str = "continuum.command.request.v1";

/// Body-hint value for reply envelopes.
pub const COMMAND_RESPONSE_BODY_HINT: &str = "continuum.command.response.v1";

// ─── Typed envelopes ─────────────────────────────────────────────────

/// Wire-out envelope: the substrate's "dispatch this command on your
/// peer" shape. Serialized as JSON in the airc frame body.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AircCommandRequest {
    pub path: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub env: Option<String>,
    #[serde(default)]
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum AircCommandResponse {
    Ok { result: Value },
    Error { message: String },
}

impl AircCommandResponse {
    pub fn ok(result: Value) -> Self {
        Self::Ok { result }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self::Error {
            message: message.into(),
        }
    }

    /// Collapse to `Result<Value, String>`.
    pub fn into_result(self) -> Result<Value, String> {
        match self {
            Self::Ok { result } => Ok(result),
            Self::Error { message } => Err(message),
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
        assert_eq!(COMMAND_REQUEST_BODY_HINT, "continuum.command.request.v1");
        assert_eq!(COMMAND_RESPONSE_BODY_HINT, "continuum.command.response.v1");
    }
}
