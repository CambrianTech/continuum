#![cfg(test)]

//! The engine's terminal receipt for a request is its final frame's finish reason, never a bare
//! EOF: the grant reservation's custody (`window_grant::settle`) reads it.

use super::*;

// what this catches (Codex on #5787): a transport EOF taken for the engine's terminal receipt. A
// stream that ends without a finish reason (no final frame, no `[DONE]`) carries no receipt, so a
// grant reservation settled on it stays held; the final frame's finish is the receipt.
#[tokio::test]
async fn only_a_finish_frame_is_a_terminal_receipt_never_a_bare_eof() {
    let adapter = test_adapter();
    let request = crate::ai::types::TextGenerationRequest::default();
    let (sink, _rx) = tokio::sync::mpsc::unbounded_channel();
    let consume = |body: &str| {
        let response = reqwest::Response::from(axum::http::Response::new(body.to_string()));
        crate::inference::sse_stream::consume_sse_stream(&adapter.config, &request, false, response, &sink)
    };
    let cut = consume("data: {\"choices\":[{\"delta\":{\"content\":\"half a th\"}}]}\n\n")
        .await
        .expect("an EOF is not an error"); // JUSTIFIED: the invariant under test
    assert_eq!(cut.terminal, None, "no finish, no [DONE]: no receipt");
    let done = consume("data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n")
        .await
        .expect("finished"); // JUSTIFIED: the invariant under test
    assert_eq!(done.terminal.map(|t| t.finish), Some("stop".to_string()));
}
