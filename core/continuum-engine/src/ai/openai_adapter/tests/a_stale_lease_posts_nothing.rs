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
    for (index, (bound_to, why)) in [
        (Some((engine_a.pid, engine_a.started_s)), "her lease is not the published grant"),
        (Some((8, 1_762_400_000)), "the endpoint admission is bound to engine B"),
        (None, "no engine the admission can name"),
    ].into_iter().enumerate() {
        // Exercise the real admission owner, not a copied incarnation certificate.
        let endpoint_root = format!("http://{address}/case-{index}");
        let pool = crate::inference::slots::directory().ensure_pool(&endpoint_root, 1);
        if let Some((pid, started_s)) = bound_to {
            crate::inference::slots::stamp_engine_incarnation(&endpoint_root, pid, started_s);
        }
        let sem = Arc::new(tokio::sync::Semaphore::new(1));
        let client = reqwest::Client::new();
        let key = crate::inference::slots::PageKey::mind(uuid::Uuid::new_v4()).expect("test persona");
        let owned = crate::inference::turn_admission::admit_turn(
            &sem, crate::inference::slots::SlotClass::Turn, Some(key), Some(pool),
            &client, &endpoint_root, 4, std::time::Duration::ZERO,
        ).await.expect("test endpoint admission");
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
                admission: Some(crate::inference::lane_send::DispatchAdmission { persona: uuid::Uuid::new_v4(), admission, endpoint: Some(&owned) }),
            },
            &mut reservation,
        )
        .await;
        assert!(matches!(sent, Err(InferenceError::GrantChanged { rendered: 148_838, .. })), "{why}: refused typed");
        assert!(reservation.is_none(), "{why}: nothing reserved");
    }
    assert_eq!(posts.load(Ordering::SeqCst), 0, "zero POSTs under a stale lease");
}

// what this catches: the binding refusing a NORMAL act. A ready shared-pool engine's admission
// names its engine, her lease is the grant published on that same engine, and the dispatch reserves
// and POSTs exactly once (the only test in this binary that installs the grants watch).
#[tokio::test]
async fn a_stamped_admission_on_her_current_lease_dispatches() {
    use axum::{routing::post, Router};
    let posts = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&posts);
    let app = Router::new().route("/v1/chat/completions", post(move || {
        let counted = Arc::clone(&counted);
        async move {
            counted.fetch_add(1, Ordering::SeqCst);
            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n"
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("isolated HTTP fixture"); // JUSTIFIED: test fixture
    let address = listener.local_addr().expect("bound address"); // JUSTIFIED: test fixture
    let _server = tokio::spawn(async move { axum::serve(listener, app).await });
    let root = format!("http://{address}");
    // The owned engine verified ready on this endpoint: its pool is stamped.
    let pool = crate::inference::slots::directory().ensure_pool(&root, 2);
    crate::inference::slots::stamp_engine_incarnation(&root, 4_242, 1_800_000_000);
    let sem = Arc::new(tokio::sync::Semaphore::new(1));
    let client = reqwest::Client::new();
    let persona = uuid::Uuid::new_v4();
    let key = crate::inference::slots::PageKey::mind(persona).expect("test: a non-nil id"); // JUSTIFIED: test fixture
    let admitted = crate::inference::turn_admission::admit_turn(&sem, crate::inference::slots::SlotClass::Turn, Some(key), Some(pool), &client, &root, 100, std::time::Duration::ZERO)
        .await
        .expect("test: admitted"); // JUSTIFIED: test fixture
    // Her grant published on that same engine.
    let engine = crate::inference::engine_residency::EngineIncarnation { pid: 4_242, started_s: 1_800_000_000, port: address.port() };
    let mut grants = crate::capacity::window_grant::PoolGrants {
        pool_tokens: 200_000,
        per_sequence_cap: 200_000,
        by_mind: std::collections::BTreeMap::from([(persona, 148_838)]),
        engine: Some(engine),
        revisions: Default::default(),
    };
    crate::capacity::window_grant::stamp_revisions(None, &mut grants);
    let (_publisher, receiver) = tokio::sync::watch::channel(Some(grants));
    assert!(crate::capacity::window_grant::install_grants(receiver), "test: the only grants watch in this binary");
    let lease = crate::capacity::window_grant::current_lease(persona).expect("her published lease"); // JUSTIFIED: the invariant under test
    let admission = crate::ai::types::RequestAdmission { window: 148_838, lease: Some(lease), priced: crate::ai::types::PromptAdmission::Estimated };
    let adapter = test_adapter();
    let mut reservation = None;
    let sent = crate::inference::lane_send::send_with_lane_retry(
        &adapter.config,
        adapter.client.post(format!("{root}/v1/chat/completions")).header("Content-Type", "application/json"),
        br#"{"messages":[{"role":"user","content":"act"}]}"#.to_vec(),
        Some(std::time::Duration::from_secs(5)),
        crate::inference::lane_send::FillReceipt {
            model: "fixture",
            prompt_tokens: 4,
            prompt_quote: None,
            admission: Some(crate::inference::lane_send::DispatchAdmission {
                persona,
                admission,
                endpoint: Some(&admitted),
            }),
        },
        &mut reservation,
    )
    .await;
    assert!(sent.is_ok(), "a normal act dispatches: {:?}", sent.err());
    assert_eq!(posts.load(Ordering::SeqCst), 1, "exactly one POST");
    let reservation = reservation.expect("her lease is reserved for the request"); // JUSTIFIED: the invariant under test
    crate::capacity::window_grant::settle(reservation, Some(&crate::capacity::window_grant::TerminalReceipt { finish: "stop".into() }));
}
