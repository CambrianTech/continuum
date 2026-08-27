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
use std::time::{SystemTime, UNIX_EPOCH};

use airc_core::{Body, PeerId, TranscriptEvent};
use airc_lib::adapter::{AdapterError, ConsumerAdapter};
use airc_lib::grid_auth::SignedCapabilityGrant;
use airc_lib::Airc;
use airc_protocol::headers_keys::{HEADER_AIRC_CAPABILITY_GRANT, HEADER_AIRC_CHANNEL_NAME};
use airc_protocol::{HEADER_AIRC_CORRELATION_ID, HEADER_AIRC_REPLY_TO};
use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD, Engine};
use tracing::{debug, warn};
use uuid::Uuid;

use super::grid_capability::{GrantAuthOutcome, GrantAuthorizer};
use super::{
    AircCommandRequest, AircCommandResponse, CallerIdentity, CommandUri, COMMAND_REQUEST_BODY_HINT,
    COMMAND_RESPONSE_BODY_HINT, HEADER_COMMAND_STATUS, HEADER_CONTINUUM_BODY_HINT,
};
use crate::runtime::{CommandExecutor, CommandResult};

/// Current wall-clock epoch-ms (for grant expiry + replay checks). `unwrap_or(0)`
/// only on a pre-1970 clock — which reads as "everything is expired", the
/// fail-closed direction for a capability check.
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Stable adapter name. Registered in the airc adapter registry so
/// operators can introspect what's consuming command envelopes.
pub const HANDLER_NAME: &str = "continuum.command.handler";

/// The peer-side command handler. Registered with the airc adapter
/// registry as a `ConsumerAdapter` claiming
/// [`COMMAND_REQUEST_BODY_HINT`].
pub struct CommandRequestHandler {
    airc: Arc<Airc>,
    executor: Arc<CommandExecutor>,
    /// Verifies a capability grant the caller presents (the contracted-grid path).
    /// `None` → grants are ignored; a caller is gated purely by tier (the current
    /// default). `Some` → a presented [`SignedCapabilityGrant`] is verified against
    /// the owner key + this node's mesh + the AUTHENTICATED sender key, and its
    /// conferred capabilities ride into the gate via
    /// [`CallerIdentity::with_granted_capabilities`].
    grant_authorizer: Option<Arc<GrantAuthorizer>>,
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
    /// The channel the request ARRIVED on (the envelope's `room_id`).
    /// The reply must ride this exact channel: the requester awaits on the
    /// room it asked in, and the responder's own current room may be a
    /// different room entirely (operator CLI in #general vs citizens landed
    /// in #academy — the 2026-08-27 grid-smoke 0/3 deadline class).
    pub request_channel: airc_core::RoomId,
    /// The request's stamped human channel name
    /// ([`airc_protocol::HEADER_AIRC_CHANNEL_NAME`]), if present — carried
    /// onto the reply for the blind-room heal.
    pub request_channel_name: Option<String>,
    /// A capability grant the caller PRESENTED on the envelope (base64
    /// [`HEADER_AIRC_CAPABILITY_GRANT`], decoded). `None` if absent. Decoded in
    /// [`parse_envelope`](CommandRequestHandler::parse_envelope) so the parse step
    /// stays the one place the wire shape is read; verified later against the
    /// authenticated sender key.
    pub presented_grant: Option<SignedCapabilityGrant>,
}

impl CommandRequestHandler {
    /// Build a handler against an existing airc handle and the
    /// substrate's CommandExecutor, with NO grant authorizer — a caller is gated
    /// purely by tier (presented grants are ignored). Returns `Arc<Self>` because
    /// the airc adapter registry stores adapters as `Arc<dyn ConsumerAdapter>`.
    pub fn new(airc: Arc<Airc>, executor: Arc<CommandExecutor>) -> Arc<Self> {
        Arc::new(Self {
            airc,
            executor,
            grant_authorizer: None,
        })
    }

    /// Build a handler that VERIFIES presented capability grants (the
    /// contracted-grid path). A caller presenting an owner-signed grant that
    /// confers the command is authorized regardless of its tier ceiling; absent or
    /// invalid grants fall back to tier gating.
    pub fn with_grant_authorizer(
        airc: Arc<Airc>,
        executor: Arc<CommandExecutor>,
        grant_authorizer: Arc<GrantAuthorizer>,
    ) -> Arc<Self> {
        Arc::new(Self {
            airc,
            executor,
            grant_authorizer: Some(grant_authorizer),
        })
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
                    "inbound command body was Binary; expected Json(AircCommandRequest)"
                        .to_string(),
                ));
            }
        };

        let request: AircCommandRequest = serde_json::from_value(body_value).map_err(|e| {
            AdapterError::Consumer(format!("decode AircCommandRequest from body JSON: {e}"))
        })?;

        // Optional capability grant the caller presents (base64 JSON). Absent is
        // normal (tier-gated call). PRESENT-but-undecodable is a client error we
        // surface LOUDLY (per [[no-fallbacks-ever]]) rather than silently dropping
        // — the caller intended to present a grant; we won't pretend they didn't.
        let presented_grant = match envelope.headers.get(HEADER_AIRC_CAPABILITY_GRANT) {
            None => None,
            Some(b64) => {
                let bytes = STANDARD.decode(b64).map_err(|e| {
                    AdapterError::Consumer(format!(
                        "header {HEADER_AIRC_CAPABILITY_GRANT} is not valid base64: {e}"
                    ))
                })?;
                let grant: SignedCapabilityGrant = serde_json::from_slice(&bytes).map_err(|e| {
                    AdapterError::Consumer(format!(
                        "header {HEADER_AIRC_CAPABILITY_GRANT} did not decode as a SignedCapabilityGrant: {e}"
                    ))
                })?;
                Some(grant)
            }
        };

        Ok(ParsedEnvelope {
            caller_peer_id,
            reply_to,
            correlation_id,
            request,
            request_channel: envelope.room_id,
            request_channel_name: envelope.headers.get(HEADER_AIRC_CHANNEL_NAME).cloned(),
            presented_grant,
        })
    }

    /// Dispatch a parsed request through the local CommandExecutor
    /// with the caller identity threaded into the policy gate.
    /// Returns the typed response shape the wire carries back.
    ///
    /// Tests can invoke this via [`Self::process_request_via`] with
    /// just a `&CommandExecutor` — no Airc handle required — so the
    /// executor-side of the handler is exercisable without standing
    /// up real airc plumbing.
    pub async fn process_request(&self, parsed: &ParsedEnvelope) -> AircCommandResponse {
        // Verify any presented capability grant against the AUTHENTICATED sender
        // key; on success its conferred capabilities ride into the gate. Absent /
        // invalid grant → empty caps → pure tier gating (unchanged behavior).
        let granted = self.verify_presented_grant(parsed).await;
        let caller = CallerIdentity::airc(parsed.caller_peer_id).with_granted_capabilities(granted);
        Self::dispatch_request(&self.executor, parsed, caller).await
    }

    /// Verify the grant the caller presented (if any) and return the capability
    /// tags it confers — empty when there is no authorizer, no presented grant, or
    /// the grant fails to authorize.
    ///
    /// The presenting key is `airc.peer_public_key(sender)` — the enrolled key from
    /// the SAME registry that signature-verified the inbound envelope, so it is the
    /// AUTHENTICATED sender's key, never the grant's self-asserted `grantee_pubkey`
    /// (the hard gate the review flagged). A non-Authorized outcome logs at debug +
    /// confers nothing; the caller then falls back to tier gating.
    async fn verify_presented_grant(&self, parsed: &ParsedEnvelope) -> Vec<String> {
        let (Some(authorizer), Some(grant)) = (
            self.grant_authorizer.as_ref(),
            parsed.presented_grant.as_ref(),
        ) else {
            return Vec::new();
        };
        let Some(presenting) = self.airc.peer_public_key(parsed.caller_peer_id) else {
            warn!(
                caller = %parsed.caller_peer_id.0,
                "presented a capability grant but the sender is not enrolled — \
                 cannot establish an authenticated presenting key; ignoring the grant"
            );
            return Vec::new();
        };
        match authorizer
            .authorize_command(grant, &presenting, &parsed.request.path, now_ms())
            .await
        {
            GrantAuthOutcome::Authorized => grant.grant.capabilities.clone(),
            other => {
                debug!(
                    caller = %parsed.caller_peer_id.0,
                    path = %parsed.request.path,
                    outcome = ?other,
                    "presented capability grant did not authorize; falling back to tier gate"
                );
                Vec::new()
            }
        }
    }

    /// Process a request against a borrowed `CommandExecutor` directly, with NO
    /// grant verification (the caller is the plain authenticated sender). Same
    /// behavior as the pre-grant path — tests + the `LocalGridTransport` fixture
    /// lease this without an `Arc<Airc>`. The grant-aware path is
    /// [`process_request`](Self::process_request).
    pub async fn process_request_via(
        executor: &CommandExecutor,
        parsed: &ParsedEnvelope,
    ) -> AircCommandResponse {
        let caller = CallerIdentity::airc(parsed.caller_peer_id);
        Self::dispatch_request(executor, parsed, caller).await
    }

    /// Dispatch the parsed request through the executor as `caller` (whose
    /// `granted_capabilities` the gate honors). Shared by the grant-aware
    /// [`process_request`](Self::process_request) and the plain
    /// [`process_request_via`](Self::process_request_via).
    async fn dispatch_request(
        executor: &CommandExecutor,
        parsed: &ParsedEnvelope,
        caller: CallerIdentity,
    ) -> AircCommandResponse {
        Self::execute_command_request(executor, &parsed.request, caller).await
    }

    /// The single owner of "an [`AircCommandRequest`] + a [`CallerIdentity`] →
    /// an [`AircCommandResponse`]". Decodes the envelope's routing intent
    /// (`kind`/`env`), maps the path to a local [`CommandUri`], dispatches through
    /// the executor as `caller` (whose tier + granted capabilities the gate
    /// honors), and encodes the [`CommandResult`] back into the wire response.
    ///
    /// Every remote-ingress path funnels through here — the airc peer handler
    /// ([`dispatch_request`](Self::dispatch_request)) AND the WS ingress
    /// (`ipc::ws`) both call this so the wire-dispatch contract has ONE owner
    /// ([[the-compression-principle]]). Callers differ only in how they
    /// authenticate the `caller` (airc = signed sender; WS = Provisional TCP-tier
    /// ceiling until a GH-auth handshake lands).
    pub async fn execute_command_request(
        executor: &CommandExecutor,
        request: &AircCommandRequest,
        caller: CallerIdentity,
    ) -> AircCommandResponse {
        // PR #1529 reviewer 2 BLOCK fix: the request envelope carries
        // `kind` (peer / room / broadcast) and `env` (the embodiment
        // filter). The handler MUST honor these — silently routing
        // everything as Local would discard the routing intent and
        // violate [[no-fallbacks-ever]].
        //
        // Today only `kind="peer"` with `env=None` is supported. Room
        // and broadcast semantics need their own slice (see
        // AircTransport::dispatch for the same hard-error rationale).
        // Env-aware local routing (route an inbound call to a specific
        // local embodiment service) also needs its own slice — the
        // substrate has the EnvironmentId typed primitive (Slice P)
        // but no per-env service registration yet. Until then,
        // env-targeted calls hard-error so the caller knows the
        // semantics aren't wired.
        if request.kind != continuum_airc_protocol::KIND_PEER {
            return AircCommandResponse::error(format!(
                "remote dispatch kind={:?} not yet implemented — \
                 only kind=\"peer\" is wired. Room broadcast and \
                 env-wildcard broadcast need their own slices to define \
                 fan-out semantics (all-replies-collect vs first-reply-wins \
                 vs fire-and-forget). Per [[no-fallbacks-ever]] the \
                 handler refuses to silently substitute Local routing.",
                request.kind
            ));
        }
        if let Some(env) = &request.env {
            return AircCommandResponse::error(format!(
                "remote dispatch with env={:?} not yet implemented — \
                 the substrate has the EnvironmentId typed primitive \
                 (Slice P) but no per-env service registration yet. \
                 Until env-aware local routing lands, env-targeted calls \
                 hard-error so callers know the semantics aren't wired.",
                env
            ));
        }

        // The path the remote dispatched maps to a local URI. The
        // local AuthPolicy gate sees the remote caller (including any verified
        // granted capabilities) and decides whether to allow — the gate's verdict
        // variants (Allowed / Forbidden / Deferred) propagate as the canonical
        // error string from execute_with_caller's Err arm.
        let uri = CommandUri::local(&request.path);

        match executor
            .execute_with_caller(uri, request.params.clone(), Some(caller))
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
        let body_value = serde_json::to_value(response)
            .map_err(|e| AdapterError::Consumer(format!("serialize AircCommandResponse: {e}")))?;
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

        // reply_in, NOT reply: the answer must ride the channel the request
        // arrived on. The requester awaits on the room it asked in; this
        // responder's current room can be a different room entirely (citizens
        // land in #academy, an operator CLI asks in #general) — reply() sent
        // the answer somewhere the requester never subscribed and every
        // dispatch died at the command deadline (grid-smoke 0/3, 2026-08-27).
        self.airc
            .reply_in(
                parsed.request_channel,
                parsed.request_channel_name.as_deref(),
                parsed.reply_to,
                parsed.correlation_id,
                headers,
                body,
            )
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
        // No capability-grant header → no presented grant (the tier-gated default).
        assert!(parsed.presented_grant.is_none());
    }

    /// base64(JSON) of an owner-signed grant — the wire shape a caller presents in
    /// [`HEADER_AIRC_CAPABILITY_GRANT`].
    fn signed_grant_header(
        owner: &airc_protocol::PeerKeypair,
        grantee: PeerId,
        grantee_pubkey: Vec<u8>,
        caps: &[&str],
    ) -> String {
        use airc_lib::grid_auth::{CapabilityGrant, SignedCapabilityGrant};
        use airc_lib::subscriptions::MeshIdentity;
        let grant = CapabilityGrant {
            grantee,
            grantee_pubkey,
            capabilities: caps.iter().map(|c| c.to_string()).collect(),
            granted_in: MeshIdentity::new("test-mesh"),
            issued_at_ms: 1,
            expires_at_ms: None,
            epoch: 1,
        };
        let signed = SignedCapabilityGrant::sign(owner, grant).expect("sign grant");
        STANDARD.encode(serde_json::to_vec(&signed).expect("serialize grant"))
    }

    // what this catches: parse_envelope DECODES a presented capability grant from
    // the base64 HEADER_AIRC_CAPABILITY_GRANT into the typed SignedCapabilityGrant
    // (the wire shape the verifier later checks). This is the one place the grant
    // wire format is read.
    #[test]
    fn parse_envelope_extracts_presented_grant() {
        let owner = airc_protocol::PeerKeypair::generate();
        let grantee = PeerId::new();
        let request = sample_request();
        let mut envelope = make_envelope(grantee, PeerId::new(), Uuid::new_v4(), &request);
        envelope.headers.insert(
            HEADER_AIRC_CAPABILITY_GRANT.to_string(),
            signed_grant_header(&owner, grantee, vec![1, 2, 3], &["ai/generate"]),
        );

        let parsed = CommandRequestHandler::parse_envelope(&envelope).expect("parse succeeds");
        let grant = parsed.presented_grant.expect("grant decoded");
        assert_eq!(grant.grant.capabilities, vec!["ai/generate".to_string()]);
        assert_eq!(grant.grant.grantee, grantee);
    }

    // what this catches: a PRESENT but undecodable grant header is surfaced LOUDLY
    // (per [[no-fallbacks-ever]]) rather than silently dropped — the caller intended
    // to present a grant; we don't pretend they didn't.
    #[test]
    fn parse_envelope_rejects_malformed_grant_header() {
        let mut envelope = make_envelope(
            PeerId::new(),
            PeerId::new(),
            Uuid::new_v4(),
            &sample_request(),
        );
        envelope.headers.insert(
            HEADER_AIRC_CAPABILITY_GRANT.to_string(),
            "!!!not-base64!!!".to_string(),
        );

        let err = CommandRequestHandler::parse_envelope(&envelope)
            .expect_err("malformed grant header should fail");
        match err {
            AdapterError::Consumer(msg) => assert!(
                msg.contains(HEADER_AIRC_CAPABILITY_GRANT),
                "error must name the grant header, got: {msg}"
            ),
            other => panic!("expected Consumer error, got {other:?}"),
        }
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

        let err =
            CommandRequestHandler::parse_envelope(&envelope).expect_err("missing body should fail");
        match err {
            AdapterError::Consumer(msg) => {
                assert!(
                    msg.contains("no body"),
                    "error must name missing body: {msg}"
                );
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

        let err =
            CommandRequestHandler::parse_envelope(&envelope).expect_err("binary body should fail");
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

    // ─── PR #1529 reviewer fix tests ──────────────────────────────────

    /// Build a request envelope shape suitable for direct
    /// `process_request` testing. Lets us hit the kind/env rejection
    /// paths without going through the full envelope parser.
    fn make_parsed(request: AircCommandRequest) -> ParsedEnvelope {
        ParsedEnvelope {
            caller_peer_id: PeerId::new(),
            reply_to: PeerId::new(),
            correlation_id: Uuid::new_v4(),
            request,
            request_channel: airc_core::RoomId::from_uuid(Uuid::new_v4()),
            request_channel_name: None,
            presented_grant: None,
        }
    }

    /// Reviewer 1 + 2 BLOCK: prove `process_request_via` rejects
    /// `kind="room"` rather than silently substituting Local routing.
    /// Uses the Airc-free entry point so the executor-side logic is
    /// exercisable without standing up airc plumbing.
    #[tokio::test]
    async fn process_request_rejects_room_kind() {
        let registry = Arc::new(crate::runtime::ModuleRegistry::new());
        let executor = CommandExecutor::new(registry);

        let parsed = make_parsed(AircCommandRequest {
            path: "chat/post".into(),
            kind: "room".into(),
            env: None,
            params: serde_json::Value::Null,
        });

        let response = CommandRequestHandler::process_request_via(&executor, &parsed).await;
        match response {
            AircCommandResponse::Error { message } => {
                assert!(
                    message.contains("kind=\"room\""),
                    "error must name the rejected kind, got: {message}"
                );
                assert!(
                    message.contains("not yet implemented"),
                    "error must signal not-yet-implemented, got: {message}"
                );
            }
            AircCommandResponse::Ok { .. } => {
                panic!("kind=room must be rejected, got Ok");
            }
        }
    }

    /// Reviewer 1 + 2 BLOCK: same shape, prove `kind="broadcast"`
    /// also rejected.
    #[tokio::test]
    async fn process_request_rejects_broadcast_kind() {
        let registry = Arc::new(crate::runtime::ModuleRegistry::new());
        let executor = CommandExecutor::new(registry);

        let parsed = make_parsed(AircCommandRequest {
            path: "notification/send".into(),
            kind: "broadcast".into(),
            env: None,
            params: serde_json::Value::Null,
        });

        let response = CommandRequestHandler::process_request_via(&executor, &parsed).await;
        assert!(matches!(response, AircCommandResponse::Error { .. }));
    }

    /// Reviewer 2 BLOCK: prove env-targeted dispatch is rejected
    /// (until env-aware local routing lands) instead of being silently
    /// dropped.
    #[tokio::test]
    async fn process_request_rejects_env_targeted_dispatch() {
        let registry = Arc::new(crate::runtime::ModuleRegistry::new());
        let executor = CommandExecutor::new(registry);

        let parsed = make_parsed(AircCommandRequest {
            path: "widget/show".into(),
            kind: "peer".into(),
            env: Some("vr".into()),
            params: serde_json::Value::Null,
        });

        let response = CommandRequestHandler::process_request_via(&executor, &parsed).await;
        match response {
            AircCommandResponse::Error { message } => {
                assert!(
                    message.contains("env=\"vr\""),
                    "error must name the rejected env, got: {message}"
                );
                assert!(
                    message.contains("env-aware local routing"),
                    "error must explain what's missing, got: {message}"
                );
            }
            AircCommandResponse::Ok { .. } => {
                panic!("env-targeted dispatch must be rejected");
            }
        }
    }

    /// Reviewer 3 BLOCK: prove `process_request_via` actually threads
    /// the verified caller_peer_id into the AuthPolicy gate. Closes
    /// the headline-of-the-PR coverage gap where the
    /// `execute_with_caller` branch was added but no test asserted
    /// the caller actually reached the gate.
    ///
    /// Builds a ClosureAuthPolicy that captures the caller it
    /// receives, dispatches via process_request_via, then asserts the
    /// captured caller matches what we packed into the envelope.
    #[tokio::test]
    async fn process_request_via_threads_caller_into_gate() {
        use crate::routing::{ClosurePolicy, RouteDecision};
        use std::sync::{Arc as StdArc, Mutex};

        let captured: StdArc<Mutex<Option<crate::routing::CallerIdentity>>> =
            StdArc::new(Mutex::new(None));
        let captured_clone = captured.clone();

        // Policy that records the caller it receives, then allows.
        let policy = ClosurePolicy::new(
            "record-caller",
            move |_decision: &RouteDecision, caller: Option<&crate::routing::CallerIdentity>| {
                *captured_clone.lock().unwrap() = caller.cloned();
                crate::routing::Verdict::Allowed
            },
        );

        let registry = Arc::new(crate::runtime::ModuleRegistry::new());
        let executor =
            crate::runtime::CommandExecutor::new(registry).with_policy(StdArc::new(policy));

        // Build an envelope with a known sender peer_id; assert
        // process_request_via threads it into the gate.
        let sender_peer_id = PeerId::new();
        let parsed = ParsedEnvelope {
            caller_peer_id: sender_peer_id,
            reply_to: PeerId::new(),
            correlation_id: Uuid::new_v4(),
            request: AircCommandRequest {
                path: "anything/no-such-command".into(),
                kind: "peer".into(),
                env: None,
                params: serde_json::Value::Null,
            },
            request_channel: airc_core::RoomId::from_uuid(Uuid::new_v4()),
            request_channel_name: None,
            presented_grant: None,
        };

        // The path doesn't resolve to a module, so execute_with_caller
        // returns a typed CommandNotFound error per [[no-fallbacks-ever]]
        // (task #219 removed the silent TS fallthrough). We don't care
        // about that error here — the gate ran FIRST, recorded the
        // caller, and that's the property under test.
        let _ = CommandRequestHandler::process_request_via(&executor, &parsed).await;

        let observed = captured.lock().unwrap().clone();
        let observed = observed.expect(
            "AuthPolicy::gate must have been invoked with Some(caller) — \
             process_request_via failed to thread the caller through",
        );
        assert_eq!(
            observed.peer_id, sender_peer_id,
            "caller's peer_id must match the envelope sender — \
             closes the silent-privilege-escalation seam reviewer 2 flagged"
        );
        assert!(
            matches!(observed.source, crate::routing::CallerSource::Airc),
            "caller source must be Airc (cross-grid), not Local: {observed:?}"
        );
    }

    /// Reviewer 1 nit: prove the handler refuses non-Json
    /// CommandResult shapes (Handle / Stream / Lambda) cleanly
    /// rather than silently coercing or panicking. Locks the
    /// behavior so a future refactor can't start sending HandleRef
    /// over the wire without a deliberate decision.
    #[tokio::test]
    async fn process_request_refuses_non_json_command_result() {
        use crate::runtime::{CommandResult, HandleRef};
        use async_trait::async_trait;
        use std::any::Any;

        // Module that returns a Handle result. The handler must
        // refuse to wire it across.
        struct HandleReturningModule;
        impl HandleReturningModule {
            const PREFIXES: &'static [&'static str] = &["handle-test/"];
        }
        #[async_trait]
        impl crate::runtime::ServiceModule for HandleReturningModule {
            fn config(&self) -> crate::runtime::ModuleConfig {
                crate::runtime::ModuleConfig {
                    name: "handle-test",
                    priority: crate::runtime::ModulePriority::Normal,
                    command_prefixes: Self::PREFIXES,
                    event_subscriptions: &[],
                    needs_dedicated_thread: false,
                    max_concurrency: 0,
                    tick_interval: None,
                }
            }
            async fn initialize(&self, _ctx: &crate::runtime::ModuleContext) -> Result<(), String> {
                Ok(())
            }
            async fn handle_command(
                &self,
                _command: &str,
                _params: serde_json::Value,
            ) -> Result<CommandResult, String> {
                Ok(CommandResult::Handle(HandleRef::with_id(
                    "handle-test",
                    Uuid::new_v4(),
                    "handle-test::TestHandle",
                )))
            }
            fn as_any(&self) -> &dyn Any {
                self
            }
        }

        let registry = Arc::new(crate::runtime::ModuleRegistry::new());
        registry.register(Arc::new(HandleReturningModule));
        let executor = CommandExecutor::new(registry);

        let parsed = make_parsed(AircCommandRequest {
            path: "handle-test/mint".into(),
            kind: "peer".into(),
            env: None,
            params: serde_json::Value::Null,
        });

        let response = CommandRequestHandler::process_request_via(&executor, &parsed).await;
        match response {
            AircCommandResponse::Error { message } => {
                assert!(
                    message.contains("non-Json"),
                    "error must name the non-Json variant, got: {message}"
                );
                assert!(
                    message.contains("Handle"),
                    "error should mention the cell shape, got: {message}"
                );
            }
            AircCommandResponse::Ok { .. } => {
                panic!("Handle result must not silently wire across as Ok");
            }
        }
    }
}
