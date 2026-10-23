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
}

mod served_generations;

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
            generations: Some(Arc::default()),
            generation_bound: served_generations::admission_bound,
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
            generations: Some(Arc::default()),
            generation_bound: served_generations::admission_bound,
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
            .authorize_command(grant, &presenting, &parsed.request.path, EpochMs::now().millis())
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

/// A command request is answered by exactly the peer it names. `All` and `Room`
/// targets are not peer RPCs (the only dispatch kind wired today), so they are
/// nobody's to answer either.
pub(crate) fn addressed_to(target: &airc_core::MentionTarget, me: PeerId) -> bool {
    matches!(target, airc_core::MentionTarget::Peer(p) if *p == me)
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
        // ONLY THE ADDRESSED PEER ANSWERS. Every node in the room hears every
        // command request; this handler answered all of them — so a peer-addressed
        // `ai/generate` was answered by whoever heard it first, the SENDER included
        // (measured 2026-09-06 03:30Z on the M5: a request to an unknown peer came
        // back with this node's own serving state as "the remote peer's adapter").
        // A request for another peer is not ours to answer; a request for nobody in
        // particular is not a peer RPC (only kind=peer is wired) — both are ignored
        // by name, never answered.
        if !addressed_to(&envelope.target, self.airc.peer_id()) {
            continuum_base::probe!(
                class = "airc.command.request_not_for_me",
                target = ?envelope.target,
                me = %self.airc.peer_id().0,
                "command request addressed to another peer (or to nobody) — not answering"
            );
            return Ok(());
        }
        // THE ACCEPT PATH NAMES ITSELF (BigMama, 2026-09-06 22:0xZ): the only probe on
        // inbound peer commands was the refusal, so a request that was ACCEPTED left no
        // row and silence read as ambiguous. Now every accepted request says who was
        // asked, by whom, and what — the receipt the grid's first hop was missing.
        continuum_base::probe!(
            class = "airc.command.request_accepted",
            target = ?envelope.target,
            me = %self.airc.peer_id().0,
            from = %envelope.peer_id.0,
            "command request addressed to this citizen — executing"
        );
        let parsed = Self::parse_envelope(&envelope)?;
        // THE ACCEPT PATH HAD NO PROBE. Only the refusal above emitted one, so a
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
            me = %self.airc.peer_id().0,
            correlation = %parsed.correlation_id,
            "peer command request ACCEPTED — this node answers it"
        );
        // A served generation holds a lane for minutes: it runs as its own task so the
        // pump keeps reading (card b3e5b582). Classified from the parsed body, the path
        // that executes, never from a header. Over the bound it is refused at once with a
        // typed overload reply instead of queueing an unbounded population of handlers.
        if parsed.request.path == STREAMED_GENERATE_PATH {
            if let Some(generations) = &self.generations {
                let bound = (self.generation_bound)();
                match generations.admit(parsed.correlation_id, bound) {
                    Ok(admission) => {
                        let correlation = parsed.correlation_id;
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
                        let response = AircCommandResponse::Error { message: refusal.to_string() };
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
        }
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
            AircCommandResponse::Error { message } => (
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

/// Tokens are coalesced and flushed at most this often — the same cadence the local
/// persona turn uses into a room; one frame per token would be 35 frames/s on a 27B.
const STREAM_FLUSH_EVERY: std::time::Duration = std::time::Duration::from_millis(250);


/// Publishes one generation's chunks into the request's room under its stream id.
struct StreamPublisher {
    airc: Arc<Airc>,
    room: airc_core::RoomId,
    stream_id: Uuid,
    /// When this seat RECEIVED the request. The first prefill frame or token published
    /// after it is the seat's own receipt-to-first-progress wait — the queue as the seat
    /// experiences it, which is what it may honestly advertise on its beacon.
    received_at: std::time::Instant,
}

impl StreamPublisher {
    async fn publish(&self, seq: u64, kind: &str, text: String, is_final: bool) -> bool {
        let mut headers = airc_core::Headers::new();
        headers.insert(airc_lib::HEADER_STREAM_ID.into(), self.stream_id.to_string());
        headers.insert(airc_lib::HEADER_STREAM_SEQ.into(), seq.to_string());
        headers.insert(airc_lib::HEADER_STREAM_KIND.into(), kind.to_string());
        if is_final {
            headers.insert(airc_lib::HEADER_STREAM_FINAL.into(), "true".to_string());
        }
        match self
            .airc
            .publish_with_delivery(
                airc_lib::PublishTarget::RoomByName(self.room.as_uuid().to_string()),
                airc_protocol::FrameKind::Event,
                Body::text(text),
                headers,
                airc_bus::DeliveryClass::StreamChunk,
            )
            .await
        {
            Ok(_) => true,
            Err(e) => {
                warn!(correlation = %self.stream_id, seq, error = %e, "stream chunk publish failed");
                false
            }
        }
    }

    /// Drain the command's sink onto the wire until it closes; returns chunks published.
    async fn drain(
        self,
        mut rx: tokio::sync::mpsc::UnboundedReceiver<crate::ai::adapter::GenerationChunk>,
    ) -> u64 {
        use crate::ai::adapter::GenerationChunk;
        let mut seq = 0u64;
        let mut published = 0u64;
        let mut token = String::new();
        let mut reasoning = String::new();
        let mut prefill: Option<String> = None;
        let mut last_flush = std::time::Instant::now();
        // Recorded once, at the FIRST progress the requester can see (prefill frame or
        // token) — receipt-to-first-progress is the seat's queue, and only the first
        // publish is that measurement; every later flush is generation.
        let mut first_progress_recorded = false;
        loop {
            let next = tokio::time::timeout(STREAM_FLUSH_EVERY, rx.recv()).await;
            let closed = matches!(next, Ok(None));
            if let Ok(Some(chunk)) = next {
                match chunk {
                    GenerationChunk::Token(t) => token.push_str(&t),
                    GenerationChunk::Reasoning(r) => reasoning.push_str(&r),
                    GenerationChunk::Prefill { processed, total, cached } => {
                        prefill = Some(
                            serde_json::json!({ "processed": processed, "total": total, "cached": cached })
                                .to_string(),
                        );
                    }
                }
            }
            if closed || last_flush.elapsed() >= STREAM_FLUSH_EVERY {
                if !first_progress_recorded && (prefill.is_some() || !token.is_empty() || !reasoning.is_empty()) {
                    first_progress_recorded = true;
                    crate::inference::resource_admission::note_leased_in_first_progress_ms(
                        self.received_at.elapsed().as_millis().min(u64::MAX as u128) as u64,
                    );
                }
                if let Some(p) = prefill.take() {
                    published += self.publish(seq, crate::routing::STREAM_KIND_PREFILL, p, false).await as u64;
                    seq += 1;
                }
                if !reasoning.is_empty() {
                    let flushed = std::mem::take(&mut reasoning);
                    published += self
                        .publish(seq, airc_lib::STREAM_KIND_TEXT_REASONING, flushed, false)
                        .await as u64;
                    seq += 1;
                }
                if !token.is_empty() {
                    let flushed = std::mem::take(&mut token);
                    published += self
                        .publish(seq, airc_lib::STREAM_KIND_TEXT_TOKEN, flushed, false)
                        .await as u64;
                    seq += 1;
                }
                last_flush = std::time::Instant::now();
            }
            if closed {
                break;
            }
        }
        published += self
            .publish(seq, airc_lib::STREAM_KIND_TEXT_TOKEN, String::new(), true)
            .await as u64;
        published
    }
}

#[cfg(test)]
mod tests;
