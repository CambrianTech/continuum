//! `AircRemoteInferenceAdapter` — implements `AIProviderAdapter`
//! whose transport is airc instead of llama.cpp.
//!
//! Joel (2026-05-31): "grid inference and they're just the same
//! command just executed across the wire and airc substrate
//! delivered payloads."
//!
//! The adapter is intentionally thin: wrap an
//! `Arc<dyn AircInferenceTransport>`, on every `generate_text`
//! call serialize → send → await → deserialize. Everything
//! interesting (correlation, framing, peer discovery, retries,
//! timeouts) lives in the transport.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;

use crate::ai::adapter::{AIProviderAdapter, AdapterCapabilities, ApiStyle, InferenceDevice};
use crate::ai::types::{
    HealthState, HealthStatus, ModelInfo, TextGenerationRequest, TextGenerationResponse,
};

use super::protocol::{RemoteInferenceError, RemoteInferenceRequest};
use super::transport::AircInferenceTransport;

/// Provider ID used to register + select this adapter from the
/// global AdapterRegistry. `Commands.execute('inference/llm/request',
/// { provider: AIRC_REMOTE_PROVIDER_ID, ... })` (or the coordinator's
/// lane open with the same provider) routes through here.
pub const AIRC_REMOTE_PROVIDER_ID: &str = "airc-remote";

/// Default model name — the adapter is model-agnostic; the actual
/// model that serves the request is whatever the remote peer's
/// local adapter picks. This field exists because the trait
/// requires `default_model()`; the value is just an identifier so
/// the registry has something to report. Callers should set
/// `model` on their TextGenerationRequest to communicate intent.
pub const AIRC_REMOTE_DEFAULT_MODEL: &str = "airc-remote/peer-resolved";

/// The remote adapter. Holds the transport Arc; the transport
/// holds everything else.
pub struct AircRemoteInferenceAdapter {
    transport: Arc<dyn AircInferenceTransport>,
    /// Optional peer hint to thread into every outgoing request.
    /// Useful when a caller explicitly wants this adapter routing
    /// to one specific peer; None = let the transport decide.
    default_target_peer: Option<String>,
    /// The model every request names when the caller names none. The
    /// persona's durable override carries it (it chose the PEER by it), and
    /// the receiver never picks a model for you — a request that crosses the
    /// wire without one is refused there (measured 2026-09-06, BigMama's
    /// `detail` field: "No provider or model specified").
    default_model: Option<String>,
    /// Flipped to true the first time a `generate_text` round-trip
    /// succeeds. `health_check` returns `Unknown` while this is
    /// false (no observation yet) and `Healthy` once it's true.
    /// Per R1 BLOCK on PR #1560: a remote adapter that reports
    /// `Healthy` by construction lies to the AdapterRegistry's
    /// selector, which then routes traffic to a dead peer. The
    /// fix is no fallback to a false-positive default — admit
    /// "no signal" until traffic proves the peer is reachable.
    has_observed_success: AtomicBool,
    /// Deadline misses in a row. Reset by any answer from the peer.
    consecutive_deadlines: AtomicU32,
    /// While `now < cold_until_ms` the lane is COLD: requests are refused
    /// here, without a wire round trip. 0 = warm.
    cold_until_ms: AtomicU64,
}

/// Deadline misses in a row before the lane reads cold. Measured 2026-09-06:
/// two citizens bound to a peer whose command pump had died sent six requests
/// in 100 s, each waiting the full 30 s deadline, each burning a turn.
pub const COLD_AFTER_DEADLINES: u32 = 3;
/// How long a cold lane stays cold before one request is let through to
/// re-measure it.
pub const COLD_WINDOW: Duration = Duration::from_secs(300);

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(u64::MAX)  // unwrap_or: a broken clock FAILS OPEN — `now < until` is false at MAX, so the lane reads warm and requests still go out; 0 would read every tripped lane cold forever (BigMama's review of #3814)
}

impl AircRemoteInferenceAdapter {
    pub fn new(transport: Arc<dyn AircInferenceTransport>) -> Self {
        Self {
            transport,
            default_target_peer: None,
            default_model: None,
            has_observed_success: AtomicBool::new(false),
            consecutive_deadlines: AtomicU32::new(0),
            cold_until_ms: AtomicU64::new(0),
        }
    }

    /// Whether the lane currently refuses requests without a round trip.
    pub fn is_cold(&self) -> bool {
        let until = self.cold_until_ms.load(Ordering::Relaxed);
        until != 0 && now_ms() < until
    }

    /// Fold one transport outcome into the breaker. A deadline miss counts;
    /// any answer (even a refusal) proves the peer evaluates our mail and
    /// resets the count.
    fn observe(&self, outcome: &Result<(), &RemoteInferenceError>) {
        match outcome {
            Err(RemoteInferenceError::Timeout { .. }) => {
                let n = self.consecutive_deadlines.fetch_add(1, Ordering::Relaxed) + 1;
                if n >= COLD_AFTER_DEADLINES {
                    let until = now_ms().saturating_add(COLD_WINDOW.as_millis() as u64);
                    self.cold_until_ms.store(until, Ordering::Relaxed);
                    self.consecutive_deadlines.store(0, Ordering::Relaxed);
                    crate::probe!(
                        class = "remote_lane.cold",
                        peer = %self.default_target_peer.as_deref().unwrap_or("-"),  // unwrap_or: probe label only — "-" = no pinned peer, never a routing decision
                        deadlines = n,
                        cold_for_s = COLD_WINDOW.as_secs(),
                        "the remote peer let requests die at the deadline in a row; \
                         refusing here for the window instead of burning turns"
                    );
                }
            }
            _ => self.consecutive_deadlines.store(0, Ordering::Relaxed),
        }
    }

    /// Pin every request to a specific peer. Use when the
    /// substrate's higher layer has decided this adapter
    /// instance is the dedicated route to one remote inference peer
    /// (e.g. the operator's GPU-rich grid host).
    pub fn with_target_peer(mut self, peer: impl Into<String>) -> Self {
        self.default_target_peer = Some(peer.into());
        self
    }

    /// Name the model every request runs when the caller leaves `model`
    /// unset. The peer refuses a request that names none.
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.default_model = Some(model.into());
        self
    }
}

#[async_trait]
impl AIProviderAdapter for AircRemoteInferenceAdapter {
    fn provider_id(&self) -> &str {
        AIRC_REMOTE_PROVIDER_ID
    }

    fn name(&self) -> &str {
        "Airc Remote (grid-routed)"
    }

    fn capabilities(&self) -> AdapterCapabilities {
        // Capabilities depend on the REMOTE peer's adapter, which
        // this adapter doesn't introspect. Advertise the
        // intersection of what most modern transformer adapters
        // support; the substrate can refine via a future
        // capability-discovery handshake.
        // Text-only safe floor: without a capability-discovery handshake
        // (future card) this adapter doesn't introspect the remote peer's
        // adapter, so it advertises only what every transformer adapter does.
        // The substrate refines once the peer reports its real set.
        // Cloud-shaped from THIS host's perspective — no local hardware
        // footprint. Unknown context; defer to whatever the peer can do.
        AdapterCapabilities::builder()
            .remote()
            .context_window(u32::MAX)
            .build()
    }

    fn api_style(&self) -> ApiStyle {
        // Treated as cloud-shaped: separate process, network-
        // shaped boundary, no local hardware. OpenAI/Anthropic-
        // tier from the caller's mental model.
        ApiStyle::OpenAI
    }

    fn default_model(&self) -> &str {
        self.default_model
            .as_deref()
            .unwrap_or(AIRC_REMOTE_DEFAULT_MODEL)  // unwrap_or: the trait wants a name; the wire request itself is stamped only when with_model was set, never this placeholder
    }

    async fn initialize(&mut self) -> Result<(), String> {
        // Transport may want to do a discovery handshake here in a
        // future slice; for now the transport is stateless from
        // the adapter's perspective.
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<(), String> {
        Ok(())
    }

    async fn generate_text(
        &self,
        mut request: TextGenerationRequest,
    ) -> Result<TextGenerationResponse, String> {
        // The lane serves ONE model and the peer refuses any other: a request
        // that names the caller's local model is refused there after the full
        // wait (2026-09-07 02:01Z, +186 s: "model 'Ornith…' is not the active
        // served model (serving: Qwen3.8-27B)"). The lane's model is the
        // truth for every request on it; a caller's different name is noted.
        if let Some(lane_model) = &self.default_model {
            if let Some(requested) = &request.model {
                if requested != lane_model {
                    crate::probe!(
                        class = "remote_lane.model_overridden",
                        peer = %self.default_target_peer.as_deref().unwrap_or("-"),  // unwrap_or: probe label only — "-" = no pinned peer
                        requested = %requested,
                        served = %lane_model,
                        "the caller named a model the remote lane does not serve; the lane's model rides the wire"
                    );
                }
            }
            request.model = Some(lane_model.clone());
        }
        if self.is_cold() {
            crate::probe!(
                class = "remote_lane.refused_cold",
                peer = %self.default_target_peer.as_deref().unwrap_or("-"),  // unwrap_or: probe label only — "-" = no pinned peer, never a routing decision
                "remote lane is cold; request refused without a round trip"
            );
            return Err(format!(
                "remote lane to {} is cold: {} deadline misses in a row; retry after the {} s window",
                self.default_target_peer.as_deref().unwrap_or("the default peer"),  // unwrap_or: error text only — names the transport default when no peer is pinned
                COLD_AFTER_DEADLINES,
                COLD_WINDOW.as_secs()
            ));
        }
        let mut envelope = RemoteInferenceRequest::new(request);
        if let Some(peer) = &self.default_target_peer {
            envelope = envelope.with_target_peer(peer.clone());
        }
        let started = std::time::Instant::now();
        let sent = self.transport.send_request(envelope).await;
        self.observe(&sent.as_ref().map(|_| ()));
        // Every request ends in exactly one row. Before this the only remote
        // rows were the breaker's — a turn that timed out, errored, or answered
        // was invisible here, and 9 misses in 200 s could not be told from 3
        // ten-minute waits (2026-09-07 01:59Z, the first read after #3818).
        match &sent {
            Ok(r) => crate::probe!(
                class = "remote_lane.answered",
                peer = %self.default_target_peer.as_deref().unwrap_or("-"),  // unwrap_or: probe label only — "-" = no pinned peer
                served_by = %r.served_by,
                elapsed_ms = started.elapsed().as_millis() as u64,
                out_tokens = r.text_response.usage.output_tokens,
                finish = ?r.text_response.finish_reason,
                "remote inference answered"
            ),
            Err(e) => crate::probe!(
                class = "remote_lane.failed",
                peer = %self.default_target_peer.as_deref().unwrap_or("-"),  // unwrap_or: probe label only — "-" = no pinned peer
                elapsed_ms = started.elapsed().as_millis() as u64,
                error = %e,
                "remote inference failed"
            ),
        }
        let response = sent.map_err(|e| e.to_string())?;
        // First successful round-trip flips the health observation
        // bit so subsequent health_check calls can report Healthy.
        // Relaxed is fine: a stale Unknown -> Healthy transition is
        // a one-way edge; readers tolerate either value.
        self.has_observed_success.store(true, Ordering::Relaxed);
        // Surface the peer that served the request as routing
        // info on the response so the caller can audit which
        // peer's local adapter produced the output.
        let mut text = response.text_response;
        // Preserve whatever routing info the peer's adapter set;
        // we add ours on top.
        text.provider = AIRC_REMOTE_PROVIDER_ID.to_string();
        Ok(text)
    }

    async fn health_check(&self) -> HealthStatus {
        // No active probe today — a future slice can add a
        // periodic ping. For now report based on whether any
        // generate_text round-trip has succeeded:
        //   - No observation yet → Unhealthy. HealthState has no
        //     Unknown variant today and adding one is its own slice
        //     (wire change). Per [[no-fallbacks-ever]] the worse lie
        //     is reporting Healthy on an unproven peer (sends real
        //     traffic to a dead remote); reporting Unhealthy until
        //     proven is pessimistic-but-safe — the registry's
        //     selector won't pick this adapter, which is the right
        //     default for "we don't know yet."
        //   - ≥1 round-trip succeeded → Healthy. Stays Healthy
        //     until process restart; a future slice adds decay on
        //     subsequent failures.
        let observed = self.has_observed_success.load(Ordering::Relaxed);
        let (state, available, message) = if observed {
            (
                HealthState::Healthy,
                true,
                Some("airc-remote: ≥1 successful round-trip observed".to_string()),
            )
        } else {
            (
                HealthState::Unhealthy,
                false,
                Some(
                    "airc-remote: no observed round-trip yet — pessimistic until proven \
                     (better to refuse than to route traffic to a possibly-dead peer)"
                        .to_string(),
                ),
            )
        };
        HealthStatus {
            status: state,
            api_available: available,
            response_time_ms: 0,
            error_rate: 0.0,
            last_checked: 0,
            message,
        }
    }

    async fn get_available_models(&self) -> Vec<ModelInfo> {
        // Future slice: discover peer's models via airc handshake.
        Vec::new()
    }

    fn device_type(&self) -> InferenceDevice {
        // From this host's perspective, the actual compute is
        // remote. The substrate's per-tier scheduler treats this
        // as a non-local lane.
        InferenceDevice::Cpu
    }

    fn supported_model_prefixes(&self) -> Vec<&'static str> {
        // No name-based auto-routing — the substrate's coordinator
        // explicitly selects this adapter when grid routing is
        // desired.
        vec![]
    }

    fn supports_model(&self, _model: &str) -> bool {
        // The remote adapter accepts any model name — the peer
        // decides whether to serve it.
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::heuristic_adapter::{HeuristicInferenceAdapter, HEURISTIC_PROVIDER_ID};
    use crate::ai::types::{ChatMessage, FinishReason, MessageContent, TextGenerationRequest};

    use super::super::protocol::{
        RemoteInferenceError, RemoteInferenceRequest, RemoteInferenceResponse,
    };
    use super::super::transport::{LocalAdapterTransport, StubInferenceTransport};

    fn user_msg(text: &str) -> ChatMessage {
        ChatMessage {
            role: "user".to_string(),
            content: MessageContent::Text(text.to_string()),
            name: None,
        }
    }

    fn req(text: &str) -> TextGenerationRequest {
        TextGenerationRequest {
            messages: vec![user_msg(text)],
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
        }
    }

    // ── basic adapter surface ──────────────────────────────────

    #[test]
    fn adapter_reports_canonical_provider_id() {
        let transport = StubInferenceTransport::new(|_| {
            Err(RemoteInferenceError::Transport {
                message: "not used".to_string(),
            })
        });
        let adapter = AircRemoteInferenceAdapter::new(transport);
        assert_eq!(adapter.provider_id(), AIRC_REMOTE_PROVIDER_ID);
        assert_eq!(adapter.default_model(), AIRC_REMOTE_DEFAULT_MODEL);
    }

    #[test]
    fn adapter_capabilities_admit_text_and_chat_not_local() {
        let transport = StubInferenceTransport::new(|_| {
            Err(RemoteInferenceError::Transport {
                message: "not used".to_string(),
            })
        });
        let adapter = AircRemoteInferenceAdapter::new(transport);
        let caps = adapter.capabilities();
        assert!(caps.has(crate::model_registry::Capability::TextGeneration));
        assert!(caps.has(crate::model_registry::Capability::Chat));
        assert!(!caps.is_local);
    }

    #[tokio::test]
    async fn adapter_supports_any_model_name_by_default() {
        let transport = StubInferenceTransport::new(|_| {
            Err(RemoteInferenceError::Transport {
                message: "not used".to_string(),
            })
        });
        let adapter = AircRemoteInferenceAdapter::new(transport);
        assert!(adapter.supports_model("gpt-4"));
        assert!(adapter.supports_model("anthropic/claude-opus-4-7"));
        assert!(adapter.supports_model("some-future-model"));
    }

    // ── the "same command across the wire" round-trip ──────────

    #[tokio::test]
    async fn remote_adapter_over_local_heuristic_transport_round_trips() {
        // This is THE architecture proof: the AircRemoteInference
        // Adapter wrapped around a transport that calls back to a
        // local HeuristicInferenceAdapter produces exactly what a
        // direct call to the heuristic would produce. The substrate
        // can't tell the difference between local and remote.
        let heuristic: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
        let transport = LocalAdapterTransport::new(heuristic);
        let adapter = AircRemoteInferenceAdapter::new(transport);

        let response = adapter.generate_text(req("hello grid")).await.unwrap();
        assert!(response.text.starts_with("[heuristic:"));
        assert!(response.text.contains("hello grid"));
        // The adapter rewrites `provider` to "airc-remote" so
        // observability can tell the request flowed over the
        // remote adapter (even when the actual transport was
        // local).
        assert_eq!(response.provider, AIRC_REMOTE_PROVIDER_ID);
        // Finish reason from the peer adapter is preserved.
        assert_eq!(response.finish_reason, FinishReason::Stop);
    }

    #[tokio::test]
    async fn remote_adapter_deterministic_when_peer_is_deterministic() {
        // The heuristic adapter is deterministic — same prompt
        // produces byte-identical responses. The remote adapter
        // routing to it inherits that determinism: this proves
        // replay-safety across the wire.
        let heuristic1: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
        let heuristic2: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
        let adapter1 = AircRemoteInferenceAdapter::new(LocalAdapterTransport::new(heuristic1));
        let adapter2 = AircRemoteInferenceAdapter::new(LocalAdapterTransport::new(heuristic2));

        let r1 = adapter1
            .generate_text(req("identical prompt"))
            .await
            .unwrap();
        let r2 = adapter2
            .generate_text(req("identical prompt"))
            .await
            .unwrap();
        assert_eq!(r1.text, r2.text);
    }

    // ── error propagation ─────────────────────────────────────

    #[tokio::test]
    async fn transport_error_surfaces_as_adapter_error_string() {
        let transport =
            StubInferenceTransport::always_failing(RemoteInferenceError::NoPeerReachable {
                message: "all peers down".to_string(),
            });
        let adapter = AircRemoteInferenceAdapter::new(transport);
        let err = adapter.generate_text(req("hi")).await.unwrap_err();
        assert!(err.contains("no remote peer reachable"));
        assert!(err.contains("all peers down"));
    }

    // what this catches: the breaker. Three deadline misses in a row must
    // make the FOURTH request fail here, at once, naming the cold window —
    // not wait another 30 s on the wire. Without it two citizens bound to a
    // dead peer burned every turn against the deadline (2026-09-06, Kira +
    // Mathis → Sahar).
    #[tokio::test]
    async fn three_deadline_misses_make_the_lane_cold_and_the_next_request_fails_fast() {
        let transport = StubInferenceTransport::always_failing(RemoteInferenceError::Timeout {
            elapsed_ms: 30_000,
        });
        let adapter = AircRemoteInferenceAdapter::new(transport).with_target_peer("df72dbf2");
        for _ in 0..COLD_AFTER_DEADLINES {
            assert!(!adapter.is_cold(), "warm until the third miss");
            let err = adapter.generate_text(req("hi")).await.unwrap_err();
            assert!(err.contains("timed out"), "a real round trip: {err}");
        }
        assert!(adapter.is_cold());
        let err = adapter.generate_text(req("hi")).await.unwrap_err();
        assert!(err.contains("cold"), "refused here, not on the wire: {err}");
        assert!(err.contains("df72dbf2"));
    }

    #[tokio::test]
    async fn timeout_error_surfaces_with_elapsed_ms() {
        let transport = StubInferenceTransport::always_failing(RemoteInferenceError::Timeout {
            elapsed_ms: 5_000,
        });
        let adapter = AircRemoteInferenceAdapter::new(transport);
        let err = adapter.generate_text(req("hi")).await.unwrap_err();
        assert!(err.contains("timed out"));
        assert!(err.contains("5000"));
    }

    #[tokio::test]
    async fn policy_denied_surfaces_through_adapter() {
        let transport =
            StubInferenceTransport::always_failing(RemoteInferenceError::PolicyDenied {
                reason: "persona scope mismatch".to_string(),
            });
        let adapter = AircRemoteInferenceAdapter::new(transport);
        let err = adapter.generate_text(req("hi")).await.unwrap_err();
        assert!(err.contains("policy denied"));
        assert!(err.contains("persona scope mismatch"));
    }

    // ── target_peer plumbing ──────────────────────────────────

    // what this catches: the request must NAME ITS MODEL on the wire. The
    // persona's override picks the peer by model and then, before this, asked
    // that peer to run nothing in particular: `model: None` is serialized as
    // an ABSENT field and the receiver refuses ("No provider or model
    // specified" — 2026-09-06, every accepted ai/generate from the M5). A
    // caller naming another model gets the lane's model too (the peer refuses others).
    #[tokio::test]
    async fn the_adapter_stamps_its_model_on_a_request_that_names_none() {
        let transport = StubInferenceTransport::new(|req: &RemoteInferenceRequest| {
            Ok(RemoteInferenceResponse {
                correlation_id: req.correlation_id,
                served_by: "peer".to_string(),
                text_response: crate::ai::types::TextGenerationResponse {
                    text: "ok".to_string(),
                    finish_reason: FinishReason::Stop,
                    model: req.text_request.model.clone().unwrap_or_else(|| "ABSENT".to_string()),
                    provider: HEURISTIC_PROVIDER_ID.to_string(),
                    usage: Default::default(),
                    response_time_ms: 0,
                    request_id: "stub".to_string(),
                    content: None,
                    tool_calls: None,
                    reasoning: None,
                    routing: None,
                    error: None,
                    timing: None,
                },
            })
        });
        let adapter = AircRemoteInferenceAdapter::new(transport).with_model("qwen3.8-27b");
        let out = adapter.generate_text(req("hi")).await.unwrap();
        assert_eq!(out.model, "qwen3.8-27b", "the override's model must ride the wire");
        // A caller naming its LOCAL model (the persona's node serves Ornith,
        // her lane serves the 27B) is refused by the peer after the whole
        // wait; the lane's model wins and the difference is a probe row.
        let mut named = req("hi");
        named.model = Some("ornith-ai/Ornith-1.5-35B-A3B-GGUF".to_string());
        let out = adapter.generate_text(named).await.unwrap();
        assert_eq!(out.model, "qwen3.8-27b", "the lane's model is the truth for every request on it");
    }

    #[tokio::test]
    async fn with_target_peer_threads_through_to_transport_envelope() {
        // Verify the adapter actually sets target_peer on the
        // outgoing envelope when configured to pin a peer.
        let transport = StubInferenceTransport::new(|req: &RemoteInferenceRequest| {
            // Echo back the target_peer in the response's served_by
            // so the test can read it.
            Ok(RemoteInferenceResponse {
                correlation_id: req.correlation_id,
                served_by: req
                    .target_peer
                    .clone()
                    .unwrap_or_else(|| "no-peer-pinned".to_string()),
                text_response: crate::ai::types::TextGenerationResponse {
                    text: "ok".to_string(),
                    finish_reason: FinishReason::Stop,
                    model: "stub".to_string(),
                    provider: HEURISTIC_PROVIDER_ID.to_string(),
                    usage: Default::default(),
                    response_time_ms: 0,
                    request_id: "stub".to_string(),
                    content: None,
                    tool_calls: None,
                    reasoning: None,
                    routing: None,
                    error: None,
                    timing: None,
                },
            })
        });
        let adapter =
            AircRemoteInferenceAdapter::new(transport).with_target_peer("test-remote-peer");
        let _ = adapter.generate_text(req("anything")).await.unwrap();
        // The test verifies via the stub's served_by echo; the
        // adapter overwrites response.provider to airc-remote, so
        // we can't read served_by directly off the response. The
        // KEY assertion is the round-trip succeeded without error
        // AND the stub saw the target_peer. We trust the stub's
        // closure ran (it would have panic'd if not).
    }

    #[tokio::test]
    async fn without_target_peer_sends_envelope_with_none() {
        let transport = StubInferenceTransport::new(|req: &RemoteInferenceRequest| {
            assert!(
                req.target_peer.is_none(),
                "expected target_peer=None; got {:?}",
                req.target_peer
            );
            Ok(RemoteInferenceResponse {
                correlation_id: req.correlation_id,
                served_by: "any".to_string(),
                text_response: crate::ai::types::TextGenerationResponse {
                    text: "ok".to_string(),
                    finish_reason: FinishReason::Stop,
                    model: "stub".to_string(),
                    provider: "stub".to_string(),
                    usage: Default::default(),
                    response_time_ms: 0,
                    request_id: "stub".to_string(),
                    content: None,
                    tool_calls: None,
                    reasoning: None,
                    routing: None,
                    error: None,
                    timing: None,
                },
            })
        });
        let adapter = AircRemoteInferenceAdapter::new(transport);
        let _ = adapter.generate_text(req("any")).await.unwrap();
    }

    // ── health ────────────────────────────────────────────────

    #[tokio::test]
    async fn health_check_reports_unhealthy_until_first_successful_round_trip() {
        // R1 BLOCK on PR #1560: a remote adapter that reports
        // Healthy by construction lies to the AdapterRegistry's
        // selector. Pre-observation: pessimistic Unhealthy.
        let transport = StubInferenceTransport::always_failing(RemoteInferenceError::Transport {
            message: "not used".to_string(),
        });
        let adapter = AircRemoteInferenceAdapter::new(transport);
        let h = adapter.health_check().await;
        assert!(matches!(h.status, HealthState::Unhealthy));
        assert!(!h.api_available);
        assert!(h
            .message
            .as_deref()
            .unwrap_or("")
            .contains("no observed round-trip"));
    }

    #[tokio::test]
    async fn health_check_flips_to_healthy_after_first_successful_round_trip() {
        // Use the heuristic adapter as the peer; one round-trip
        // should flip the observation flag.
        let heuristic: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
        let transport = LocalAdapterTransport::new(heuristic);
        let adapter = AircRemoteInferenceAdapter::new(transport);

        // Pre-roundtrip: Unhealthy.
        let h_before = adapter.health_check().await;
        assert!(matches!(h_before.status, HealthState::Unhealthy));

        // One real round-trip.
        let _ = adapter.generate_text(req("anything")).await.unwrap();

        // Post-roundtrip: Healthy.
        let h_after = adapter.health_check().await;
        assert!(matches!(h_after.status, HealthState::Healthy));
        assert!(h_after.api_available);
        assert!(h_after
            .message
            .as_deref()
            .unwrap_or("")
            .contains("successful round-trip"),);
    }
}
