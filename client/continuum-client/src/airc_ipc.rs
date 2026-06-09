//! `AircIpcTransport` — the canonical local-substrate `Transport` impl.
//!
//! Wraps an `Arc<airc_lib::Airc>` pointed at a continuum-core-server's
//! peer. Speaks the substrate's command-envelope wire shape via
//! `continuum-airc-protocol`, identical to what the server-side
//! `command_handler.rs` consumes — no wire drift possible because both
//! ends import the same types.
//!
//! Subscribe is not implemented in this slice; the substrate event
//! protocol (subscribe → ack → deliver → unsubscribe → ack) is a
//! multi-frame dance that deserves its own focused slice. `request()`
//! is the 80/20.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;

use airc_core::{Body, MentionTarget, PeerId};
use airc_lib::Airc;
use continuum_airc_protocol::command::{
    AircCommandRequest, AircCommandResponse, COMMAND_REQUEST_BODY_HINT, HEADER_COMMAND_ENV,
    HEADER_COMMAND_KIND, HEADER_COMMAND_PATH, HEADER_CONTINUUM_BODY_HINT, KIND_PEER,
};
use uuid::Uuid;

use crate::error::ClientError;
use crate::event::EventStream;
use crate::transport::Transport;

/// Default round-trip deadline. Re-export of the shared
/// `continuum_airc_protocol::DEFAULT_COMMAND_DEADLINE` so client and
/// substrate agree by import, not literal duplication. Override with
/// [`AircIpcTransport::with_deadline`] for long LLM generations.
pub use continuum_airc_protocol::command::DEFAULT_COMMAND_DEADLINE as DEFAULT_DEADLINE;

/// Local substrate transport over airc IPC.
///
/// Clone is cheap (one Arc + Copy for the PeerId/Duration/Atomic).
pub struct AircIpcTransport {
    airc: Arc<Airc>,
    target: PeerId,
    deadline: Duration,
    closed: Arc<AtomicBool>,
}

impl std::fmt::Debug for AircIpcTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AircIpcTransport")
            .field("target", &self.target)
            .field("deadline", &self.deadline)
            .field("closed", &self.closed.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

impl AircIpcTransport {
    /// Build a transport against an existing airc handle + the
    /// substrate's peer UUID.
    pub fn new(airc: Arc<Airc>, target_peer: Uuid) -> Self {
        Self {
            airc,
            target: PeerId(target_peer),
            deadline: DEFAULT_DEADLINE,
            closed: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Replace the default deadline. Builder-style.
    pub fn with_deadline(mut self, deadline: Duration) -> Self {
        self.deadline = deadline;
        self
    }

    fn build_headers(request: &AircCommandRequest) -> airc_core::Headers {
        let mut headers = airc_core::Headers::new();
        headers.insert(HEADER_COMMAND_PATH.to_string(), request.path.clone());
        headers.insert(HEADER_COMMAND_KIND.to_string(), request.kind.clone());
        if let Some(env) = &request.env {
            headers.insert(HEADER_COMMAND_ENV.to_string(), env.clone());
        }
        headers.insert(
            HEADER_CONTINUUM_BODY_HINT.to_string(),
            COMMAND_REQUEST_BODY_HINT.to_string(),
        );
        headers
    }

    fn decode_reply(reply_body: Option<Body>) -> Result<Value, ClientError> {
        let reply_body = reply_body.ok_or_else(|| {
            ClientError::Transport(
                "reply has no body (peer-side handler must attach Body::Json)".to_string(),
            )
        })?;

        let response_value = match reply_body {
            Body::Json(v) => v,
            Body::Binary(_) => {
                return Err(ClientError::Transport(
                    "reply body was Binary; expected Json (AircCommandResponse is JSON)".to_string(),
                ));
            }
        };

        let response: AircCommandResponse = serde_json::from_value(response_value)?;

        response.into_result().map_err(|message| ClientError::Refused {
            command: "<unknown>".to_string(),
            reason: message,
        })
    }
}

#[async_trait]
impl Transport for AircIpcTransport {
    async fn request(&self, command: &str, params: Value) -> Result<Value, ClientError> {
        if self.closed.load(Ordering::Relaxed) {
            return Err(ClientError::Closed);
        }

        // Client side always uses the "peer" route kind — we're
        // dispatching at a specific substrate peer over its IPC.
        let request =
            AircCommandRequest::new(command.to_string(), KIND_PEER.to_string(), None, params);

        let body_value = serde_json::to_value(&request)?;
        let body = Body::Json(body_value);
        let headers = Self::build_headers(&request);

        let pending = self
            .airc
            .request(MentionTarget::Peer(self.target), headers, body, self.deadline)
            .await
            .map_err(|e| ClientError::Transport(format!("airc request failed: {e}")))?;

        let reply = self
            .airc
            .await_reply(pending)
            .await
            .map_err(|e| ClientError::Transport(format!("await_reply failed: {e}")))?;

        Self::decode_reply(reply.body).map_err(|e| match e {
            ClientError::Refused { reason, .. } => ClientError::Refused {
                command: command.to_string(),
                reason,
            },
            other => other,
        })
    }

    async fn subscribe(&self, _class: &str) -> Result<EventStream, ClientError> {
        Err(ClientError::NotImplemented(
            "AircIpcTransport::subscribe — event protocol (subscribe/ack/deliver/unsubscribe) is a multi-frame dance deferred to its own slice",
        ))
    }

    async fn close(&self) -> Result<(), ClientError> {
        // Idempotent: first close wins, later calls return Closed.
        if self.closed.swap(true, Ordering::Relaxed) {
            return Err(ClientError::Closed);
        }
        // airc-lib's Airc handle is shared; dropping our Arc when this
        // transport goes out of scope releases our reference. Other
        // holders (the substrate, sibling transports) keep theirs.
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_headers_includes_path_kind_and_hint() {
        let req = AircCommandRequest::new(
            "ai/inference/generate".to_string(),
            "peer".to_string(),
            None,
            serde_json::json!({"prompt": "hi"}),
        );
        let headers = AircIpcTransport::build_headers(&req);
        assert_eq!(
            headers.get(HEADER_COMMAND_PATH).map(|s| s.as_str()),
            Some("ai/inference/generate")
        );
        assert_eq!(
            headers.get(HEADER_COMMAND_KIND).map(|s| s.as_str()),
            Some("peer")
        );
        assert_eq!(
            headers
                .get(HEADER_CONTINUUM_BODY_HINT)
                .map(|s| s.as_str()),
            Some(COMMAND_REQUEST_BODY_HINT)
        );
        assert!(
            headers.get(HEADER_COMMAND_ENV).is_none(),
            "no env should mean no env header"
        );
    }

    #[test]
    fn build_headers_adds_env_when_set() {
        let req = AircCommandRequest::new(
            "interface/screenshot".to_string(),
            "peer".to_string(),
            Some("vr".to_string()),
            Value::Null,
        );
        let headers = AircIpcTransport::build_headers(&req);
        assert_eq!(
            headers.get(HEADER_COMMAND_ENV).map(|s| s.as_str()),
            Some("vr")
        );
    }

    #[test]
    fn decode_reply_ok_returns_value() {
        let resp = AircCommandResponse::ok(serde_json::json!({"text": "hello"}));
        let body = Body::Json(serde_json::to_value(resp).unwrap());
        let decoded = AircIpcTransport::decode_reply(Some(body)).expect("decode");
        assert_eq!(decoded, serde_json::json!({"text": "hello"}));
    }

    #[test]
    fn decode_reply_error_returns_refused() {
        let resp = AircCommandResponse::error("policy denied");
        let body = Body::Json(serde_json::to_value(resp).unwrap());
        let err = AircIpcTransport::decode_reply(Some(body)).unwrap_err();
        match err {
            ClientError::Refused { reason, .. } => assert_eq!(reason, "policy denied"),
            other => panic!("expected Refused, got {other:?}"),
        }
    }

    #[test]
    fn decode_reply_no_body_returns_transport_error() {
        let err = AircIpcTransport::decode_reply(None).unwrap_err();
        assert!(matches!(err, ClientError::Transport(_)));
    }

    #[test]
    fn decode_reply_binary_body_returns_transport_error() {
        let err = AircIpcTransport::decode_reply(Some(Body::Binary(vec![1, 2, 3]))).unwrap_err();
        assert!(matches!(err, ClientError::Transport(_)));
    }
}
