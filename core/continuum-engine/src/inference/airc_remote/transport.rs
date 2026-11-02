//! `AircInferenceTransport` — the trait the adapter calls to send a
//! request envelope to a remote peer and await the response.
//!
//! Three impls ship today:
//! - `AircLiveTransport` — the production transport. Wraps an
//!   `Arc<airc_lib::Airc>` + a target `PeerId` and dispatches via
//!   `Airc::request` / `await_reply`, framing the inner
//!   `TextGenerationRequest` as an `AircCommandRequest{path="ai/generate",
//!   kind=KIND_PEER}` per `continuum-airc-protocol`, sent as airc's typed
//!   `Command` frame; the seat's addressed inbox receives it and its
//!   ingress contract decides it.
//! - `StubInferenceTransport` — closure-driven stub for unit tests.
//! - `LocalAdapterTransport` — a "round-trip via local adapter"
//!   variant that lets a single-process test prove the
//!   AircRemoteInferenceAdapter is functionally identical to a
//!   local adapter when the transport happens to call back to a
//!   local one. This IS the "same command across the wire" proof.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

use airc_core::{Body, MentionTarget, PeerId};
use airc_lib::{Airc, CommandEvent};

mod progress;
use continuum_airc_protocol::{AircCommandRequest, AircCommandResponse, KIND_PEER};
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
    /// Default: a transport with no live wire (the stub, the local-adapter loopback)
    /// has nothing to stream — it answers whole and honestly emits that as a single
    /// trailing chunk, the same capability statement `AIProviderAdapter::generate_stream`
    /// makes for a one-shot backend.
    async fn send_request_streaming(
        &self,
        request: RemoteInferenceRequest,
        sink: tokio::sync::mpsc::UnboundedSender<GenerationChunk>,
    ) -> Result<RemoteInferenceResponse, RemoteInferenceError> {
        let response = self.send_request(request).await?;
        if !sink.is_closed() {
            if let Some(r) = response
                .text_response
                .reasoning
                .as_ref()
                .filter(|r| !r.is_empty())
            {
                let _ = sink.send(GenerationChunk::Reasoning(r.clone()));
            }
            if !response.text_response.text.is_empty() {
                let _ = sink.send(GenerationChunk::Token(response.text_response.text.clone()));
            }
        }
        Ok(response)
    }
}

/// A live stream that has started and then produces NOTHING for this long is dead —
/// a decode does not pause a minute and resume. Before the first chunk the command
/// deadline governs (a request queued behind a busy slot is silent, legitimately).
pub const STREAM_IDLE_BOUND: Duration = Duration::from_secs(90);

/// Closure-driven stub for unit tests. Construct with a function
/// that maps a request to either a response or an error; the stub
/// invokes it inline.
pub struct StubInferenceTransport {
    handler: Box<StubInferenceHandler>,
}

type StubInferenceHandler = dyn Fn(&RemoteInferenceRequest) -> Result<RemoteInferenceResponse, RemoteInferenceError>
    + Send
    + Sync;

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

#[async_trait]
impl AircInferenceTransport for StubInferenceTransport {
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
}

#[async_trait]
impl AircInferenceTransport for LocalAdapterTransport {
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

/// PURE: the seat's reply as the requester acts on it. A typed overload refusal (card
/// 03660a77) is [`RemoteInferenceError::SeatOverloaded`], so her next act goes home instead
/// of asking the full seat again; every other error stays the peer's failure, by its text.
pub(super) fn reply_result(
    response: AircCommandResponse,
) -> Result<serde_json::Value, RemoteInferenceError> {
    let overloaded =
        response.refusal() == Some(continuum_airc_protocol::GenerateRefusal::Overloaded);
    response.into_result().map_err(|message| {
        if overloaded {
            RemoteInferenceError::SeatOverloaded { message }
        } else {
            RemoteInferenceError::PeerAdapterFailed { message }
        }
    })
}

// Preserve a typed refusal even if progress ended abruptly; only successful work
// claims a complete stream and therefore must carry its final marker.
fn settle_response(
    response: AircCommandResponse,
    progress: &progress::Progress,
) -> Result<serde_json::Value, RemoteInferenceError> {
    let result = reply_result(response)?;
    progress.complete()?;
    Ok(result)
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
        self.exchange(request, None).await
    }

    async fn send_request_streaming(
        &self,
        request: RemoteInferenceRequest,
        sink: tokio::sync::mpsc::UnboundedSender<GenerationChunk>,
    ) -> Result<RemoteInferenceResponse, RemoteInferenceError> {
        self.exchange(request, Some(sink)).await
    }
}

impl AircLiveTransport {
    async fn exchange(
        &self,
        request: RemoteInferenceRequest,
        sink: Option<tokio::sync::mpsc::UnboundedSender<GenerationChunk>>,
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
        // ...and the SEAT's own measured service time above both (card b3e5b582): a slow
        // seat's beacon says how long this decode takes there, so the requester waits for
        // the answer it asked for instead of abandoning it at a floor sized for fast seats.
        let seat_estimate = super::seat_deadline::heard_seat_offer(
            target.0,
            request.text_request.model.as_deref(),
            continuum_base::time::EpochMs::now().millis(),
        )
        .and_then(|offer| {
            super::seat_deadline::seat_service_estimate(&offer, request.text_request.max_tokens)
        });
        let (deadline, deadline_source) = super::seat_deadline::raise_to_seat(
            crate::inference::turn_bound::effective_bound_with_source(self.deadline, turn_bound),
            seat_estimate,
        );
        if deadline_source == crate::inference::turn_bound::BoundSource::SeatMeasured {
            continuum_base::probe!(
                class = "remote_lane.deadline_from_seat",
                peer = %target.0,
                deadline_s = deadline.as_secs(),
                max_tokens = u64::from(request.text_request.max_tokens.unwrap_or(0)), // probe field: 0 = no cap stated
                "the seat's measured lane wait plus decode raised this request's deadline above the floor and the turn bound"
            );
        }
        let bound_name = format!(
            "{}@{}",
            request
                .text_request
                .model
                .as_deref()
                .unwrap_or("unspecified-model"), // unwrap_or: a request without a model is routed by the peer; the receipt still names the peer
            target.0
        );

        // Wire envelope: substrate's `ai/generate` handler reads
        // `params` as a `TextGenerationRequest`. RemoteInferenceRequest
        // is the transport-internal envelope; only its `text_request`
        // crosses the wire.
        let params = serde_json::to_value(&request.text_request).map_err(|e| {
            RemoteInferenceError::Transport {
                message: format!("serialize TextGenerationRequest: {e}"),
            }
        })?;

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
        // place. `AircTransport::build_headers` covers path + kind;
        // env is None on our envelopes so the env-header branch is a no-op.
        let mut headers = AircTransport::build_headers(&envelope);
        // The requester's own id for this generation, on the wire BEFORE publication: the
        // seat keys the served work by it, so a cancel can name it even if this turn is
        // dropped while airc is still publishing (card 83f774fb).
        headers.insert(
            continuum_airc_protocol::HEADER_GENERATE_ID.to_string(),
            correlation_id.to_string(),
        );
        // ...and how long it is wanted: past this the seat refuses it, so a frame delayed
        // in transit never starts work nobody waits for, and the cancel carries the same
        // value so the seat remembers an early cancel exactly that long.
        let valid_until_ms = continuum_base::time::EpochMs::now()
            .millis()
            .saturating_add(deadline.as_millis() as u64);
        headers.insert(
            continuum_airc_protocol::HEADER_GENERATE_VALID_UNTIL_MS.to_string(),
            valid_until_ms.to_string(),
        );
        // Arm cancellation before the sealed request leaves. Its prearmed inbox owns
        // progress and completion together; no room subscription can stand in for it.
        let abandon = super::abandon::AbandonGuard::new(
            Arc::clone(&self.airc),
            target,
            correlation_id,
            valid_until_ms,
        );
        let (mut pending, abandon) = super::abandon::publish_armed(
            abandon,
            self.airc
                .request(MentionTarget::Peer(target), headers, body, deadline),
        )
        .await
        .map_err(|error| match error {
            airc_lib::AircError::CommandDeadline { .. } => RemoteInferenceError::Timeout {
                elapsed_ms: start.elapsed().as_millis() as u64,
            },
            airc_lib::AircError::UnknownPeer(_) | airc_lib::AircError::Route(_) => {
                RemoteInferenceError::NoPeerReachable {
                    message: error.to_string(),
                }
            }
            other => RemoteInferenceError::Transport {
                message: other.to_string(),
            },
        })?;

        let pending_correlation = pending.correlation_id;
        let stream_id = pending_correlation.to_string();
        let mut progress = progress::Progress::default();
        let mut last_chunk_at: Option<std::time::Instant> = None;
        let reply = loop {
            let idle = async {
                match last_chunk_at {
                    Some(at) => tokio::time::sleep_until((at + STREAM_IDLE_BOUND).into()).await,
                    None => std::future::pending::<()>().await,
                }
            };
            let consumer_closed = async {
                match sink.as_ref() {
                    Some(sink) => sink.closed().await,
                    None => std::future::pending::<()>().await,
                }
            };
            let event = tokio::select! {
                event = self.airc.next_command_event(&mut pending) => event,
                _ = consumer_closed => return Err(RemoteInferenceError::Transport {
                    message: "remote inference progress consumer closed".into(),
                }),
                _ = idle => return Err(RemoteInferenceError::Timeout {
                    elapsed_ms: start.elapsed().as_millis() as u64,
                }),
            };
            match event {
                Ok(CommandEvent::Progress(frame)) => {
                    let is_final = progress.forward(
                        frame.headers(),
                        frame.body(),
                        &stream_id,
                        sink.as_ref(),
                    )?;
                    last_chunk_at = if is_final {
                        None
                    } else {
                        Some(std::time::Instant::now())
                    };
                }
                Ok(CommandEvent::Completed(frame)) => {
                    break frame;
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
                Err(error) => {
                    return Err(RemoteInferenceError::Transport {
                        message: error.to_string(),
                    })
                }
            }
        };
        // The seat answered: its work is done, there is nothing left to cancel.
        abandon.disarm();

        let responding_peer = reply.from().0.to_string();
        let reply_body = reply.body().clone();
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

        let result_value = settle_response(response, &progress)?;

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
mod tests;
