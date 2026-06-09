//! Integration test: `AircRemoteInferenceAdapter` round-trips a
//! `TextGenerationRequest` against the substrate's actual
//! `CommandRequestHandler::parse_envelope` + `send_reply` paths
//! (mirroring `ai/generate` on the remote substrate).
//!
//! This is the live wire proof that an Intel Mac persona can dispatch
//! inference at `airc://<rtx5090>/ai/generate` and get a typed response
//! back, with the substrate's real parser in the loop on both ends.
//!
//! ## Topology
//!
//! - peer_a = "the 5090" — substrate, hosts ai/generate. Test stubs the
//!   responder so we control the canned `TextGenerationResponse` and
//!   can assert the request's wire shape (path, kind, params) that
//!   AircRemoteInferenceAdapter emits.
//! - peer_b = "the Intel Mac" — has an AircRemoteInferenceAdapter
//!   pointed at peer_a. Persona-side code (well, the test) calls
//!   adapter.generate_text(request).
//!
//! ## What this proves
//!
//! - `AircRemoteInferenceAdapter::generate_text` serializes the request
//!   correctly per `continuum-airc-protocol`.
//! - The wire envelope passes through airc-lib framing + LAN +
//!   correlation_id stamping intact.
//! - Substrate's `CommandRequestHandler::parse_envelope` accepts the
//!   envelope (kind = KIND_PEER, path = "ai/generate", params decode as
//!   TextGenerationRequest).
//! - Substrate's `send_reply` ships an `AircCommandResponse::Ok`
//!   carrying a serialized `TextGenerationResponse`.
//! - The client adapter decodes the response and returns a typed
//!   `TextGenerationResponse` — what every `AIProviderAdapter` consumer
//!   expects.

use std::sync::Arc;
use std::time::Duration;

use airc_test_fixtures::TwoAircLoopback;
use continuum_airc_protocol::{
    AircCommandResponse, COMMAND_REQUEST_BODY_HINT, HEADER_CONTINUUM_BODY_HINT, KIND_PEER,
};
use continuum_core::ai::adapter::AIProviderAdapter;
use continuum_core::ai::airc_remote_adapter::AircRemoteInferenceAdapter;
use continuum_core::ai::types::{
    ChatMessage, FinishReason, MessageContent, TextGenerationRequest, TextGenerationResponse,
    UsageMetrics,
};
use continuum_core::routing::CommandRequestHandler;
use continuum_core::runtime::command_executor::CommandExecutor;
use continuum_core::runtime::ModuleRegistry;
use futures::stream::StreamExt;

fn build_handler(peer_a: Arc<airc_lib::Airc>) -> Arc<CommandRequestHandler> {
    let registry = Arc::new(ModuleRegistry::new());
    let executor = Arc::new(CommandExecutor::new(registry));
    CommandRequestHandler::new(peer_a, executor)
}

/// Substrate-equivalent responder for `ai/generate`. Uses the
/// substrate's REAL `parse_envelope` + `send_reply` paths so this test
/// catches drift between AircRemoteInferenceAdapter's wire envelope
/// and what the substrate accepts.
async fn spawn_ai_generate_responder(
    handler: Arc<CommandRequestHandler>,
    peer_a: Arc<airc_lib::Airc>,
    canned: TextGenerationResponse,
) -> tokio::task::JoinHandle<()> {
    let self_id = peer_a.peer_id();
    tokio::spawn(async move {
        let mut stream = peer_a.subscribe().await.expect("peer_a subscribe");
        while let Some(event) = stream.next().await {
            let event = match event {
                Ok(e) => e,
                Err(_) => continue,
            };
            if event.peer_id == self_id {
                continue;
            }
            let hint = match event.headers.get(HEADER_CONTINUUM_BODY_HINT) {
                Some(h) => h,
                None => continue,
            };
            if hint != COMMAND_REQUEST_BODY_HINT {
                continue;
            }

            // Substrate's actual parser.
            let parsed = match CommandRequestHandler::parse_envelope(&event) {
                Ok(p) => p,
                Err(e) => panic!("substrate parse_envelope rejected envelope: {e:?}"),
            };

            // Wire-shape assertions: AircRemoteInferenceAdapter is
            // expected to set kind=KIND_PEER and path="ai/generate"
            // with no env. If any drifts, the test fails here.
            assert_eq!(
                parsed.request.kind, KIND_PEER,
                "remote inference dispatch must be kind=peer"
            );
            assert_eq!(
                parsed.request.path, "ai/generate",
                "remote inference must target ai/generate"
            );
            assert_eq!(parsed.request.env, None);

            // Params should decode as the typed request, not just JSON.
            // Mirror what the substrate's ai_provider.rs does at the
            // beginning of handling ai/generate.
            let decoded: TextGenerationRequest = serde_json::from_value(parsed.request.params.clone())
                .expect("params decode as TextGenerationRequest");
            // Sanity: the messages survived the round-trip.
            assert!(!decoded.messages.is_empty(), "non-empty messages");

            // Canned response — the test pins the wire, not the
            // executor's policy. Substrate's send_reply serializes +
            // stamps headers.
            let response_value =
                serde_json::to_value(&canned).expect("serialize TextGenerationResponse");
            let response = AircCommandResponse::ok(response_value);
            handler
                .send_reply(&parsed, &response)
                .await
                .expect("substrate send_reply");
            return;
        }
    })
}

#[tokio::test]
async fn airc_remote_inference_adapter_round_trips_against_substrate() {
    let loop_back = TwoAircLoopback::new()
        .await
        .expect("fixture setup should succeed");

    let canned = TextGenerationResponse {
        text: "pong from the 5090".to_string(),
        finish_reason: FinishReason::Stop,
        model: "qwen3.5-4b-code-forged".to_string(),
        provider: "llamacpp-on-5090".to_string(),
        usage: UsageMetrics {
            input_tokens: 12,
            output_tokens: 7,
            total_tokens: 19,
            estimated_cost: None,
        },
        response_time_ms: 123,
        request_id: "test-req-1".to_string(),
        content: None,
        tool_calls: None,
        routing: None,
        error: None,
    };

    // peer_a = remote substrate.
    let handler = build_handler(Arc::clone(loop_back.peer_a()));
    let responder = spawn_ai_generate_responder(
        Arc::clone(&handler),
        Arc::clone(loop_back.peer_a()),
        canned.clone(),
    )
    .await;

    // Give the responder time to install its subscribe filter.
    tokio::time::sleep(Duration::from_millis(50)).await;

    // peer_b = the persona's host. Build the adapter pointing at peer_a.
    let adapter = AircRemoteInferenceAdapter::new(
        Arc::clone(loop_back.peer_b()),
        loop_back.peer_a_id(),
        "qwen3.5-4b-code-forged",
    );

    let request = TextGenerationRequest {
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: MessageContent::Text("ping?".to_string()),
            name: None,
        }],
        system_prompt: Some("you are a substrate-side echo".to_string()),
        model: Some("qwen3.5-4b-code-forged".to_string()),
        provider: None,
        temperature: Some(0.7),
        max_tokens: Some(64),
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
    };

    let response: TextGenerationResponse = adapter
        .generate_text(request)
        .await
        .expect("AircRemoteInferenceAdapter round-trip");

    assert_eq!(response.text, canned.text);
    assert_eq!(response.model, canned.model);
    assert_eq!(response.provider, canned.provider);
    assert_eq!(response.usage.total_tokens, canned.usage.total_tokens);
    assert_eq!(response.request_id, canned.request_id);

    responder.await.expect("responder task joined");
}
