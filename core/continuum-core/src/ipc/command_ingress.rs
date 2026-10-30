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

use continuum_base::time::EpochMs;
use std::sync::Arc;

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

use crate::routing::grid_capability::{GrantAuthOutcome, GrantAuthorizer};
use crate::routing::{AircCommandRequest, AircCommandResponse, CallerIdentity, CommandUri, COMMAND_REQUEST_BODY_HINT, COMMAND_RESPONSE_BODY_HINT, HEADER_COMMAND_STATUS, HEADER_CONTINUUM_BODY_HINT};
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
    /// Verifies a capability grant the caller presents (the contracted-grid path).
    /// `None` → grants are ignored; a caller is gated purely by tier (the current
    /// default). `Some` → a presented [`SignedCapabilityGrant`] is verified against
    /// the owner key + this node's mesh + the AUTHENTICATED sender key, and its
    /// conferred capabilities ride into the gate via
    /// [`CallerIdentity::with_granted_capabilities`].
    grant_authorizer: Option<Arc<GrantAuthorizer>>,
    /// Generations this seat is serving concurrently ([`served_generations`]). `None` on
    /// a task's detached copy: only the pump's handler owns the registry, so a stopped
    /// pump drops it and aborts what it was serving.
    generations: Option<Arc<served_generations::ServedGenerations>>,
    /// The admission bound for served generations, read per request
    /// ([`served_generations::admission_bound`]; a test pins it).
    generation_bound: fn() -> usize,
    /// Where a caller's airc trust tier is read for the ingress contract
    /// ([`ingress_contract`]): this node's airc trust store.
    tiers: Arc<dyn PeerTierSource>,
}

mod ingress_contract;
mod served_generations;
mod stream_publisher;

pub use ingress_contract::{
    admit_caller, admit_frame, required_tier, AddressedCommand, AircTrustStore, IngressRefusal, PeerTierSource,
};
use stream_publisher::StreamPublisher;

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
    /// The requester's own id for this generation and how long it is wanted
    /// ([`continuum_airc_protocol::HEADER_GENERATE_ID`] and
    /// [`continuum_airc_protocol::HEADER_GENERATE_VALID_UNTIL_MS`]); `None` from a requester
    /// that predates them, or one that sent an id without its validity.
    pub generation: Option<GenerationStamp>,
    /// A capability grant the caller PRESENTED on the envelope (base64
    /// [`HEADER_AIRC_CAPABILITY_GRANT`], decoded). `None` if absent. Decoded in
    /// [`parse_envelope`](CommandRequestHandler::parse_envelope) so the parse step
    /// stays the one place the wire shape is read; verified later against the
    /// authenticated sender key.
    pub presented_grant: Option<SignedCapabilityGrant>,
    /// The capability tags [`presented_grant`](Self::presented_grant) was VERIFIED to
    /// confer (against the authenticated sender key), set once by the ingress contract
    /// and threaded into the gate. Empty from [`parse_envelope`](CommandRequestHandler::parse_envelope).
    pub verified_capabilities: Vec<String>,
}

/// A requester's stamp on an `ai/generate` request: its own id for the generation, and the
/// epoch ms past which it no longer waits. Both or neither: an id without a validity could
/// be delayed past any bound the seat keeps its cancel for, so it is treated as unstamped
/// (keyed by the airc correlation, which no cancel names).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GenerationStamp {
    pub id: Uuid,
    pub valid_until_ms: u64,
}

impl GenerationStamp {
    fn from_headers(headers: &airc_core::Headers) -> Option<Self> {
        Some(Self {
            id: headers
                .get(continuum_airc_protocol::HEADER_GENERATE_ID)?
                .parse()
                .ok()?,
            valid_until_ms: headers
                .get(continuum_airc_protocol::HEADER_GENERATE_VALID_UNTIL_MS)?
                .parse()
                .ok()?,
        })
    }
}

impl CommandRequestHandler {
    /// Build a handler against an existing airc handle and the
    /// substrate's CommandExecutor, with NO grant authorizer — a caller is gated
    /// purely by tier (presented grants are ignored). Returns `Arc<Self>` because
    /// the airc adapter registry stores adapters as `Arc<dyn ConsumerAdapter>`.
    pub fn new(airc: Arc<Airc>, executor: Arc<CommandExecutor>) -> Arc<Self> {
        let tiers = AircTrustStore::new(airc.home().to_path_buf());
        Arc::new(Self {
            airc,
            executor,
            grant_authorizer: None,
            generations: Some(Arc::default()),
            generation_bound: served_generations::admission_bound,
            tiers,
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
        let tiers = AircTrustStore::new(airc.home().to_path_buf());
        Arc::new(Self {
            airc,
            executor,
            grant_authorizer: Some(grant_authorizer),
            generations: Some(Arc::default()),
            generation_bound: served_generations::admission_bound,
            tiers,
        })
    }

    /// The reply addressing a request carries in its headers (`airc.reply_to` +
    /// `airc.correlation_id`, auto-stamped by `Airc::request()`). Header-only, so a
    /// refusal can be answered without decoding the payload.
    fn reply_route(envelope: &TranscriptEvent) -> Result<(PeerId, Uuid), AdapterError> {
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
        Ok((reply_to, correlation_id))
    }

    /// The capability grant a caller PRESENTED in its headers (base64
    /// [`HEADER_AIRC_CAPABILITY_GRANT`]), decoded. Absent is normal (a tier-gated call).
    /// PRESENT-but-undecodable is a client error surfaced LOUDLY (per
    /// [[no-fallbacks-ever]]) rather than silently dropped.
    fn presented_grant(headers: &airc_core::Headers) -> Result<Option<SignedCapabilityGrant>, AdapterError> {
        let Some(b64) = headers.get(HEADER_AIRC_CAPABILITY_GRANT) else {
            return Ok(None);
        };
        let bytes = STANDARD.decode(b64).map_err(|e| {
            AdapterError::Consumer(format!("header {HEADER_AIRC_CAPABILITY_GRANT} is not valid base64: {e}"))
        })?;
        let grant: SignedCapabilityGrant = serde_json::from_slice(&bytes).map_err(|e| {
            AdapterError::Consumer(format!(
                "header {HEADER_AIRC_CAPABILITY_GRANT} did not decode as a SignedCapabilityGrant: {e}"
            ))
        })?;
        Ok(Some(grant))
    }

    /// Parse a `TranscriptEvent` into the typed pieces the handler
    /// needs. Pure function — tests don't need airc to exercise it.
    ///
    /// Returns an error if any required header is missing, the body
    /// is absent or non-JSON, or the body doesn't deserialize as
    /// [`AircCommandRequest`].
    pub fn parse_envelope(envelope: &TranscriptEvent) -> Result<ParsedEnvelope, AdapterError> {
        let caller_peer_id = envelope.peer_id;
        let (reply_to, correlation_id) = Self::reply_route(envelope)?;

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

        let presented_grant = Self::presented_grant(&envelope.headers)?;

        Ok(ParsedEnvelope {
            caller_peer_id,
            reply_to,
            correlation_id,
            request,
            request_channel: envelope.room_id,
            request_channel_name: envelope.headers.get(HEADER_AIRC_CHANNEL_NAME).cloned(),
            presented_grant,
            verified_capabilities: Vec::new(),
            generation: GenerationStamp::from_headers(&envelope.headers),
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
    /// [`process_request`](Self::process_request), with the tokens of an `ai/generate`
    /// on the wire AS THEY ARE PRODUCED — airc stream chunks (`airc.stream.*` headers,
    /// `DeliveryClass::StreamChunk`, never a durable row) in the room the request
    /// arrived in, `stream_id` = the request's correlation id, then the settled
    /// response as the reply exactly as before.
    ///
    /// INFERENCE IS A STREAM, NOT A PROMISE. Awaiting the whole answer under one
    /// deadline is how a working lane and a dead one looked identical for ten
    /// minutes (2026-09-17: every remote turn in the Intel node's history died at
    /// 600 s; the first one that was ever answered took 51 s in silence). With the
    /// chunks on the wire the requester's liveness is the NEXT chunk, and it renders
    /// as the answer forms. Same primitive the local persona turn rides into a room
    /// (`service_loop`: `publish_stream_chunk`, coalesced every 250 ms), same
    /// command path (ACL, interceptors, caller identity) — the only addition is the
    /// `streamId` the command resolves to the sink registered here.
    pub async fn process_request_streaming(&self, parsed: &ParsedEnvelope) -> AircCommandResponse {
        if parsed.request.path != STREAMED_GENERATE_PATH {
            return self.process_request(parsed).await;
        }
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let stream_id = parsed.correlation_id;
        let guard = crate::ai::stream_sinks::register(stream_id, tx);
        let mut streamed = parsed.clone();
        if let Some(obj) = streamed.request.params.as_object_mut() {
            obj.insert(
                crate::ai::stream_sinks::STREAM_ID_PARAM.to_string(),
                serde_json::Value::String(stream_id.to_string()),
            );
        }
        let publisher = StreamPublisher {
            airc: Arc::clone(&self.airc),
            room: parsed.request_channel,
            stream_id,
            // The seat's receipt instant: its own queue clock starts here, not on the
            // requester's node (card c84d885a, S1 — the wait is measured where it forms).
            received_at: std::time::Instant::now(),
        };
        let drain = tokio::spawn(publisher.drain(rx));
        let response = self.process_request(&streamed).await;
        // The sink's sender is dropped by the command (or by the guard, if the
        // command never took it); the drain sees the close, flushes, marks the
        // end, and only THEN does the settled reply go out — a requester never
        // sees the reply before the last chunk.
        drop(guard);
        match drain.await {
            Ok(published) => continuum_base::probe!(
                class = "airc.command.streamed",
                path = %parsed.request.path,
                correlation = %stream_id,
                chunks = published,
                "the answer's tokens rode the wire as they were produced; the settled reply follows"
            ),
            Err(e) => warn!(correlation = %stream_id, error = %e, "stream drain task failed"),
        }
        response
    }

    pub async fn process_request(&self, parsed: &ParsedEnvelope) -> AircCommandResponse {
        // The capabilities the ingress contract VERIFIED the presented grant to confer
        // (once, against the AUTHENTICATED sender key) ride into the gate. None
        // presented / invalid → empty caps → pure tier gating.
        let caller = CallerIdentity::airc(parsed.caller_peer_id)
            .with_granted_capabilities(parsed.verified_capabilities.clone());
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
    async fn verify_presented_grant(
        &self,
        caller: PeerId,
        grant: Option<&SignedCapabilityGrant>,
        path: &str,
    ) -> Vec<String> {
        let (Some(authorizer), Some(grant)) = (self.grant_authorizer.as_ref(), grant) else {
            return Vec::new();
        };
        let Some(presenting) = self.airc.peer_public_key(caller) else {
            warn!(
                caller = %caller.0,
                "presented a capability grant but the sender is not enrolled — \
                 cannot establish an authenticated presenting key; ignoring the grant"
            );
            return Vec::new();
        };
        match authorizer
            .authorize_command(grant, &presenting, path, EpochMs::now().millis())
            .await
        {
            GrantAuthOutcome::Authorized => grant.grant.capabilities.clone(),
            other => {
                debug!(
                    caller = %caller.0,
                    path = %path,
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
        // A generate that arrives over the wire is inference THIS seat performs for a
        // mind hosted on ANOTHER node — leased-in demand. Held across the whole dispatch
        // (receipt to settled reply; every inbound path converges here, streamed or not),
        // it is the term the serving plan sizes its slot count to
        // (`ServingDemand::leased_in`, card c84d885a). Before this guard the seat's plan
        // saw only its own roster while it served three nodes' coders on one slot —
        // lane wait p50 216 s, p90 1,387 s, max 7,058 s — and no receipt said why.
        let _leased_in = (parsed.request.path == STREAMED_GENERATE_PATH)
            .then(crate::inference::resource_admission::LeasedInCall::enter);
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
        self.reply(
            parsed.request_channel,
            parsed.request_channel_name.as_deref(),
            parsed.reply_to,
            parsed.correlation_id,
            response,
        )
        .await
    }

    /// [`send_reply`](Self::send_reply) from the reply addressing alone: the refusal path
    /// answers from headers without decoding the payload.
    async fn reply(
        &self,
        channel: airc_core::RoomId,
        channel_name: Option<&str>,
        reply_to: PeerId,
        correlation_id: Uuid,
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
            .reply_in(channel, channel_name, reply_to, correlation_id, headers, body)
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
        let me = self.airc.peer_id();
        let event = Arc::new(envelope);
        // THE INGRESS CONTRACT (`ingress_contract`), header facts first. Every node in
        // the room hears every command request; only a DIRECT request ADDRESSED to this
        // node is ours (a peer-addressed `ai/generate` used to be answered by whoever
        // heard it first, the sender included: 2026-09-06 on the M5). A room broadcast
        // is never executed, whatever its payload says (2026-10-30: an untrusted peer's
        // `persona/placement/reserve` requests in #general ran here).
        let command = match admit_frame(Arc::clone(&event), me) {
            Ok(command) => command,
            Err(refusal) => return self.refuse(&event, "", "unread", refusal).await,
        };
        let caller = command.envelope.sender();
        // The caller's trust tier, and a grant it presented (a HEADER, verified once
        // against the authenticated sender key), decide before the payload is decoded.
        let grant = Self::presented_grant(&event.headers)?;
        let granted = self.verify_presented_grant(caller, grant.as_ref(), &command.path).await;
        let tier = self.tiers.tier_of(caller).await;
        let tier_seen = match &tier {
            Ok(Some(tier)) => tier.as_wire_str(),
            Ok(None) => "unenrolled",
            Err(_) => "unreadable",
        };
        if let Err(refusal) = admit_caller(tier, &command.path, !granted.is_empty()) {
            return self.refuse(&event, &command.path, tier_seen, refusal).await;
        }
        // Only now is the payload decoded, by its one consumer; it must name the command
        // the header decision was made on.
        let mut parsed = Self::parse_envelope(&event)?;
        if parsed.request.path != command.path {
            return self.refuse(&event, &command.path, tier_seen, IngressRefusal::PathMismatch).await;
        }
        parsed.verified_capabilities = granted;
        // THE ACCEPT PATH HAD NO PROBE. Only the refusal emitted one, so a
        // request that ARRIVED AND RAN was indistinguishable from one that was never
        // sent — and on a node whose citizens generate continuously, a downstream
        // inference row cannot be attributed to a peer without a join key. Measured
        // 2026-09-06 22:04:21Z: a real cross-grid `ai/generate` reached this node
        // addressed to a seat no citizen owned, bounced off the refusal probe, and
        // the question "did anything ever land?" stayed unanswerable for a day
        // because landing is silent. `correlation` is the join key: it is the same
        // uuid the CALLER awaits on, so both sides of a grid turn can be matched.
        continuum_base::probe!(
            class = "airc.command.accepted",
            path = %parsed.request.path,
            kind = %parsed.request.kind,
            caller = %parsed.caller_peer_id.0,
            tier = %tier_seen,
            me = %me.0,
            correlation = %parsed.correlation_id,
            "peer command request ACCEPTED by the ingress contract — this node answers it"
        );
        // The requester abandoned a generation it asked for (card 83f774fb): abort it here
        // instead of finishing it for no one. A control command, answered inline before
        // any generation logic, so it never queues behind the work it cancels. Honored
        // only from the peer that asked (the registry records the caller).
        if parsed.request.path == continuum_airc_protocol::COMMAND_PATH_AI_GENERATE_CANCEL {
            let response = match serde_json::from_value::<continuum_airc_protocol::GenerateCancel>(
                parsed.request.params.clone(),
            ) {
                Ok(cancel) => {
                    let outcome = match &self.generations {
                        Some(generations) => generations.cancel(
                            cancel.generation,
                            parsed.caller_peer_id,
                            (self.generation_bound)(),
                            cancel.valid_until_ms,
                            EpochMs::now().millis(),
                        ),
                        None => served_generations::CancelOutcome::NotServing,
                    };
                    continuum_base::probe!(
                        class = "airc.command.generation_cancel",
                        generation = %cancel.generation,
                        requester = %parsed.caller_peer_id.0,
                        outcome = outcome.as_str(),
                        "a requester abandoned a served generation"
                    );
                    AircCommandResponse::Ok {
                        result: serde_json::json!({
                            "cancelled": outcome == served_generations::CancelOutcome::Cancelled,
                            "outcome": outcome.as_str(),
                        }),
                    }
                }
                Err(e) => AircCommandResponse::error(format!(
                    "ai/generate/cancel needs params {{generation: uuid, valid_until_ms: u64}}: {e}"
                )),
            };
            self.send_reply(&parsed, &response).await?;
            return Ok(());
        }
        // A served generation holds a lane for minutes: it runs as its own task so the
        // pump keeps reading (card b3e5b582). Classified from the parsed body, the path
        // that executes, never from a header. Over the bound it is refused at once with a
        // typed overload reply instead of queueing an unbounded population of handlers.
        if parsed.request.path == STREAMED_GENERATE_PATH {
            if let Some(generations) = &self.generations {
                let bound = (self.generation_bound)();
                // Keyed by the requester's generation id, so its cancel can name the work
                // even if it abandoned the request before airc returned the correlation.
                let key = parsed
                    .generation
                    .map_or(parsed.correlation_id, |stamp| stamp.id);
                let valid_until_ms = parsed.generation.map(|stamp| stamp.valid_until_ms);
                match generations.admit(
                    key,
                    parsed.caller_peer_id,
                    bound,
                    valid_until_ms,
                    EpochMs::now().millis(),
                ) {
                    Ok(admission) => {
                        let correlation = key;
                        let answerer = self.detached();
                        let task = tokio::spawn(async move {
                            let _admission = admission;
                            if let Err(e) = answerer.answer(parsed).await {
                                warn!(error = %e, "served generation could not send its reply");
                            }
                        });
                        generations.attach(correlation, task.abort_handle());
                        continuum_base::probe!(
                            class = "airc.command.generation_dispatched",
                            correlation = %correlation,
                            outstanding = generations.outstanding() as u64,
                            bound = bound as u64,
                            "a served generation runs as its own task; the pump keeps reading"
                        );
                        return Ok(());
                    }
                    Err(refusal) => {
                        continuum_base::probe!(
                            class = "airc.command.generation_refused",
                            correlation = %parsed.correlation_id,
                            caller = %parsed.caller_peer_id.0,
                            reason = %refusal,
                            "a served generation was refused at admission and answered at once"
                        );
                        let response =
                            AircCommandResponse::refused(refusal.to_string(), refusal.wire());
                        self.send_reply(&parsed, &response).await?;
                        return Ok(());
                    }
                }
            }
        }
        self.answer(parsed).await
    }
}

impl CommandRequestHandler {
    /// A copy for one served generation's task: the same peer, executor and grants, and
    /// no registry (only the pump's handler owns it).
    fn detached(&self) -> Self {
        Self {
            airc: Arc::clone(&self.airc),
            executor: Arc::clone(&self.executor),
            grant_authorizer: self.grant_authorizer.clone(),
            generations: None,
            generation_bound: self.generation_bound,
            tiers: Arc::clone(&self.tiers),
        }
    }

    /// A frame the ingress contract refused: probed with the rule, the caller, the
    /// command, the channel and the tier; a direct request here is answered with the rule
    /// (never its params); a frame that was not ours to answer draws no reply.
    async fn refuse(
        &self,
        event: &TranscriptEvent,
        path: &str,
        tier: &str,
        refusal: IngressRefusal,
    ) -> Result<(), AdapterError> {
        continuum_base::probe!(
            class = "airc.command.ingress_refused",
            rule = refusal.rule(),
            caller = %event.peer_id.0,
            path = %path,
            addressing = ?event.target,
            channel = %event.room_id.as_uuid(),
            tier = %tier,
            me = %self.airc.peer_id().0,
            "peer command frame refused by the ingress contract"
        );
        if !refusal.answered() {
            return Ok(());
        }
        let (reply_to, correlation_id) = Self::reply_route(event)?;
        let response = AircCommandResponse::error(refusal.reply_message(path));
        self.reply(
            event.room_id,
            event.headers.get(HEADER_AIRC_CHANNEL_NAME).map(String::as_str),
            reply_to,
            correlation_id,
            &response,
        )
        .await
    }

    /// Pin the generation admission bound (tests: the live lane count is process-global).
    #[cfg(test)]
    pub(crate) fn with_generation_bound(mut this: Arc<Self>, bound: fn() -> usize) -> Arc<Self> {
        if let Some(handler) = Arc::get_mut(&mut this) {
            handler.generation_bound = bound;
        }
        this
    }

    /// Execute an accepted request and send its reply.
    async fn answer(&self, parsed: ParsedEnvelope) -> Result<(), AdapterError> {
        let started = std::time::Instant::now();
        // WORK SIZE, or elapsed_ms is uninterpretable. Measured 2026-09-06: nine
        // successful cross-grid generations split cleanly bimodal — two at ~32s, six
        // at 135-292s, nothing between — and the probe could not say whether the slow
        // mode did MORE WORK or was merely slower, because it recorded duration and
        // not size. I checked contention (concurrent local inferences per run) and it
        // was REFUTED: the two fastest runs carried the HIGHEST concurrent load. With
        // no size field there was no next hypothesis to test. A stopwatch with no
        // odometer measures something, but it cannot explain anything.
        //
        // Serialized byte counts rather than tokens on purpose: this handler is
        // path-agnostic (it dispatches ANY command), so it must not special-case
        // `ai/generate`'s payload shape to reach a `usage` field. Bytes are a coarse
        // proxy that stays correct for every path, including ones not written yet.
        //
        // COST GATE. The probe system's contract is a Noop default at ZERO hot-path
        // cost, and this handler is the front door for EVERY cross-grid command, not
        // just generations. Measuring by re-serializing is not free — the params are
        // the largest thing in the request (a 10k-token prompt gets re-encoded purely
        // to be measured, then dropped). `tracing::enabled!` is the same question the
        // probe itself asks, so with no sink listening this costs a level check and
        // nothing else. Raised in review of #3816 by the IntelMac: "we added a probe
        // and the accept path got slower" is a bad way to learn this.
        let measure = tracing::enabled!(tracing::Level::INFO);
        let params_bytes = if measure {
            serde_json::to_vec(&parsed.request.params)
                .map(|v| v.len() as u64)
                .unwrap_or(0) // unwrap_or: the params ALREADY deserialized to get here, so re-serialization failing is not a real branch; 0 reads as "unmeasured", never as "empty request"
        } else {
            0
        };
        let response = self.process_request_streaming(&parsed).await;
        // Carry the refusal TEXT, not just the fact of refusal. `outcome=error`
        // with elapsed_ms=0 says a gate declined before any work happened but not
        // WHICH gate — and the message is the whole diagnosis (kind/env refusal vs
        // the AuthPolicy verdict vs an unknown path). Truncated: a gate string is
        // a sentence, and an unbounded field in a hot probe is a log-flood risk.
        let (outcome, detail, result_bytes) = match &response {
            AircCommandResponse::Error { message, .. } => (
                "error",
                message.chars().take(400).collect::<String>(),
                0u64,
            ),
            AircCommandResponse::Ok { result } => (
                "ok",
                String::new(),
                if measure {
                    serde_json::to_vec(result).map(|v| v.len() as u64).unwrap_or(0) // unwrap_or: the value came FROM serde and is about to be sent; 0 means unmeasured
                } else {
                    0
                },
            ),
        };
        continuum_base::probe!(
            class = "airc.command.completed",
            path = %parsed.request.path,
            caller = %parsed.caller_peer_id.0,
            me = %self.airc.peer_id().0,
            correlation = %parsed.correlation_id,
            outcome = %outcome,
            detail = %detail,
            params_bytes = params_bytes,
            result_bytes = result_bytes,
            elapsed_ms = started.elapsed().as_millis() as u64,
            "peer command request answered"
        );
        self.send_reply(&parsed, &response).await?;
        Ok(())
    }
}

/// The one command whose answer is streamed on the wire, and the one a seat serves
/// concurrently: one owner for the wire literal (`continuum-airc-protocol`).
const STREAMED_GENERATE_PATH: &str = continuum_airc_protocol::COMMAND_PATH_AI_GENERATE;

#[cfg(test)]
mod tests;
