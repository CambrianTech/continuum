//! The substrate's wire shape for general command dispatch over airc.
//!
//! This module defines the typed envelopes a future
//! `AircTransport` (next Slice P commit) packages and the peer-side
//! handler (commit after) consumes. The transport flow:
//!
//! 1. Caller dispatches `airc://maya/inference/llm/generate`.
//! 2. `route()` produces `RouteDecision::Peer { peer: maya, path:
//!    "inference/llm/generate", ... }`.
//! 3. AircTransport packages this as
//!    [`AircCommandRequest`] (typed) + JSON body, attaches the
//!    [protocol headers](#header-constants), and calls
//!    `Airc::request(MentionTarget::Peer(maya), headers, body,
//!    deadline)`.
//! 4. The peer-side handler subscribes to incoming command
//!    requests, parses each envelope, dispatches via its local
//!    CommandExecutor, and replies with [`AircCommandResponse`].
//! 5. AircTransport's `await_reply()` decodes the response into
//!    `Result<CommandResult, String>` and returns to the caller.
//!
//! ## Why a separate protocol module
//!
//! `inference/airc_remote/protocol.rs` already exists for
//! inference-specific envelopes — that's a coincidence of history,
//! not a general primitive. This module is the **substrate-wide**
//! protocol for arbitrary command paths. The inference-specific
//! types stay because (a) they predate this, (b) they carry
//! inference-shaped fields (model, tokens) that don't generalize.
//! When the substrate stabilizes, inference dispatch composes
//! against this protocol by setting `path = "ai/inference/...".
//!
//! ## Header constants
//!
//! airc-lib already stamps `airc.correlation_id`, `airc.reply_to`,
//! `airc.deadline` on every `Airc::request`. The substrate adds:
//!
//! - [`HEADER_COMMAND_PATH`] — the dispatched URI's bare path
//!   (e.g. `"inference/llm/generate"`). Peer-side handler uses
//!   this to route to its local CommandExecutor.
//! - [`HEADER_COMMAND_KIND`] — the [`RouteKind`] string from the
//!   typed routing decision (`"peer" | "room" | "broadcast"`).
//!   Lets the peer-side handler audit how it was called.
//! - [`HEADER_COMMAND_ENV`] — optional env constraint (when the
//!   caller specified `:vr` etc.).
//! - [`HEADER_COMMAND_STATUS`] — on the reply side, `"ok"` /
//!   `"error"`. Lets middleware filter without parsing the body.
//!
//! ## Body shape
//!
//! The body is JSON: [`AircCommandRequest`] on the wire-out side,
//! [`AircCommandResponse`] on the wire-back side. JSON is the
//! substrate's default wire format already; using it here keeps the
//! envelope inspectable and aligns with the TS bridge / external
//! tool interop.
//!
//! ## Body hints
//!
//! airc-lib's adapter registry routes events by a `body_hint`
//! header. The substrate's command envelopes set:
//!
//! - [`COMMAND_REQUEST_BODY_HINT`] = `"continuum.command.request.v1"`
//! - [`COMMAND_RESPONSE_BODY_HINT`] = `"continuum.command.response.v1"`
//!
//! `v1` because the envelope shape will evolve and the hint pins
//! the version. The peer-side handler subscribes only to events
//! matching the request hint.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{RouteDecision, RouteKind};

// ─── Header constants ────────────────────────────────────────────────

/// airc header naming the URI path being dispatched. Peer-side
/// handler reads this to know which command to invoke locally.
pub const HEADER_COMMAND_PATH: &str = "continuum.command.path";

/// airc header naming the `RouteKind` of the dispatch
/// (`"peer" | "room" | "broadcast"`). Audit / observability surface.
pub const HEADER_COMMAND_KIND: &str = "continuum.command.kind";

/// airc header carrying the optional env constraint (e.g.
/// `"vr" | "desktop" | "tty"`). Absent when the URI didn't include
/// an env filter.
pub const HEADER_COMMAND_ENV: &str = "continuum.command.env";

/// airc header on the reply side: `"ok"` if the dispatched command
/// succeeded, `"error"` if it failed. Lets header-routing middleware
/// filter without parsing the body.
pub const HEADER_COMMAND_STATUS: &str = "continuum.command.status";

/// airc adapter `body_hint` for request envelopes. The peer-side
/// handler's adapter subscribes against this.
pub const COMMAND_REQUEST_BODY_HINT: &str = "continuum.command.request.v1";

/// airc adapter `body_hint` for reply envelopes. The caller-side
/// AircTransport filters reply streams against this.
pub const COMMAND_RESPONSE_BODY_HINT: &str = "continuum.command.response.v1";

// ─── Typed envelopes ─────────────────────────────────────────────────

/// The wire-out envelope: the substrate's universal "dispatch this
/// command on your peer" shape. Serialized as JSON in the airc
/// frame body.
///
/// `kind` is the [`RouteKind`] string (`"peer" | "room" |
/// "broadcast"`). Peer-side handler can refuse `"room"` /
/// `"broadcast"` if it doesn't implement them yet without parsing
/// the rest of the envelope.
///
/// `env` is `Some(...)` when the caller specified `airc://...:env/...`
/// and `None` otherwise. Peer-side handler uses this to pick which
/// embodiment serves the call.
///
/// `params` is the same JSON the local dispatch would have used.
/// The transport doesn't transform params — it just packages them
/// across the wire.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AircCommandRequest {
    pub path: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub env: Option<String>,
    #[serde(default)]
    pub params: Value,
}

/// The wire-back envelope: typed success/error.
///
/// `Ok { result }` carries the JSON the peer's CommandExecutor
/// produced (the same shape the caller would have gotten from a
/// local dispatch).
///
/// `Error { message }` carries the peer's error string. The
/// caller's AircTransport propagates this as `Err(message)` so
/// the local caller can't tell the difference between a local
/// failure and a remote one (the URI is the only locus of identity;
/// errors stay shape-uniform).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum AircCommandResponse {
    Ok { result: Value },
    Error { message: String },
}

impl AircCommandRequest {
    /// Package a non-Local [`RouteDecision`] + params into the wire
    /// envelope. Returns `None` for `RouteDecision::Local` — local
    /// decisions don't go over the wire and shouldn't reach this
    /// constructor.
    ///
    /// Used by AircTransport (next commit) to build the request body
    /// from what `route()` produced.
    pub fn from_route_decision(decision: &RouteDecision, params: Value) -> Option<Self> {
        match decision {
            RouteDecision::Local { .. } => None,
            RouteDecision::Peer { path, env, .. } => Some(Self {
                path: path.clone(),
                kind: RouteKind::Peer.as_str().to_string(),
                env: env.as_ref().map(|e| e.to_string()),
                params,
            }),
            RouteDecision::Room { path, env, .. } => Some(Self {
                path: path.clone(),
                kind: RouteKind::Room.as_str().to_string(),
                env: env.as_ref().map(|e| e.to_string()),
                params,
            }),
            RouteDecision::Broadcast { path, .. } => Some(Self {
                path: path.clone(),
                kind: RouteKind::Broadcast.as_str().to_string(),
                env: None,
                params,
            }),
        }
    }
}

impl AircCommandResponse {
    /// Construct an `Ok` response from a JSON result.
    pub fn ok(result: Value) -> Self {
        Self::Ok { result }
    }

    /// Construct an `Error` response from a message.
    pub fn error(message: impl Into<String>) -> Self {
        Self::Error {
            message: message.into(),
        }
    }

    /// Collapse to the canonical `Result<Value, String>` shape the
    /// substrate's CommandExecutor uses. Caller-side AircTransport
    /// calls this to bridge from wire envelope to local result.
    pub fn into_result(self) -> Result<Value, String> {
        match self {
            Self::Ok { result } => Ok(result),
            Self::Error { message } => Err(message),
        }
    }

    /// Header value to attach to the reply. `"ok"` or `"error"` per
    /// the variant. Lets routing middleware filter on success
    /// without parsing the body.
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
    use crate::routing::{route, CommandUri};
    use uuid::Uuid;

    #[test]
    fn request_round_trips_json() {
        let req = AircCommandRequest {
            path: "inference/llm/generate".into(),
            kind: "peer".into(),
            env: Some("vr".into()),
            params: serde_json::json!({"model": "qwen30b", "tokens": 256}),
        };
        let json = serde_json::to_string(&req).expect("serialize");
        let back: AircCommandRequest = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, req);
    }

    #[test]
    fn request_omits_env_when_none() {
        let req = AircCommandRequest {
            path: "data/list".into(),
            kind: "peer".into(),
            env: None,
            params: serde_json::json!({"collection": "users"}),
        };
        let json = serde_json::to_string(&req).expect("serialize");
        assert!(
            !json.contains("\"env\""),
            "None env should be skipped on the wire, got: {json}"
        );
        // And it still deserializes
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

    #[test]
    fn from_route_decision_local_returns_none() {
        let decision = route(&CommandUri::local("inference/llm/generate"));
        assert!(
            AircCommandRequest::from_route_decision(&decision, Value::Null).is_none(),
            "Local decisions never go over the wire"
        );
    }

    #[test]
    fn from_route_decision_peer_packages_correctly() {
        let decision =
            route(&CommandUri::parse("airc://maya/inference/llm/generate").expect("parse"));
        let req = AircCommandRequest::from_route_decision(
            &decision,
            serde_json::json!({"prompt": "hi"}),
        )
        .expect("peer dec produces wire request");
        assert_eq!(req.path, "inference/llm/generate");
        assert_eq!(req.kind, "peer");
        assert_eq!(req.env, None);
        assert_eq!(req.params, serde_json::json!({"prompt": "hi"}));
    }

    #[test]
    fn from_route_decision_peer_with_env_preserves_env() {
        let decision = route(&CommandUri::parse("airc://maya:vr/screenshot").expect("parse"));
        let req = AircCommandRequest::from_route_decision(&decision, Value::Null)
            .expect("peer dec produces wire request");
        assert_eq!(req.env.as_deref(), Some("vr"));
    }

    #[test]
    fn from_route_decision_room_packages_correctly() {
        let room_id = Uuid::new_v4();
        let decision =
            route(&CommandUri::parse(&format!("airc://room:{room_id}/chat/post")).expect("parse"));
        let req = AircCommandRequest::from_route_decision(&decision, Value::Null)
            .expect("room dec produces wire request");
        assert_eq!(req.kind, "room");
        assert_eq!(req.path, "chat/post");
    }

    #[test]
    fn from_route_decision_broadcast_packages_correctly() {
        let decision =
            route(&CommandUri::parse("airc://maya:*/notification/send").expect("parse"));
        let req = AircCommandRequest::from_route_decision(&decision, Value::Null)
            .expect("broadcast dec produces wire request");
        assert_eq!(req.kind, "broadcast");
        assert_eq!(req.path, "notification/send");
    }

    /// Headers are constants — pin them so a refactor renaming a
    /// header is impossible without updating the test. The peer-side
    /// handler and middleware filter on these names; drift breaks
    /// the wire.
    #[test]
    fn header_names_are_stable_strings() {
        assert_eq!(HEADER_COMMAND_PATH, "continuum.command.path");
        assert_eq!(HEADER_COMMAND_KIND, "continuum.command.kind");
        assert_eq!(HEADER_COMMAND_ENV, "continuum.command.env");
        assert_eq!(HEADER_COMMAND_STATUS, "continuum.command.status");
        assert_eq!(COMMAND_REQUEST_BODY_HINT, "continuum.command.request.v1");
        assert_eq!(COMMAND_RESPONSE_BODY_HINT, "continuum.command.response.v1");
    }
}
