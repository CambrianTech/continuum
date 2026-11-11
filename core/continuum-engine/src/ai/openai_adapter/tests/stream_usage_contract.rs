#![cfg(test)]

//! The usage frame every stream asks for (`stream_options.include_usage`, request_body.rs).

use super::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

// what this catches (Codex on #5902): a cloud-path stream that ends without the usage frame it
// requested read as zero tokens, a count `generate.rs` feeds to resource admission and the gateway
// reports to drivers. A provider honouring the request yields its numbers; one that drops the frame
// is a typed protocol refusal naming the provider, never a fabricated zero.
#[tokio::test]
async fn a_stream_without_its_requested_usage_frame_is_a_protocol_refusal() {
    use axum::{routing::post, Json, Router};
    let with_usage = Arc::new(AtomicBool::new(true));
    let serving = Arc::clone(&with_usage);
    let app = Router::new().route("/v1/chat/completions", post(move |Json(body): Json<serde_json::Value>| {
        let usage = serving.load(Ordering::SeqCst);
        async move {
            assert_eq!(body["stream_options"]["include_usage"], true, "every stream requests its usage frame");
            let content = "data: {\"model\":\"test-model\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}\n\n";
            let frame = if usage {
                "data: {\"model\":\"test-model\",\"choices\":[],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":2}}\n\n"
            } else {
                ""
            };
            ([("content-type", "text/event-stream")], format!("{content}{frame}data: [DONE]\n\n"))
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("isolated HTTP fixture"); // JUSTIFIED: test fixture
    let address = listener.local_addr().expect("bound address"); // JUSTIFIED: test fixture
    let _server = tokio::spawn(async move { axum::serve(listener, app).await });
    let mut adapter = test_adapter();
    adapter.config.base_url = format!("http://{address}");
    let request = || TextGenerationRequest {
        messages: vec![ChatMessage::text("user", "act")],
        ..Default::default()
    };
    let generate = |request| adapter.generate_text(request, crate::ai::adapter::Dispatched::fixture());

    let answered = tokio::time::timeout(std::time::Duration::from_secs(5), generate(request()))
        .await
        .expect("test: the fixture answers")
        .expect("a stream carrying its usage frame is a response");
    assert_eq!((answered.usage.input_tokens, answered.usage.output_tokens, answered.usage.total_tokens), (7, 2, 9),
        "the provider's numbers, total by definition when omitted");

    with_usage.store(false, Ordering::SeqCst);
    let refused = tokio::time::timeout(std::time::Duration::from_secs(5), generate(request()))
        .await
        .expect("test: the fixture answers");
    match refused {
        Err(why) => assert!(
            why.contains("Test Gateway") && why.contains("without the usage frame"),
            "a protocol refusal naming the provider and the frame: {why}"
        ),
        Ok(response) => panic!("a stream without its usage frame must be refused, got usage {:?}", response.usage),
    }
}
