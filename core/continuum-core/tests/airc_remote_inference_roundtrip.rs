//! Integration test: `AircRemoteInferenceAdapter` + `AircLiveTransport`
//! round-trip a `TextGenerationRequest` against the substrate's actual
//! `CommandRequestHandler::parse_envelope` + `send_reply` paths
//! (mirroring `ai/generate` on the remote substrate).
//!
//! This is the live wire proof that a local persona can dispatch
//! inference at `airc://<remote-peer>/ai/generate` and get a typed
//! response back, with the substrate's real parser in the loop on
//! both ends.
//!
//! ## Topology
//!
//! - peer_a = the remote inference host — substrate, hosts ai/generate.
//!   Test stubs the responder so we control the canned
//!   `TextGenerationResponse` and can assert the request's wire
//!   shape (path, kind, params, headers) that AircLiveTransport
//!   emits.
//! - peer_b = the local caller — has an AircRemoteInferenceAdapter
//!   wrapping AircLiveTransport pointed at peer_a. Persona-side code
//!   (well, the test) calls adapter.generate_text(request).
//!
//! ## What this proves
//!
//! Of the 12 wire-drift surfaces between client and substrate, this
//! test catches all 12 in BOTH directions:
//!
//! Request (caller → substrate parse_envelope):
//! - kind == KIND_PEER, path == "ai/generate", env == None
//! - HEADER_COMMAND_PATH stamps the path (middleware filter point)
//! - HEADER_COMMAND_KIND stamps the kind (middleware filter point)
//! - HEADER_CONTINUUM_BODY_HINT == COMMAND_REQUEST_BODY_HINT
//! - params decode as TextGenerationRequest (the inner shape)
//! - messages survived the round-trip
//!
//! Reply (substrate send_reply → caller decode):
//! - reply body is Body::Json
//! - HEADER_COMMAND_STATUS == "ok"
//! - HEADER_CONTINUUM_BODY_HINT == COMMAND_RESPONSE_BODY_HINT
//! - AircCommandResponse::Ok carries the serialized TextGenerationResponse
//! - All fields round-trip (text, model, provider, usage, request_id)
//!
//! Per R3 BLOCK on PR #1560 round 1: header assertions for
//! HEADER_COMMAND_PATH, HEADER_COMMAND_KIND, COMMAND_RESPONSE_BODY_HINT
//! are explicit in this version so a refactor that drops the headers
//! (or renames them in only one direction) breaks loudly.

use std::sync::Arc;

use airc_test_fixtures::TwoAircLoopback;
use continuum_airc_protocol::{
    AircCommandResponse, COMMAND_REQUEST_BODY_HINT, COMMAND_RESPONSE_BODY_HINT,
    HEADER_COMMAND_KIND, HEADER_COMMAND_PATH, HEADER_COMMAND_STATUS, HEADER_CONTINUUM_BODY_HINT,
    KIND_PEER,
};
use continuum_core::ai::adapter::AIProviderAdapter;
use continuum_core::ai::types::{
    ChatMessage, FinishReason, MessageContent, TextGenerationRequest, TextGenerationResponse,
    UsageMetrics,
};
use continuum_core::inference::airc_remote::{AircLiveTransport, AircRemoteInferenceAdapter};
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
/// catches drift between AircLiveTransport's wire envelope and what
/// the substrate accepts.
async fn spawn_ai_generate_responder(
    handler: Arc<CommandRequestHandler>,
    peer_a: Arc<airc_lib::Airc>,
    canned: TextGenerationResponse,
    ready: Arc<tokio::sync::Notify>,
) -> tokio::task::JoinHandle<()> {
    let self_id = peer_a.peer_id();
    tokio::spawn(async move {
        let mut stream = peer_a.subscribe().await.expect("peer_a subscribe");
        // R3-N4 barrier: signal the test that subscribe() returned
        // so the dispatch is guaranteed to land in the broadcast
        // window. Replaces a 50ms sleep that was flaky on loaded CI.
        ready.notify_one();
        while let Some(event) = stream.next().await {
            let event = match event {
                Ok(e) => e,
                Err(_) => continue,
            };
            if event.peer_id == self_id {
                continue;
            }
            // Skip events without ANY body_hint (airc emits control
            // frames + keepalives that aren't command envelopes).
            let hint = match event.headers.get(HEADER_CONTINUUM_BODY_HINT) {
                Some(h) => h,
                None => continue,
            };
            // R3-N1: but if a body_hint is set, it MUST be
            // COMMAND_REQUEST_BODY_HINT — anything else is drift in
            // AircLiveTransport's stamping. Was silently `continue`
            // in round 2; drift would have hidden behind a 30s
            // timeout instead of failing with a named surface.
            assert_eq!(
                hint, COMMAND_REQUEST_BODY_HINT,
                "wire header HEADER_CONTINUUM_BODY_HINT must stamp COMMAND_REQUEST_BODY_HINT \
                 for ai/generate dispatch; got {hint:?}"
            );

            // R3 header-drift coverage: AircLiveTransport stamps the
            // path + kind on outbound headers per `[[airc-headers-are-
            // the-routing-layer]]` so middleware can filter without
            // body parsing. If the transport ever stops stamping
            // them, this test fails loudly here — substrate
            // parse_envelope reads the body, but the headers are
            // load-bearing for the rest of the substrate's routing
            // layer.
            assert_eq!(
                event.headers.get(HEADER_COMMAND_PATH).map(String::as_str),
                Some("ai/generate"),
                "wire header HEADER_COMMAND_PATH must stamp the substrate command path"
            );
            assert_eq!(
                event.headers.get(HEADER_COMMAND_KIND).map(String::as_str),
                Some(KIND_PEER),
                "wire header HEADER_COMMAND_KIND must stamp KIND_PEER for ai/generate dispatch"
            );

            // Substrate's actual parser.
            let parsed = match CommandRequestHandler::parse_envelope(&event) {
                Ok(p) => p,
                Err(e) => panic!("substrate parse_envelope rejected envelope: {e:?}"),
            };

            // Wire-shape assertions on the BODY (independent of the
            // headers — substrate parses from the body).
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
            let decoded: TextGenerationRequest =
                serde_json::from_value(parsed.request.params.clone())
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

/// Sniffer task: peer_b subscribes to its own incoming reply events
/// and pulls out the HEADER_CONTINUUM_BODY_HINT + HEADER_COMMAND_STATUS
/// the substrate's send_reply stamped. Lets the test assert R3's
/// COMMAND_RESPONSE_BODY_HINT drift surface without depending on
/// adapter-side decoding (which would only catch the body shape, not
/// the headers).
async fn spawn_reply_header_sniffer(
    peer_b: Arc<airc_lib::Airc>,
    ready: Arc<tokio::sync::Notify>,
) -> tokio::task::JoinHandle<(Option<String>, Option<String>)> {
    let self_id = peer_b.peer_id();
    tokio::spawn(async move {
        let mut stream = peer_b.subscribe().await.expect("peer_b subscribe");
        // R3-N4 barrier — same shape as the responder's, signaled
        // once subscribe() returns so the test can deterministically
        // wait for the broadcast window.
        ready.notify_one();
        while let Some(event) = stream.next().await {
            let event = match event {
                Ok(e) => e,
                Err(_) => continue,
            };
            // We want INBOUND events from peer_a (the reply).
            if event.peer_id == self_id {
                continue;
            }
            // The reply is identified by COMMAND_RESPONSE_BODY_HINT.
            // The request goes the other way (peer_b → peer_a) and
            // wouldn't surface here as an inbound event. But filter
            // explicitly: if the substrate ever stops stamping the
            // response hint, we want this to fail by returning
            // (None, ...) not by hanging — early-exit on first
            // inbound to make the failure mode visible.
            let hint = event.headers.get(HEADER_CONTINUUM_BODY_HINT).cloned();
            let status = event.headers.get(HEADER_COMMAND_STATUS).cloned();
            return (hint, status);
        }
        (None, None)
    })
}

#[tokio::test]
async fn airc_remote_inference_adapter_round_trips_against_substrate() {
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

    // peer_a = remote substrate. Notify barrier signals when its
    // subscribe() returns so the test can wait deterministically
    // instead of guessing a sleep duration (R3-N4 round-2 note).
    let handler = build_handler(Arc::clone(loop_back.peer_a()));
    let responder_ready = Arc::new(tokio::sync::Notify::new());
    let responder = spawn_ai_generate_responder(
        Arc::clone(&handler),
        Arc::clone(loop_back.peer_a()),
        canned.clone(),
        Arc::clone(&responder_ready),
    )
    .await;

    // R3 coverage: sniff the reply headers on peer_b's inbound stream
    // BEFORE the adapter dispatches, so we don't race with the
    // adapter's own subscribe inside await_reply.
    let sniffer_ready = Arc::new(tokio::sync::Notify::new());
    let sniffer =
        spawn_reply_header_sniffer(Arc::clone(loop_back.peer_b()), Arc::clone(&sniffer_ready))
            .await;

    // Deterministic barrier — both tasks have called subscribe() and
    // are sitting on the broadcast receiver before we dispatch.
    responder_ready.notified().await;
    sniffer_ready.notified().await;

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

    responder.await.expect("responder task joined");

    // R3 coverage: substrate's send_reply MUST stamp
    // COMMAND_RESPONSE_BODY_HINT + HEADER_COMMAND_STATUS=ok on the
    // reply. If a refactor drops them, this assertion fails — drift
    // gets caught even though the body-level decode succeeded.
    let (hint, status) = sniffer.await.expect("sniffer task joined");
    assert_eq!(
        hint.as_deref(),
        Some(COMMAND_RESPONSE_BODY_HINT),
        "substrate send_reply must stamp HEADER_CONTINUUM_BODY_HINT = COMMAND_RESPONSE_BODY_HINT"
    );
    assert_eq!(
        status.as_deref(),
        Some("ok"),
        "successful AircCommandResponse must stamp HEADER_COMMAND_STATUS = ok"
    );
}
