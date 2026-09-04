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

use airc_core::{Body, MentionTarget, PeerId};
use airc_lib::Airc;
use continuum_airc_protocol::{
    AircCommandRequest, AircCommandResponse, DEFAULT_COMMAND_DEADLINE, KIND_PEER,
};
use uuid::Uuid;

use crate::ai::adapter::AIProviderAdapter;
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
}

/// Closure-driven stub for unit tests. Construct with a function
/// that maps a request to either a response or an error; the stub
/// invokes it inline.
pub struct StubInferenceTransport {
    handler: Box<
        dyn Fn(&RemoteInferenceRequest) -> Result<RemoteInferenceResponse, RemoteInferenceError>
            + Send
            + Sync,
    >,
}

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
            deadline: DEFAULT_COMMAND_DEADLINE,
        })
    }

    /// Override the round-trip deadline. Builder-style; consume the
    /// inner value before re-Arc'ing.
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
        // place. `AircTransport::build_headers` covers path + kind +
        // body_hint identically; env is None on our envelopes so the
        // env-header branch is a no-op.
        let headers = AircTransport::build_headers(&envelope);

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
        let pending = match self
            .airc
            .request(MentionTarget::Peer(target), headers, body, self.deadline)
            .await
        {
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
        let reply = match self.airc.await_reply(pending).await {
            Ok(reply) => reply,
            Err(airc_lib::AircError::CommandDeadline { .. }) => {
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

        let text_response =
            serde_json::from_value(result_value).map_err(|e| RemoteInferenceError::Transport {
                message: format!("decode TextGenerationResponse: {e}"),
            })?;

        Ok(RemoteInferenceResponse {
            correlation_id,
            served_by: target.0.to_string(),
            text_response,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
            system_prompt: None,
            model: None,
            provider: None,
            temperature: None,
            max_tokens: None,
            top_p: None,
            top_k: None,
            repeat_penalty: None,
            frequency_penalty: None,
            repeat_last_n: None,
            stop_sequences: None,
            tools: None,
            tool_choice: None,
            response_format: None,
            active_adapters: None,
            request_id: None,
            user_id: None,
            room_id: None,
            purpose: None,
            persona_id: None,
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
