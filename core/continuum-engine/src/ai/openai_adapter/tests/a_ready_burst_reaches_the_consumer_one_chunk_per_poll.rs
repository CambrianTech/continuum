#![cfg(test)]

//! The engine's stream producer honours `stream_sinks::deliver` against a real burst: one
//! network read carrying many events reaches the consumer one chunk per poll.

use super::*;

// what this catches: the real adapter turning one network read into a burst of chunks in
// a single poll (llama-server sends many events per read, and `consume_sse_stream` parsed
// them all before returning), so a consumer that polls the generation only when it can
// take more (the gateway's streamed answer) still holds a whole read: a one-chunk bound
// claimed from a synthetic producer that yields would not hold. Here 64 tokens arrive READY
// in ONE body chunk, through the real `consume_sse_stream`, with nothing else yielding.
#[tokio::test]
async fn a_ready_burst_reaches_the_consumer_one_chunk_per_poll() {
    use std::future::Future;
    use std::task::{Context, Poll};
    const BURST_EVENTS: usize = 64;
    let mut body = String::new();
    for i in 0..BURST_EVENTS {
        body.push_str(&format!(
            "data: {}\n\n",
            serde_json::json!({"choices": [{"delta": {"content": format!("t{i}")}}]})
        ));
    }
    body.push_str(
        "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n",
    );
    let response = reqwest::Response::from(axum::http::Response::new(body));
    let (sink, mut received) = tokio::sync::mpsc::unbounded_channel();
    let adapter = test_adapter();
    let request = crate::ai::types::TextGenerationRequest::default();
    let mut generation = std::pin::pin!(crate::inference::sse_stream::consume_sse_stream(
        &adapter.config,
        &request,
        false,
        response,
        &sink
    ));
    let mut cx = Context::from_waker(std::task::Waker::noop());
    let mut tokens = 0;
    let mut polls = 0;
    let outcome = loop {
        polls += 1;
        assert!(
            polls <= 10 * BURST_EVENTS,
            "test: the in-memory stream must finish"
        );
        let polled = generation.as_mut().poll(&mut cx);
        assert!(
            received.len() <= 1,
            "one poll left {} chunks waiting",
            received.len()
        );
        while let Ok(chunk) = received.try_recv() {
            if matches!(chunk, GenerationChunk::Token(_)) {
                tokens += 1;
            }
        }
        if let Poll::Ready(outcome) = polled {
            break outcome;
        }
    };
    let outcome = outcome.expect("test: the stream completes"); // JUSTIFIED: the invariant under test
    assert_eq!(tokens, BURST_EVENTS, "every token reached the consumer");
    assert!(
        outcome.acc_content.starts_with("t0t1") && outcome.acc_content.ends_with("t63"),
        "the answer is whole"
    );
}
