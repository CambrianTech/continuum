#![cfg(test)]

//! A request whose grant lease is stale (no longer her published grant, or bound to another engine
//! than its endpoint admission) is refused before any POST reaches the engine.

use super::*;
use crate::ai::inference_error::InferenceError;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

// what this catches (Codex on #5787), at the HTTP level: a retry POSTed under a stale lease. A
// fixture engine counts every POST; a dispatch whose lease is not her published grant (here none
// is published), or whose lease names another engine incarnation than the one its endpoint
// admission is bound to, is refused typed and the engine sees ZERO POSTs.
#[tokio::test]
async fn a_stale_lease_posts_nothing() {
    use axum::{routing::post, Router};
    let posts = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&posts);
    let app = Router::new().route("/v1/chat/completions", post(move || {
        let counted = Arc::clone(&counted);
        async move {
            counted.fetch_add(1, Ordering::SeqCst);
            "data: [DONE]\n\n"
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("isolated HTTP fixture"); // JUSTIFIED: test fixture
    let address = listener.local_addr().expect("bound address"); // JUSTIFIED: test fixture
    let _server = tokio::spawn(async move { axum::serve(listener, app).await });
    let adapter = test_adapter();
    let engine_a = crate::inference::engine_residency::EngineIncarnation { pid: 7, started_s: 1_762_300_000, port: 8_085 };
    let lease = crate::capacity::window_grant::GrantLease {
        tokens: 148_838,
        revision: crate::capacity::window_grant::GrantRevision::test(1),
        engine: Some(engine_a),
        owner: crate::capacity::window_grant::GrantOwner::this_process(),
    };
    let admission = crate::ai::types::RequestAdmission { window: 148_838, lease: Some(lease), priced: crate::ai::types::PromptAdmission::Estimated };
    for (bound_to, why) in [
        (Some((engine_a.pid, engine_a.started_s)), "her lease is not the published grant"),
        (Some((8, 1_762_400_000)), "the endpoint admission is bound to engine B"),
        (None, "no engine the admission can name"),
    ] {
        let mut reservation = None;
        let sent = crate::inference::lane_send::send_with_lane_retry(
            &adapter.config,
            adapter.client.post(format!("http://{address}/v1/chat/completions")).header("Content-Type", "application/json"),
            br#"{"messages":[{"role":"user","content":"act"}]}"#.to_vec(),
            Some(std::time::Duration::from_secs(5)),
            crate::inference::lane_send::FillReceipt {
                model: "fixture",
                prompt_tokens: 4,
                prompt_quote: None,
                admission: Some(crate::inference::lane_send::DispatchAdmission { persona: uuid::Uuid::new_v4(), admission, engine: bound_to }),
            },
            &mut reservation,
        )
        .await;
        assert!(matches!(sent, Err(InferenceError::GrantChanged { rendered: 148_838, .. })), "{why}: refused typed");
        assert!(reservation.is_none(), "{why}: nothing reserved");
    }
    assert_eq!(posts.load(Ordering::SeqCst), 0, "zero POSTs under a stale lease");
}
