//! Substrate-side bridge to the shared `airc-protocol` command wire types.
//!
//! The wire envelopes ([`AircCommandRequest`], [`AircCommandResponse`])
//! and the header / body-hint constants live in `airc-protocol` so both
//! continuum-core (server-side handler + cross-grid transport) and
//! continuum-client (client lib) share them without coupling client →
//! server compilation.
//!
//! What stays substrate-side: [`command_request_from_route_decision`],
//! which couples to the typed routing primitives ([`RouteDecision`],
//! [`RouteKind`]). The cross-grid transport
//! ([`super::airc_transport::AircTransport`]) calls it.

use serde_json::Value;

pub use continuum_airc_protocol::command::{
    AircCommandRequest, AircCommandResponse, COMMAND_REQUEST_BODY_HINT, COMMAND_RESPONSE_BODY_HINT,
    HEADER_COMMAND_ENV, HEADER_COMMAND_KIND, HEADER_COMMAND_PATH, HEADER_COMMAND_STATUS,
    HEADER_CONTINUUM_BODY_HINT,
};

use super::{RouteDecision, RouteKind};

/// Package a non-Local [`RouteDecision`] + params into the wire envelope.
/// Returns `None` for `RouteDecision::Local` — local decisions don't go
/// over the wire and shouldn't reach this constructor.
pub fn command_request_from_route_decision(
    decision: &RouteDecision,
    params: Value,
) -> Option<AircCommandRequest> {
    match decision {
        RouteDecision::Local { .. } => None,
        RouteDecision::Peer { path, env, .. } => Some(AircCommandRequest::new(
            path.clone(),
            RouteKind::Peer.as_str().to_string(),
            env.as_ref().map(|e| e.to_string()),
            params,
        )),
        RouteDecision::Room { path, env, .. } => Some(AircCommandRequest::new(
            path.clone(),
            RouteKind::Room.as_str().to_string(),
            env.as_ref().map(|e| e.to_string()),
            params,
        )),
        RouteDecision::Broadcast { path, .. } => Some(AircCommandRequest::new(
            path.clone(),
            RouteKind::Broadcast.as_str().to_string(),
            None,
            params,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::{route, CommandUri};
    use uuid::Uuid;

    #[test]
    fn from_route_decision_local_returns_none() {
        let decision = route(&CommandUri::local("inference/llm/generate"));
        assert!(
            command_request_from_route_decision(&decision, Value::Null).is_none(),
            "Local decisions never go over the wire"
        );
    }

    #[test]
    fn from_route_decision_peer_packages_correctly() {
        let decision =
            route(&CommandUri::parse("airc://maya/inference/llm/generate").expect("parse"));
        let req =
            command_request_from_route_decision(&decision, serde_json::json!({"prompt": "hi"}))
                .expect("peer dec produces wire request");
        assert_eq!(req.path, "inference/llm/generate");
        assert_eq!(req.kind, "peer");
        assert_eq!(req.env, None);
        assert_eq!(req.params, serde_json::json!({"prompt": "hi"}));
    }

    #[test]
    fn from_route_decision_peer_with_env_preserves_env() {
        let decision = route(&CommandUri::parse("airc://maya:vr/screenshot").expect("parse"));
        let req = command_request_from_route_decision(&decision, Value::Null)
            .expect("peer dec produces wire request");
        assert_eq!(req.env.as_deref(), Some("vr"));
    }

    #[test]
    fn from_route_decision_room_packages_correctly() {
        let room_id = Uuid::new_v4();
        let decision =
            route(&CommandUri::parse(&format!("airc://room:{room_id}/chat/post")).expect("parse"));
        let req = command_request_from_route_decision(&decision, Value::Null)
            .expect("room dec produces wire request");
        assert_eq!(req.kind, "room");
        assert_eq!(req.path, "chat/post");
    }

    #[test]
    fn from_route_decision_broadcast_packages_correctly() {
        let decision = route(&CommandUri::parse("airc://maya:*/notification/send").expect("parse"));
        let req = command_request_from_route_decision(&decision, Value::Null)
            .expect("broadcast dec produces wire request");
        assert_eq!(req.kind, "broadcast");
        assert_eq!(req.path, "notification/send");
    }
}
