#![cfg(test)]

use super::*;

// what this catches: the `{"_raw": …}` fallback returning. Unparseable tool
// arguments used to be wrapped as a params object with a key NOTHING in the tree
// reads, which destroyed the real cause here and made the failure resurface
// downstream as a misleading "missing field X" about params that were never
// parsed. Glass-boxed from Asha's live capture 2026-08-06: Devstral emitted
// `write_file` whose file_path ran away into a repeating token block, breaking
// the JSON. The marker must NAME the failure and carry the parser's own error.
// [[fallbacks-are-illegal-fail-loud]] (#334)
#[test]
fn unparseable_tool_arguments_are_marked_malformed_never_silently_wrapped() {
    let runaway = format!(
        "{{\"content\":\"x\",\"file_path\":\"/a{}",
        "e072".repeat(50)
    );
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(&runaway);
    let err = parsed.expect_err("fixture must actually be invalid JSON");

    let input = json!({
        "__malformed_tool_arguments": { "error": err.to_string(), "raw": runaway.clone() }
    });

    // The marker is self-describing: a reader downstream can tell that the
    // ARGUMENTS never parsed, rather than guessing at a missing field.
    let m = input
        .get("__malformed_tool_arguments")
        .expect("malformed marker must be present and named for what happened");
    assert!(
        m.get("error")
            .and_then(|e| e.as_str())
            .is_some_and(|e| !e.is_empty()),
        "the parser's own error must survive — it is the only account of WHY"
    );
    assert_eq!(
        m.get("raw").and_then(|r| r.as_str()),
        Some(runaway.as_str()),
        "the raw text is kept for diagnosis, but under a key that says it is broken"
    );
    // The dead escape hatch must not come back: `_raw` had zero consumers, so a
    // params object carrying it reads as valid to every caller and is not.
    assert!(
        input.get("_raw").is_none(),
        "the silent `_raw` wrapper must stay gone"
    );
}

// what this catches (card 394017ed, f3ba3b56 item 5): a /props transport failure left
// the request unpinned with only a debug! line. It is now a typed miss the caller refuses,
// never a latch: the next request probes again.
#[tokio::test]
async fn a_props_transport_failure_is_a_named_miss_and_never_latches() {
    // Port 0 is never connectable, so the refusal is deterministic (no released port another
    // test could rebind); the path keeps this root's directory entry its own.
    let mut adapter = test_adapter();
    adapter.config.base_url = "http://127.0.0.1:0/slot-discovery-transport-miss".into();
    let Err(miss) = adapter.ensure_slot_pool(&adapter.endpoints()).await else { panic!("test: nothing listens, yet a pool came back") };
    assert_eq!(miss.as_str(), "transport", "{miss:?}");
    assert!(crate::inference::slots::directory().get(adapter.endpoints().root()).is_none(), "a transport error never latches");
}

// Exercise the generation boundary, not just discovery: no miss may turn into
// permit-only admission and an HTTP generation on an unowned slot.
#[tokio::test]
async fn slot_discovery_failure_refuses_generation_before_http_dispatch() {
    use axum::{http::StatusCode, routing::{get, post}, Router};
    use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};

    for (status, body, cause) in [
        (StatusCode::SERVICE_UNAVAILABLE, "loading", "not_ready"),
        (StatusCode::NOT_FOUND, "absent", "endpoint_absent"),
        (StatusCode::OK, "not json", "body_unparsed"),
        (StatusCode::OK, "{}", "no_geometry"),
    ] {
        let generations = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&generations);
        let app = Router::new()
            .route("/props", get(move || async move { (status, body) }))
            .route("/v1/chat/completions", post(move || {
                let counted = Arc::clone(&counted);
                async move {
                    counted.fetch_add(1, Ordering::SeqCst);
                    (StatusCode::INTERNAL_SERVER_ERROR, "generation must not be reached")
                }
            }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("fixture bind");
        let address = listener.local_addr().expect("fixture address");
        let server = tokio::spawn(async move { axum::serve(listener, app).await });
        let mut adapter = test_adapter();
        adapter.config.base_url = format!("http://{address}");
        adapter.config.llamacpp_sampling_extensions = true;
        let request = TextGenerationRequest {
            messages: vec![ChatMessage::text("user", "act")],
            purpose: Some("turn".into()),
            ..Default::default()
        };
        let (sink, _receiver) = tokio::sync::mpsc::unbounded_channel();
        let error = tokio::time::timeout(
            std::time::Duration::from_secs(5), adapter.generate_stream_checked(request, sink),
        ).await.expect("bounded fixture").expect_err("discovery must refuse");
        assert!(matches!(error, crate::ai::inference_error::InferenceError::Unavailable(_)), "{error}");
        assert!(error.to_string().contains(cause), "{error}");
        assert_eq!(generations.load(Ordering::SeqCst), 0, "{cause}");
        server.abort();
    }
}


// what this catches: be553169 — `live_served_window` is now the ONLY thing that stamps
// RoutingInfo.served_context_window (ai/generate no longer gates on `is_local`, which
// blocked this adapter and permitted the in-process one), and its two refusal arms had no
// test at all. Both exist to stop a lane reporting a window that describes a DIFFERENT
// server — the failure that starved an eval fork's prompt, webdev-rs 0/6, 2026-07-20. A
// requester on another node BUDGETS against whatever crosses the wire here, so a wrong
// number is worse than none.
//
// NOT covered, deliberately and stated rather than implied: the third arm, where a shared
// single-resident gateway returns the live `/props` window. That arm reads the
// process-wide `SERVING_STATE` OnceLock, and installing it from a test would poison every
// other test in the same binary. It is covered live instead, by the card's one-command
// repro (`continuum ai/generate --provider llama-server --model <served>` must return a
// servedContextWindow equal to `serving/status`).
#[test]
fn a_lane_that_is_not_the_shared_gateway_reports_no_window_at_all() {
    // A dedicated lane owns its own server; its window rides on the binding, pinned from
    // ITS /props at spawn. The global snapshot is a different process entirely.
    let mut dedicated = test_adapter().with_dedicated_lane();
    dedicated.config.single_resident_model = true;
    assert_eq!(
        dedicated.live_served_window(),
        None,
        "a dedicated lane must never report the GLOBAL serving snapshot as its own"
    );

    // A multi-model gateway has no single resident slot to describe, so the global
    // snapshot does not describe "the" lane this answer came from either.
    let shared_multi = test_adapter(); // single_resident_model: false
    assert_eq!(
        shared_multi.live_served_window(),
        None,
        "a non-single-resident gateway has no one lane to report"
    );
}

// what this catches: card 7eef3e05, a wedged engine's mid-stream error frame read as an empty
// `stop` completion (32 such acts on the M5, 2026-10-23); the act must fail with the server's
// words so a resident never loops on empty "completed" acts.
#[tokio::test]
async fn a_server_error_frame_fails_the_act_never_an_empty_completion() {
    use axum::{routing::post, Router};

    const WEDGED: &str = "data: {\"choices\":[{\"finish_reason\":null,\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":null}}],\"model\":\"test-model\",\"prompt_progress\":{\"total\":113106,\"cache\":4096,\"processed\":4096,\"time_ms\":0}}\n\ndata: {\"error\":{\"code\":500,\"message\":\"decode() failed: failed to process speculative batch\",\"type\":\"server_error\"}}\n\n";
    let app = Router::new().route(
        "/v1/chat/completions",
        post(|| async { ([("content-type", "text/event-stream")], WEDGED) }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("isolated HTTP fixture");
    let address = listener.local_addr().expect("bound address");
    let server = tokio::spawn(async move { axum::serve(listener, app).await });
    let mut adapter = test_adapter();
    adapter.config.base_url = format!("http://{address}");
    let request = TextGenerationRequest {
        messages: vec![ChatMessage::text("user", "act")],
        ..Default::default()
    };
    let (sink, receiver) = tokio::sync::mpsc::unbounded_channel();
    drop(receiver);
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        adapter.generate_stream_checked(request, sink),
    )
    .await
    .expect("bounded fixture request");
    let error = result.expect_err("a server error frame is a failed act, never a completion");
    assert!(
        error.to_string().contains("failed to process speculative batch"),
        "the server's own words reach the act's failure: {error}"
    );
    server.abort();
}

// what this catches: aa5888a9 — the actual HTTP boundary must preserve
// structured counts, and a transport retry must send the same prepared bytes.
#[tokio::test]
async fn structured_overflow_survives_adapter_and_prepared_transport_retry() {
    use crate::ai::inference_error::InferenceError;
    use axum::{http::StatusCode, routing::post, Router};
    use std::sync::{Arc, Mutex};

    let received = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
    let log = Arc::clone(&received);
    let quotes = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
    let quote_log = Arc::clone(&quotes);
    let app = Router::new().route("/v1/chat/completions", post(move |body: axum::body::Bytes| {
        let log = Arc::clone(&log);
        async move {
            let attempt = {
                let mut log = log.lock().expect("fixture request log");
                log.push(body.to_vec());
                log.len()
            };
            if attempt == 2 {
                (StatusCode::SERVICE_UNAVAILABLE, "loading")
            } else {
                (StatusCode::BAD_REQUEST, r#"{"error":{"type":"exceed_context_size_error","message":"no counts may be inferred from this text","n_prompt_tokens":29722,"n_ctx":29440}}"#)
            }
        }
    }))
    .route("/v1/chat/completions/input_tokens", post(move |body: axum::body::Bytes| {
        let log = Arc::clone(&quote_log);
        async move {
            log.lock().expect("quote log").push(body.to_vec());
            (StatusCode::OK, r#"{"input_tokens":1319}"#)
        }
    }))
    .route("/bad/chat/completions/input_tokens", post(|| async {
        (StatusCode::OK, r#"{"input_tokens":"unknown"}"#)
    }))
    .route("/silent/chat/completions/input_tokens", post(|| async {
        std::future::pending::<StatusCode>().await
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("isolated HTTP fixture");
    let address = listener.local_addr().expect("bound address");
    let server = tokio::spawn(async move { axum::serve(listener, app).await });
    let mut adapter = test_adapter();
    adapter.config.base_url = format!("http://{address}");
    let request = TextGenerationRequest {
        messages: vec![ChatMessage::text(
            "user",
            "Inspect the selected artifact — λ",
        )],
        ..Default::default()
    };
    let (sink, receiver) = tokio::sync::mpsc::unbounded_channel();
    drop(receiver);
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        adapter.generate_stream_checked(request, sink),
    )
    .await
    .expect("bounded fixture request");
    assert!(matches!(
        result,
        Err(InferenceError::ContextExceeded {
            requested: 29722,
            available: 29440
        })
    ));

    let body =
        br#"{"messages":[{"role":"user","content":"prepared request"}],"tools":[]}"#.to_vec();
    adapter.config.single_resident_model = true;
    let retry = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        crate::inference::lane_send::send_with_lane_retry(
            &adapter.config,
            adapter
                .client
                .post(format!("http://{address}/v1/chat/completions"))
                .header("Content-Type", "application/json"),
            body.clone(),
            // fe7960bf: a short measured occupancy is not an expired end-to-end
            // deadline. Even after admission spent it, quote + retry retain the
            // existing wire floor; no readiness-patience value enters this API.
            Some(std::time::Duration::ZERO),
            crate::inference::lane_send::FillReceipt {
                model: "fixture",
                prompt_tokens: 4,
                prompt_quote: Some(crate::inference::lane_send::PromptQuote {
                    client: &adapter.client,
                    dedicated_lane: true,
                    caller: "fixture",
                    reply: 0,
                    request_id: "fixture",
                }),
            },
        ),
    )
    .await
    .expect("bounded retry fixture");
    assert!(matches!(
        retry,
        Err(InferenceError::ContextExceeded {
            requested: 29722,
            available: 29440
        })
    ));
    assert_eq!(*quotes.lock().expect("actual quotes"), vec![body.clone(), body.clone()],
        "the 503 transport retry must re-quote the unchanged prepared body");
    // Regression: text density varies across Kimi's turns. The finalized HTTP
    // count, not the previous turn's byte estimate, drives the same capacity
    // check used by guard_resident_model. Reply allowance stays separate from
    // the typed prompt count returned to the existing corrective fitter.
    let measured = crate::inference::serving_guard::quote_prompt(
        &adapter.client,
        &adapter.client.post(format!("http://{address}/v1/chat/completions")).body(body.clone()),
        std::time::Duration::from_secs(5),
    ).await.unwrap();
    use crate::inference::serving_guard::PromptCount;
    assert!(PromptCount::Estimated(4).check(1400, "fixture", "fixture").is_ok());
    assert!(matches!(PromptCount::Measured { prompt: measured, reply: 128 }
        .check(1400, "fixture", "fixture"),
        Err(InferenceError::ContextExceeded { requested: 1319, available: 1400 })));
    assert!(PromptCount::Estimated(2000).check(1600, "fixture", "fixture").is_err());
    assert!(PromptCount::Measured { prompt: measured, reply: 128 }
        .check(1600, "fixture", "fixture").is_ok(),
        "a heuristic overestimate cannot veto a measured fit");
    assert_eq!(received.lock().unwrap().len(), 3, "counting and refusal never generate");
    // Use this same transport fixture to verify that quote failures remain
    // errors, never a zero-cost image estimate or a generation request.
    for (path, expected) in [
        ("v1/chat/completions", Ok(1319)),
        ("bad/chat/completions", Err("positive input_tokens")),
        ("missing/chat/completions", Err("HTTP 404")),
    ] {
        let quoted = crate::inference::serving_guard::quote_prompt(
            &adapter.client,
            &adapter.client.post(format!("http://{address}/{path}")).body(body.clone()),
            std::time::Duration::from_secs(5),
        ).await;
        match expected {
            Ok(tokens) => assert_eq!(quoted.unwrap(), tokens),
            Err(message) => assert!(quoted.unwrap_err().contains(message)),
        }
    }
    // Card 2617c7f2: the same HTTP fixture distinguishes an unanswered quote
    // from a failed connection without leaking URL, auth, or prompt data.
    // The short patience is fixture input, not a production timeout policy.
    let secret = "quote-fixture-secret";
    let quote_failure = |path: &str| adapter.client
        .post(format!("http://{address}/{path}?credential={secret}"))
        .bearer_auth(secret).body(secret);
    let timeout = crate::inference::serving_guard::quote_prompt(
        &adapter.client, &quote_failure("silent/chat/completions"),
        std::time::Duration::from_millis(50),
    ).await.unwrap_err();
    assert!(timeout.contains("timeout=true"), "{timeout}");
    assert!(!timeout.contains(secret) && !timeout.contains(&address.to_string()), "{timeout}");
    server.abort();
    let _ = server.await;
    // Do not reuse a keep-alive connection accepted before listener shutdown.
    let disconnected_client = reqwest::Client::builder().no_proxy().build().unwrap();
    let disconnected = crate::inference::serving_guard::quote_prompt(
        &disconnected_client, &quote_failure("v1/chat/completions"),
        std::time::Duration::from_secs(5),
    ).await.unwrap_err();
    assert!(disconnected.contains("connect=true"), "{disconnected}");
    assert!(disconnected.contains("io_source=Some"), "{disconnected}");
    assert!(!disconnected.contains(secret) && !disconnected.contains(&address.to_string()), "{disconnected}");
    let received = received.lock().expect("actual wire requests");
    assert_eq!(received.len(), 3);
    assert_eq!(received[1], body);
    assert_eq!(received[2], body);
    let first: Value = serde_json::from_slice(&received[0]).expect("actual adapter JSON body");
    assert_eq!(
        first["messages"][0]["content"],
        "Inspect the selected artifact — λ"
    );
}

// Regression for 73d66475: use the actual adapter HTTP path, not just the
// one-slot pool constructor. Background traffic must save the resident before
// overwrite, then restore it on return; a failed save cannot remain restorable
// or make the next call save background KV under the activity's filename.
// A failed model request must likewise lose its provisional slot identity.
#[tokio::test]
async fn single_slot_background_use_preserves_only_successfully_saved_activity_pages() {
    use axum::{
        extract::Query,
        http::StatusCode,
        response::IntoResponse,
        routing::{get, post},
        Json, Router,
    };
    use std::sync::{
        atomic::{AtomicBool, AtomicU8, Ordering},
        Arc, Mutex,
    };

    let operations = Arc::new(Mutex::new(Vec::<Value>::new()));
    let fail_save = Arc::new(AtomicBool::new(false));
    let fail_restore = Arc::new(AtomicBool::new(false));
    // 0: complete, 1: HTTP error, 2: premature EOF, 3: read error, 4: pending.
    let generation_mode = Arc::new(AtomicU8::new(0));
    let release_stream = Arc::new(tokio::sync::Notify::new());
    let page_log = Arc::clone(&operations);
    let page_failure = Arc::clone(&fail_save);
    let restore_failure = Arc::clone(&fail_restore);
    let generate_log = Arc::clone(&operations);
    let generate_mode = Arc::clone(&generation_mode);
    let generate_release = Arc::clone(&release_stream);
    let props_slots = Arc::new(AtomicU8::new(2));
    let advertised_slots = Arc::clone(&props_slots);
    let app = Router::new()
        .route("/props", get(move || {
            let slots = advertised_slots.load(Ordering::SeqCst);
            async move { Json(json!({"total_slots": slots, "default_generation_settings": {"n_ctx": 8192}})) }
        }))
        .route("/slots/{slot}", post(move |
            Query(query): Query<std::collections::HashMap<String, String>>,
            Json(body): Json<Value>,
        | {
            let log = Arc::clone(&page_log);
            let fail = Arc::clone(&page_failure);
            let restore_fail = Arc::clone(&restore_failure);
            async move {
                let action = query.get("action").expect("fixture page action");
                log.lock().expect("fixture log").push(json!({
                    "action": action, "filename": body["filename"]
                }));
                if (action == "save" && fail.load(Ordering::SeqCst))
                    || (action == "restore" && restore_fail.load(Ordering::SeqCst))
                {
                    (StatusCode::BAD_REQUEST, Json(json!({"error": {"code": 400, "type": "invalid_request_error", "message": "fixture page rejected"}})))
                } else {
                    let mut receipt = json!({"id_slot": 0, "filename": body["filename"]});
                    receipt[if action == "save" { "n_saved" } else { "n_restored" }] = json!(4);
                    (StatusCode::OK, Json(receipt))
                }
            }
        }))
        .route("/v1/chat/completions", post(move |Json(body): Json<Value>| {
            let log = Arc::clone(&generate_log);
            let mode = generate_mode.load(Ordering::SeqCst);
            let release = Arc::clone(&generate_release);
            async move {
                log.lock().expect("fixture log").push(json!({"action": "generate", "body": body}));
                if mode == 1 {
                    return (StatusCode::BAD_REQUEST, "fixture generation failed").into_response();
                }
                if mode == 5 {
                    return ([("content-type", "text/event-stream")],
                        "data: {\"model\":\"foreign-alias\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"ok\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":5555,\"completion_tokens\":1}}\n\ndata: [DONE]\n\n").into_response();
                }
                const PARTIAL: &str = "data: {\"model\":\"test-model\",\"choices\":[{\"delta\":{\"content\":\"partial\"}}],\"usage\":{\"prompt_tokens\":4444,\"completion_tokens\":1}}\n\n";
                if mode == 2 {
                    return ([("content-type", "text/event-stream")], PARTIAL).into_response();
                }
                if mode == 3 || mode == 4 {
                    use futures::StreamExt;
                    let stream = futures::stream::once(async {
                        Ok::<_, std::io::Error>(PARTIAL)
                    }).chain(futures::stream::once(async move {
                        if mode == 4 {
                            release.notified().await;
                        } else {
                            tokio::task::yield_now().await;
                        }
                        Err::<&str, _>(std::io::Error::other("fixture interrupted stream"))
                    }));
                    return ([("content-type", "text/event-stream")], axum::body::Body::from_stream(stream)).into_response();
                }
                ([ ("content-type", "text/event-stream") ],
                 "data: {\"model\":\"test-model\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"ok\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":4,\"completion_tokens\":1,\"total_tokens\":5,\"prompt_tokens_details\":{\"cached_tokens\":3}}}\n\ndata: [DONE]\n\n").into_response()
            }
        }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("isolated HTTP fixture");
    let address = listener.local_addr().expect("bound address");
    let server = tokio::spawn(async move { axum::serve(listener, app).await });
    let mut adapter = test_adapter();
    adapter.config.base_url = format!("http://{address}");
    adapter.config.llamacpp_sampling_extensions = true;
    adapter.concurrency = Arc::new(tokio::sync::Semaphore::new(1));
    let root = adapter.endpoints().root().to_owned();
    let endpoint = crate::inference::slots::directory().endpoint(&root);
    let transition = endpoint.transition().await;
    let generation = transition.start_generation().expect("fixture engine generation");
    transition.ready(&root, &generation, crate::inference::slots::KvPageContract {
        model_id: "test-model".into(), model: "fixture.gguf".into(), adapters: vec![],
        page_dir: None, context: 8192, slots: 1, cache_type: None, flash_attn: None,
        revisions: None,
    }).expect("fixture owned engine readiness");
    drop(transition);
    assert!(matches!(adapter.ensure_slot_pool(&adapter.endpoints()).await,
        Err(crate::ai::slot_discovery::SlotDiscoveryMiss::GeometryMismatch { .. })),
        "props cannot authorize an already installed pool with different geometry");
    assert!(!crate::inference::slots::directory().page_admission_known(&root));
    props_slots.store(1, Ordering::SeqCst);
    assert!(adapter.ensure_slot_pool(&adapter.endpoints()).await.is_ok());
    assert!(crate::inference::slots::directory().page_admission_known(&root));
    let owned_geometry = endpoint.served_geometry().expect("fixture owned geometry");
    let observed_service = || crate::inference::resource_admission::leased_in_sent_samples()
        .iter().filter_map(|sample| sample.served(&root, Some("test-model"))).collect::<Vec<_>>();
    let activity =
        crate::inference::slots::PageKey::mind(uuid::Uuid::new_v4())
            .expect("non-nil fixture activity");
    let turn = TextGenerationRequest {
        messages: vec![ChatMessage::text("user", "activity")],
        persona_id: Some(activity.persona().to_string()),
        room_id: Some(uuid::Uuid::new_v4().to_string()),
        purpose: Some("cognition/deliberation".into()),
        ..Default::default()
    };
    let background = TextGenerationRequest {
        messages: vec![ChatMessage::text("user", "background")],
        purpose: Some("dream-belief-review".into()),
        ..Default::default()
    };
    for (request, fails) in [
        (turn.clone(), false),
        (background.clone(), false),
        (turn.clone(), false),
        (background.clone(), true),
        (turn.clone(), false),
    ] {
        fail_save.store(fails, Ordering::SeqCst);
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            adapter.generate_text(request),
        )
        .await
        .expect("bounded adapter fixture")
        .expect("fixture generation");
    }
    // Another MIND's turn: hers in another room would be her own page (one page per mind).
    let failed_turn = TextGenerationRequest {
        persona_id: Some(uuid::Uuid::new_v4().to_string()),
        ..turn.clone()
    };
    assert!(!observed_service().is_empty(), "real adapter terminal records owned service");
    assert!(observed_service().iter().all(|tokens| *tokens == 4), "full prompt includes cached prefix; evaluated suffix is not the count");
    assert_eq!(crate::inference::resource_admission::PromptSample::geometry_median(
        &crate::inference::resource_admission::leased_in_sent_samples(), &root, &owned_geometry,
    ), Some(4), "real adapter terminal preserves launch model, window, lanes and generation");
    generation_mode.store(1, Ordering::SeqCst);
    assert!(tokio::time::timeout(
        std::time::Duration::from_secs(5),
        adapter.generate_text(failed_turn.clone()),
    )
    .await
    .expect("bounded failed generation")
    .is_err());
    generation_mode.store(0, Ordering::SeqCst);
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        adapter.generate_text(turn.clone()),
    )
    .await
    .expect("bounded return after failure")
    .expect("restored activity generation");
    let initial = std::mem::take(&mut *operations.lock().expect("actual HTTP operations"));
    let actions: Vec<_> = initial
        .iter()
        .map(|op| op["action"].as_str().unwrap())
        .collect();
    assert_eq!(
        actions,
        [
            "generate", "save", "generate", "restore", "generate", "save", "generate",
            "generate", "save", "generate", "restore", "generate"
        ]
    );
    let filename = crate::inference::slots::page_filename(&activity);
    for op in initial.iter().filter(|op| op["action"] != "generate") {
        assert_eq!(op["filename"], filename);
    }
    let requests: Vec<_> = initial.iter().filter_map(|op| op.get("body")).collect();
    for (index, body) in requests.iter().enumerate() {
        assert_eq!(body["id_slot"], 0, "every request uses the one actual slot");
        assert_eq!(
            body["cache_prompt"],
            [true, false, true, false, true, true, true][index]
        );
    }

    // Neither an early clean EOF nor a transport error establishes a warm
    // activity. A's saved page must survive and B must never be saved.
    for mode in [2, 3, 4] {
        generation_mode.store(mode, Ordering::SeqCst);
        if mode == 4 {
            // Poll the real client until it receives partial output, then
            // cancel it. Releasing this fixture stream separately makes no
            // assertion that cancellation stops a production backend.
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            {
                let generation = adapter.generate_stream(failed_turn.clone(), tx);
                tokio::pin!(generation);
                tokio::time::timeout(std::time::Duration::from_secs(5), async {
                    tokio::select! {
                        result = &mut generation => panic!("stream completed before cancellation: {result:?}"),
                        chunk = rx.recv() => assert!(matches!(chunk, Some(GenerationChunk::Token(_)))),
                    }
                }).await.expect("bounded partial stream");
            }
            assert_eq!(adapter.concurrency.available_permits(), 1);
            release_stream.notify_one();
        } else {
            let result = tokio::time::timeout(
                std::time::Duration::from_secs(5),
                adapter.generate_text(failed_turn.clone()),
            )
            .await
            .expect("bounded interrupted generation");
            if mode == 3 {
                assert!(result.is_err(), "stream read failure must remain an error");
            } else {
                // Existing response compatibility accepts EOF; the cache
                // ledger must still require an actual successful terminal.
                assert_eq!(result.expect("existing EOF response").text, "partial");
            }
        }
        assert!(!observed_service().contains(&4444), "partial usage before EOF/error/cancellation cannot certify service");
        generation_mode.store(0, Ordering::SeqCst);
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            adapter.generate_text(turn.clone()),
        )
        .await
        .expect("bounded restore")
        .expect("return to A");
        let observed = std::mem::take(&mut *operations.lock().expect("HTTP operations"));
        assert_eq!(
            observed
                .iter()
                .map(|op| op["action"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["save", "generate", "restore", "generate"],
            "mode {mode}"
        );
        for op in observed.iter().filter(|op| op["action"] != "generate") {
            assert_eq!(
                op["filename"], filename,
                "never persist incomplete B as resident"
            );
        }
    }

    // A failed restore invalidates the saved-page claim even when the
    // following generation also fails; retry must prefill, not retry that page.
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        adapter.generate_text(background),
    )
    .await
    .expect("bounded background")
    .expect("background");
    fail_restore.store(true, Ordering::SeqCst);
    generation_mode.store(1, Ordering::SeqCst);
    assert!(tokio::time::timeout(
        std::time::Duration::from_secs(5),
        adapter.generate_text(turn.clone())
    )
    .await
    .expect("bounded failed restore")
    .is_err());
    fail_restore.store(false, Ordering::SeqCst);
    generation_mode.store(0, Ordering::SeqCst);
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        adapter.generate_text(turn),
    )
    .await
    .expect("bounded cold retry")
    .expect("cold retry");
    let observed = std::mem::take(&mut *operations.lock().expect("HTTP operations"));
    assert_eq!(
        observed
            .iter()
            .map(|op| op["action"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["save", "generate", "restore", "generate", "generate"]
    );
    generation_mode.store(5, Ordering::SeqCst);
    tokio::time::timeout(std::time::Duration::from_secs(5), adapter.generate_text(TextGenerationRequest::default()))
        .await.expect("bounded foreign alias response").expect("response compatibility");
    assert!(!crate::inference::resource_admission::leased_in_sent_samples().iter()
        .any(|sample| sample.served(&root, Some("foreign-alias")).is_some()),
        "a wire alias that differs from the admitted engine cannot certify local service");
    server.abort();
    let _ = server.await;
}

// what this catches (6780e79b): API/capability labels cannot decide admission;
// the actual endpoint must match the shared engine, excluding dedicated lanes.
#[test]
fn generation_admission_follows_effective_endpoint_not_provider_label() {
    use crate::ai::adapter::GenerationAdmissionOwner;
    let mut adapter = test_adapter();
    adapter.config.provider_id = "arbitrary-http-protocol".into();
    assert!(!adapter.capabilities().is_local, "historical HTTP metadata is remote");
    assert_eq!(adapter.generation_admission_for("http://127.0.0.1:0/v1/"), GenerationAdmissionOwner::SharedLocal);
    assert_eq!(adapter.generation_admission_for("http://127.0.0.1:42"), GenerationAdmissionOwner::Adapter);
    let adapter = adapter.with_dedicated_lane();
    assert_eq!(adapter.generation_admission_for("http://127.0.0.1:0"), GenerationAdmissionOwner::Adapter);
}
