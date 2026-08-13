//! End-to-end integration test: `AircRemoteInferenceAdapter` round-trips
//! against a substrate that runs the FULL receive→dispatch→reply loop
//! through its real `CommandExecutor` + `ServiceModule` chain.
//!
//! ## What this test adds beyond PR #1560's wire test
//!
//! `tests/airc_remote_inference_roundtrip.rs` (PR #1560) proves the
//! wire SHAPE is correct: substrate's `parse_envelope` + `send_reply`
//! accept and produce the right envelope, headers, body. But it
//! HAND-CANS the response — it never actually runs an adapter. That
//! left a load-bearing gap: the substrate's `CommandExecutor` →
//! `ServiceModule` → `AIProviderAdapter` chain is untested across
//! the airc wire.
//!
//! This test closes that gap. peer_a runs:
//!
//! ```text
//!   CommandRequestHandler::on_envelope
//!     -> parse_envelope                      [REAL]
//!     -> process_request_via(&executor)      [REAL]
//!        -> CommandExecutor::execute_with_caller
//!           -> AuthPolicy::gate (AllowAll)   [REAL]
//!           -> TestInferenceModule::handle_command
//!              -> HeuristicInferenceAdapter::generate_text  [REAL]
//!        -> AircCommandResponse::ok(result)
//!     -> send_reply                          [REAL]
//! ```
//!
//! No hand-canned responses. The heuristic adapter's signature output
//! `[heuristic:<8-char-hash>] ack: "<prompt-tail>"` is the proof —
//! a canned test stub can't produce that without actually running.
//!
//! ## Why a test-only ServiceModule (not AIProviderModule)
//!
//! `AIProviderModule` uses a process-global `AdapterRegistry`
//! (`ai/provider.rs:67-77 GLOBAL_REGISTRY`). Multi-test parallelism
//! would race on it. The substrate's wire path is module-agnostic
//! though — any `ServiceModule` registered for `"ai/generate"` exercises
//! the same dispatch chain. So this test uses a tiny `TestInferenceModule`
//! that wraps an `Arc<dyn AIProviderAdapter>` directly. Each test owns
//! its own adapter instance; no global registry pollution; tests can
//! run in parallel.
//!
//! ## Test cases
//!
//! 1. `end_to_end_heuristic_dispatch_through_substrate_stack` — happy
//!    path. HeuristicInferenceAdapter answers; the [heuristic:...]
//!    prefix proves a real adapter ran.
//!
//! 2. `end_to_end_peer_adapter_failure_surfaces_as_typed_error` —
//!    adapter returns Err; substrate's `execute_with_caller` wraps
//!    as `AircCommandResponse::error`; caller's `AircLiveTransport`
//!    surfaces `RemoteInferenceError::PeerAdapterFailed`. Closes
//!    task #218 (deferred from PR #1560 R3-N3).
//!
//! 3. `end_to_end_missing_module_returns_typed_error` — peer_a's
//!    executor has NO Rust module for `ai/generate`. Substrate-side
//!    `execute_with_caller` doesn't return "no module" directly: it
//!    falls through to the legacy TypeScript bridge at
//!    `/tmp/jtag-command-router.sock` (historical path for
//!    unmigrated commands). Since the bridge isn't running in tests,
//!    the caller sees the bridge-connect error verbatim. This test
//!    observes (not endorses) that behavior — the substrate's fall-
//!    through to TS-land for un-Rust-handled paths is itself a
//!    `[[no-fallbacks-ever]]` smell that wants its own follow-up
//!    slice. Until then, we pin the observed surface so any change
//!    is loud rather than silent.

use std::sync::Arc;

use airc_test_fixtures::TwoAircLoopback;
use async_trait::async_trait;
use continuum_core::ai::adapter::{
    AIProviderAdapter, AdapterCapabilities, ApiStyle, InferenceDevice,
};
use continuum_core::ai::heuristic_adapter::HeuristicInferenceAdapter;
use continuum_core::ai::types::{
    ChatMessage, FinishReason, HealthState, HealthStatus, MessageContent, ModelInfo,
    TextGenerationRequest, TextGenerationResponse,
};
use continuum_core::inference::airc_remote::{AircLiveTransport, AircRemoteInferenceAdapter};
use continuum_core::routing::CommandRequestHandler;
use continuum_core::runtime::command_executor::CommandExecutor;
use continuum_core::runtime::{
    CommandResult, ModuleConfig, ModulePriority, ModuleRegistry, ServiceModule,
};
use futures::stream::StreamExt;

/// A minimal test-only ServiceModule that routes `ai/generate` to an
/// injected `AIProviderAdapter`. Mirrors the shape of
/// `runtime/command_executor.rs::CannedModule` but parameterized over
/// an adapter so tests can swap heuristic / failing / no-op behaviors.
struct TestInferenceModule {
    adapter: Arc<dyn AIProviderAdapter>,
}

impl TestInferenceModule {
    const PREFIXES: &'static [&'static str] = &["ai/generate"];

    fn new(adapter: Arc<dyn AIProviderAdapter>) -> Self {
        Self { adapter }
    }
}

#[async_trait]
impl ServiceModule for TestInferenceModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "test-inference",
            priority: ModulePriority::Normal,
            command_prefixes: Self::PREFIXES,
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    async fn initialize(
        &self,
        _ctx: &continuum_core::runtime::ModuleContext,
    ) -> Result<(), String> {
        Ok(())
    }

    async fn handle_command(
        &self,
        _command: &str,
        params: serde_json::Value,
    ) -> Result<CommandResult, String> {
        let request: TextGenerationRequest = serde_json::from_value(params)
            .map_err(|e| format!("TestInferenceModule: decode TextGenerationRequest: {e}"))?;
        let response = self.adapter.generate_text(request).await?;
        let value = serde_json::to_value(&response)
            .map_err(|e| format!("TestInferenceModule: serialize TextGenerationResponse: {e}"))?;
        Ok(CommandResult::Json(value))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// Adapter that always returns an error string. Lets the error-path
/// test exercise the AircCommandResponse::Error variant end-to-end
/// without depending on a real model's failure mode.
struct AlwaysFailingAdapter {
    message: String,
}

#[async_trait]
impl AIProviderAdapter for AlwaysFailingAdapter {
    fn provider_id(&self) -> &str {
        "test-always-failing"
    }
    fn name(&self) -> &str {
        "test-always-failing"
    }
    fn capabilities(&self) -> AdapterCapabilities {
        AdapterCapabilities::default()
    }
    fn api_style(&self) -> ApiStyle {
        ApiStyle::Local
    }
    fn default_model(&self) -> &str {
        "test-always-failing/no-model"
    }
    async fn initialize(&mut self) -> Result<(), String> {
        Ok(())
    }
    async fn shutdown(&mut self) -> Result<(), String> {
        Ok(())
    }
    async fn generate_text(
        &self,
        _request: TextGenerationRequest,
    ) -> Result<TextGenerationResponse, String> {
        Err(self.message.clone())
    }
    async fn health_check(&self) -> HealthStatus {
        HealthStatus {
            status: HealthState::Unhealthy,
            api_available: false,
            response_time_ms: 0,
            error_rate: 1.0,
            last_checked: 0,
            message: Some("always-failing".to_string()),
        }
    }
    async fn get_available_models(&self) -> Vec<ModelInfo> {
        Vec::new()
    }
    fn device_type(&self) -> InferenceDevice {
        InferenceDevice::Cpu
    }
}

/// Build a `CommandRequestHandler` wired against a `CommandExecutor`
/// whose `ModuleRegistry` holds the supplied module (or an empty
/// registry, when `module` is `None`, to exercise the missing-module
/// error path).
fn build_handler(
    peer_a: Arc<airc_lib::Airc>,
    module: Option<Arc<dyn ServiceModule>>,
) -> Arc<CommandRequestHandler> {
    let registry = Arc::new(ModuleRegistry::new());
    if let Some(m) = module {
        registry.register(m);
    }
    let executor = Arc::new(CommandExecutor::new(registry));
    CommandRequestHandler::new(peer_a, executor)
}

/// Generic substrate-side responder. Subscribes to peer_a's inbound
/// stream and dispatches every command-request-shaped envelope
/// through `handler.on_envelope()` — the SAME code path a productized
/// peer-side adapter registry would call.
///
/// Returns the JoinHandle so the test can await task completion.
async fn spawn_substrate_responder(
    handler: Arc<CommandRequestHandler>,
    peer_a: Arc<airc_lib::Airc>,
    ready: Arc<tokio::sync::Notify>,
) -> tokio::task::JoinHandle<()> {
    use airc_lib::adapter::ConsumerAdapter;

    let self_id = peer_a.peer_id();
    let body_hint_filter = handler.body_hint();
    tokio::spawn(async move {
        let mut stream = peer_a.subscribe().await.expect("peer_a subscribe");
        ready.notify_one();
        while let Some(event) = stream.next().await {
            let event = match event {
                Ok(e) => e,
                Err(_) => continue,
            };
            if event.peer_id == self_id {
                continue;
            }
            let hint = match event
                .headers
                .get(continuum_airc_protocol::HEADER_CONTINUUM_BODY_HINT)
            {
                Some(h) => h,
                None => continue,
            };
            if hint != body_hint_filter {
                continue;
            }
            // SAME entry point a productized airc adapter-registry
            // dispatch loop would call. The handler internally goes
            // parse_envelope -> process_request -> send_reply.
            //
            // airc-lib's `subscribe()` yields `Arc<TranscriptEvent>`;
            // `ConsumerAdapter::on_envelope` takes the owned value, so
            // clone the inner. Production dispatch loops would do the
            // same (the Arc only exists to fan-out to multiple
            // subscribers).
            handler
                .on_envelope((*event).clone())
                .await
                .expect("substrate handler on_envelope");
            return;
        }
    })
}

fn user_msg(text: &str) -> ChatMessage {
    ChatMessage {
        role: "user".to_string(),
        content: MessageContent::Text(text.to_string()),
        name: None,
    }
}

fn request(prompt: &str) -> TextGenerationRequest {
    TextGenerationRequest {
        messages: vec![user_msg(prompt)],
        system_prompt: None,
        model: None,
        provider: None,
        temperature: None,
        max_tokens: None,
        top_p: None,
        top_k: None,
        repeat_penalty: None,
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

// ── Happy path ─────────────────────────────────────────────────────

#[tokio::test]
async fn end_to_end_heuristic_dispatch_through_substrate_stack() {
    let loop_back = TwoAircLoopback::new()
        .await
        .expect("fixture setup should succeed");

    // peer_a hosts HeuristicInferenceAdapter via a real CommandExecutor +
    // a TestInferenceModule routing `ai/generate`. No canned response.
    let heuristic: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let module: Arc<dyn ServiceModule> = Arc::new(TestInferenceModule::new(heuristic));
    let handler = build_handler(Arc::clone(loop_back.peer_a()), Some(module));

    let responder_ready = Arc::new(tokio::sync::Notify::new());
    let responder = spawn_substrate_responder(
        Arc::clone(&handler),
        Arc::clone(loop_back.peer_a()),
        Arc::clone(&responder_ready),
    )
    .await;
    responder_ready.notified().await;

    // peer_b builds the cross-grid adapter pointed at peer_a.
    let transport = AircLiveTransport::new(Arc::clone(loop_back.peer_b()), loop_back.peer_a_id());
    let adapter = AircRemoteInferenceAdapter::new(transport);

    // Dispatch + assert.
    let response = adapter
        .generate_text(request("hello grid"))
        .await
        .expect("end-to-end heuristic round-trip");

    // The heuristic adapter's signature output proves a REAL adapter
    // executed substrate-side — no canned stub can produce this prefix.
    assert!(
        response.text.starts_with("[heuristic:"),
        "expected heuristic-adapter signature prefix; got {:?}",
        response.text
    );
    // The prompt propagated through every layer.
    assert!(
        response.text.contains("hello grid"),
        "expected prompt echo; got {:?}",
        response.text
    );
    // The adapter-side layer rewrites `provider` to "airc-remote" so
    // observability can tell traffic came via cross-grid dispatch.
    assert_eq!(response.provider, "airc-remote");
    assert_eq!(response.finish_reason, FinishReason::Stop);

    responder.await.expect("responder task joined");
}

// ── Adapter failure path ──────────────────────────────────────────

#[tokio::test]
async fn end_to_end_peer_adapter_failure_surfaces_as_typed_error() {
    let loop_back = TwoAircLoopback::new()
        .await
        .expect("fixture setup should succeed");

    // peer_a registers the AlwaysFailingAdapter. The substrate runs
    // the full dispatch chain; the adapter's Err propagates through
    // execute_with_caller -> AircCommandResponse::error.
    let failing: Arc<dyn AIProviderAdapter> = Arc::new(AlwaysFailingAdapter {
        message: "the model exploded".to_string(),
    });
    let module: Arc<dyn ServiceModule> = Arc::new(TestInferenceModule::new(failing));
    let handler = build_handler(Arc::clone(loop_back.peer_a()), Some(module));

    let responder_ready = Arc::new(tokio::sync::Notify::new());
    let responder = spawn_substrate_responder(
        Arc::clone(&handler),
        Arc::clone(loop_back.peer_a()),
        Arc::clone(&responder_ready),
    )
    .await;
    responder_ready.notified().await;

    let transport = AircLiveTransport::new(Arc::clone(loop_back.peer_b()), loop_back.peer_a_id());
    let adapter = AircRemoteInferenceAdapter::new(transport);

    let err = adapter
        .generate_text(request("doomed"))
        .await
        .expect_err("AlwaysFailingAdapter must surface as adapter-failure error");

    // AircRemoteInferenceAdapter classifies a non-Ok
    // AircCommandResponse as RemoteInferenceError::PeerAdapterFailed.
    // The Display impl in protocol.rs is literally
    //   "remote peer's adapter failed: <inner-error>"
    // (note: apostrophe-s, NOT "peer adapter failed"). AND-pin BOTH
    // the variant surface AND the inner message so the test catches:
    //   a) the wrong variant being returned (would miss "peer's adapter")
    //   b) the error message being swallowed or rewritten en route
    //      (would miss "the model exploded")
    // Per R1 round 1 on PR #1563: an OR-fallback masks variant drift —
    // the first-passing branch makes the second branch dead.
    assert!(
        err.contains("peer's adapter failed"),
        "expected RemoteInferenceError::PeerAdapterFailed Display surface \
         ('remote peer's adapter failed: ...'); got {err:?}"
    );
    assert!(
        err.contains("the model exploded"),
        "expected substrate-side error message to propagate verbatim; got {err:?}"
    );

    responder.await.expect("responder task joined");
}

// ── Missing-module path ───────────────────────────────────────────

#[tokio::test]
async fn end_to_end_missing_module_returns_typed_error() {
    let loop_back = TwoAircLoopback::new()
        .await
        .expect("fixture setup should succeed");

    // peer_a's executor has NO module for ai/generate. Substrate's
    // CommandExecutor returns a typed "no handler" error; that
    // propagates through send_reply as AircCommandResponse::error.
    let handler = build_handler(Arc::clone(loop_back.peer_a()), None);

    let responder_ready = Arc::new(tokio::sync::Notify::new());
    let responder = spawn_substrate_responder(
        Arc::clone(&handler),
        Arc::clone(loop_back.peer_a()),
        Arc::clone(&responder_ready),
    )
    .await;
    responder_ready.notified().await;

    let transport = AircLiveTransport::new(Arc::clone(loop_back.peer_b()), loop_back.peer_a_id());
    let adapter = AircRemoteInferenceAdapter::new(transport);

    let err = adapter
        .generate_text(request("ping a void"))
        .await
        .expect_err("missing module must produce a typed error");

    // What we OBSERVE today: CommandExecutor doesn't shortcut on a
    // missing Rust module — it tries the TypeScript bridge at
    // `/tmp/jtag-command-router.sock` (legacy router for unmigrated
    // commands). The bridge isn't running in tests, so the caller
    // sees the connect failure verbatim.
    //
    // Architecturally this is a `[[no-fallbacks-ever]]` violation —
    // a Rust-only deployment should hard-error with the missing
    // module name immediately, not silently route to TS-land. Filed
    // as task #219.
    //
    // Per R1 round 1 on PR #1563: pin EXACTLY the current
    // bridge-passthrough surface — NOT a permissive OR over plausible
    // future error shapes. When task #219 lands, this test SHOULD
    // fail loudly with "actual" not matching "commandrouterserver",
    // forcing the test author to update to the new typed
    // missing-module error AND verify the substrate fix actually
    // produces it. A permissive assertion would silently stay green
    // and let either a good fix (typed error) or a bad regression
    // (200 on nothing) pass undetected.
    let lower = err.to_lowercase();
    assert!(
        lower.contains("commandrouterserver") || lower.contains("jtag-command-router"),
        "expected the TS-bridge connect-failure surface — the substrate \
         currently falls through to /tmp/jtag-command-router.sock on \
         missing Rust modules. If THIS assertion fired because task #219 \
         landed and CommandExecutor now hard-errors on missing modules, \
         update the assertion to pin the new typed-error surface and \
         verify the substrate change. Got: {err:?}"
    );

    responder.await.expect("responder task joined");
}
