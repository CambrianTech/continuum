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
        let mut dispatched = 0;
        let sent = crate::inference::lane_send::send_with_lane_retry(
            &adapter.config,
            adapter.client.post(format!("http://{address}/v1/chat/completions")).header("Content-Type", "application/json"),
            br#"{"messages":[{"role":"user","content":"act"}]}"#.to_vec(),
            Some(std::time::Duration::from_secs(5)),
            crate::inference::lane_send::FillReceipt {
                output: crate::ai::reply_room::OutputIntent::Fixed(std::num::NonZeroU32::new(1).expect("positive fixture")),
                model: "fixture",
                prompt_tokens: 4,
                prompt_quote: None,
                admission: Some(crate::inference::lane_send::DispatchAdmission { persona: uuid::Uuid::new_v4(), admission, endpoint: Some(&owned) }),
            },
            &mut reservation,
            || dispatched += 1,
        )
        .await;
        assert!(matches!(sent, Err(InferenceError::GrantChanged { rendered: 148_838, .. })), "{why}: refused typed");
        assert!(reservation.is_none(), "{why}: nothing reserved");
        // Her turn admission is never told it sent, so it keeps her residency (Benchy, M5 ed142f14c).
        assert_eq!(dispatched, 0, "{why}: a request refused before its POST is never reported dispatched");
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
    let prompt = Arc::new(AtomicUsize::new(27221));
    let quote_prompt = prompt.clone();
    let quotes = Arc::new(AtomicUsize::new(0));
    let quote_counter = quotes.clone();
    let quoted_bodies = Arc::new(std::sync::Mutex::new(Vec::<serde_json::Value>::new()));
    let quoted_capture = quoted_bodies.clone();
    let sent_bodies = Arc::new(std::sync::Mutex::new(Vec::<serde_json::Value>::new()));
    let sent_capture = sent_bodies.clone();
    let app = Router::new().route("/v1/chat/completions", post(move |axum::Json(body): axum::Json<serde_json::Value>| {
        let sent_capture = sent_capture.clone();
        let counted = Arc::clone(&counted);
        async move {
            sent_capture.lock().expect("sent capture").push(body);
            counted.fetch_add(1, Ordering::SeqCst);
            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n"
        }
    })).route("/v1/chat/completions/input_tokens", post(move |axum::Json(body): axum::Json<serde_json::Value>| {
        let quote_prompt = quote_prompt.clone(); let quote_counter = quote_counter.clone(); let quoted_capture = quoted_capture.clone();
        async move {
            quoted_capture.lock().expect("quote capture").push(body);
            quote_counter.fetch_add(1, Ordering::SeqCst);
            axum::Json(serde_json::json!({"input_tokens": quote_prompt.load(Ordering::SeqCst)}))
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("isolated HTTP fixture"); // JUSTIFIED: test fixture
    let address = listener.local_addr().expect("bound address"); // JUSTIFIED: test fixture
    let _server = tokio::spawn(async move { axum::serve(listener, app).await });
    let root = format!("http://{address}");
    // The owned engine verified ready on this endpoint: its pool is stamped.
    let endpoint = crate::inference::slots::directory().endpoint(&root);
    let transition = endpoint.transition().await;
    let generation = transition.start_generation().expect("fixture generation");
    transition.ready(&root, &generation, crate::inference::slots::KvPageContract {
        model_id: "fixture".into(), model: "fixture.gguf".into(), adapters: vec![],
        page_dir: None, context: 32512, slots: 1, cache_type: None,
        flash_attn: None, revisions: None,
    }).expect("fixture owned geometry");
    drop(transition);
    let pool = endpoint.admit().await.expect("fixture reader").pool.expect("fixture pool");
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
    let adapter = test_adapter();
    for (index, (count, window, required, reply)) in [(27221, 32512, 4534, 5290), (18702, 22435, 3558, 3732)].into_iter().enumerate() {
        prompt.store(count, Ordering::SeqCst);
        let admission = crate::ai::types::RequestAdmission { window, lease: Some(lease), priced: crate::ai::types::PromptAdmission::EngineCounted };
        let mut reservation = None;
        let sent = crate::inference::lane_send::send_with_lane_retry(
            &adapter.config,
            adapter.client.post(format!("{root}/v1/chat/completions")).header("Content-Type", "application/json"),
            br#"{"messages":[{"role":"user","content":"act"}],"max_tokens":999999,"tools":[]}"#.to_vec(),
            Some(std::time::Duration::from_secs(5)),
            crate::inference::lane_send::FillReceipt {
                output: crate::ai::reply_room::OutputIntent::UseAdmittedRoom { required_reply: std::num::NonZeroU32::new(required).expect("positive fixture") },
                model: "fixture", prompt_tokens: 4,
                prompt_quote: Some(crate::inference::lane_send::PromptQuote { client: &adapter.client, dedicated_lane: true, caller: "fixture", request_id: "owned-recorded-room" }),
                admission: Some(crate::inference::lane_send::DispatchAdmission { persona, admission, endpoint: Some(&admitted) }),
            },
            &mut reservation,
            || admitted.dispatching(),
        ).await.expect("owned exact dispatch");
        assert!(sent.response.status().is_success());
        let crate::inference::lane_send::SentOutput::Counted(room) = sent.output else { panic!("counted receipt required") };
        assert_eq!((room.prompt, room.window, room.reply), (count as u32, window, reply));
        assert_eq!(quotes.load(Ordering::SeqCst), index + 1, "one quote per actual attempt");
        assert_eq!(posts.load(Ordering::SeqCst), index + 1, "one generation per actual attempt");
        let mut quoted = quoted_bodies.lock().expect("quoted")[index].clone();
        let mut wire = sent_bodies.lock().expect("sent")[index].clone();
        assert_eq!(wire["max_tokens"], reply);
        quoted.as_object_mut().expect("quote object").remove("max_tokens");
        wire.as_object_mut().expect("wire object").remove("max_tokens");
        assert_eq!(quoted, wire, "all prompt-affecting fields remain identical");
        let reservation = reservation.expect("actual lease reservation");
        crate::capacity::window_grant::settle(reservation, Some(&crate::capacity::window_grant::TerminalReceipt { finish: "stop".into() }));
    }
}

// The actual endpoint owner closes during an HTTP quote. A valid count alone
// must not authorize generation; no global readiness watch substitutes for it.
#[tokio::test]
async fn quarantine_during_quote_posts_nothing() {
    owned_quote_refusal(true, 4, 100).await;
}

#[tokio::test]
async fn owned_physical_window_refuses_a_larger_admitted_window() {
    owned_quote_refusal(false, 100, 1000).await;
}

async fn owned_quote_refusal(quarantine: bool, prompt: u32, admitted_window: u32) {
    use axum::{routing::post, Router};
    use crate::inference::lane_send::{DispatchAdmission, FillReceipt, PromptQuote};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("fixture bind");
    let address = listener.local_addr().expect("fixture address");
    let root = format!("http://{address}");
    let endpoint = crate::inference::slots::directory().endpoint(&root);
    let transition = endpoint.transition().await;
    let generation = transition.start_generation().expect("fixture generation");
    transition.ready(&root, &generation, crate::inference::slots::KvPageContract {
        model_id: "fixture".into(), model: "fixture.gguf".into(), adapters: vec![],
        page_dir: None, context: 100, slots: 1, cache_type: None,
        flash_attn: None, revisions: None,
    }).expect("fixture owned geometry");
    drop(transition);
    let pool = endpoint.admit().await.expect("fixture reader").pool.expect("fixture pool");
    let posts = Arc::new(AtomicUsize::new(0));
    let posted = posts.clone();
    let app = Router::new()
        .route("/v1/chat/completions/input_tokens", post(move || {
            let endpoint = endpoint.clone();
            async move {
                if quarantine { endpoint.quarantine_paging(); }
                axum::Json(serde_json::json!({"input_tokens":prompt}))
            }
        }))
        .route("/v1/chat/completions", post(move || {
            let posted = posted.clone();
            async move { posted.fetch_add(1, Ordering::SeqCst); "unexpected generation" }
        }));
    let server = tokio::spawn(async move { axum::serve(listener, app).await });
    let adapter = test_adapter();
    let sem = Arc::new(tokio::sync::Semaphore::new(1));
    let persona = uuid::Uuid::new_v4();
    let key = crate::inference::slots::PageKey::mind(persona).expect("fixture persona");
    let owned = crate::inference::turn_admission::admit_turn(
        &sem, crate::inference::slots::SlotClass::Turn, Some(key), Some(pool),
        &adapter.client, &root, 4, std::time::Duration::ZERO,
    ).await.expect("fixture admission");
    assert_eq!(owned.dispatch_geometry("fixture").expect("owned model").per_slot_window, 100);
    assert!(matches!(owned.dispatch_geometry("other-model"), Err(InferenceError::AdmissionUncounted { .. })));
    let mut reservation = None;
    let mut dispatched = 0;
    let result = crate::inference::lane_send::send_with_lane_retry(
        &adapter.config,
        adapter.client.post(format!("{root}/v1/chat/completions")).header("Content-Type", "application/json"),
        br#"{"messages":[{"role":"user","content":"act"}],"max_tokens":1}"#.to_vec(),
        None,
        FillReceipt {
                output: crate::ai::reply_room::OutputIntent::Fixed(std::num::NonZeroU32::new(1).expect("positive fixture")),
            model: "fixture", prompt_tokens: 4,
            prompt_quote: Some(PromptQuote { client: &adapter.client, dedicated_lane: true, caller: "fixture", request_id: "quarantined-quote" }),
            admission: Some(DispatchAdmission {
                persona, endpoint: Some(&owned),
                admission: crate::ai::types::RequestAdmission { window: admitted_window, lease: None, priced: crate::ai::types::PromptAdmission::EngineCounted },
            }),
        }, &mut reservation, || dispatched += 1,
    ).await;
    if quarantine {
        assert!(matches!(result, Err(InferenceError::Unavailable(_))));
    } else {
        assert!(matches!(result, Err(InferenceError::ContextExceeded { requested: 100, available: 100 })));
    }
    assert_eq!(posts.load(Ordering::SeqCst), 0);
    assert!(reservation.is_none());
    assert_eq!(dispatched, 0, "refused quote never marks dispatch");
    server.abort();

}
