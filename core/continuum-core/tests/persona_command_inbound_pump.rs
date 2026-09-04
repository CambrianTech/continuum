//! Integration test: `PersonaCommandInboundPump` makes a persona-shaped
//! airc peer actually receive + dispatch cross-grid `ai/generate`
//! envelopes WITHOUT any manual handler wiring.
//!
//! ## What this proves beyond PRs #1560/#1563
//!
//! PR #1560 proved the wire SHAPE. PR #1563 proved a manually-wired
//! `CommandRequestHandler` runs the full substrate dispatch chain.
//! Both tests CONSTRUCTED the handler + spawned the subscribe loop
//! inline. In production today, NO code calls `CommandRequestHandler::new`
//! outside tests — substrates silently ignore inbound command
//! envelopes.
//!
//! This test closes that gap. peer_a here is the persona side; the
//! ONLY production-shape API the test touches is
//! `PersonaCommandInboundPump::spawn(persona_id, airc, executor)`.
//! No `CommandRequestHandler::new`, no manual subscribe loop, no
//! manual `on_envelope` call. If the pump's install path is wrong,
//! the test fails. If the pump silently drops command envelopes
//! (the old behavior), the test fails by timeout.
//!
//! ## Topology
//!
//! - peer_a = a persona. Owns an `Arc<Airc>` + a `CommandExecutor`
//!   with a `TestInferenceModule` (re-implemented inline; same
//!   shape as PR #1563's test-only module — task #221 promotes it
//!   to a system fixture later). `PersonaCommandInboundPump::spawn`
//!   installs the production-shape inbound pump on that handle +
//!   executor.
//! - peer_b = a remote caller. Builds `AircRemoteInferenceAdapter`
//!   wrapped around `AircLiveTransport` pointed at peer_a's
//!   peer_id. Calls `.generate_text(request)`.

use std::sync::Arc;

use airc_test_fixtures::TwoAircLoopback;
use async_trait::async_trait;
use continuum_core::ai::adapter::AIProviderAdapter;
use continuum_core::ai::heuristic_adapter::HeuristicInferenceAdapter;
use continuum_core::ai::types::{ChatMessage, FinishReason, MessageContent, TextGenerationRequest};
use continuum_core::inference::airc_remote::{AircLiveTransport, AircRemoteInferenceAdapter};
use continuum_core::persona::command_inbound_pump::PersonaCommandInboundPump;
use continuum_core::runtime::command_executor::CommandExecutor;
use continuum_core::runtime::{
    CommandResult, ModuleConfig, ModulePriority, ModuleRegistry, ServiceModule,
};
use uuid::Uuid;

/// Inline test-only ServiceModule that routes ai/generate to an
/// injected adapter. Mirror of the one in PR #1563's e2e test;
/// task #221 promotes both inline copies to a single system fixture.
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
            .map_err(|e| format!("decode TextGenerationRequest: {e}"))?;
        let response = self.adapter.generate_text(request).await?;
        let value = serde_json::to_value(&response)
            .map_err(|e| format!("serialize TextGenerationResponse: {e}"))?;
        Ok(CommandResult::Json(value))
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// A minimal request: one user message, every knob at its default.
/// `..Default::default()` (the struct derives `Default`) instead of an
/// explicit `None` per field — #1952 added `frequency_penalty` +
/// `repeat_last_n` and this initializer silently stopped compiling
/// because it enumerated every field. Struct-update syntax makes the
/// fixture immune to the next sampling knob.
fn request(prompt: &str) -> TextGenerationRequest {
    TextGenerationRequest {
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: MessageContent::Text(prompt.to_string()),
            name: None,
        }],
        ..Default::default()
    }
}

#[tokio::test]
async fn persona_command_pump_makes_persona_addressable_for_ai_generate() {
    let loop_back = TwoAircLoopback::new()
        .await
        .expect("fixture setup should succeed");

    // peer_a = a persona. Build the substrate-side state the SAME
    // way `PersonaAircRuntime::bootstrap` would (post-wiring):
    //   - a ModuleRegistry + ServiceModule registered for ai/generate
    //   - a CommandExecutor wrapping that registry
    //   - a PersonaCommandInboundPump installed on this persona's
    //     airc handle, bound to the executor
    let heuristic: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let module: Arc<dyn ServiceModule> = Arc::new(TestInferenceModule::new(heuristic));
    let registry = Arc::new(ModuleRegistry::new());
    registry.register(module);
    let executor = Arc::new(CommandExecutor::new(registry));

    // THIS is the production-shape install step. No manual handler
    // construction, no manual subscribe loop, no manual on_envelope
    // call. The test only touches the entry point a real
    // PersonaAircRuntime would touch at bootstrap.
    //
    // `spawn` is now async + returns Result so subscribe failure
    // surfaces at the call site (per R2 round-1 review of PR #1567).
    // Production callers can fail the persona's bootstrap with the
    // same error; here we unwrap because the loopback fixture's
    // subscribe must succeed for the test to make sense.
    let persona_id = loop_back.peer_a_id();
    // Production-shape: the persona builds its capability-grant authorizer (own key
    // + mesh + a durable watermark under its home) before installing the pump.
    let home = tempfile::tempdir().expect("tempdir for grant watermark");
    let grant_authorizer = continuum_core::persona::command_inbound_pump::build_grant_authorizer(
        loop_back.peer_a(),
        home.path(),
    )
    .await
    .expect("build grant authorizer for the loopback fixture");
    let pump = PersonaCommandInboundPump::spawn(
        persona_id,
        Arc::clone(loop_back.peer_a()),
        executor,
        grant_authorizer,
    )
    .await
    .expect(
        "PersonaCommandInboundPump::spawn must succeed for the test fixture — \
         if subscribe failed the persona would be unaddressable",
    );

    // No more 10ms sleep barrier: subscribe is now synchronous in
    // `spawn`, so by the time `spawn().await` returns the broadcast
    // receiver is already armed and the spawned task is moving the
    // stream forward. Subsequent dispatches land in the receiver
    // queue regardless of when the task's first `stream.next().await`
    // resolves (broadcast::Receiver buffers behind the cursor).

    // peer_b = a remote caller. The standard production-shape:
    //   AircRemoteInferenceAdapter(AircLiveTransport(peer_b, peer_a_id))
    let transport = AircLiveTransport::new(Arc::clone(loop_back.peer_b()), loop_back.peer_a_id());
    let adapter = AircRemoteInferenceAdapter::new(transport);

    let response = adapter
        .generate_text(request("ping the pump"))
        .await
        .expect(
            "PersonaCommandInboundPump must install the handler so that a remote \
             peer can dispatch ai/generate at the persona — if this fails by \
             timeout, the pump never ran its subscribe loop; if it fails by \
             error, the pump installed but the dispatch path is broken",
        );

    // The heuristic adapter's signature prefix proves the substrate's
    // FULL dispatch chain ran: pump -> CommandRequestHandler ->
    // CommandExecutor -> TestInferenceModule -> HeuristicAdapter.
    // Same surface as PR #1563's e2e test, but reached via the
    // PRODUCTION install path this PR lands.
    assert!(
        response.text.starts_with("[heuristic:"),
        "expected heuristic-signature prefix; got {:?}",
        response.text
    );
    assert!(
        response.text.contains("ping the pump"),
        "expected prompt echo; got {:?}",
        response.text
    );
    assert_eq!(response.provider, "airc-remote");
    assert_eq!(response.finish_reason, FinishReason::Stop);

    // Clean shutdown — proves the pump's `shutdown` API works.
    // Future PRs can graft this onto PersonaAircRuntime's Drop /
    // explicit shutdown path.
    pump.shutdown().await;

    // Pin: the persona_id we addressed equals the peer_id_a the
    // adapter routed to. Sanity check the test setup, not the pump.
    assert_eq!(persona_id, loop_back.peer_a_id());

    // Touch this just so the Uuid import isn't dead. Pin the type
    // so a future refactor that loses the typed peer_id surfaces.
    let _: Uuid = persona_id;
}
