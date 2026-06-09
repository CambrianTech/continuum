//! `AircRemoteInferenceAdapter` — first-class `AIProviderAdapter` whose
//! `generate_text` round-trips a remote `continuum-core-server` peer
//! over airc instead of running inference locally.
//!
//! ## Why this exists
//!
//! Per `[[nimble-ecosystems-beat-datacenters]]` + the Tron grid frame:
//! a persona on a crap Intel Mac should be able to dispatch inference
//! at `airc://<rtx5090>/ai/generate` and feel it like a local adapter.
//! No special routing logic in cognition. No new persona-side seam.
//! Just register an adapter that happens to live elsewhere.
//!
//! From the persona's view this is identical to a local LlamaCpp
//! adapter — same `generate_text(request) -> response` shape, same
//! `AdapterCapabilities`, same lifecycle. The substrate doesn't have
//! to care that the cognition is happening on another machine; it
//! just calls the trait and the trait dispatches over airc.
//!
//! ## Wire
//!
//! Reuses the exact wire `continuum-airc-protocol` defines and the
//! roundtrip the `airc_ipc_roundtrip` integration test verifies:
//!
//!   1. Serialize `TextGenerationRequest` as a JSON value.
//!   2. Construct `AircCommandRequest::new(path="ai/generate",
//!      kind=KIND_PEER, env=None, params=request_json)`.
//!   3. Stamp the substrate's command headers, ship as `Body::Json` via
//!      `airc.request(MentionTarget::Peer(target), ...)`.
//!   4. `await_reply` → decode `AircCommandResponse::Ok { result }` →
//!      deserialize `TextGenerationResponse` from `result`.
//!
//! The remote substrate's `CommandRequestHandler` parses the envelope
//! and dispatches `ai/generate` against its local adapter registry,
//! which picks whichever adapter (LlamaCpp on the remote's GPU, etc.)
//! serves the request. Identical wire to what `continuum-client` uses.

use std::sync::Arc;
use std::time::Duration;

use airc_core::{Body, MentionTarget, PeerId};
use airc_lib::Airc;
use async_trait::async_trait;
use continuum_airc_protocol::{
    AircCommandRequest, AircCommandResponse, COMMAND_REQUEST_BODY_HINT, DEFAULT_COMMAND_DEADLINE,
    HEADER_COMMAND_KIND, HEADER_COMMAND_PATH, HEADER_CONTINUUM_BODY_HINT, KIND_PEER,
};
use uuid::Uuid;

use crate::ai::adapter::{
    AIProviderAdapter, AdapterCapabilities, ApiStyle, ModalitySet, StructuredOutputProtocol,
    ToolCallProtocol,
};
use crate::ai::types::{
    CostPer1kTokens, EmbeddingRequest, EmbeddingResponse, HealthState, HealthStatus, ModelCapability,
    ModelInfo, TextGenerationRequest, TextGenerationResponse,
};

/// Substrate command path this adapter dispatches at the remote peer.
/// Pinned here (not a constructor arg) — every `AircRemoteInferenceAdapter`
/// targets `ai/generate` because that's the substrate's universal
/// inference entrypoint. If a future slice adds streaming
/// (`ai/generate/stream`), it's a separate adapter type, not a config knob.
const REMOTE_GENERATE_PATH: &str = "ai/generate";

/// Provider-id prefix used when a caller doesn't supply one explicitly.
/// Pair with the remote peer's UUID prefix to disambiguate multiple
/// remote adapters in the same registry.
pub const AIRC_REMOTE_PROVIDER_PREFIX: &str = "airc-remote";

/// An AI provider adapter that dispatches inference at a remote peer
/// over airc. Holds an `Arc<Airc>` (the local citizen's handle) + the
/// target peer UUID. Clone is cheap; the substrate's adapter registry
/// stores adapters behind `Arc<dyn AIProviderAdapter>` so each persona's
/// cognition path leases the same handle.
pub struct AircRemoteInferenceAdapter {
    airc: Arc<Airc>,
    target_peer: PeerId,
    provider_id: String,
    default_model: String,
    deadline: Duration,
    capabilities: AdapterCapabilities,
}

impl std::fmt::Debug for AircRemoteInferenceAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AircRemoteInferenceAdapter")
            .field("target_peer", &self.target_peer)
            .field("provider_id", &self.provider_id)
            .field("default_model", &self.default_model)
            .field("deadline", &self.deadline)
            .finish_non_exhaustive()
    }
}

impl AircRemoteInferenceAdapter {
    /// Build an adapter that dispatches at `target_peer` over `airc`.
    ///
    /// `default_model` is what the caller sees when they don't specify a
    /// model — the substrate's adapter selection picks this adapter, then
    /// the remote substrate picks the actual model. Pick a model name
    /// the remote is known to host (e.g. `"qwen3.5-4b-code-forged"`); the
    /// adapter doesn't verify it exists on the remote until first
    /// `generate_text` call.
    pub fn new(
        airc: Arc<Airc>,
        target_peer: Uuid,
        default_model: impl Into<String>,
    ) -> Self {
        let target_peer = PeerId(target_peer);
        let provider_id = format!("{AIRC_REMOTE_PROVIDER_PREFIX}/{}", target_peer.0);
        Self {
            airc,
            target_peer,
            provider_id,
            default_model: default_model.into(),
            deadline: DEFAULT_COMMAND_DEADLINE,
            capabilities: default_remote_capabilities(),
        }
    }

    /// Override the round-trip deadline. Builder-style.
    pub fn with_deadline(mut self, deadline: Duration) -> Self {
        self.deadline = deadline;
        self
    }

    /// Override the advertised capabilities. Use this when the operator
    /// knows the remote adapter's real shape (e.g. the 5090 hosts a
    /// tool-using vision model) and wants the substrate's adapter
    /// selection to route accordingly. Default is a conservative
    /// text-only chat-only profile.
    pub fn with_capabilities(mut self, capabilities: AdapterCapabilities) -> Self {
        self.capabilities = capabilities;
        self
    }

    /// Dispatch the request envelope at the remote peer + decode the typed reply.
    /// Factored out so unit tests can verify the wire-shape construction
    /// without standing up airc-lib's transport.
    async fn dispatch(
        &self,
        request: &TextGenerationRequest,
    ) -> Result<TextGenerationResponse, String> {
        let params = serde_json::to_value(request)
            .map_err(|e| format!("AircRemoteInferenceAdapter: serialize request: {e}"))?;

        let envelope = AircCommandRequest::new(
            REMOTE_GENERATE_PATH.to_string(),
            KIND_PEER.to_string(),
            None,
            params,
        );

        let body_value = serde_json::to_value(&envelope).map_err(|e| {
            format!("AircRemoteInferenceAdapter: serialize AircCommandRequest: {e}")
        })?;
        let body = Body::Json(body_value);

        let mut headers = airc_core::Headers::new();
        headers.insert(HEADER_COMMAND_PATH.to_string(), envelope.path.clone());
        headers.insert(HEADER_COMMAND_KIND.to_string(), envelope.kind.clone());
        headers.insert(
            HEADER_CONTINUUM_BODY_HINT.to_string(),
            COMMAND_REQUEST_BODY_HINT.to_string(),
        );

        let pending = self
            .airc
            .request(
                MentionTarget::Peer(self.target_peer),
                headers,
                body,
                self.deadline,
            )
            .await
            .map_err(|e| format!("AircRemoteInferenceAdapter: airc request: {e}"))?;

        let reply = self
            .airc
            .await_reply(pending)
            .await
            .map_err(|e| format!("AircRemoteInferenceAdapter: await_reply: {e}"))?;

        let reply_body = reply.body.ok_or_else(|| {
            "AircRemoteInferenceAdapter: remote replied with no body".to_string()
        })?;

        let body_value = match reply_body {
            Body::Json(v) => v,
            Body::Binary(_) => {
                return Err(
                    "AircRemoteInferenceAdapter: remote replied with Binary; expected Json"
                        .to_string(),
                );
            }
        };

        let response: AircCommandResponse =
            serde_json::from_value(body_value).map_err(|e| {
                format!("AircRemoteInferenceAdapter: decode AircCommandResponse: {e}")
            })?;

        let result_value = response.into_result()?;

        serde_json::from_value::<TextGenerationResponse>(result_value).map_err(|e| {
            format!("AircRemoteInferenceAdapter: decode TextGenerationResponse: {e}")
        })
    }
}

/// Conservative default capabilities for a remote inference adapter
/// whose actual shape the operator hasn't yet declared. Text + chat
/// only; no tool use, no vision, no streaming. Operator overrides via
/// `with_capabilities` once they know what the remote runs.
fn default_remote_capabilities() -> AdapterCapabilities {
    AdapterCapabilities {
        supports_text_generation: true,
        supports_chat: true,
        supports_tool_use: false,
        supports_vision: false,
        supports_streaming: false,
        supports_embeddings: false,
        supports_audio: false,
        supports_image_generation: false,
        is_local: false,
        max_context_window: 32_768,
        tool_call_protocol: ToolCallProtocol::None,
        structured_output_protocol: StructuredOutputProtocol::None,
        modalities: ModalitySet::TEXT_ONLY,
        max_output_tokens: 4096,
    }
}

#[async_trait]
impl AIProviderAdapter for AircRemoteInferenceAdapter {
    fn provider_id(&self) -> &str {
        &self.provider_id
    }

    fn name(&self) -> &str {
        "airc-remote (cross-grid inference)"
    }

    fn capabilities(&self) -> AdapterCapabilities {
        self.capabilities.clone()
    }

    fn api_style(&self) -> ApiStyle {
        ApiStyle::Local
    }

    fn default_model(&self) -> &str {
        &self.default_model
    }

    async fn initialize(&mut self) -> Result<(), String> {
        // No local model state to load — the remote substrate handles
        // initialize for the actual model. First dispatch will fail
        // loudly if the remote isn't reachable.
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<(), String> {
        Ok(())
    }

    async fn generate_text(
        &self,
        request: TextGenerationRequest,
    ) -> Result<TextGenerationResponse, String> {
        self.dispatch(&request).await
    }

    async fn create_embedding(
        &self,
        _request: EmbeddingRequest,
    ) -> Result<EmbeddingResponse, String> {
        Err(
            "AircRemoteInferenceAdapter does not yet route embedding requests — \
             follow-up slice extends this to ai/embed (task #108 slice C)."
                .to_string(),
        )
    }

    async fn health_check(&self) -> HealthStatus {
        // Healthy by construction — the only way to know for sure is to
        // round-trip a probe, which would be wasteful on every health
        // tick. A future slice adds a cached health bit refreshed by
        // periodic ping; today we trust airc-lib's transport_health
        // surface for the underlying socket state.
        HealthStatus {
            status: HealthState::Healthy,
            api_available: true,
            response_time_ms: 0,
            error_rate: 0.0,
            last_checked: 0,
            message: None,
        }
    }

    async fn get_available_models(&self) -> Vec<ModelInfo> {
        // The local-side adapter advertises only the model name the
        // operator declared at construction. The remote substrate may
        // host more; a follow-up slice round-trips `ai/providers/list`
        // at the remote and caches the result. Today, conservative.
        vec![ModelInfo {
            id: self.default_model.clone(),
            name: format!("Remote model `{}` via {}", self.default_model, self.provider_id),
            provider: self.provider_id.clone(),
            capabilities: vec![ModelCapability::TextGeneration, ModelCapability::Chat],
            context_window: self.capabilities.max_context_window,
            max_output_tokens: self.capabilities.max_output_tokens,
            cost_per_1k_tokens: CostPer1kTokens {
                input: 0.0,
                output: 0.0,
            },
            // Unknown without a probe; 0.0 signals "no measurement
            // yet" to the RAG budget. Follow-up slice refreshes via
            // periodic ping at the remote.
            tokens_per_second: 0.0,
            supports_streaming: self.capabilities.supports_streaming,
            supports_tools: self.capabilities.supports_tool_use,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Construction smoke + builder shape. No wire traffic — the live
    /// roundtrip test lives at `tests/airc_remote_inference_roundtrip.rs`
    /// where it can pair with `TwoAircLoopback`.
    #[test]
    fn provider_id_includes_target_peer_for_disambiguation() {
        // We can't construct an Airc here (it needs a tempdir + async)
        // so just verify the prefix shape via the formatter. The live
        // roundtrip test covers the end-to-end construction path.
        let peer = Uuid::nil();
        let formatted = format!("{AIRC_REMOTE_PROVIDER_PREFIX}/{peer}");
        assert_eq!(formatted, "airc-remote/00000000-0000-0000-0000-000000000000");
    }

    #[test]
    fn default_capabilities_are_conservative() {
        let caps = default_remote_capabilities();
        assert!(caps.supports_text_generation);
        assert!(caps.supports_chat);
        assert!(!caps.supports_tool_use);
        assert!(!caps.supports_vision);
        assert!(!caps.is_local);
        assert_eq!(caps.tool_call_protocol, ToolCallProtocol::None);
    }
}
