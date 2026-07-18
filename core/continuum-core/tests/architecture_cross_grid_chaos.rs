//! Architecture test — proves the "cross-grid composition" doctrine
//! clause via an adversarial / chaos test (shape 4).
//!
//! See `docs/architecture/PROVING-THE-DOCTRINE.md` for the matrix
//! this file populates. The clause pinned here:
//!
//! > "Cross-grid composition — peer subscriptions work over airc.
//! > When a remote peer disconnects mid-stream (crashes, hangs, never
//! > replies), the caller's transport surfaces a TYPED error within
//! > the configured deadline, never hangs, and remains usable for
//! > the next request."
//!
//! ## Why an adversarial / chaos test
//!
//! The happy-path roundtrip (`airc_remote_inference_roundtrip.rs`)
//! proves the substrate's wire shape works END-TO-END when both peers
//! cooperate. The chaos shape proves the substrate handles a peer
//! that STOPS cooperating — the only way the substrate's federation
//! claim ("absent or hostile peer cannot dominate") can be verified
//! is to FEED IT silence and watch what happens, per Shape 4.
//!
//! ## Why drive the transport directly, not the adapter
//!
//! `AircRemoteInferenceAdapter::generate_text` flattens its typed
//! errors into `Result<_, String>` to match the
//! `AIProviderAdapter` trait. The CHAOS proof needs the typed
//! variant — `RemoteInferenceError::Timeout { elapsed_ms }` — so
//! we drive `AircLiveTransport::send_request(RemoteInferenceRequest)`
//! directly. That's the seam where the typed semantics live, and
//! exercising it is what proves the doctrine clause at the right
//! granularity.
//!
//! ## What this proves
//!
//! 1. `silent_peer_request_times_out_with_typed_error`:
//!    - Wire two peers via `TwoAircLoopback`.
//!    - Peer A spawns a "silent" responder that subscribes and OBSERVES
//!      every incoming request but NEVER calls `send_reply`.
//!    - Peer B's `AircLiveTransport::with_deadline(300ms)` dispatches
//!      a real `RemoteInferenceRequest` at peer A.
//!    - Within the deadline window, `send_request` returns
//!      `Err(RemoteInferenceError::Timeout { elapsed_ms })`, NOT a
//!      hang, NOT a panic, NOT a generic transport error.
//!    - `elapsed_ms` is in the deadline neighborhood (sanity check
//!      that the deadline plumbing is honored end-to-end).
//!
//! 2. `transport_remains_callable_after_peer_timeout`:
//!    - Same fixture. First request times out (silent responder).
//!    - Then a NORMAL responder is spawned on peer A and a SECOND
//!      request succeeds with a typed `Ok`.
//!    - Proves the transport has no global state corruption — a
//!      timed-out request doesn't poison the airc-lib correlation
//!      tables or wedge subsequent dispatches.
//!
//! Together: peer silence is recoverable; peer crash is recoverable;
//! the substrate's federation-by-default story holds under disconnect.
//!
//! ## What this does NOT cover (intentionally — follow-up shapes)
//!
//! - Malformed reply (wrong correlation id, wrong body) — that's a
//!   decode-error shape, separate from disconnect.
//! - Half-open TCP (peer hung in kernel buffer with no FIN) — needs
//!   tcpkill-style harness; not loop-back-fixture friendly.
//! - Hostile flood (peer DoSes by emitting events without correlation)
//!   — tracked under matrix row "Backpressure is intrinsic".
//! - Sentinel-quorum domination (malicious peer claims authority it
//!   doesn't have) — tracked under "Federated alignment".
//!
//! ## Tag
//!
//! proves: cross-grid composition (peer disconnect mid-stream surfaces
//! typed error within deadline; transport stays usable after)

use std::sync::Arc;
use std::time::{Duration, Instant};

use airc_test_fixtures::TwoAircLoopback;
use continuum_airc_protocol::{
    AircCommandResponse, COMMAND_REQUEST_BODY_HINT, HEADER_CONTINUUM_BODY_HINT,
};
use continuum_core::ai::types::{
    ChatMessage, FinishReason, MessageContent, TextGenerationRequest, TextGenerationResponse,
    UsageMetrics,
};
use continuum_core::inference::airc_remote::{
    AircInferenceTransport, AircLiveTransport, RemoteInferenceError, RemoteInferenceRequest,
};
use continuum_core::routing::CommandRequestHandler;
use continuum_core::runtime::command_executor::CommandExecutor;
use continuum_core::runtime::ModuleRegistry;
use futures::stream::StreamExt;
use tokio::sync::Notify;

/// Deadline for the chaos requests. Has to be (a) long enough that
/// the fixture finishes its real LAN handshake before the deadline
/// fires (so the test isn't proving "fixture is slow"), and (b) short
/// enough that the test runs in under a second so CI doesn't groan.
/// 300ms threads that needle on the loopback fixture today.
const CHAOS_DEADLINE: Duration = Duration::from_millis(300);

/// Spawn a "silent" responder on `peer`: it subscribes, sees the
/// request land, signals the test it received the request (so the
/// test can be SURE the request was wire-delivered, not lost), then
/// drops the request on the floor and exits. No `send_reply` is ever
/// called — the caller will hit its deadline.
async fn spawn_silent_responder(
    peer: Arc<airc_lib::Airc>,
    received: Arc<Notify>,
    subscribed: Arc<Notify>,
) -> tokio::task::JoinHandle<()> {
    let self_id = peer.peer_id();
    tokio::spawn(async move {
        let mut stream = peer.subscribe().await.expect("peer subscribe");
        subscribed.notify_one();
        while let Some(event) = stream.next().await {
            let event = match event {
                Ok(e) => e,
                Err(_) => continue,
            };
            if event.peer_id == self_id {
                continue;
            }
            // Only count actual command-request envelopes — keepalive
            // and control frames don't count as "request received."
            let hint = match event.headers.get(HEADER_CONTINUUM_BODY_HINT) {
                Some(h) => h.clone(),
                None => continue,
            };
            if hint == COMMAND_REQUEST_BODY_HINT {
                received.notify_one();
                // Done. Drop the request on the floor; the caller's
                // deadline does the proving.
                return;
            }
        }
    })
}

/// Spawn a normal responder that DOES send a typed reply. Used by
/// the "callable after timeout" test to prove the transport recovers.
async fn spawn_replying_responder(
    handler: Arc<CommandRequestHandler>,
    peer: Arc<airc_lib::Airc>,
    canned: TextGenerationResponse,
    subscribed: Arc<Notify>,
) -> tokio::task::JoinHandle<()> {
    let self_id = peer.peer_id();
    tokio::spawn(async move {
        let mut stream = peer.subscribe().await.expect("peer subscribe");
        subscribed.notify_one();
        while let Some(event) = stream.next().await {
            let event = match event {
                Ok(e) => e,
                Err(_) => continue,
            };
            if event.peer_id == self_id {
                continue;
            }
            let hint = match event.headers.get(HEADER_CONTINUUM_BODY_HINT) {
                Some(h) => h,
                None => continue,
            };
            if hint != COMMAND_REQUEST_BODY_HINT {
                continue;
            }
            let parsed = match CommandRequestHandler::parse_envelope(&event) {
                Ok(p) => p,
                Err(e) => panic!("substrate parse_envelope rejected envelope: {e:?}"),
            };
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

fn build_handler(peer: Arc<airc_lib::Airc>) -> Arc<CommandRequestHandler> {
    let registry = Arc::new(ModuleRegistry::new());
    let executor = Arc::new(CommandExecutor::new(registry));
    CommandRequestHandler::new(peer, executor)
}

fn build_text_request() -> TextGenerationRequest {
    TextGenerationRequest {
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: MessageContent::Text("ping into the void".to_string()),
            name: None,
        }],
        system_prompt: Some("chaos test — silent peer".to_string()),
        model: Some("nonexistent-but-routable".to_string()),
        provider: None,
        temperature: Some(0.7),
        max_tokens: Some(16),
        top_p: None,
        top_k: None,
        repeat_penalty: None,
        frequency_penalty: None,
        repeat_last_n: None,
        stop_sequences: None,
        tools: None,
        tool_choice: None,
        response_format: None,
        active_adapters: None,
        request_id: None,
        user_id: None,
        room_id: None,
        purpose: None,
        persona_id: None,
    }
}

// proves: cross-grid composition (silent peer surfaces typed timeout
// error within deadline window — no hang, no panic, no swallowed error)
#[tokio::test]
async fn silent_peer_request_times_out_with_typed_error() {
    let loop_back = TwoAircLoopback::new()
        .await
        .expect("fixture setup should succeed");

    // peer_a hosts the (silent) responder.
    let received = Arc::new(Notify::new());
    let subscribed = Arc::new(Notify::new());
    let responder = spawn_silent_responder(
        Arc::clone(loop_back.peer_a()),
        Arc::clone(&received),
        Arc::clone(&subscribed),
    )
    .await;

    // Wait for peer_a's subscribe() to land in the broadcast window
    // BEFORE peer_b dispatches — same R3-N4 deterministic barrier
    // pattern as the happy-path roundtrip test, otherwise the test
    // flakes when subscribe() races the dispatch.
    subscribed.notified().await;

    // peer_b dispatches via a tight-deadline transport.
    // Note: `AircLiveTransport::new` returns `Arc<Self>` and
    // `with_deadline` consumes `self`, so they don't chain. Since new()
    // yields a freshly-allocated Arc with refcount = 1, `Arc::into_inner`
    // reliably unwraps it. (Production API ergonomics nit — could be
    // fixed by making `with_deadline` take `Arc<Self>` or by having
    // `new` return `Self` and letting the caller Arc it themselves.)
    let transport = Arc::into_inner(AircLiveTransport::new(
        Arc::clone(loop_back.peer_b()),
        loop_back.peer_a_id(),
    ))
    .expect("freshly-allocated Arc must have refcount == 1")
    .with_deadline(CHAOS_DEADLINE);

    let request = RemoteInferenceRequest::new(build_text_request());
    let start = Instant::now();
    let result = transport.send_request(request).await;
    let elapsed = start.elapsed();

    // The responder MUST have seen the request — proves silence isn't
    // because the wire is broken; it's because the peer chose silence.
    // 1 second is generous; the wire delivery is sub-100ms on loopback.
    tokio::time::timeout(Duration::from_secs(1), received.notified())
        .await
        .expect("silent responder must have observed the request on the wire");

    // The transport MUST surface RemoteInferenceError::Timeout.
    let err = result.expect_err("silent peer should not produce a successful response");
    match err {
        RemoteInferenceError::Timeout { elapsed_ms } => {
            // `elapsed_ms` is the TRUE wall-clock since `send_request`
            // entry — so it should be >= the configured deadline
            // (the transport waited at least that long) and not
            // dramatically larger (deadline must actually fire, not
            // get exceeded by airc-lib's own internal poll cycle).
            //
            // Bounds: deadline (300ms) <= elapsed_ms < deadline + 500ms
            // window for CI noise on loopback. A bug where the deadline
            // is silently ignored AND something else bounds the wait
            // would fail this assertion loudly, where the previous
            // 100..=2000ms range would have absorbed it.
            let deadline_ms = CHAOS_DEADLINE.as_millis() as u64;
            assert!(
                elapsed_ms >= deadline_ms.saturating_sub(50),
                "Timeout.elapsed_ms = {elapsed_ms} is below the \
                 {deadline_ms}ms deadline (allowing 50ms scheduling \
                 slop) — the transport returned before the deadline \
                 fired, meaning the deadline plumbing isn't honored \
                 end-to-end"
            );
            assert!(
                elapsed_ms <= deadline_ms + 500,
                "Timeout.elapsed_ms = {elapsed_ms} is too far past the \
                 {deadline_ms}ms deadline; airc-lib's own poll cycle \
                 should bound overshoot"
            );
        }
        other => panic!("expected RemoteInferenceError::Timeout for silent peer; got {other:?}"),
    }

    // Wall-clock sanity: deadline (300ms) + generous CI fudge.
    // A future bug where the deadline arg is silently dropped AND
    // airc-lib falls back to its internal multi-second default would
    // PASS a 2s upper bound — so cap at 800ms (deadline + 500ms).
    assert!(
        elapsed < Duration::from_millis(800),
        "send_request against silent peer took {elapsed:?} — \
         the {CHAOS_DEADLINE:?} deadline must bound the wall-clock; \
         drift past this window means deadline plumbing isn't \
         honored end-to-end"
    );

    responder.await.expect("silent responder task joined");
}

// proves: cross-grid composition (transport remains callable after a
// peer-side timeout — no global state corruption, no wedged
// correlation table, second request returns typed Ok)
#[tokio::test]
async fn transport_remains_callable_after_peer_timeout() {
    let loop_back = TwoAircLoopback::new()
        .await
        .expect("fixture setup should succeed");

    // ------ FIRST request: silent peer, expect Timeout ------
    let received_silent = Arc::new(Notify::new());
    let subscribed_silent = Arc::new(Notify::new());
    let silent = spawn_silent_responder(
        Arc::clone(loop_back.peer_a()),
        Arc::clone(&received_silent),
        Arc::clone(&subscribed_silent),
    )
    .await;
    subscribed_silent.notified().await;

    // Note: `AircLiveTransport::new` returns `Arc<Self>` and
    // `with_deadline` consumes `self`, so they don't chain. Since new()
    // yields a freshly-allocated Arc with refcount = 1, `Arc::into_inner`
    // reliably unwraps it. (Production API ergonomics nit — could be
    // fixed by making `with_deadline` take `Arc<Self>` or by having
    // `new` return `Self` and letting the caller Arc it themselves.)
    let transport = Arc::into_inner(AircLiveTransport::new(
        Arc::clone(loop_back.peer_b()),
        loop_back.peer_a_id(),
    ))
    .expect("freshly-allocated Arc must have refcount == 1")
    .with_deadline(CHAOS_DEADLINE);

    let first_result = transport
        .send_request(RemoteInferenceRequest::new(build_text_request()))
        .await;
    assert!(
        matches!(first_result, Err(RemoteInferenceError::Timeout { .. })),
        "first request against silent peer should time out; got {first_result:?}"
    );

    tokio::time::timeout(Duration::from_secs(1), received_silent.notified())
        .await
        .expect("silent responder must have observed the first request");
    silent.await.expect("silent responder joined");

    // ------ SECOND request: real reply, expect Ok ------
    let canned = TextGenerationResponse {
        text: "pong after the silence".to_string(),
        finish_reason: FinishReason::Stop,
        model: "test-model".to_string(),
        provider: "test-remote-llamacpp".to_string(),
        usage: UsageMetrics {
            input_tokens: 4,
            output_tokens: 4,
            total_tokens: 8,
            estimated_cost: None,
        },
        response_time_ms: 1,
        request_id: "chaos-recovery-req".to_string(),
        content: None,
        tool_calls: None,
        routing: None,
        error: None,
        reasoning: None,
        timing: None,
    };

    let handler = build_handler(Arc::clone(loop_back.peer_a()));
    let subscribed_reply = Arc::new(Notify::new());
    let replier = spawn_replying_responder(
        Arc::clone(&handler),
        Arc::clone(loop_back.peer_a()),
        canned.clone(),
        Arc::clone(&subscribed_reply),
    )
    .await;
    subscribed_reply.notified().await;

    // Same transport instance — proves the previous Timeout didn't
    // corrupt the airc-lib pending-correlation table or wedge the
    // transport. (If `transport` was effectively single-use, this
    // would surface as either a panic, a second Timeout, or a
    // CorrelationMismatch error.)
    let response = transport
        .send_request(RemoteInferenceRequest::new(build_text_request()))
        .await
        .expect("second request after a timeout should succeed");

    assert_eq!(response.text_response.text, canned.text);
    assert_eq!(response.text_response.finish_reason, FinishReason::Stop);
    assert_eq!(response.text_response.request_id, canned.request_id);

    replier.await.expect("replying responder joined");
}
