//! WebSocket framing envelope for the thin-client transport.
//!
//! A single airc peer socket carries one request/response at a time; the
//! thin-client WS socket **multiplexes** — a browser fires N concurrent
//! commands over one connection and matches replies by a correlation id.
//! So the WS wire wraps [`AircCommandRequest`]/[`AircCommandResponse`]
//! (the dispatch shape, owned by `command.rs`) in a framed envelope that
//! carries that id.
//!
//! Both enums are **tagged** (`type` discriminant) and deliberately
//! single-variant today — the tag makes them forward-compatible: `Subscribe`
//! / `Emit` (client→server) and `Event` (server→client) slot in as new
//! variants without breaking the `Command`/`Response` wire shape. The nested
//! `AircCommandRequest`/`AircCommandResponse` keep their own `status` tag at a
//! deeper nesting level, so there is no discriminant collision.
//!
//! One owner of the wire shape: these types derive `TS` and generate the
//! TypeScript mirror the `WebSocketTransport` consumes — never hand-written
//! on the client side.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::command::{AircCommandRequest, AircCommandResponse};

/// Client→server WS frame.
///
/// `id` correlates the eventual [`WsServerMessage::Response`]. It is a
/// per-connection monotonic counter minted by the client; the server echoes
/// it back verbatim and never interprets it. `u64` is mapped to a TS `number`
/// (correlation ids never approach 2^53 on a single connection).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export, export_to = "../../../protocol/typescript/transport/WsClientMessage.ts")]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsClientMessage {
    /// Dispatch a command; reply arrives as [`WsServerMessage::Response`]
    /// with the same `id`.
    Command {
        #[ts(type = "number")]
        id: u64,
        request: AircCommandRequest,
    },
}

/// Server→client WS frame.
///
/// `id` mirrors the request's correlation id so the client can resolve the
/// matching pending promise.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export, export_to = "../../../protocol/typescript/transport/WsServerMessage.ts")]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsServerMessage {
    /// The reply to a [`WsClientMessage::Command`] carrying the same `id`.
    Response {
        #[ts(type = "number")]
        id: u64,
        response: AircCommandResponse,
    },
}

impl WsServerMessage {
    /// Build a `Response` frame for a given correlation id.
    pub fn response(id: u64, response: AircCommandResponse) -> Self {
        Self::Response { id, response }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // what this catches: the WS envelope must round-trip AND keep the outer
    // `type` tag distinct from the nested `status` tag — a flattening
    // regression would collide the two discriminants and corrupt dispatch.
    #[test]
    fn client_command_round_trips_with_nested_request() {
        let msg = WsClientMessage::Command {
            id: 7,
            request: AircCommandRequest::new(
                "data/list".into(),
                "peer".into(),
                None,
                json!({"collection": "users"}),
            ),
        };
        let wire = serde_json::to_string(&msg).expect("serialize");
        assert!(wire.contains("\"type\":\"command\""), "outer tag: {wire}");
        assert!(wire.contains("\"path\":\"data/list\""), "nested request: {wire}");
        let back: WsClientMessage = serde_json::from_str(&wire).expect("deserialize");
        assert_eq!(back, msg);
    }

    // what this catches: server Response nests a `status`-tagged
    // AircCommandResponse under the `type`-tagged frame; both tags must
    // survive at their own nesting level.
    #[test]
    fn server_response_round_trips_both_tags() {
        let msg = WsServerMessage::response(7, AircCommandResponse::ok(json!({"n": 3})));
        let wire = serde_json::to_string(&msg).expect("serialize");
        assert!(wire.contains("\"type\":\"response\""), "outer tag: {wire}");
        assert!(wire.contains("\"status\":\"ok\""), "nested status tag: {wire}");
        let back: WsServerMessage = serde_json::from_str(&wire).expect("deserialize");
        assert_eq!(back, msg);
    }
}
