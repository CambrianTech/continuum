//! Integration test: `AircRemoteInferenceAdapter` + `AircLiveTransport`
//! round-trip a `TextGenerationRequest` against the substrate's actual command
//! receiver (`CommandRequestHandler::on_addressed`, fed by peer_a's addressed inbox).
//!
//! This is the live wire proof that a local persona can dispatch inference at
//! `airc://<remote-peer>/ai/generate` and get a typed response back, with the
//! substrate's real ingress contract, decode and reply in the loop.
//!
//! ## Topology
//!
//! - peer_a = the remote inference host. Its executor runs a canned `ai/generate`
//!   module so the test controls the `TextGenerationResponse`; the responder asserts
//!   the request's wire shape (frame kind, headers, body) before the receiver runs.
//! - peer_b = the local caller: an `AircRemoteInferenceAdapter` over
//!   `AircLiveTransport` pointed at peer_a.
//!
//! ## What this proves
//!
//! Request (caller → substrate):
//! - the frame is airc's typed `Command`, addressed to peer_a, verified from peer_b
//! - HEADER_COMMAND_PATH / HEADER_COMMAND_KIND stamp the path and kind (the ingress
//!   contract decides authority on them before decoding)
//! - kind == KIND_PEER, path == "ai/generate", env == None in the body
//! - params decode as TextGenerationRequest and the messages survived
//!
//! Reply (substrate → caller):
//! - AircCommandResponse::Ok carries the serialized TextGenerationResponse, every field
//!   round-trips, and the receipt's correlation is the request's wire correlation

use std::sync::Arc;

use airc_test_fixtures::TwoAircLoopback;
use async_trait::async_trait;
use continuum_airc_protocol::{HEADER_COMMAND_KIND, HEADER_COMMAND_PATH, KIND_PEER};
use continuum_core::ai::adapter::AIProviderAdapter;
use continuum_core::ai::types::{
    ChatMessage, FinishReason, MessageContent, TextGenerationRequest, TextGenerationResponse,
    UsageMetrics,
};
use continuum_core::inference::airc_remote::{AircLiveTransport, AircRemoteInferenceAdapter};
use continuum_core::ipc::command_ingress::CommandRequestHandler;
use continuum_core::routing::addressed_inbox::{AddressedHandler, AddressedKind};
use continuum_core::runtime::command_executor::CommandExecutor;
use continuum_core::runtime::{
    CommandResult, ModuleConfig, ModulePriority, ModuleRegistry, ServiceModule,
};

/// `ai/generate` answered with a canned response: the test pins the wire, not inference.
struct CannedGenerate(TextGenerationResponse);

impl CannedGenerate {
    const PREFIXES: &'static [&'static str] = &["ai/generate"];
}

#[async_trait]
impl ServiceModule for CannedGenerate {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "canned-generate",
            priority: ModulePriority::Normal,
            command_prefixes: Self::PREFIXES,
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: None,
        }
    }
    async fn initialize(
        &self,
        _ctx: &continuum_core::runtime::ModuleContext,
    ) -> Result<(), String> {
        Ok(())
    }
    async fn handle_command(
        &self,
        _command: &str,
        _params: serde_json::Value,
    ) -> Result<CommandResult, String> {
        serde_json::to_value(&self.0)
            .map(CommandResult::Json)
            .map_err(|e| format!("serialize canned response: {e}"))
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

fn build_handler(
    peer_a: Arc<airc_lib::Airc>,
    canned: TextGenerationResponse,
) -> Arc<CommandRequestHandler> {
    let registry = Arc::new(ModuleRegistry::new());
    registry.register(Arc::new(CannedGenerate(canned)));
    let executor = Arc::new(CommandExecutor::new(registry));
    CommandRequestHandler::new(peer_a, executor)
}

/// Reads peer_a's addressed inbox, asserts the request's wire shape, then hands the
/// frame to the substrate's real receiver. Returns the request's wire correlation.
async fn spawn_ai_generate_responder(
    handler: Arc<CommandRequestHandler>,
    peer_a: Arc<airc_lib::Airc>,
    peer_b: airc_core::PeerId,
    ready: Arc<tokio::sync::Notify>,
) -> tokio::task::JoinHandle<uuid::Uuid> {
    tokio::spawn(async move {
        let mut inbox = peer_a
            .addressed_inbox(tokio::time::Instant::now() + airc_ipc::client::DEFAULT_RPC_TIMEOUT)
            .await
            .expect("peer_a addressed inbox");
        ready.notify_one();
        let frame = inbox.next().await.expect("the request arrives");
        assert_eq!(
            frame.kind(),
            AddressedKind::Command,
            "a request is airc's typed Command"
        );
        assert_eq!(frame.from(), peer_b, "verified sender");
        assert_eq!(frame.to(), peer_a.peer_id(), "addressed to the seat");
        assert_eq!(
            frame.headers().get(HEADER_COMMAND_PATH).map(String::as_str),
            Some("ai/generate"),
            "wire header HEADER_COMMAND_PATH must stamp the substrate command path"
        );
        assert_eq!(
            frame.headers().get(HEADER_COMMAND_KIND).map(String::as_str),
            Some(KIND_PEER),
            "wire header HEADER_COMMAND_KIND must stamp KIND_PEER for ai/generate dispatch"
        );
        let airc_core::Body::Json(body) = frame.body() else {
            panic!("the request body is JSON");
        };
        let request: continuum_airc_protocol::AircCommandRequest =
            serde_json::from_value(body.clone()).expect("an AircCommandRequest");
        assert_eq!(request.kind, KIND_PEER);
        assert_eq!(request.path, "ai/generate");
        assert_eq!(request.env, None);
        let decoded: TextGenerationRequest =
            serde_json::from_value(request.params).expect("params decode as TextGenerationRequest");
        assert!(!decoded.messages.is_empty(), "non-empty messages");
        let correlation = frame.correlation();
        handler.on_addressed(frame).await;
        correlation
    })
}

#[tokio::test]
async fn airc_remote_inference_adapter_round_trips_against_substrate() {
    round_trip().await;
}

async fn round_trip() {
    let loop_back = TwoAircLoopback::new()
        .await
        .expect("fixture setup should succeed");

    let canned = TextGenerationResponse {
        text: "pong from the remote peer".to_string(),
        finish_reason: FinishReason::Stop,
        model: "test-model".to_string(),
        provider: "test-remote-llamacpp".to_string(),
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
        reasoning: None,
        timing: None,
    };

    // peer_a = remote substrate; the barrier signals once its inbox is open.
    let handler = build_handler(Arc::clone(loop_back.peer_a()), canned.clone());
    let responder_ready = Arc::new(tokio::sync::Notify::new());
    let responder = spawn_ai_generate_responder(
        Arc::clone(&handler),
        Arc::clone(loop_back.peer_a()),
        loop_back.peer_b().peer_id(),
        Arc::clone(&responder_ready),
    )
    .await;
    responder_ready.notified().await;

    // peer_b = the persona's host. Build the adapter wrapped around
    // the live airc transport pointed at peer_a.
    let transport = AircLiveTransport::new(Arc::clone(loop_back.peer_b()), loop_back.peer_a_id());
    let adapter = AircRemoteInferenceAdapter::new(transport);

    let request = TextGenerationRequest {
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: MessageContent::Text("ping?".to_string()),
            name: None,
        }],
        system_prompt: Some("you are a substrate-side echo".to_string()),
        model: Some("qwen3.5-4b-code-forged".to_string()),
        temperature: Some(0.7),
        max_tokens: Some(64),
        ..Default::default()
    };

    let started = std::time::Instant::now();
    let response: TextGenerationResponse = adapter
        .generate_text(request)
        .await
        .expect("AircRemoteInferenceAdapter round-trip");

    // Body assertions.
    assert_eq!(response.text, canned.text);
    assert_eq!(response.model, canned.model);
    // The adapter overwrites `provider` to "airc-remote" so observers
    // know the result came via cross-grid dispatch (preserving the
    // remote's provider would lose that signal).
    assert_eq!(response.provider, "airc-remote");
    assert_eq!(response.usage.total_tokens, canned.usage.total_tokens);
    assert_eq!(response.request_id, canned.request_id);
    assert_eq!(response.finish_reason, FinishReason::Stop);
    let routing = response
        .routing
        .as_ref()
        .expect("remote routing receipt survives adapter");
    assert!(!routing.is_local);
    let receipt = routing.remote.as_ref().expect("live transport receipt");
    assert_eq!(receipt.requested_peer, loop_back.peer_a_id().to_string());
    assert_eq!(receipt.responding_peer, loop_back.peer_a_id().to_string());
    assert!(receipt.elapsed_ms <= started.elapsed().as_millis() as u64);

    let correlation = responder.await.expect("responder task joined");
    assert_eq!(
        receipt.correlation_id,
        correlation.to_string(),
        "receipt uses wire correlation, not the adapter's unrelated internal id"
    );
}
