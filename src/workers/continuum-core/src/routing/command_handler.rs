//! `CommandRequestHandler` — the inbound symmetric of [`AircTransport`].
//!
//! When a remote substrate dispatches `airc://<this-peer>/<path>`, its
//! AircTransport packages an [`AircCommandRequest`] + protocol headers
//! and sends via `Airc::request()`. The airc daemon on this side
//! routes the envelope by body_hint to whichever `ConsumerAdapter`
//! claims [`COMMAND_REQUEST_BODY_HINT`]. That adapter is this handler.
//!
//! The handler:
//!
//! 1. Parses the [`TranscriptEvent`] to extract:
//!    - **Verified sender** (the `peer_id` field, signed by airc)
//!    - **Reply addressing** (`airc.reply_to` + `airc.correlation_id`
//!      headers, auto-stamped by `Airc::request()`)
//!    - **Typed request body** ([`AircCommandRequest`])
//!
//! 2. Constructs a local [`CommandUri`] from the request's path. The
//!    path the remote substrate dispatched maps 1:1 to a local URI;
//!    auth + routing decisions happen locally based on the local
//!    substrate's policy.
//!
//! 3. Builds a [`CallerIdentity::airc(verified_sender)`] and threads
//!    it through [`CommandExecutor::execute_with_caller`]. The local
//!    [`AuthPolicy`](crate::routing::AuthPolicy) gate sees the real
//!    remote caller and applies the right verdict — Allowed,
//!    Forbidden (with typed reason), or Deferred.
//!
//! 4. Packages the result as [`AircCommandResponse`] — Ok with the
//!    JSON result OR Error with the typed message. Errors here might
//!    come from the gate (Forbidden / Deferred), from the module
//!    handler, or from the substrate itself; the caller-side
//!    AircTransport propagates the message into the local
//!    `Result<CommandResult, String>` shape exactly.
//!
//! 5. Replies via `Airc::reply()` with [`HEADER_COMMAND_STATUS`] +
//!    [`HEADER_CONTINUUM_BODY_HINT`] stamped on the headers (so
//!    middleware can filter responses without parsing bodies, same
//!    property as the request path).
//!
//! ## Why typed errors flow through the wire
//!
//! Per [[no-fallbacks-ever]]: a Forbidden verdict on the remote
//! substrate becomes an `Error { message: "forbidden: NoPermissionForUri(...)" }`
//! on the wire, becomes `Err("forbidden: ...")` on the caller side,
//! becomes the substrate's typed `SubstrateError::Forbidden(...)` at
//! the typed-consumer layer. The caller can match on the variant and
//! decide how to recover — locally or by surfacing to the user.
//!
//! ## Composition at boot
//!
//! ```ignore
//! use std::sync::Arc;
//! use continuum_core::routing::CommandRequestHandler;
//!
//! let handler = CommandRequestHandler::new(airc.clone(), executor.clone());
//! airc.register_consumer_adapter(handler).await?;
//! ```
//!
//! After registration, every inbound `airc://<this-peer>/<path>`
//! dispatch flows through this handler → local CommandExecutor →
//! local AuthPolicy → local module → reply. Symmetric with the
//! AircTransport's outbound path.

use std::sync::Arc;

use airc_core::{Body, PeerId, TranscriptEvent};
use airc_lib::adapter::{AdapterError, ConsumerAdapter};
use airc_lib::Airc;
use airc_protocol::{HEADER_AIRC_CORRELATION_ID, HEADER_AIRC_REPLY_TO};
use async_trait::async_trait;
use uuid::Uuid;

use super::{
    AircCommandRequest, AircCommandResponse, CallerIdentity, CommandUri, COMMAND_REQUEST_BODY_HINT,
    COMMAND_RESPONSE_BODY_HINT, HEADER_COMMAND_STATUS, HEADER_CONTINUUM_BODY_HINT,
};
use crate::runtime::{CommandExecutor, CommandResult};

/// Stable adapter name. Registered in the airc adapter registry so
/// operators can introspect what's consuming command envelopes.
pub const HANDLER_NAME: &str = "continuum.command.handler";

/// The peer-side command handler. Registered with the airc adapter
/// registry as a `ConsumerAdapter` claiming
/// [`COMMAND_REQUEST_BODY_HINT`].
pub struct CommandRequestHandler {
    airc: Arc<Airc>,
    executor: Arc<CommandExecutor>,
}

/// Parsed pieces of an incoming envelope. Pure data; the rest of
/// the handler operates on this struct so the parse step is testable
/// in isolation from airc.
#[derive(Debug, Clone)]
pub struct ParsedEnvelope {
    /// Verified sender peer_id from the envelope's `peer_id` field.
    /// airc has already validated the signature; we trust this as
    /// the authentic caller.
    pub caller_peer_id: PeerId,
    /// Where to send the reply. From the `airc.reply_to` header
    /// auto-stamped by `Airc::request()`.
    pub reply_to: PeerId,
    /// Correlation id matching the caller's pending request.
    pub correlation_id: Uuid,
    /// The typed request body.
    pub request: AircCommandRequest,
}

impl CommandRequestHandler {
    /// Build a handler against an existing airc handle and the
    /// substrate's CommandExecutor. Returns `Arc<Self>` because the
    /// airc adapter registry stores adapters as
    /// `Arc<dyn ConsumerAdapter>`.
    pub fn new(airc: Arc<Airc>, executor: Arc<CommandExecutor>) -> Arc<Self> {
        Arc::new(Self { airc, executor })
    }

    /// Parse a `TranscriptEvent` into the typed pieces the handler
    /// needs. Pure function — tests don't need airc to exercise it.
    ///
    /// Returns an error if any required header is missing, the body
    /// is absent or non-JSON, or the body doesn't deserialize as
    /// [`AircCommandRequest`].
    pub fn parse_envelope(envelope: &TranscriptEvent) -> Result<ParsedEnvelope, AdapterError> {
        let caller_peer_id = envelope.peer_id;

        let reply_to_raw = envelope.headers.get(HEADER_AIRC_REPLY_TO).ok_or_else(|| {
            AdapterError::Consumer(format!(
                "missing required header {HEADER_AIRC_REPLY_TO} on inbound command envelope"
            ))
        })?;
        let reply_to_uuid: Uuid = reply_to_raw.parse().map_err(|e| {
            AdapterError::Consumer(format!(
                "header {HEADER_AIRC_REPLY_TO}={reply_to_raw:?} is not a valid UUID: {e}"
            ))
        })?;
        let reply_to = PeerId(reply_to_uuid);

        let correlation_raw =
            envelope.headers.get(HEADER_AIRC_CORRELATION_ID).ok_or_else(|| {
                AdapterError::Consumer(format!(
                    "missing required header {HEADER_AIRC_CORRELATION_ID} on inbound command envelope"
                ))
            })?;
        let correlation_id: Uuid = correlation_raw.parse().map_err(|e| {
            AdapterError::Consumer(format!(
                "header {HEADER_AIRC_CORRELATION_ID}={correlation_raw:?} is not a valid UUID: {e}"
            ))
        })?;

        let body = envelope.body.as_ref().ok_or_else(|| {
            AdapterError::Consumer(
                "inbound command envelope has no body (expected Body::Json(AircCommandRequest))"
                    .to_string(),
            )
        })?;

        let body_value = match body {
            Body::Json(v) => v.clone(),
            Body::Binary(_) => {
                return Err(AdapterError::Consumer(
                    "inbound command body was Binary; expected Json(AircCommandRequest)".to_string(),
                ));
            }
        };

        let request: AircCommandRequest = serde_json::from_value(body_value).map_err(|e| {
            AdapterError::Consumer(format!("decode AircCommandRequest from body JSON: {e}"))
        })?;

        Ok(ParsedEnvelope {
            caller_peer_id,
            reply_to,
            correlation_id,
            request,
        })
    }

    /// Dispatch a parsed request through the local CommandExecutor
    /// with the caller identity threaded into the policy gate.
    /// Returns the typed response shape the wire carries back.
    ///
    /// Exposed (`pub`) so tests can exercise the executor-side of
    /// the handler with a real `CommandExecutor` + CannedModule
    /// without needing airc plumbing.
    pub async fn process_request(&self, parsed: &ParsedEnvelope) -> AircCommandResponse {
        // The path the remote dispatched maps to a local URI. The
        // local AuthPolicy gate sees the remote caller and decides
        // whether to allow — the gate's verdict variants
        // (Allowed / Forbidden / Deferred) propagate as the canonical
        // error string from execute_with_caller's Err arm.
        let uri = CommandUri::local(&parsed.request.path);
        let caller = CallerIdentity::airc(parsed.caller_peer_id.0);

        match self
            .executor
            .execute_with_caller(uri, parsed.request.params.clone(), Some(caller))
            .await
        {
            Ok(CommandResult::Json(value)) => AircCommandResponse::ok(value),
            // For now, non-Json results (Handle / Stream / Lambda
            // shapes) don't cross the wire — they require the wire
            // protocols those cell shapes are reserved for. Surface a
            // typed error rather than silently dropping or coercing.
            Ok(other) => AircCommandResponse::error(format!(
                "remote dispatch returned non-Json CommandResult ({other:?}); \
                 wire-stable shapes for Handle/Stream/Lambda are reserved per \
                 Slice 60 design"
            )),
            Err(msg) => AircCommandResponse::error(msg),
        }
    }

    /// Send the typed response back to the caller. Stamps
    /// [`HEADER_COMMAND_STATUS`] + [`HEADER_CONTINUUM_BODY_HINT`] so
    /// reply-routing middleware can filter without parsing the body.
    ///
    /// Exposed (`pub`) so a test fixture or `LocalGridTransport` can
    /// reuse the same reply shape without going through real airc.
    pub async fn send_reply(
        &self,
        parsed: &ParsedEnvelope,
        response: &AircCommandResponse,
    ) -> Result<(), AdapterError> {
        let body_value = serde_json::to_value(response).map_err(|e| {
            AdapterError::Consumer(format!("serialize AircCommandResponse: {e}"))
        })?;
        let body = Body::Json(body_value);

        let mut headers = airc_core::Headers::new();
        headers.insert(
            HEADER_COMMAND_STATUS.to_string(),
            response.status_header_value().to_string(),
        );
        headers.insert(
            HEADER_CONTINUUM_BODY_HINT.to_string(),
            COMMAND_RESPONSE_BODY_HINT.to_string(),
        );

        self.airc
            .reply(parsed.reply_to, parsed.correlation_id, headers, body)
            .await
            .map_err(|e| AdapterError::Io(format!("airc reply: {e}")))?;
        Ok(())
    }
}

#[async_trait]
impl ConsumerAdapter for CommandRequestHandler {
    fn name(&self) -> &'static str {
        HANDLER_NAME
    }

    fn body_hint(&self) -> &'static str {
        COMMAND_REQUEST_BODY_HINT
    }

    async fn on_envelope(&self, envelope: TranscriptEvent) -> Result<(), AdapterError> {
        let parsed = Self::parse_envelope(&envelope)?;
        let response = self.process_request(&parsed).await;
        self.send_reply(&parsed, &response).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airc_core::{ClientId, EventId, MentionTarget, RoomId, TranscriptKind};
    use serde_json::Value;
    use uuid::Uuid;

    /// Build a minimal TranscriptEvent with the headers + body shape
    /// the handler expects. Used by parse + process tests.
    fn make_envelope(
        sender: PeerId,
        reply_to: PeerId,
        correlation: Uuid,
        request: &AircCommandRequest,
    ) -> TranscriptEvent {
        let body_value = serde_json::to_value(request).expect("serialize request");
        let mut headers = airc_core::Headers::new();
        headers.insert(HEADER_AIRC_REPLY_TO.to_string(), reply_to.0.to_string());
        headers.insert(
            HEADER_AIRC_CORRELATION_ID.to_string(),
            correlation.to_string(),
        );
        TranscriptEvent {
            event_id: EventId::new(),
            room_id: RoomId::new(),
            peer_id: sender,
            client_id: ClientId::new(),
            kind: TranscriptKind::Message,
            occurred_at_ms: 1_700_000_000,
            lamport: 1,
            target: MentionTarget::Peer(reply_to),
            headers,
            body: Some(Body::Json(body_value)),
            attachment: None,
            receipt: None,
            metadata: Value::Null,
        }
    }

    fn sample_request() -> AircCommandRequest {
        AircCommandRequest {
            path: "code/exists".into(),
            kind: "peer".into(),
            env: None,
            params: serde_json::json!({"path": "foo.rs"}),
        }
    }

    #[test]
    fn parse_envelope_extracts_caller_reply_correlation_and_request() {
        let sender = PeerId::new();
        let reply_to = PeerId::new();
        let correlation = Uuid::new_v4();
        let request = sample_request();
        let envelope = make_envelope(sender, reply_to, correlation, &request);

        let parsed = CommandRequestHandler::parse_envelope(&envelope).expect("parse succeeds");

        assert_eq!(parsed.caller_peer_id, sender);
        assert_eq!(parsed.reply_to, reply_to);
        assert_eq!(parsed.correlation_id, correlation);
        assert_eq!(parsed.request, request);
    }

    #[test]
    fn parse_envelope_rejects_missing_reply_to() {
        let sender = PeerId::new();
        let reply_to = PeerId::new();
        let correlation = Uuid::new_v4();
        let request = sample_request();
        let mut envelope = make_envelope(sender, reply_to, correlation, &request);
        envelope.headers.remove(HEADER_AIRC_REPLY_TO);

        let err = CommandRequestHandler::parse_envelope(&envelope)
            .expect_err("missing reply_to should fail");
        match err {
            AdapterError::Consumer(msg) => {
                assert!(
                    msg.contains(HEADER_AIRC_REPLY_TO),
                    "error must name the missing header, got: {msg}"
                );
            }
            other => panic!("expected Consumer error, got {other:?}"),
        }
    }

    #[test]
    fn parse_envelope_rejects_missing_correlation_id() {
        let sender = PeerId::new();
        let reply_to = PeerId::new();
        let correlation = Uuid::new_v4();
        let request = sample_request();
        let mut envelope = make_envelope(sender, reply_to, correlation, &request);
        envelope.headers.remove(HEADER_AIRC_CORRELATION_ID);

        let err = CommandRequestHandler::parse_envelope(&envelope)
            .expect_err("missing correlation_id should fail");
        match err {
            AdapterError::Consumer(msg) => {
                assert!(msg.contains(HEADER_AIRC_CORRELATION_ID));
            }
            other => panic!("expected Consumer error, got {other:?}"),
        }
    }

    #[test]
    fn parse_envelope_rejects_missing_body() {
        let sender = PeerId::new();
        let reply_to = PeerId::new();
        let correlation = Uuid::new_v4();
        let request = sample_request();
        let mut envelope = make_envelope(sender, reply_to, correlation, &request);
        envelope.body = None;

        let err = CommandRequestHandler::parse_envelope(&envelope)
            .expect_err("missing body should fail");
        match err {
            AdapterError::Consumer(msg) => {
                assert!(msg.contains("no body"), "error must name missing body: {msg}");
            }
            other => panic!("expected Consumer error, got {other:?}"),
        }
    }

    #[test]
    fn parse_envelope_rejects_binary_body() {
        let sender = PeerId::new();
        let reply_to = PeerId::new();
        let correlation = Uuid::new_v4();
        let request = sample_request();
        let mut envelope = make_envelope(sender, reply_to, correlation, &request);
        envelope.body = Some(Body::Binary(vec![1, 2, 3]));

        let err = CommandRequestHandler::parse_envelope(&envelope)
            .expect_err("binary body should fail");
        match err {
            AdapterError::Consumer(msg) => {
                assert!(msg.contains("Binary"));
            }
            other => panic!("expected Consumer error, got {other:?}"),
        }
    }

    #[test]
    fn parse_envelope_rejects_malformed_body() {
        let sender = PeerId::new();
        let reply_to = PeerId::new();
        let correlation = Uuid::new_v4();
        let request = sample_request();
        let mut envelope = make_envelope(sender, reply_to, correlation, &request);
        envelope.body = Some(Body::Json(serde_json::json!({"unexpected": "shape"})));

        let err = CommandRequestHandler::parse_envelope(&envelope)
            .expect_err("malformed body should fail");
        match err {
            AdapterError::Consumer(msg) => {
                assert!(msg.contains("decode"));
            }
            other => panic!("expected Consumer error, got {other:?}"),
        }
    }

    #[test]
    fn parse_envelope_rejects_invalid_correlation_uuid() {
        let sender = PeerId::new();
        let reply_to = PeerId::new();
        let correlation = Uuid::new_v4();
        let request = sample_request();
        let mut envelope = make_envelope(sender, reply_to, correlation, &request);
        envelope.headers.insert(
            HEADER_AIRC_CORRELATION_ID.to_string(),
            "not-a-uuid".to_string(),
        );

        let err = CommandRequestHandler::parse_envelope(&envelope)
            .expect_err("invalid correlation_id UUID should fail");
        match err {
            AdapterError::Consumer(msg) => {
                assert!(msg.contains("not a valid UUID"));
            }
            other => panic!("expected Consumer error, got {other:?}"),
        }
    }

    #[test]
    fn handler_name_and_body_hint_match_protocol_constants() {
        // These are part of the wire contract — pin them so a silent
        // refactor changing one breaks the test loudly. The
        // caller-side AircTransport and the peer-side handler MUST
        // agree on the body_hint value.
        assert_eq!(HANDLER_NAME, "continuum.command.handler");
        // body_hint() must be the same constant the protocol module
        // exports — that's how the caller-side AircTransport knows
        // who to address.
        struct Fake; // build a fake without needing real Arc<Airc>
        impl Fake {
            fn body_hint(&self) -> &'static str {
                COMMAND_REQUEST_BODY_HINT
            }
        }
        assert_eq!(Fake.body_hint(), "continuum.command.request.v1");
    }
}
