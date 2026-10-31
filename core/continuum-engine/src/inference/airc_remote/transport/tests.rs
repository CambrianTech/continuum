#![cfg(test)]

use super::*;
use continuum_airc_protocol::DEFAULT_COMMAND_DEADLINE;

// what this catches: a transport with no live wire still honours the streaming
// contract — the whole answer arrives as one trailing chunk, never silence.
#[tokio::test]
async fn a_transport_without_a_wire_emits_the_whole_answer_as_one_trailing_chunk() {
    use crate::ai::adapter::GenerationChunk;
    let stub = StubInferenceTransport::new(|req| {
        Ok(RemoteInferenceResponse {
            correlation_id: req.correlation_id,
            served_by: "stub".into(),
            text_response: crate::ai::types::TextGenerationResponse {
                text: "whole answer".into(),
                finish_reason: crate::ai::types::FinishReason::Stop,
                model: "stub".into(),
                provider: "stub".into(),
                usage: Default::default(),
                response_time_ms: 0,
                request_id: "stub".into(),
                content: None,
                tool_calls: None,
                reasoning: None,
                routing: None,
                error: None,
                timing: None,
            },
        })
    });
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let req = RemoteInferenceRequest::new(crate::ai::types::TextGenerationRequest {
        messages: Vec::new(),
        ..Default::default()
    });
    let out = stub
        .send_request_streaming(req, tx)
        .await
        .expect("stub answers"); // JUSTIFIED: the invariant under test
    assert_eq!(out.text_response.text, "whole answer");
    assert_eq!(
        rx.try_recv().ok(),
        Some(GenerationChunk::Token("whole answer".into()))
    );
    assert!(rx.try_recv().is_err(), "exactly one trailing chunk");
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
    assert_eq!(
        effective_bound(REMOTE_INFERENCE_DEADLINE, None),
        REMOTE_INFERENCE_DEADLINE
    );
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
        system_prompt: None,
        model: None,
        provider: None,
        temperature: None,
        max_tokens: None,
        reasoning_budget: None,
        top_p: None,
        top_k: None,
        repeat_penalty: None,
        frequency_penalty: None,
        repeat_last_n: None,
        stop_sequences: None,
        tools: None,
        tail_tools: None,
        view_messages: None,
        tool_choice: None,
        response_format: None,
        active_adapters: None,
        request_id: None,
        user_id: None,
        room_id: None,
        purpose: None,
        persona_id: None,
        turn_bound: None,
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
    let transport = StubInferenceTransport::always_failing(RemoteInferenceError::NoPeerReachable {
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

// what this catches (card 03660a77, IntelMac 10-23): a seat's typed overload refusal must
// reach the requester as SeatOverloaded, not as a peer failure read from its text, or her
// next act asks the full seat again (Paige: 13 refusals before she moved). Any other
// refusal, and an older seat's untyped error, stays PeerAdapterFailed.
#[test]
fn a_typed_overload_refusal_is_seat_overloaded_and_nothing_else_is() {
    use continuum_airc_protocol::GenerateRefusal;
    let full = AircCommandResponse::refused("seat overloaded: 2 of 2", GenerateRefusal::Overloaded);
    assert!(matches!(
        reply_result(full),
        Err(RemoteInferenceError::SeatOverloaded { message }) if message.contains("2 of 2")
    ));
    for other in [
        AircCommandResponse::refused("late", GenerateRefusal::Expired),
        AircCommandResponse::error("seat overloaded: an older seat, untyped"),
    ] {
        assert!(matches!(
            reply_result(other),
            Err(RemoteInferenceError::PeerAdapterFailed { .. })
        ));
    }
    let wire = serde_json::json!({"status": "error", "message": "old"});
    let old: AircCommandResponse = serde_json::from_value(wire).expect("decodes"); // boundary: an older seat's wire error
    assert_eq!(old.refusal(), None);
}

#[test]
fn settled_failure_survives_partial_progress_but_success_requires_final() {
    let mut progress = progress::Progress::default();
    assert!(settle_response(AircCommandResponse::ok(serde_json::json!({})), &progress).is_err());
    let headers = airc_core::Headers::from([
        (airc_lib::HEADER_STREAM_ID.into(), "exchange".into()),
        (airc_lib::HEADER_STREAM_SEQ.into(), "0".into()),
        (
            airc_lib::HEADER_STREAM_KIND.into(),
            airc_lib::STREAM_KIND_TEXT_TOKEN.into(),
        ),
    ]);
    progress
        .forward(&headers, &Body::text("partial"), "exchange", None)
        .expect("valid partial chunk");
    let failure = settle_response(AircCommandResponse::error("seat stopped"), &progress);
    assert!(
        matches!(failure, Err(RemoteInferenceError::PeerAdapterFailed { message }) if message == "seat stopped")
    );
    assert!(settle_response(AircCommandResponse::ok(serde_json::json!({})), &progress).is_err());
}
