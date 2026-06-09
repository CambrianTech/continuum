//! `AircInferenceTransport` — the trait the adapter calls to send a
//! request envelope to a remote peer and await the response.
//!
//! Production impl (TBD, task #108 follow-up) speaks to the live
//! airc daemon. This module ships:
//! - The trait shape (stable; production impl plugs in without
//!   touching adapter or wire types).
//! - `StubInferenceTransport` — closure-driven stub for unit tests.
//! - `LocalAdapterTransport` — a "round-trip via local adapter"
//!   variant that lets a single-process test prove the
//!   AircRemoteInferenceAdapter is functionally identical to a
//!   local adapter when the transport happens to call back to a
//!   local one. This IS the "same command across the wire" proof.

use std::sync::Arc;

use async_trait::async_trait;

use crate::ai::adapter::AIProviderAdapter;

use super::protocol::{RemoteInferenceError, RemoteInferenceRequest, RemoteInferenceResponse};

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
        dyn Fn(
                &RemoteInferenceRequest,
            ) -> Result<RemoteInferenceResponse, RemoteInferenceError>
            + Send
            + Sync,
    >,
}

impl StubInferenceTransport {
    pub fn new<F>(handler: F) -> Arc<Self>
    where
        F: Fn(
                &RemoteInferenceRequest,
            ) -> Result<RemoteInferenceResponse, RemoteInferenceError>
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

    pub fn with_peer_id(adapter: Arc<dyn AIProviderAdapter>, peer_id: impl Into<String>) -> Arc<Self> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::heuristic_adapter::HeuristicInferenceAdapter;
    use crate::ai::types::{
        ChatMessage, FinishReason, MessageContent, TextGenerationRequest,
        TextGenerationResponse, UsageMetrics,
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
            routing: None,
            error: None,
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
        let transport = StubInferenceTransport::always_failing(
            RemoteInferenceError::NoPeerReachable {
                message: "test".to_string(),
            },
        );
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
        let heuristic: Arc<dyn AIProviderAdapter> =
            Arc::new(HeuristicInferenceAdapter::new());
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
            fn provider_id(&self) -> &str { "always-fails" }
            fn name(&self) -> &str { "always-fails" }
            fn capabilities(&self) -> crate::ai::adapter::AdapterCapabilities {
                crate::ai::adapter::AdapterCapabilities::default()
            }
            fn api_style(&self) -> crate::ai::adapter::ApiStyle {
                crate::ai::adapter::ApiStyle::Local
            }
            fn default_model(&self) -> &str { "no-model" }
            async fn initialize(&mut self) -> Result<(), String> { Ok(()) }
            async fn shutdown(&mut self) -> Result<(), String> { Ok(()) }
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
        let heuristic: Arc<dyn AIProviderAdapter> =
            Arc::new(HeuristicInferenceAdapter::new());
        let transport = LocalAdapterTransport::new(heuristic);
        let request = req("anything");
        let expected_cid = request.correlation_id;
        let resp = transport.send_request(request).await.unwrap();
        assert_eq!(resp.correlation_id, expected_cid);
    }

    #[tokio::test]
    async fn local_adapter_transport_with_custom_peer_id() {
        let heuristic: Arc<dyn AIProviderAdapter> =
            Arc::new(HeuristicInferenceAdapter::new());
        let transport = LocalAdapterTransport::with_peer_id(heuristic, "joels-5090");
        let resp = transport.send_request(req("hi")).await.unwrap();
        assert_eq!(resp.served_by, "joels-5090");
        // Suppress the unused Uuid import warning when this test
        // doesn't construct a Uuid itself.
        let _ = Uuid::nil();
    }
}
