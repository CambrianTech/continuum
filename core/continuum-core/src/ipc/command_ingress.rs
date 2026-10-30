//! `CommandRequestHandler`: this node's receiver for peer commands.
//!
//! A peer command reaches this node ONLY through its identity's addressed inbox
//! (`routing::addressed_inbox`): a frame airc has verified (signature, sender,
//! addressee) and typed (`AddressedKind::Command`). Room subscriptions feed perception
//! and carry no command handling at all. Incident 2026-10-30 00:28-00:38Z: an untrusted
//! peer's `persona/placement/reserve` / `ai/generate` requests published into #general
//! were executed here and fed to residents as speech, because commands were recognized
//! by a header string on room events.
//!
//! For each addressed frame the handler:
//!
//! 1. Applies the ingress contract ([`ingress_contract`]) to the verified fields and
//!    typed headers, before the payload is decoded: a `Command`, to this node, naming
//!    its command; the sender's airc trust tier meets the command's declared tier (or a
//!    presented owner-signed grant, verified against the sender's enrolled key, confers
//!    it). Each refusal is a typed [`IngressRefusal`], probed, and answered with its rule
//!    (never the params) when the frame was a command to this node.
//! 2. Decodes the [`AircCommandRequest`], which must name the header's command.
//! 3. Dispatches through the local [`CommandExecutor`] as
//!    [`CallerIdentity::airc`]`(sender)` with the verified grant capabilities; the
//!    executor's own `AuthPolicy` gate still runs.
//! 4. Replies with an [`AircCommandResponse`] bound to the request
//!    ([`AddressedPort::reply`]); an `ai/generate` streams its tokens first.
//!
//! ## Why typed errors flow through the wire
//!
//! Per [[no-fallbacks-ever]]: a Forbidden verdict on the remote substrate becomes an
//! `Error { message: "forbidden: NoPermissionForUri(...)" }` on the wire, becomes
//! `Err("forbidden: ...")` on the caller side, becomes the substrate's typed
//! `SubstrateError::Forbidden(...)` at the typed-consumer layer.

use continuum_base::time::EpochMs;
use std::sync::Arc;

use airc_core::{Body, PeerId};
use airc_lib::grid_auth::SignedCapabilityGrant;
use airc_lib::Airc;
use airc_protocol::headers_keys::HEADER_AIRC_CAPABILITY_GRANT;
use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD, Engine};
use tracing::{debug, warn};
use uuid::Uuid;

use crate::routing::addressed_inbox::{AddressedFrame, AddressedHandler, AddressedPort};
use crate::routing::grid_capability::{GrantAuthOutcome, GrantAuthorizer};
use crate::routing::{
    AircCommandRequest, AircCommandResponse, CallerIdentity, CommandUri, HEADER_COMMAND_STATUS,
};
use crate::runtime::{CommandExecutor, CommandResult};

mod ingress_contract;
mod served_generations;
mod stream_publisher;

pub use ingress_contract::{
    admit_caller, admit_frame, required_tier, AircTrustStore, IngressRefusal, PeerTierSource,
};
use stream_publisher::StreamPublisher;

/// This node's peer-command receiver. The pump hands it each addressed frame.
pub struct CommandRequestHandler {
    /// This node's airc identity as the ingress needs it: who it is, a peer's enrolled
    /// key, and the reply and token stream bound to a request.
    port: Arc<dyn AddressedPort>,
    executor: Arc<CommandExecutor>,
    /// Verifies a capability grant the caller presents (the contracted-grid path).
    /// `None` → grants are ignored; a caller is gated purely by tier. `Some` → a
    /// presented [`SignedCapabilityGrant`] is verified against the owner key + this
    /// node's mesh + the AUTHENTICATED sender key.
    grant_authorizer: Option<Arc<GrantAuthorizer>>,
    /// Generations this seat is serving concurrently ([`served_generations`]). `None` on
    /// a task's detached copy: only the pump's handler owns the registry, so a stopped
    /// pump drops it and aborts what it was serving.
    generations: Option<Arc<served_generations::ServedGenerations>>,
    /// The admission bound for served generations, read per request
    /// ([`served_generations::admission_bound`]; a test pins it).
    generation_bound: fn() -> usize,
    /// Where a caller's airc trust tier is read for the ingress contract.
    tiers: Arc<dyn PeerTierSource>,
}

/// A command the ingress contract admitted: the verified frame and its decoded request.
#[derive(Debug, Clone)]
pub struct AdmittedCommand {
    pub frame: Arc<dyn AddressedFrame>,
    pub request: AircCommandRequest,
    /// The capability tags a presented grant was VERIFIED to confer (against the
    /// authenticated sender key), threaded into the gate.
    pub verified_capabilities: Vec<String>,
    /// The requester's own id for this generation and how long it is wanted
    /// ([`continuum_airc_protocol::HEADER_GENERATE_ID`] and
    /// [`continuum_airc_protocol::HEADER_GENERATE_VALID_UNTIL_MS`]).
    pub generation: Option<GenerationStamp>,
}

impl AdmittedCommand {
    /// The transport-verified sender.
    pub fn caller(&self) -> PeerId {
        self.frame.from()
    }

    /// The request/reply pairing airc stamped; the join key on both sides of a grid turn.
    pub fn correlation(&self) -> Uuid {
        self.frame.correlation()
    }
}

/// A requester's stamp on an `ai/generate` request: its own id for the generation, and the
/// epoch ms past which it no longer waits. Both are required for generation requests;
/// invalid metadata is refused before execution, never replaced by a transport ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GenerationStamp {
    pub id: Uuid,
    pub valid_until_ms: u64,
}

impl GenerationStamp {
    fn from_headers(headers: &airc_core::Headers) -> Result<Self, IngressRefusal> {
        let id = headers
            .get(continuum_airc_protocol::HEADER_GENERATE_ID)
            .ok_or(IngressRefusal::MissingGenerationStamp)?
            .parse()
            .map_err(|_| IngressRefusal::MalformedGenerationStamp)?;
        let valid_until_ms = headers
            .get(continuum_airc_protocol::HEADER_GENERATE_VALID_UNTIL_MS)
            .ok_or(IngressRefusal::MissingGenerationStamp)?
            .parse()
            .map_err(|_| IngressRefusal::MalformedGenerationStamp)?;
        Ok(Self { id, valid_until_ms })
    }
}

/// The capability grant a caller PRESENTED in its headers (base64
/// [`HEADER_AIRC_CAPABILITY_GRANT`]), decoded. Absent is normal (a tier-gated call);
/// present-but-undecodable is refused by rule, never silently dropped.
fn presented_grant(
    headers: &airc_core::Headers,
) -> Result<Option<SignedCapabilityGrant>, IngressRefusal> {
    let Some(b64) = headers.get(HEADER_AIRC_CAPABILITY_GRANT) else {
        return Ok(None);
    };
    let bytes = STANDARD
        .decode(b64)
        .map_err(|_| IngressRefusal::MalformedGrant)?;
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|_| IngressRefusal::MalformedGrant)
}

/// The payload, decoded only after the contract admitted the frame.
fn decode_request(frame: &dyn AddressedFrame) -> Result<AircCommandRequest, IngressRefusal> {
    let Body::Json(value) = frame.body() else {
        return Err(IngressRefusal::MalformedRequest);
    };
    serde::Deserialize::deserialize(value).map_err(|_| IngressRefusal::MalformedRequest)
}

impl CommandRequestHandler {
    /// The receiver on this node's airc identity, with NO grant authorizer: a caller is
    /// gated purely by tier. Its trust tiers are read from this identity's airc trust store.
    pub fn new(airc: Arc<Airc>, executor: Arc<CommandExecutor>) -> Arc<Self> {
        let tiers = AircTrustStore::new(airc.home().to_path_buf());
        Self::with_seams(airc, tiers, executor, None)
    }

    /// The receiver that VERIFIES presented capability grants (the contracted-grid path).
    pub fn with_grant_authorizer(
        airc: Arc<Airc>,
        executor: Arc<CommandExecutor>,
        grant_authorizer: Arc<GrantAuthorizer>,
    ) -> Arc<Self> {
        let tiers = AircTrustStore::new(airc.home().to_path_buf());
        Self::with_seams(airc, tiers, executor, Some(grant_authorizer))
    }

    /// The receiver over explicit seams: the port it answers through and the tier source
    /// it reads (a test supplies fakes; no daemon needed).
    pub fn with_seams(
        port: Arc<dyn AddressedPort>,
        tiers: Arc<dyn PeerTierSource>,
        executor: Arc<CommandExecutor>,
        grant_authorizer: Option<Arc<GrantAuthorizer>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            port,
            executor,
            grant_authorizer,
            generations: Some(Arc::default()),
            generation_bound: served_generations::admission_bound,
            tiers,
        })
    }

    /// The ingress contract, then the decode: an admitted command, or the typed refusal
    /// (with the tier the sender was seen at, for the probe).
    async fn admit(
        &self,
        frame: &Arc<dyn AddressedFrame>,
    ) -> Result<(AdmittedCommand, &'static str), (String, &'static str, IngressRefusal)> {
        let path = admit_frame(frame.as_ref(), self.port.me())
            .map_err(|r| (String::new(), "unread", r))?;
        let caller = frame.from();
        let grant = presented_grant(frame.headers()).map_err(|r| (path.clone(), "unread", r))?;
        let granted = self
            .verify_presented_grant(caller, grant.as_ref(), &path)
            .await
            .map_err(|r| (path.clone(), "unread", r))?;
        let tier = self.tiers.tier_of(caller).await;
        let tier_seen = match &tier {
            Ok(Some(tier)) => tier.as_wire_str(),
            Ok(None) => "unenrolled",
            Err(_) => "unreadable",
        };
        admit_caller(tier, &path, !granted.is_empty()).map_err(|r| (path.clone(), tier_seen, r))?;
        // Only now is the payload decoded, by its one consumer; it must name the command
        // the decision was made on.
        let request = decode_request(frame.as_ref()).map_err(|r| (path.clone(), tier_seen, r))?;
        if request.path != path {
            return Err((path, tier_seen, IngressRefusal::PathMismatch));
        }
        let generation = if path == STREAMED_GENERATE_PATH {
            Some(
                GenerationStamp::from_headers(frame.headers())
                    .map_err(|r| (path.clone(), tier_seen, r))?,
            )
        } else {
            None
        };
        let admitted = AdmittedCommand {
            frame: Arc::clone(frame),
            request,
            verified_capabilities: granted,
            generation,
        };
        Ok((admitted, tier_seen))
    }

    /// [`process_request`](Self::process_request), with the tokens of an `ai/generate`
    /// on the wire AS THEY ARE PRODUCED — airc stream chunks (`airc.stream.*` headers,
    /// never a durable row) bound to the request ([`AddressedPort::stream`]),
    /// `stream_id` = the request's correlation id, then the settled response as the reply.
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
    pub async fn process_request_streaming(&self, parsed: &AdmittedCommand) -> AircCommandResponse {
        if tokio::time::Instant::now() >= parsed.frame.deadline() {
            return AircCommandResponse::error("addressed request deadline elapsed");
        }
        match tokio::time::timeout_at(parsed.frame.deadline(), self.process_with_progress(parsed))
            .await
        {
            Ok(response) => response,
            Err(_) => AircCommandResponse::error("addressed request deadline elapsed".to_string()),
        }
    }

    async fn process_with_progress(&self, parsed: &AdmittedCommand) -> AircCommandResponse {
        if parsed.request.path != STREAMED_GENERATE_PATH {
            return self.process_request(parsed).await;
        }
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let stream_id = parsed.correlation();
        let guard = crate::ai::stream_sinks::register(stream_id, tx);
        let mut streamed = parsed.clone();
        if let Some(obj) = streamed.request.params.as_object_mut() {
            obj.insert(
                crate::ai::stream_sinks::STREAM_ID_PARAM.to_string(),
                serde_json::Value::String(stream_id.to_string()),
            );
        }
        let publisher = StreamPublisher {
            port: Arc::clone(&self.port),
            request: Arc::clone(&parsed.frame),
            stream_id,
            // The seat's receipt instant: its own queue clock starts here, not on the
            // requester's node (card c84d885a, S1 — the wait is measured where it forms).
            received_at: std::time::Instant::now(),
        };
        // Both futures belong to this generation. A failed publish drops the running
        // command immediately, and cancellation drops the drain instead of detaching it.
        let mut drain = std::pin::pin!(publisher.drain(rx));
        let mut command = std::pin::pin!(self.process_request(&streamed));
        let (response, published) = tokio::select! {
            drained = &mut drain => match drained {
                Ok(published) => (command.await, published),
                Err(error) => return AircCommandResponse::error(error),
            },
            response = &mut command => {
                // Release the registered sender if the command never took it. Flush
                // the final chunk before permitting the completed response to leave.
                drop(guard);
                match drain.await {
                    Ok(published) => (response, published),
                    Err(error) => return AircCommandResponse::error(error),
                }
            }
        };
        continuum_base::probe!(
            class = "airc.command.streamed",
            path = %parsed.request.path,
            correlation = %stream_id,
            chunks = published,
            "stream published completely; the settled reply follows"
        );
        response
    }

    pub async fn process_request(&self, parsed: &AdmittedCommand) -> AircCommandResponse {
        // The capabilities the ingress contract VERIFIED the presented grant to confer
        // ride into the gate. None presented / invalid → empty caps → pure tier gating.
        let caller = CallerIdentity::airc(parsed.caller())
            .with_granted_capabilities(parsed.verified_capabilities.clone());
        Self::dispatch_request(&self.executor, parsed, caller).await
    }

    /// Verify the grant the caller presented (if any) and return the capability
    /// tags it confers — empty when there is no authorizer, no presented grant, or
    /// the grant fails to authorize.
    ///
    /// The presenting key is `port.peer_public_key(sender)` — the enrolled key from
    /// the SAME registry that signature-verified the inbound envelope, so it is the
    /// AUTHENTICATED sender's key, never the grant's self-asserted `grantee_pubkey`
    /// (the hard gate the review flagged). An absent grant uses the tier contract;
    /// a presented grant must verify or the request is refused.
    async fn verify_presented_grant(
        &self,
        caller: PeerId,
        grant: Option<&SignedCapabilityGrant>,
        path: &str,
    ) -> Result<Vec<String>, IngressRefusal> {
        let Some(grant) = grant else {
            return Ok(Vec::new());
        };
        let authorizer = self
            .grant_authorizer
            .as_ref()
            .ok_or(IngressRefusal::GrantAuthorizerUnavailable)?;
        let presenting = self
            .port
            .peer_public_key(caller)
            .ok_or(IngressRefusal::PresentingKeyUnavailable)?;
        match authorizer
            .authorize_command(grant, &presenting, path, EpochMs::now().millis())
            .await
        {
            GrantAuthOutcome::Authorized => Ok(grant.grant.capabilities.clone()),
            other => {
                debug!(caller = %caller.0, path = %path, outcome = ?other,
                    "presented capability grant refused");
                Err(IngressRefusal::GrantRejected)
            }
        }
    }

    /// Dispatch the admitted request through the executor as `caller` (whose
    /// `granted_capabilities` the gate honors).
    async fn dispatch_request(
        executor: &CommandExecutor,
        parsed: &AdmittedCommand,
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

    /// Send the typed response back, bound to the request it answers. Stamps
    /// [`HEADER_COMMAND_STATUS`] so a reader can filter without parsing the body.
    pub async fn send_reply(&self, parsed: &AdmittedCommand, response: &AircCommandResponse) {
        self.reply_to(parsed.frame.as_ref(), response).await;
    }

    async fn reply_to(&self, request: &dyn AddressedFrame, response: &AircCommandResponse) {
        let body = match serde_json::to_value(response) {
            Ok(value) => Body::Json(value),
            Err(e) => {
                warn!(correlation = %request.correlation(), error = %e, "serialize AircCommandResponse");
                return;
            }
        };
        let mut headers = airc_core::Headers::new();
        headers.insert(
            HEADER_COMMAND_STATUS.to_string(),
            response.status_header_value().to_string(),
        );
        if let Err(e) = self.port.reply(request, headers, body).await {
            continuum_base::probe!(
                class = "airc.command.reply_failed",
                correlation = %request.correlation(),
                to = %request.from().0,
                error = %e,
                "the reply to an addressed command could not be sent"
            );
        }
    }

    /// A frame the ingress contract refused: probed with the rule, the sender, the
    /// command and the tier; a command to this node is answered with the rule (never its
    /// params); a frame that was not a command to this node draws no reply.
    async fn refuse(
        &self,
        frame: &dyn AddressedFrame,
        path: &str,
        tier: &str,
        refusal: IngressRefusal,
    ) {
        continuum_base::probe!(
            class = "airc.command.ingress_refused",
            rule = refusal.rule(),
            caller = %frame.from().0,
            to = %frame.to().0,
            kind = ?frame.kind(),
            path = %path,
            tier = %tier,
            me = %self.port.me().0,
            "addressed frame refused by the ingress contract"
        );
        if refusal.answered() {
            self.reply_to(
                frame,
                &AircCommandResponse::error(refusal.reply_message(path)),
            )
            .await;
        }
    }
}

#[async_trait]
impl AddressedHandler for CommandRequestHandler {
    async fn on_addressed(&self, frame: Arc<dyn AddressedFrame>) {
        let (parsed, tier_seen) = match self.admit(&frame).await {
            Ok(admitted) => admitted,
            Err((path, tier, refusal)) => {
                return self.refuse(frame.as_ref(), &path, tier, refusal).await
            }
        };
        // THE ACCEPT PATH NAMES ITSELF: `correlation` is the join key, the same uuid the
        // CALLER awaits on, so both sides of a grid turn can be matched (2026-09-06: a
        // landed cross-grid `ai/generate` was unattributable for a day without it).
        continuum_base::probe!(
            class = "airc.command.accepted",
            path = %parsed.request.path,
            kind = %parsed.request.kind,
            caller = %parsed.caller().0,
            tier = %tier_seen,
            me = %self.port.me().0,
            correlation = %parsed.correlation(),
            "peer command ACCEPTED by the ingress contract — this node answers it"
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
                            parsed.caller(),
                            (self.generation_bound)(),
                            cancel.valid_until_ms,
                            EpochMs::now().millis(),
                        ),
                        None => served_generations::CancelOutcome::NotServing,
                    };
                    continuum_base::probe!(
                        class = "airc.command.generation_cancel",
                        generation = %cancel.generation,
                        requester = %parsed.caller().0,
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
            self.send_reply(&parsed, &response).await;
            return;
        }
        // A served generation holds a lane for minutes: it runs as its own task so the
        // pump keeps reading (card b3e5b582). Classified from the decoded request, whose
        // path the contract has already matched to its header. Over the bound it is refused
        // at once with a typed overload reply instead of queueing an unbounded population.
        if let Some(stamp) = parsed.generation {
            if let Some(generations) = &self.generations {
                let bound = (self.generation_bound)();
                // Keyed by the requester's generation id, so its cancel can name the work
                // even if it abandoned the request before airc returned the correlation.
                let key = stamp.id;
                let valid_until_ms = stamp.valid_until_ms;
                match generations.admit(
                    key,
                    parsed.caller(),
                    bound,
                    valid_until_ms,
                    EpochMs::now().millis(),
                ) {
                    Ok(admission) => {
                        let correlation = key;
                        let answerer = self.detached();
                        let task = tokio::spawn(async move {
                            let _admission = admission;
                            answerer.answer(parsed).await;
                        });
                        generations.attach(correlation, task.abort_handle());
                        continuum_base::probe!(
                            class = "airc.command.generation_dispatched",
                            correlation = %correlation,
                            outstanding = generations.outstanding() as u64,
                            bound = bound as u64,
                            "a served generation runs as its own task; the pump keeps reading"
                        );
                        return;
                    }
                    Err(refusal) => {
                        continuum_base::probe!(
                            class = "airc.command.generation_refused",
                            correlation = %parsed.correlation(),
                            caller = %parsed.caller().0,
                            reason = %refusal,
                            "a served generation was refused at admission and answered at once"
                        );
                        let response =
                            AircCommandResponse::refused(refusal.to_string(), refusal.wire());
                        self.send_reply(&parsed, &response).await;
                        return;
                    }
                }
            }
        }
        self.answer(parsed).await;
    }
}

impl CommandRequestHandler {
    /// A copy for one served generation's task: the same port, executor, grants and
    /// tiers, and no registry (only the pump's handler owns it).
    fn detached(&self) -> Self {
        Self {
            port: Arc::clone(&self.port),
            executor: Arc::clone(&self.executor),
            grant_authorizer: self.grant_authorizer.clone(),
            generations: None,
            generation_bound: self.generation_bound,
            tiers: Arc::clone(&self.tiers),
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
    async fn answer(&self, parsed: AdmittedCommand) {
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
            AircCommandResponse::Error { message, .. } => {
                ("error", message.chars().take(400).collect::<String>(), 0u64)
            }
            AircCommandResponse::Ok { result } => (
                "ok",
                String::new(),
                if measure {
                    serde_json::to_vec(result) // airc wire: measure the serialized reply payload.
                        .map(|v| v.len() as u64)
                        .unwrap_or(0) // unwrap_or: the value came FROM serde and is about to be sent; 0 means unmeasured
                } else {
                    0
                },
            ),
        };
        continuum_base::probe!(
            class = "airc.command.completed",
            path = %parsed.request.path,
            caller = %parsed.caller().0,
            me = %self.port.me().0,
            correlation = %parsed.correlation(),
            outcome = %outcome,
            detail = %detail,
            params_bytes = params_bytes,
            result_bytes = result_bytes,
            elapsed_ms = started.elapsed().as_millis() as u64,
            "peer command request answered"
        );
        self.send_reply(&parsed, &response).await;
    }
}

/// The one command whose answer is streamed on the wire, and the one a seat serves
/// concurrently: one owner for the wire literal (`continuum-airc-protocol`).
const STREAMED_GENERATE_PATH: &str = continuum_airc_protocol::COMMAND_PATH_AI_GENERATE;

#[cfg(test)]
mod tests;
