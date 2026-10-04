//! `AircInferenceTransport` — the trait the adapter calls to send a
//! request envelope to a remote peer and await the response.
//!
//! Three impls ship today:
//! - `AircLiveTransport` — the production transport. Wraps an
//!   `Arc<airc_lib::Airc>` + a target `PeerId` and dispatches via
//!   `Airc::request` / `await_reply`, framing the inner
//!   `TextGenerationRequest` as an `AircCommandRequest{path="ai/generate",
//!   kind=KIND_PEER}` per `continuum-airc-protocol`. The substrate's
//!   `CommandRequestHandler::parse_envelope` on the peer side accepts
//!   this wire shape unchanged.
//! - `StubInferenceTransport` — closure-driven stub for unit tests.
//! - `LocalAdapterTransport` — a "round-trip via local adapter"
//!   variant that lets a single-process test prove the
//!   AircRemoteInferenceAdapter is functionally identical to a
//!   local adapter when the transport happens to call back to a
//!   local one. This IS the "same command across the wire" proof.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

use airc_core::{Body, MentionTarget, PeerId, TranscriptEvent};
use airc_lib::Airc;
use airc_protocol::HEADER_AIRC_CORRELATION_ID;
use continuum_airc_protocol::{
    AircCommandRequest, AircCommandResponse, COMMAND_RESPONSE_BODY_HINT,
    HEADER_CONTINUUM_BODY_HINT, KIND_PEER,
};
use uuid::Uuid;

/// FLOOR on how long a remote INFERENCE request may take, end to end. This is
/// a turn, not a command: a citizen's prompt is 15k–25k tokens of prefill plus
/// a decode of up to ~3k tokens on a lane she shares with the peer's own
/// citizens — minutes, not seconds. Measured 2026-09-07 01:2xZ: every reply
/// from the 5090 arrived 35 s or more after the request, past the 30 s
/// `DEFAULT_COMMAND_DEADLINE` this transport used to inherit, so the peer did
/// the work and the requester dropped the answer, 21 times in an hour, then
/// read the peer as cold. Sized like the local generation floor.
///
/// A floor, not the bound (card ba82d0a0): a request carrying its own
/// `turn_bound` (the mind's measured expectation with headroom,
/// `inference::turn_bound`) gets `max(this, turn_bound)` — a 30k prompt on a
/// peer that prefills at ~25 tok/s is ~1200 s before its first byte, past this.
pub const REMOTE_INFERENCE_DEADLINE: Duration = Duration::from_secs(600);

use crate::ai::adapter::{AIProviderAdapter, GenerationChunk};
use crate::routing::airc_transport::AircTransport;

use super::protocol::{RemoteInferenceError, RemoteInferenceRequest, RemoteInferenceResponse};

/// Substrate command path the live transport dispatches at. Pinned
/// here (not a constructor arg) — every `AircLiveTransport` targets
/// `ai/generate` because that's the substrate's universal inference
/// entrypoint. A future streaming transport (`ai/generate/stream`) is
/// a separate type, not a config knob on this one.
const REMOTE_GENERATE_PATH: &str = "ai/generate";

/// The transport contract: take a typed envelope, return a typed
/// envelope or a typed error. All routing / correlation / framing /
/// timeout / retry logic lives inside the impl; the adapter stays
/// dumb.
///
/// `&self` so the adapter can hold an `Arc<dyn AircInferenceTransport>`
/// and call concurrently across multiple in-flight requests.
#[async_trait]
pub trait AircInferenceTransport: Send + Sync {
    async fn send_request(
        &self,
        request: RemoteInferenceRequest,
    ) -> Result<RemoteInferenceResponse, RemoteInferenceError>;

    /// [`send_request`](Self::send_request) with every chunk the responder streams
    /// delivered to `sink` the instant it arrives, and the settled response at the
    /// end. INFERENCE IS A STREAM, NOT A PROMISE: on the live wire the requester's
    /// liveness is the next chunk, not one deadline for the whole answer.
    ///
    /// No implicit whole-response substitute for a continuous channel.
    async fn send_request_streaming(
        &self,
        _request: RemoteInferenceRequest,
        _sink: crate::ai::stream_sinks::GenerationSink,
    ) -> Result<RemoteInferenceResponse, RemoteInferenceError> {
        Err(RemoteInferenceError::Transport { message: "Transport has no incremental inference wire; batch substitution is forbidden".into() })
    }
}

/// A live stream that has started and then produces NOTHING for this long is dead —
/// a decode does not pause a minute and resume. Before the first chunk the command
/// deadline governs (a request queued behind a busy slot is silent, legitimately).
pub const STREAM_IDLE_BOUND: Duration = Duration::from_secs(90);

/// Forward one wire chunk of `stream_id` to the sink. `Some(is_final)` when the
/// event was a chunk of THIS stream, `None` for anything else on the bus.
/// Missing or unsupported kinds on this stream are errors, never answer text.
fn forward_stream_chunk(
    event: &TranscriptEvent,
    stream_id: &str,
    sink: &crate::ai::stream_sinks::GenerationSink,
    expected_sequence: u64,
) -> Result<Option<bool>, String> {
    if event.headers.get(airc_lib::HEADER_STREAM_ID).map(String::as_str) != Some(stream_id) {
        return Ok(None);
    }
    let sequence = event.headers.get(airc_lib::HEADER_STREAM_SEQ)
        .ok_or_else(|| "remote stream chunk has no sequence".to_string())?
        .parse::<u64>().map_err(|_| "remote stream sequence is invalid".to_string())?;
    if sequence != expected_sequence {
        return Err(format!("remote stream continuity lost: expected sequence {expected_sequence}, received {sequence}"));
    }
    let kind = event.headers.get(airc_lib::HEADER_STREAM_KIND)
        .ok_or_else(|| "remote stream chunk has no kind".to_string())?;
    if !matches!(kind.as_str(), airc_lib::STREAM_KIND_TEXT_TOKEN | airc_lib::STREAM_KIND_TEXT_REASONING | crate::routing::command_handler::STREAM_KIND_PREFILL | crate::inference::media_wire::KIND) {
        return Err(format!("Unsupported remote stream kind '{kind}'; refusing to substitute answer text"));
    }
    let is_final = match event.headers.get(airc_lib::HEADER_STREAM_FINAL).map(String::as_str) {
        None | Some("false") => false,
        Some("true") => true,
        Some(_) => return Err("remote stream terminal marker is invalid".into()),
    };
    // Only the responder's text end marker settles this stream. Progress or
    // reasoning marked final must not authorize completion ahead of queued media.
    if is_final && kind != airc_lib::STREAM_KIND_TEXT_TOKEN {
        return Err("Remote stream terminal marker must use the text-token end kind".into());
    }
    if kind == crate::inference::media_wire::KIND {
        if is_final { return Err("Native media chunk cannot replace stream terminal marker".into()); }
        let media = crate::inference::media_wire::decode(&event.headers, event.body.as_ref())?;
        sink.send(GenerationChunk::Media(Arc::new(media)))?;
        return Ok(Some(false));
    }
    let text = match event.body.as_ref() {
        Some(body) => body.as_text().ok_or_else(|| "remote text stream carries a non-text body".to_string())?,
        None if is_final => "",
        None => return Err("remote stream chunk has no body".into()),
    };
    if !text.is_empty() {
        let chunk = match kind.as_str() {
            airc_lib::STREAM_KIND_TEXT_REASONING => GenerationChunk::Reasoning(text.to_string()),
            crate::routing::command_handler::STREAM_KIND_PREFILL => {
                let value: serde_json::Value = serde_json::from_str(text)
                    .map_err(|error| format!("invalid remote prefill frame: {error}"))?;
                let count = |field: &str| value.get(field).and_then(serde_json::Value::as_u64)
                    .ok_or_else(|| format!("invalid remote prefill field '{field}'"));
                GenerationChunk::Prefill {
                    processed: count("processed")?,
                    total: count("total")?,
                    cached: count("cached")?,
                }
            }
            _ => GenerationChunk::Token(text.to_string()),
        };
        sink.send(chunk)?;
    }
    Ok(Some(is_final))
}

/// Closure-driven stub for unit tests. Construct with a function
/// that maps a request to either a response or an error; the stub
/// invokes it inline.
#[cfg(any(test, feature = "test-fixtures"))]
pub struct StubInferenceTransport {
    handler: Box<StubInferenceHandler>,
}

#[cfg(any(test, feature = "test-fixtures"))]
type StubInferenceHandler = dyn Fn(&RemoteInferenceRequest) -> Result<RemoteInferenceResponse, RemoteInferenceError>
    + Send
    + Sync;

#[cfg(any(test, feature = "test-fixtures"))]
impl StubInferenceTransport {
    pub fn new<F>(handler: F) -> Arc<Self>
    where
        F: Fn(&RemoteInferenceRequest) -> Result<RemoteInferenceResponse, RemoteInferenceError>
            + Send
            + Sync
            + 'static,
    {
        Arc::new(Self {
            handler: Box::new(handler),
        })
    }


    /// Always-errors variant — useful for testing the adapter's
    /// error propagation paths.
    pub fn always_failing(err: RemoteInferenceError) -> Arc<Self> {
        Self::new(move |_req| Err(err.clone()))
    }
}

#[cfg(any(test, feature = "test-fixtures"))]
#[async_trait]
impl AircInferenceTransport for StubInferenceTransport {
    async fn send_request_streaming(
        &self,
        request: RemoteInferenceRequest,
        sink: crate::ai::stream_sinks::GenerationSink,
    ) -> Result<RemoteInferenceResponse, RemoteInferenceError> {
        let response = (self.handler)(&request)?;
        sink.send(GenerationChunk::Token(response.text_response.text.clone()))
            .map_err(|message| RemoteInferenceError::Transport { message })?;
        Ok(response)
    }

    async fn send_request(
        &self,
        request: RemoteInferenceRequest,
    ) -> Result<RemoteInferenceResponse, RemoteInferenceError> {
        (self.handler)(&request)
    }
}

/// "Round-trip via local adapter" transport. Used in tests and in
/// single-process configurations where the substrate wants to
/// drive the remote-adapter code path against a local model — e.g.
/// for replay-determinism testing or for proving the substrate's
/// "same command across the wire" architecture.
///
/// The transport's `send_request`:
/// 1. Extracts the `text_request` from the envelope.
/// 2. Calls `wrapped_adapter.generate_text(text_request).await`.
/// 3. Builds a `RemoteInferenceResponse` with the same
///    correlation_id + the produced `TextGenerationResponse`.
///
/// Result: the AircRemoteInferenceAdapter wrapped around this
/// transport is functionally identical to calling the wrapped
/// adapter directly — proving the architecture.
pub struct LocalAdapterTransport {
    pub adapter: Arc<dyn AIProviderAdapter>,
    pub fake_peer_id: String,
}

impl LocalAdapterTransport {
    pub fn new(adapter: Arc<dyn AIProviderAdapter>) -> Arc<Self> {
        Arc::new(Self {
            adapter,
            fake_peer_id: "local-adapter-transport".to_string(),
        })
    }

    pub fn with_peer_id(
        adapter: Arc<dyn AIProviderAdapter>,
        peer_id: impl Into<String>,
    ) -> Arc<Self> {
        Arc::new(Self {
            adapter,
            fake_peer_id: peer_id.into(),
        })
    }
}

#[async_trait]
impl AircInferenceTransport for LocalAdapterTransport {
    async fn send_request_streaming(&self, request: RemoteInferenceRequest,
        sink: crate::ai::stream_sinks::GenerationSink) -> Result<RemoteInferenceResponse, RemoteInferenceError> {
        let text_response = self.adapter.generate_stream(request.text_request, sink).await
            .map_err(|message| RemoteInferenceError::PeerAdapterFailed { message })?;
        Ok(RemoteInferenceResponse { correlation_id: request.correlation_id,
            served_by: self.fake_peer_id.clone(), text_response })
    }

    async fn send_request(
        &self,
        request: RemoteInferenceRequest,
    ) -> Result<RemoteInferenceResponse, RemoteInferenceError> {
        let text_response = self
            .adapter
            .generate_text(request.text_request)
            .await
            .map_err(|e| RemoteInferenceError::PeerAdapterFailed { message: e })?;
        Ok(RemoteInferenceResponse {
            correlation_id: request.correlation_id,
            served_by: self.fake_peer_id.clone(),
            text_response,
        })
    }
}

/// Live production transport. Holds an `Arc<airc_lib::Airc>` + a
/// default target peer + a deadline, and dispatches every
/// `send_request` through airc's request/await_reply primitive using
/// the same `AircCommandRequest` wire shape the substrate's
/// `CommandRequestHandler::parse_envelope` expects.
///
/// The transport is the ONLY place that knows about airc framing.
/// The adapter above stays oblivious to wire mechanics.
pub struct AircLiveTransport {
    airc: Arc<Airc>,
    default_target_peer: PeerId,
    deadline: Duration,
}

impl std::fmt::Debug for AircLiveTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AircLiveTransport")
            .field("default_target_peer", &self.default_target_peer)
            .field("deadline", &self.deadline)
            .finish_non_exhaustive()
    }
}

/// A `CommandDeadline` this far before the real deadline is not a deadline:
/// it is airc reporting that the per-request reply stream closed. Two seconds
/// of slack covers clock skew between the pending's absolute deadline and ours.
fn reply_stream_ended_early(elapsed: Duration, deadline: Duration) -> bool {
    elapsed + Duration::from_secs(2) < deadline
}

/// How often the store is re-read while a reply is being recovered.
const REPLY_RECOVERY_POLL: Duration = Duration::from_secs(2);
/// How far back the store is read for the reply. Replies are recent by
/// construction: they postdate a request we sent moments ago.
const REPLY_RECOVERY_PAGE: usize = 300;

/// Validate routing metadata before decoding an inference payload. A correlation
/// alone is not a receipt: another room member can observe it on the request.
struct ReplyExpectation {
    responder: PeerId,
    requester: PeerId,
    room: airc_core::RoomId,
    correlation: String,
}

impl ReplyExpectation {
    fn matches(&self, event: &TranscriptEvent) -> bool {
        event.peer_id == self.responder
            && event.room_id == self.room
            && event.target == MentionTarget::Peer(self.requester)
            && event.headers.get(HEADER_AIRC_CORRELATION_ID) == Some(&self.correlation)
            && event
                .headers
                .get(HEADER_CONTINUUM_BODY_HINT)
                .map(String::as_str)
                == Some(COMMAND_RESPONSE_BODY_HINT)
    }
}

impl AircLiveTransport {
    /// Build the live transport. `default_target_peer` is the peer
    /// every request flows to unless the inbound
    /// `RemoteInferenceRequest.target_peer` overrides it (which
    /// today's adapter never does — `with_target_peer` on the
    /// adapter only stamps a string-hint into the envelope; future
    /// adapter slices can resolve it via airc's whois store).
    pub fn new(airc: Arc<Airc>, default_target_peer: Uuid) -> Arc<Self> {
        Arc::new(Self {
            airc,
            default_target_peer: PeerId(default_target_peer),
            deadline: REMOTE_INFERENCE_DEADLINE,
        })
    }

    /// Override the round-trip deadline FLOOR (a request's own `turn_bound`
    /// still raises it). Builder-style; consume the inner value before re-Arc'ing.
    pub fn with_deadline(self, deadline: Duration) -> Arc<Self> {
        Arc::new(Self {
            airc: self.airc,
            default_target_peer: self.default_target_peer,
            deadline,
        })
    }

    /// Poll the durable store for the reply to `correlation` until the real
    /// deadline. The reply is a message addressed to us in the room the
    /// request went to; the store keeps it whether or not our live stream was
    /// open when it arrived.
    async fn recover_reply_from_store(
        &self,
        room: &airc_lib::Room,
        expected: &ReplyExpectation,
        start: std::time::Instant,
        deadline: Duration,
    ) -> Option<TranscriptEvent> {
        let remaining = deadline.checked_sub(start.elapsed())?;
        let recovery = async {
            let mut polls: u32 = 0;
            let mut tick = tokio::time::interval(REPLY_RECOVERY_POLL);
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tick.tick().await;
                polls += 1;
                if let Ok(page) = self.airc.page_recent_in(room, REPLY_RECOVERY_PAGE).await {
                    if let Some(reply) = page.into_iter().find(|e| expected.matches(e)) {
                        crate::probe!(
                            class = "remote_lane.reply_recovered",
                            correlation = %expected.correlation,
                            polls = polls,
                            elapsed_ms = start.elapsed().as_millis() as u64,
                            "reply recovered from the durable store"
                        );
                        return reply;
                    }
                }
            }
        };
        // Bound both the cadence and a stalled store call by the original
        // deadline; recovery must not acquire a second inference-sized wait.
        tokio::time::timeout(remaining, recovery).await.ok()
    }

    /// Resolve the wire-side target peer for a given envelope.
    /// Precedence: the inbound `RemoteInferenceRequest.target_peer`
    /// string (when set + parseable as UUID) wins; otherwise the
    /// transport's `default_target_peer`. An unparseable string is a
    /// loud transport error per `[[no-fallbacks-ever]]` — silently
    /// falling back to the default would mask a miswired caller.
    fn resolve_target(
        &self,
        request: &RemoteInferenceRequest,
    ) -> Result<PeerId, RemoteInferenceError> {
        match &request.target_peer {
            None => Ok(self.default_target_peer),
            Some(s) => {
                Uuid::parse_str(s)
                    .map(PeerId)
                    .map_err(|e| RemoteInferenceError::Transport {
                        message: format!(
                            "AircLiveTransport: RemoteInferenceRequest.target_peer \
                         must be a peer UUID, got {s:?}: {e}"
                        ),
                    })
            }
        }
    }
}

#[async_trait]
impl AircInferenceTransport for AircLiveTransport {
    async fn send_request(
        &self,
        request: RemoteInferenceRequest,
    ) -> Result<RemoteInferenceResponse, RemoteInferenceError> {
        // The drain: same wire, the chunks discarded (the sink is dropped, so nothing
        // is even cloned for it).
        let sink = crate::ai::stream_sinks::GenerationSink::discard();

        self.send_request_streaming(request, sink).await
    }

    async fn send_request_streaming(
        &self,
        request: RemoteInferenceRequest,
        sink: crate::ai::stream_sinks::GenerationSink,
    ) -> Result<RemoteInferenceResponse, RemoteInferenceError> {
        // Stamp the send-entry instant up-front so the eventual Timeout
        // surfaces TRUE elapsed wall-clock, not a parroted copy of the
        // deadline constant. Caught by adversarial review on PR #1593:
        // reporting `self.deadline.as_millis()` regardless of how long
        // we actually waited makes the metric a tautology — a future
        // deadline-plumbing bug that returned immediately with
        // `elapsed_ms = self.deadline` would still look "honest" in
        // probes. The honest read is `start.elapsed()`.
        let start = std::time::Instant::now();
        let target = self.resolve_target(&request)?;
        let correlation_id = request.correlation_id;
        // The deadline in force for THIS turn: the transport's floor, or the request's
        // measured bound above it (card ba82d0a0). The peer's own lane reads the same
        // field off the wire, so both ends wait out the same prefill.
        let turn_bound = request.text_request.turn_bound;
        let (deadline, deadline_source) =
            crate::inference::turn_bound::effective_bound_with_source(self.deadline, turn_bound);
        let bound_name = format!(
            "{}@{}",
            request.text_request.model.as_deref().unwrap_or("unspecified-model"), // unwrap_or: a request without a model is routed by the peer; the receipt still names the peer
            target.0
        );

        // Wire envelope: substrate's `ai/generate` handler reads
        // `params` as a `TextGenerationRequest`. RemoteInferenceRequest
        // is the transport-internal envelope; only its `text_request`
        // crosses the wire.
        let mut params = serde_json::to_value(&request.text_request).map_err(|e| { // boundary: AIRC command wire envelope carries the remote peer's ai/generate parameters.
            RemoteInferenceError::Transport {
                message: format!("serialize TextGenerationRequest: {e}"),
            }
        })?;

        if !sink.is_closed() {
            params[crate::inference::media_wire::ACCEPT_PARAM] = serde_json::json!(1);
        }
        let envelope = AircCommandRequest::new(
            REMOTE_GENERATE_PATH.to_string(),
            KIND_PEER.to_string(),
            None,
            params,
        );

        let body_value =
            serde_json::to_value(&envelope).map_err(|e| RemoteInferenceError::Transport {
                message: format!("serialize AircCommandRequest: {e}"),
            })?;
        let body = Body::Json(body_value);
        // Reuse the substrate's canonical command-header stamper per
        // R2-N1 on round 1 review: one logical decision lives in one
        // place. `AircTransport::build_headers` covers path + kind +
        // body_hint identically; env is None on our envelopes so the
        // env-header branch is a no-op.
        let headers = AircTransport::build_headers(&envelope);

        // Subscribe BEFORE the request leaves, or the first chunks race the
        // subscription and are lost. No filter: the daemon inverted a header filter
        // once (2026-09-04) and citizens heard only heartbeats; match on receive.
        // Even a text-only drain must observe stream liveness; it only discards
        // local presentation, never switches to a whole-response network path.
        let mut chunks = Some(crate::persona::airc_citizen::subscribe_every_room(&self.airc)
            .await.map_err(|e| RemoteInferenceError::Transport {
                message: format!("Inference stream subscription failed: {e}; batch substitution is forbidden"),
            })?);

        // Send-side classification: airc-lib's `request()` cannot
        // surface `CommandDeadline` (the deadline only fires while
        // waiting in `await_reply`). It CAN surface routing/setup
        // failures that semantically mean "no peer is reachable" —
        // `NoCurrentRoom`, `NotSubscribed`, `UnknownPeer` — which
        // belong in `RemoteInferenceError::NoPeerReachable` so the
        // coordinator's retry policy backs off the right way.
        // Anything else lands in `Transport { message }` per
        // [[strong-typing-across-boundaries]]: match the variant,
        // not the Display string.
        // Capture one room for dispatch AND recovery. A default-room change
        // during a long generation must not strand its durable answer.
        let dispatch = async {
            let room = self.airc.current_room().await?;
            let pending = self
                .airc
                .request_in(
                    &room,
                    MentionTarget::Peer(target),
                    headers,
                    body,
                    deadline,
                )
                .await?;
            Ok::<_, airc_lib::AircError>((room, pending))
        }
        .await;
        let (room, pending) = match dispatch {
            Ok(p) => p,
            Err(airc_lib::AircError::NoCurrentRoom)
            | Err(airc_lib::AircError::NotSubscribed(_))
            | Err(airc_lib::AircError::UnknownPeer(_))
            | Err(airc_lib::AircError::Route(_)) => {
                // `Route(_)` is airc-lib's "route resolver refused or
                // selected a route the current sender cannot execute"
                // — same semantic category as the other three: there
                // is no actionable path to the target peer right now.
                // Coordinator backoff handles all four identically.
                return Err(RemoteInferenceError::NoPeerReachable {
                    message: format!("airc.request to {target:?}: no reachable peer"),
                });
            }
            Err(other) => {
                return Err(RemoteInferenceError::Transport {
                    message: format!("airc.request to {target:?}: {other}"),
                });
            }
        };

        // Reply-side classification: airc-lib's `AircError` is a
        // typed enum with a dedicated `CommandDeadline` variant for
        // the deadline-elapsed case. The coordinator's retry policy
        // distinguishes Timeout from generic Transport errors, so we
        // classify on the VARIANT — not on the Display string —
        // per [[strong-typing-across-boundaries]].
        //
        // History: an earlier classifier substring-matched
        // `format!("{e}")` for "timeout" / "timed out" and
        // mis-classified every real deadline (airc-lib's Display is
        // "command deadline elapsed (correlation_id=…)") as
        // `RemoteInferenceError::Transport`, silently breaking the
        // coordinator's retry path. The `architecture_cross_grid_chaos`
        // Shape-4 test caught it.
        //
        // The reported `elapsed_ms` is the TRUE wall-clock since
        // `send_request` entry — not a parroted copy of the deadline
        // constant. Probes downstream (latency histograms, sentinel
        // verdicts) need the honest value.
        //
        // TODO: classify `AircError::Subscription(_)` if/when
        // await_reply starts surfacing it (today the substrate
        // pre-arms the reply_stream so this path is unreachable
        // through `Airc::request`; a future caller that bypasses
        // the pre-arm would land in the `Err(other)` catch-all and
        // get classified as Transport when NoPeerReachable would
        // be more semantically accurate).
        let pending_correlation = pending.correlation_id;
        let expected = ReplyExpectation {
            responder: target,
            requester: self.airc.peer_id(),
            room: room.channel,
            correlation: pending_correlation.to_string(),
        };
        // THE HANDLE, NOT THE PROMISE: the reply future settles the turn; the chunk
        // stream is what proves the lane alive meanwhile. A stream that started and
        // then goes silent past the idle bound is dead — no waiting out ten minutes.
        let stream_id = pending_correlation.to_string();
        // Recover durable replies inside the polled future so media keeps draining.
        // The outer select still owns cancellation, idle detection and the deadline.
        let mut reply_fut = std::pin::pin!(async {
            let reply = match self.airc.await_reply(pending).await {
                Ok(reply) if expected.matches(&reply) => reply,
                // The generic bus can return the first correlated event. Never
                // accept an unrelated responder; recover the intended answer.
                Ok(_) => self
                    .recover_reply_from_store(&room, &expected, start, deadline)
                    .await
                    .ok_or_else(|| {
                        crate::inference::turn_bound::probe_tripped(
                            "remote_deadline",
                            &bound_name,
                            deadline,
                            deadline_source,
                            turn_bound,
                        );
                        RemoteInferenceError::Timeout {
                            elapsed_ms: start.elapsed().as_millis() as u64,
                        }
                    })?,
                Err(airc_lib::AircError::CommandDeadline { .. })
                    if reply_stream_ended_early(start.elapsed(), deadline) =>
                {
                    // airc's `await_reply` reports a closed reply stream as a
                    // deadline (`Ok(None)` → `CommandDeadline`), and the per-request
                    // stream closes whenever the daemon re-subscribes this handle
                    // (a room join, a restart). Measured 2026-09-07 02:0xZ: nine
                    // "timeouts" in 200 s under a 600 s deadline, tripping the
                    // breaker three times while the peer's answers sat in the
                    // store. The reply is a durable event — recover it from there.
                    crate::probe!(
                        class = "remote_lane.reply_stream_ended",
                        peer = %target.0,
                        correlation = %pending_correlation,
                        elapsed_ms = start.elapsed().as_millis() as u64,
                        "reply stream closed before the deadline; recovering the reply from the store"
                    );
                    match self
                        .recover_reply_from_store(&room, &expected, start, deadline)
                        .await
                    {
                        Some(reply) => reply,
                        None => {
                            crate::inference::turn_bound::probe_tripped(
                                "remote_deadline",
                                &bound_name,
                                deadline,
                                deadline_source,
                                turn_bound,
                            );
                            return Err(RemoteInferenceError::Timeout {
                                elapsed_ms: start.elapsed().as_millis() as u64,
                            });
                        }
                    }
                }
                Err(airc_lib::AircError::CommandDeadline { .. }) => {
                    crate::inference::turn_bound::probe_tripped(
                        "remote_deadline",
                        &bound_name,
                        deadline,
                        deadline_source,
                        turn_bound,
                    );
                    return Err(RemoteInferenceError::Timeout {
                        elapsed_ms: start.elapsed().as_millis() as u64,
                    });
                }
                Err(other) => {
                    return Err(RemoteInferenceError::Transport {
                        message: format!("{other}"),
                    });
                }
            };
            Ok::<_, RemoteInferenceError>(reply)
        });
        let mut settled_reply = None;
        let mut stream_finished = false;
        let mut streamed = 0u64;
        let mut last_chunk_at: Option<std::time::Instant> = None;
        let reply = loop {
            // The durable reply and live stream have independent delivery queues.
            // Completion requires both; a fast reply must not discard queued chunks.
            if stream_finished {
                if let Some(reply) = settled_reply.take() { break reply; }
            }
            use futures::StreamExt;
            let idle = async {
                match last_chunk_at {
                    Some(at) => tokio::time::sleep_until((at + STREAM_IDLE_BOUND).into()).await,
                    None => std::future::pending::<()>().await,
                }
            };
            let next_chunk = async {
                match chunks.as_mut() {
                    Some(s) => s.next().await,
                    None => std::future::pending().await,
                }
            };
            tokio::select! {
                _ = sink.closed() => return Err(RemoteInferenceError::Transport { message: "Inference stream consumer cancelled".into() }),
                r = &mut reply_fut, if settled_reply.is_none() => match r {
                    Ok(reply) => {
                        if !expected.matches(&reply) {
                            return Err(RemoteInferenceError::Transport { message: "remote reply identity does not match bound request".into() });
                        }
                        if let Some(Body::Json(body)) = reply.body.as_ref() {
                            if let Ok(AircCommandResponse::Error { message }) = serde_json::from_value(body.clone()) {
                                return Err(RemoteInferenceError::PeerAdapterFailed { message });
                            }
                        }
                        settled_reply = Some(reply);
                    },
                    Err(error) => return Err(error),
                },
                _ = tokio::time::sleep_until((start + deadline).into()) => return Err(RemoteInferenceError::Timeout {
                    elapsed_ms: start.elapsed().as_millis() as u64,
                }),
                ev = next_chunk => match ev {
                    Some(Ok(event)) => {
                        if event.headers.get(airc_lib::HEADER_STREAM_ID).map(String::as_str) == Some(stream_id.as_str())
                            && (event.peer_id != expected.responder || event.room_id != expected.room)
                        {
                            return Err(RemoteInferenceError::Transport { message: "remote stream sender or room does not match bound request".into() });
                        }
                        if let Some(is_final) = forward_stream_chunk(&event, &stream_id, &sink, streamed)
                            .map_err(|message| RemoteInferenceError::Transport { message })? {
                            streamed += 1;
                            last_chunk_at = Some(std::time::Instant::now());
                            if streamed == 1 {
                                crate::probe!(
                                    class = "remote_lane.stream_started",
                                    peer = %target.0,
                                    correlation = %stream_id,
                                    first_chunk_ms = start.elapsed().as_millis() as u64,
                                    "the remote answer is arriving live"
                                );
                            }
                            if is_final {
                                // The wire is done; the settled reply is moments behind.
                                // Stop the idle clock — silence now is not death.
                                last_chunk_at = None;
                                stream_finished = true;
                                chunks = None;
                            }
                        }
                    }
                    Some(Err(lag)) => {
                        // This subscription includes every room, so shared-bus
                        // lag does not prove that THIS request lost a chunk.
                        // Keep its expected sequence unchanged: the next chunk
                        // (including the required terminal marker) proves exact
                        // continuity or fails before reaching the consumer. If
                        // no next chunk arrives, the idle/deadline owner retires
                        // the request; a settled reply alone cannot succeed.
                        crate::probe!(class = "remote_lane.shared_bus_lag",
                            peer = %target.0, correlation = %stream_id,
                            skipped = lag.skipped, expected_sequence = streamed,
                            "shared bus lagged; awaiting request sequence continuity");
                    },
                    None => return Err(RemoteInferenceError::Transport { message: "Inference stream ended without terminal marker".into() }),
                },
                _ = idle => {
                    crate::probe!(
                        class = "remote_lane.stream_stalled",
                        peer = %target.0,
                        correlation = %stream_id,
                        chunks = streamed,
                        idle_s = STREAM_IDLE_BOUND.as_secs(),
                        elapsed_ms = start.elapsed().as_millis() as u64,
                        "the remote answer started and then stopped arriving — the lane is dead, not slow"
                    );
                    return Err(RemoteInferenceError::Timeout {
                        elapsed_ms: start.elapsed().as_millis() as u64,
                    });
                }
            }
        };
        if streamed > 0 {
            crate::probe!(
                class = "remote_lane.streamed",
                peer = %target.0,
                correlation = %stream_id,
                chunks = streamed,
                elapsed_ms = start.elapsed().as_millis() as u64,
                "the remote answer arrived as chunks; settling on the reply"
            );
        }
        let responding_peer = reply.peer_id.0.to_string();
        let reply_body = reply.body.ok_or_else(|| RemoteInferenceError::Transport {
            message: "remote replied with no body".to_string(),
        })?;
        let reply_value = match reply_body {
            Body::Json(v) => v,
            Body::Binary(_) => {
                return Err(RemoteInferenceError::Transport {
                    message: "remote replied with Binary; expected Json".to_string(),
                });
            }
        };

        let response: AircCommandResponse =
            serde_json::from_value(reply_value).map_err(|e| RemoteInferenceError::Transport {
                message: format!("decode AircCommandResponse: {e}"),
            })?;

        let result_value = response
            .into_result()
            .map_err(|e| RemoteInferenceError::PeerAdapterFailed { message: e })?;

        let mut text_response: crate::ai::types::TextGenerationResponse =
            serde_json::from_value(result_value).map_err(|e| RemoteInferenceError::Transport {
                message: format!("decode TextGenerationResponse: {e}"),
            })?;

        let routing = crate::ai::types::RoutingInfo::stamp(
            &mut text_response.routing,
            "airc-remote",
            false,
            "remote_inference",
        );
        routing.remote = Some(crate::ai::types::RemoteInferenceReceipt {
            requested_peer: target.0.to_string(),
            responding_peer: responding_peer.clone(),
            correlation_id: pending_correlation.to_string(),
            elapsed_ms: start.elapsed().as_millis() as u64,
        });
        Ok(RemoteInferenceResponse {
            correlation_id,
            served_by: responding_peer,
            text_response,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use continuum_airc_protocol::DEFAULT_COMMAND_DEADLINE;

    // what this catches: actual AIRC peer framing must preserve binary media,
    // timing and stream headers, not just an in-memory TranscriptEvent.
    #[tokio::test]
    async fn native_media_crosses_two_airc_peers_before_terminal() {
        use futures::StreamExt;
        use crate::ai::stream_sinks::{channel, MediaChunk};
        use crate::inference::media_wire;
        let peers = airc_test_fixtures::TwoAircLoopback::new().await.unwrap();
        let mut events = peers.peer_b().subscribe().await.unwrap();
        let id = Uuid::new_v4().to_string();
        let media = MediaChunk {
            sequence: 0, presentation_time_us: 80_000,
            mime_type: crate::inference::native_output::PCM_MIME.into(),
            data: Arc::from([0u8, 128, 255, 127]),
        };
        let (mut headers, body) = media_wire::encode(&media).unwrap();
        headers.insert(airc_lib::HEADER_STREAM_ID.into(), id.clone());
        headers.insert(airc_lib::HEADER_STREAM_SEQ.into(), "0".into());
        headers.insert(airc_lib::HEADER_STREAM_KIND.into(), media_wire::KIND.into());
        peers.peer_a().publish_with_delivery(
            airc_lib::PublishTarget::RoomByName(peers.shared_room().into()),
            airc_protocol::FrameKind::Event, body, headers,
            airc_bus::DeliveryClass::StreamChunk,
        ).await.unwrap();
        let (sink, mut receiver) = channel();
        tokio::time::timeout(Duration::from_secs(5), async {
            while let Some(event) = events.next().await {
                let event = event.unwrap();
                if event.headers.get(airc_lib::HEADER_STREAM_ID) != Some(&id) { continue; }
                assert_eq!(event.peer_id, peers.peer_a().peer_id());
                assert_eq!(forward_stream_chunk(&event, &id, &sink, 0), Ok(Some(false)));
                assert_eq!(receiver.recv().await.unwrap(), GenerationChunk::Media(Arc::new(media)));
                return;
            }
            panic!("peer stream ended before native media arrived");
        }).await.expect("native media must arrive without waiting for terminal/reply");
    }

    // what this catches: native bytes/timing survive the AIRC event boundary,
    // while malformed frames, discontinuities and text drains cannot fake delivery.
    #[test]
    fn native_media_wire_preserves_bytes_and_refuses_invalid_delivery() {
        use crate::ai::stream_sinks::{channel, GenerationSink, MediaChunk, MAX_GENERATION_CHUNK_BYTES};
        use crate::inference::media_wire;
        let media = MediaChunk {
            sequence: 7, presentation_time_us: 125_000,
            mime_type: crate::inference::native_output::PCM_MIME.into(),
            data: Arc::from([0u8, 1, 255, 128]),
        };
        let id = Uuid::new_v4().to_string();
        let (headers, body) = media_wire::encode(&media).unwrap();
        let mut event = chunk_event(&id, media_wire::KIND, "", false);
        event.headers.extend(headers);
        event.body = Some(body);
        let (tx, mut rx) = channel();
        assert_eq!(forward_stream_chunk(&event, &id, &tx, 3), Ok(Some(false)));
        assert_eq!(rx.try_recv().unwrap(), GenerationChunk::Media(Arc::new(media.clone())));
        assert!(forward_stream_chunk(&event, &id, &GenerationSink::discard(), 3).is_err());
        assert!(forward_stream_chunk(&event, &id, &tx, 4).unwrap_err().contains("continuity"));
        let mut bad = event.clone();
        bad.body = Some(Body::text("not native bytes"));
        assert!(forward_stream_chunk(&bad, &id, &tx, 3).unwrap_err().contains("binary"));
        bad.body = Some(Body::Binary(vec![0; MAX_GENERATION_CHUNK_BYTES + 1]));
        assert!(forward_stream_chunk(&bad, &id, &tx, 3).unwrap_err().contains("budget"));
        bad = event.clone();
        bad.headers.insert(airc_lib::HEADER_STREAM_FINAL.into(), "true".into());
        assert!(forward_stream_chunk(&bad, &id, &tx, 3).unwrap_err().contains("terminal"));
        bad = event;
        bad.headers.remove("continuum.media.presentation-us");
        assert!(forward_stream_chunk(&bad, &id, &tx, 3).is_err());
        assert!(rx.try_recv().is_err());
    }

    // what this catches: an observed correlation is not authorization to answer
    // for another peer; recovery must enforce the same boundary as live replies.
    fn chunk_event(stream_id: &str, kind: &str, text: &str, is_final: bool) -> TranscriptEvent {
        let mut headers = airc_core::Headers::from([
            (airc_lib::HEADER_STREAM_ID.to_string(), stream_id.to_string()),
            (airc_lib::HEADER_STREAM_SEQ.to_string(), "3".to_string()),
            (airc_lib::HEADER_STREAM_KIND.to_string(), kind.to_string()),
        ]);
        if is_final {
            headers.insert(airc_lib::HEADER_STREAM_FINAL.to_string(), "true".to_string());
        }
        TranscriptEvent {
            event_id: airc_core::EventId::new(),
            room_id: airc_core::RoomId::new(),
            peer_id: PeerId(Uuid::new_v4()),
            client_id: airc_core::ClientId::new(),
            kind: airc_core::TranscriptKind::Message,
            occurred_at_ms: 0,
            lamport: 0,
            target: MentionTarget::All,
            headers,
            body: Some(Body::text(text.to_string())),
            attachment: None,
            receipt: None,
            metadata: serde_json::Value::Null,
        }
    }

    // what this catches (2026-09-17): a remote answer's chunks reach the requester's
    // sink AS THEY ARRIVE, demuxed on the stream id — another stream's chunk and a
    // plain message are ignored, reasoning and prefill keep their own variants (no
    // chain-of-thought leaks into the answer), and the final marker is reported so
    // the idle clock stops. Inference is a stream, not a promise.
    #[test]
    fn wire_chunks_of_this_stream_reach_the_sink_typed_and_others_are_ignored() {
        use crate::ai::adapter::GenerationChunk;
        let (tx, mut rx) = crate::ai::stream_sinks::channel();
        let id = Uuid::new_v4().to_string();
        assert_eq!(
            forward_stream_chunk(&chunk_event(&id, airc_lib::STREAM_KIND_TEXT_TOKEN, "fn ", false), &id, &tx, 3),
            Ok(Some(false))
        );
        assert_eq!(
            forward_stream_chunk(&chunk_event(&id, airc_lib::STREAM_KIND_TEXT_REASONING, "hmm", false), &id, &tx, 3),
            Ok(Some(false))
        );
        assert_eq!(
            forward_stream_chunk(
                &chunk_event(&id, crate::routing::command_handler::STREAM_KIND_PREFILL, r#"{"processed":10,"total":40,"cached":2}"#, false),
                &id,
                &tx, 3
            ),
            Ok(Some(false))
        );
        assert_eq!(
            forward_stream_chunk(&chunk_event("other-stream", airc_lib::STREAM_KIND_TEXT_TOKEN, "nope", false), &id, &tx, 3),
            Ok(None),
            "another stream's chunk is not ours"
        );
        let mut plain = chunk_event(&id, airc_lib::STREAM_KIND_TEXT_TOKEN, "x", false);
        plain.headers = airc_core::Headers::new();
        assert_eq!(forward_stream_chunk(&plain, &id, &tx, 3), Ok(None), "a plain message is not a chunk");
        // A future media/extension chunk must never become spoken answer text,
        // nor may an unsupported final marker make the stream look complete.
        for final_marker in [false, true] {
            assert!(forward_stream_chunk(
                &chunk_event(&id, "audio/encoded", "not text", final_marker), &id, &tx, 3
            ).unwrap_err().contains("Unsupported remote stream kind"));
        }
        assert_eq!(
            forward_stream_chunk(&chunk_event(&id, airc_lib::STREAM_KIND_TEXT_TOKEN, "", true), &id, &tx, 3),
            Ok(Some(true)),
            "the final marker is reported"
        );
        // Malformed frames must not become empty text or fabricated zero progress.
        for kind in [airc_lib::STREAM_KIND_TEXT_REASONING,
            crate::routing::command_handler::STREAM_KIND_PREFILL] {
            assert!(forward_stream_chunk(
                &chunk_event(&id, kind, "", true), &id, &tx, 3,
            ).unwrap_err().contains("terminal marker"),
                "progress/reasoning cannot settle an unfinished media stream");
        }
        let mut binary = chunk_event(&id, airc_lib::STREAM_KIND_TEXT_TOKEN, "", true);
        binary.body = Some(Body::Binary(vec![1, 2]));
        assert!(forward_stream_chunk(&binary, &id, &tx, 3).unwrap_err().contains("non-text"));
        let mut absent = chunk_event(&id, airc_lib::STREAM_KIND_TEXT_TOKEN, "", false);
        absent.body = None;
        assert!(forward_stream_chunk(&absent, &id, &tx, 3).unwrap_err().contains("no body"));
        for malformed in ["not json", r#"{"processed":1}"#, r#"{"processed":1,"total":2,"cached":-1}"#] {
            assert!(forward_stream_chunk(
                &chunk_event(&id, crate::routing::command_handler::STREAM_KIND_PREFILL, malformed, false),
                &id, &tx, 3,
            ).unwrap_err().contains("prefill"));
        }
        // Gaps, duplicates, missing sequence and malformed terminal markers fail before delivery.
        let valid = chunk_event(&id, airc_lib::STREAM_KIND_TEXT_TOKEN, "must not arrive", false);
        for expected_sequence in [2, 4] {
            assert!(forward_stream_chunk(&valid, &id, &tx, expected_sequence)
                .unwrap_err().contains("continuity lost"));
        }
        let mut missing = valid.clone();
        missing.headers.remove(airc_lib::HEADER_STREAM_SEQ);
        assert!(forward_stream_chunk(&missing, &id, &tx, 3).unwrap_err().contains("no sequence"));
        let mut invalid = valid;
        invalid.headers.insert(airc_lib::HEADER_STREAM_FINAL.into(), "yes".into());
        assert!(forward_stream_chunk(&invalid, &id, &tx, 3).unwrap_err().contains("terminal marker"));
        drop(tx);
        let mut got = Vec::new();
        while let Ok(c) = rx.try_recv() {
            got.push(c);
        }
        assert_eq!(
            got,
            vec![
                GenerationChunk::Token("fn ".into()),
                GenerationChunk::Reasoning("hmm".into()),
                GenerationChunk::Prefill { processed: 10, total: 40, cached: 2 },
            ]
        );
    }

    // what this catches: missing continuous transport must not invoke a batch call.
    #[tokio::test]
    async fn a_transport_without_a_wire_refuses_batch_substitution() {
        // Strip only the streaming capability from the shared fixture, so this
        // still exercises the production trait's default refusal.
        struct BatchOnly(Arc<StubInferenceTransport>);
        #[async_trait]
        impl AircInferenceTransport for BatchOnly {
            async fn send_request(&self, request: RemoteInferenceRequest)
                -> Result<RemoteInferenceResponse, RemoteInferenceError>
            {
                self.0.send_request(request).await
            }
        }
        let stub = BatchOnly(StubInferenceTransport::new(|_| panic!("batch fallback invoked")));
        let (tx, mut rx) = crate::ai::stream_sinks::channel();
        let req = RemoteInferenceRequest::new(crate::ai::types::TextGenerationRequest::default());
        assert!(stub.send_request_streaming(req, tx).await.unwrap_err().to_string().contains("batch substitution"));
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn reply_identity_room_address_and_framing_are_all_required() {
        let expected = ReplyExpectation {
            responder: PeerId(Uuid::new_v4()),
            requester: PeerId(Uuid::new_v4()),
            room: airc_core::RoomId::new(),
            correlation: Uuid::new_v4().to_string(),
        };
        let mut event = TranscriptEvent {
            event_id: airc_core::EventId::new(),
            room_id: expected.room,
            peer_id: expected.responder,
            client_id: airc_core::ClientId::new(),
            kind: airc_core::TranscriptKind::Message,
            occurred_at_ms: 0,
            lamport: 0,
            target: MentionTarget::Peer(expected.requester),
            headers: airc_core::Headers::from([
                (
                    HEADER_AIRC_CORRELATION_ID.to_string(),
                    expected.correlation.clone(),
                ),
                (
                    HEADER_CONTINUUM_BODY_HINT.to_string(),
                    COMMAND_RESPONSE_BODY_HINT.to_string(),
                ),
            ]),
            // No body required to reject an unrelated event: no payload decode.
            body: None,
            attachment: None,
            receipt: None,
            metadata: serde_json::Value::Null,
        };
        assert!(expected.matches(&event));
        event.peer_id = PeerId(Uuid::new_v4());
        assert!(!expected.matches(&event));
        event.peer_id = expected.responder;
        event.room_id = airc_core::RoomId::new();
        assert!(!expected.matches(&event));
        event.room_id = expected.room;
        event.target = MentionTarget::All;
        assert!(!expected.matches(&event));
        event.target = MentionTarget::Peer(expected.requester);
        event.headers.remove(HEADER_AIRC_CORRELATION_ID);
        assert!(!expected.matches(&event));
        event.headers.insert(
            HEADER_AIRC_CORRELATION_ID.to_string(),
            expected.correlation.clone(),
        );
        event.headers.remove(HEADER_CONTINUUM_BODY_HINT);
        assert!(!expected.matches(&event));
    }

    // what this catches: the two failures must stay distinguishable. A
    // CommandDeadline at 3 s under a 600 s deadline is a closed stream (recover
    // from the store); one at 599 s is the deadline itself (report a timeout).
    // Collapsing them either way reproduces tonight's class: nine false
    // timeouts in 200 s, or a ten-minute wait on a dead peer.
    #[test]
    fn an_early_command_deadline_is_a_closed_stream_a_late_one_is_the_deadline() {
        let d = Duration::from_secs(600);
        assert!(reply_stream_ended_early(Duration::from_secs(3), d));
        assert!(reply_stream_ended_early(Duration::from_secs(500), d));
        assert!(!reply_stream_ended_early(Duration::from_secs(599), d));
        assert!(!reply_stream_ended_early(d, d));
    }

    // what this catches: a revert to the COMMAND deadline. A command answers
    // in milliseconds; an inference answers in minutes. With 30 s here the
    // peer completes every turn and the requester drops every answer, which
    // reads exactly like a dead peer (2026-09-07: 21 wasted completions).
    #[test]
    fn the_inference_deadline_is_sized_for_a_turn_not_a_command() {
        assert!(REMOTE_INFERENCE_DEADLINE >= Duration::from_secs(300));
        assert!(REMOTE_INFERENCE_DEADLINE > DEFAULT_COMMAND_DEADLINE * 5);
    }

    // what this catches (card ba82d0a0): the constant is a FLOOR. A request whose
    // measured bound is longer (a 30k prompt on a ~25 tok/s peer: ~1200 s to the first
    // byte, ×2 headroom) waits its own bound; one with none, or a shorter one, waits
    // the floor. A revert to `self.deadline` alone kills every slow-peer turn again.
    #[test]
    fn a_request_with_a_longer_measured_bound_raises_the_remote_deadline_never_lowers_it() {
        use crate::inference::turn_bound::effective_bound;
        let long = Duration::from_secs(2_400);
        assert_eq!(effective_bound(REMOTE_INFERENCE_DEADLINE, Some(long)), long);
        assert_eq!(effective_bound(REMOTE_INFERENCE_DEADLINE, None), REMOTE_INFERENCE_DEADLINE);
        assert_eq!(
            effective_bound(REMOTE_INFERENCE_DEADLINE, Some(Duration::from_secs(30))),
            REMOTE_INFERENCE_DEADLINE
        );
    }
    use crate::ai::heuristic_adapter::HeuristicInferenceAdapter;
    use crate::ai::types::{
        ChatMessage, FinishReason, MessageContent, TextGenerationRequest, TextGenerationResponse,
        UsageMetrics,
    };
    use uuid::Uuid;

    fn req(text: &str) -> RemoteInferenceRequest {
        RemoteInferenceRequest::new(TextGenerationRequest {
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: MessageContent::Text(text.to_string()),
                name: None,
            }],
            ..Default::default()
        })
    }

    fn canned_text_response(text: &str) -> TextGenerationResponse {
        TextGenerationResponse {
            text: text.to_string(),
            finish_reason: FinishReason::Stop,
            model: "stub".to_string(),
            provider: "stub".to_string(),
            usage: UsageMetrics::default(),
            response_time_ms: 0,
            request_id: "stub".to_string(),
            content: None,
            tool_calls: None,
            reasoning: None,
            routing: None,
            error: None,
            timing: None,
        }
    }

    // ── StubInferenceTransport ───────────────────────────────────

    #[tokio::test]
    async fn stub_transport_returns_canned_response() {
        let transport = StubInferenceTransport::new(|req| {
            Ok(RemoteInferenceResponse {
                correlation_id: req.correlation_id,
                served_by: "test-peer".to_string(),
                text_response: canned_text_response("hello back"),
            })
        });
        let request = req("ping");
        let cid = request.correlation_id;
        let resp = transport.send_request(request).await.unwrap();
        assert_eq!(resp.correlation_id, cid);
        assert_eq!(resp.served_by, "test-peer");
        assert_eq!(resp.text_response.text, "hello back");
    }

    #[tokio::test]
    async fn stub_transport_can_return_typed_error() {
        let transport =
            StubInferenceTransport::always_failing(RemoteInferenceError::NoPeerReachable {
                message: "test".to_string(),
            });
        let result = transport.send_request(req("anything")).await;
        match result {
            Err(RemoteInferenceError::NoPeerReachable { message }) => {
                assert_eq!(message, "test");
            }
            other => panic!("expected NoPeerReachable, got {other:?}"),
        }
    }

    // ── LocalAdapterTransport (the architecture proof) ──────────

    #[tokio::test]
    async fn local_adapter_transport_round_trips_via_heuristic() {
        // This proves the "same command across the wire"
        // architecture: when the transport happens to call back
        // to a local adapter, the result is exactly what the local
        // adapter would have produced. The
        // AircRemoteInferenceAdapter wrapping this transport is
        // functionally identical to calling the wrapped adapter
        // directly.
        let heuristic: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
        let transport = LocalAdapterTransport::new(heuristic);
        let request = req("hello world");
        let resp = transport.send_request(request).await.unwrap();
        assert!(resp.text_response.text.starts_with("[heuristic:"));
        // The transport's fake peer_id surfaces in served_by.
        assert_eq!(resp.served_by, "local-adapter-transport");
    }

    #[tokio::test]
    async fn local_adapter_transport_propagates_peer_adapter_errors() {
        // Adapter that always errors.
        struct AlwaysFails;
        #[async_trait]
        impl AIProviderAdapter for AlwaysFails {
            fn provider_id(&self) -> &str {
                "always-fails"
            }
            fn name(&self) -> &str {
                "always-fails"
            }
            fn capabilities(&self) -> crate::ai::adapter::AdapterCapabilities {
                crate::ai::adapter::AdapterCapabilities::default()
            }
            fn api_style(&self) -> crate::ai::adapter::ApiStyle {
                crate::ai::adapter::ApiStyle::Local
            }
            fn default_model(&self) -> &str {
                "no-model"
            }
            async fn initialize(&mut self) -> Result<(), String> {
                Ok(())
            }
            async fn shutdown(&mut self) -> Result<(), String> {
                Ok(())
            }
            async fn generate_text(
                &self,
                _r: TextGenerationRequest,
            ) -> Result<TextGenerationResponse, String> {
                Err("simulated peer failure".to_string())
            }
            async fn health_check(&self) -> crate::ai::types::HealthStatus {
                crate::ai::types::HealthStatus {
                    status: crate::ai::types::HealthState::Healthy,
                    api_available: true,
                    response_time_ms: 0,
                    error_rate: 0.0,
                    last_checked: 0,
                    message: None,
                }
            }
            async fn get_available_models(&self) -> Vec<crate::ai::types::ModelInfo> {
                vec![]
            }
        }
        let failing: Arc<dyn AIProviderAdapter> = Arc::new(AlwaysFails);
        let transport = LocalAdapterTransport::new(failing);
        let result = transport.send_request(req("doomed")).await;
        match result {
            Err(RemoteInferenceError::PeerAdapterFailed { message }) => {
                assert!(message.contains("simulated peer failure"));
            }
            other => panic!("expected PeerAdapterFailed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn local_adapter_transport_preserves_correlation_id() {
        let heuristic: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
        let transport = LocalAdapterTransport::new(heuristic);
        let request = req("anything");
        let expected_cid = request.correlation_id;
        let resp = transport.send_request(request).await.unwrap();
        assert_eq!(resp.correlation_id, expected_cid);
    }

    #[tokio::test]
    async fn local_adapter_transport_with_custom_peer_id() {
        let heuristic: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
        let transport = LocalAdapterTransport::with_peer_id(heuristic, "test-remote-peer");
        let resp = transport.send_request(req("hi")).await.unwrap();
        assert_eq!(resp.served_by, "test-remote-peer");
        // Suppress the unused Uuid import warning when this test
        // doesn't construct a Uuid itself.
        let _ = Uuid::nil();
    }
}
