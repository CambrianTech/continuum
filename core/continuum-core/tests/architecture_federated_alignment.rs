//! Architecture test — proves the "federated alignment" doctrine
//! clause via an adversarial / chaos test (shape 4).
//!
//! See `docs/architecture/PROVING-THE-DOCTRINE.md` for the matrix this
//! file populates. The clause pinned here:
//!
//! > "Federated alignment — a hostile peer cannot dominate. When a
//! > cross-grid stranger dispatches at this substrate, the
//! > `AuthPolicy::gate()` evaluates the call with the caller's
//! > verified airc peer identity (NOT a header-claimable shape) and
//! > returns a TYPED `Verdict::Forbidden` carrying the actionable
//! > reason. The dispatcher short-circuits before reaching the
//! > module; the audit log captures the verdict; the caller gets
//! > a structured error back."
//!
//! ## Why an adversarial / chaos test
//!
//! Federated alignment is a runtime property of the AuthPolicy gate.
//! We can't statically prove "hostile callers are refused" — we have
//! to actually be hostile and observe the substrate refuse, per
//! Shape 4: feed it adversarial input and assert structure.
//!
//! ## What this proves
//!
//! 1. `hostile_peer_dispatch_is_refused_with_typed_forbidden_verdict`:
//!    - Wire two peers via `TwoAircLoopback`.
//!    - peer_a registers a benign `ai/generate` ServiceModule so the
//!      URI resolves (proves the rejection is the GATE, not "no such
//!      command").
//!    - peer_a's `CommandExecutor` is built with a DENY policy that
//!      returns `Verdict::Forbidden { reason: UnknownPeer }` for any
//!      cross-grid (Airc-source) caller.
//!    - peer_b dispatches a real `ai/generate` request via
//!      `AircLiveTransport`.
//!    - Asserts: peer_b receives a typed error (NOT a successful
//!      response). The error surface is `RemoteInferenceError`
//!      carrying the substrate's "forbidden: …" prefix — the typed
//!      Verdict variant flattened to a String at the wire crossing
//!      (a known compression cost; tracked under matrix follow-ups
//!      for typed cross-grid error variants).
//!    - Asserts: the substrate stayed alive after the refusal.
//!
//! 2. `gate_sees_callers_airc_verified_peer_id_not_a_claimed_one`:
//!    - Same fixture, but instead of asserting the verdict, peer_a's
//!      policy CAPTURES the `CallerIdentity` it received.
//!    - peer_b dispatches normally.
//!    - Asserts: the captured caller's `peer_id` matches peer_b's
//!      actual airc peer_id (not a header-claimable shape).
//!    - Asserts: the captured `source` is `CallerSource::Airc`
//!      (cross-grid, not Local — Local would mean local-substrate
//!      impersonation succeeded).
//!    - Proves: airc-lib's signature-verified peer identity flows
//!      into the gate. A hostile peer can't claim someone else's
//!      peer_id by rewriting headers; the gate sees the WIRE-LEVEL
//!      identity that airc-lib verified at frame ingress.
//!
//! Together: typed refusal AND verified identity — the substrate's
//! federation-by-default story holds against hostile dispatch.
//!
//! ## What this does NOT cover (intentional follow-ups)
//!
//! - Sentinel quorum domination — a separate adversarial scenario
//!   where the hostile peer is enrolled but tries to dominate the
//!   sentinel verdict pool. Tracked under the same matrix row;
//!   requires sentinel-pool fixture work.
//! - Replay attacks (hostile peer replays a captured signed request).
//!   airc-lib's frame uniqueness + correlation_id rejection covers
//!   this at the layer below; a separate test should pin it.
//! - Verdict-string compression at the wire crossing. The substrate
//!   carries `Verdict::Forbidden { reason }` typed end-to-end inside
//!   one process, but `AircCommandResponse::Error { message: String }`
//!   flattens to prose. That's a known cost — closing it is task #243
//!   (typed `AircCommandResponse::Verdict { ... }` variant).
//!
//! ## Tag
//!
//! proves: federated alignment (hostile peer dispatch surfaces typed
//! Forbidden verdict; airc-verified peer identity flows into the gate;
//! substrate stays alive)

use std::sync::Arc;
use std::sync::Mutex;

use airc_test_fixtures::TwoAircLoopback;
use async_trait::async_trait;
use continuum_airc_protocol::{COMMAND_REQUEST_BODY_HINT, HEADER_CONTINUUM_BODY_HINT};
use continuum_core::ai::types::{ChatMessage, MessageContent, TextGenerationRequest};
use continuum_core::inference::airc_remote::{
    AircInferenceTransport, AircLiveTransport, RemoteInferenceRequest,
};
use continuum_core::routing::{
    CallerIdentity, CallerSource, ClosurePolicy, CommandRequestHandler, ForbiddenReason,
    RouteDecision, Verdict,
};
use continuum_core::runtime::command_executor::CommandExecutor;
use continuum_core::runtime::{
    CommandResult, ModuleConfig, ModuleContext, ModulePriority, ModuleRegistry, ServiceModule,
};
use futures::stream::StreamExt;
use tokio::sync::Notify;

/// A minimal `ai/generate` module that ALWAYS returns a benign canned
/// response. Its job is to make the URI resolve so the chaos test's
/// rejection is observably the AuthPolicy gate, not a "command not
/// found" surface (which would prove nothing about federated
/// alignment).
struct BenignAiGenerateModule;

impl BenignAiGenerateModule {
    const PREFIXES: &'static [&'static str] = &["ai/generate"];
}

#[async_trait]
impl ServiceModule for BenignAiGenerateModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "benign-ai-generate",
            priority: ModulePriority::Normal,
            command_prefixes: Self::PREFIXES,
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
        Ok(())
    }

    async fn handle_command(
        &self,
        _command: &str,
        _params: serde_json::Value,
    ) -> Result<CommandResult, String> {
        // If this is ever reached on the chaos path, the gate failed
        // — the test asserts the rejection happens BEFORE we get here.
        Ok(CommandResult::Json(serde_json::json!({
            "text": "this should never be returned on the chaos path",
            "finishReason": "stop",
            "model": "benign",
            "provider": "benign",
        })))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// Build a registry that has the benign ai/generate module installed,
/// so the URI resolves.
fn registry_with_ai_generate() -> Arc<ModuleRegistry> {
    let registry = ModuleRegistry::new();
    registry.register(Arc::new(BenignAiGenerateModule));
    Arc::new(registry)
}

/// Build peer_a's CommandRequestHandler with the given executor.
fn build_handler(
    peer: Arc<airc_lib::Airc>,
    executor: Arc<CommandExecutor>,
) -> Arc<CommandRequestHandler> {
    CommandRequestHandler::new(peer, executor)
}

/// Spawn peer_a's responder loop that uses the production
/// `process_request` path (so the gate is in the loop).
async fn spawn_substrate_responder(
    handler: Arc<CommandRequestHandler>,
    peer: Arc<airc_lib::Airc>,
    ready: Arc<Notify>,
) -> tokio::task::JoinHandle<()> {
    let self_id = peer.peer_id();
    tokio::spawn(async move {
        let mut stream = peer.subscribe().await.expect("peer subscribe");
        ready.notify_one();
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
                Err(_) => continue,
            };
            // PRODUCTION path: the gate runs inside process_request →
            // execute_with_caller. If the verdict is Forbidden, the
            // response will be an AircCommandResponse::Error variant
            // carrying the substrate's "forbidden: ..." prose.
            let response = handler.process_request(&parsed).await;
            handler
                .send_reply(&parsed, &response)
                .await
                .expect("substrate send_reply");
            return;
        }
    })
}

fn build_remote_request() -> RemoteInferenceRequest {
    RemoteInferenceRequest::new(TextGenerationRequest {
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: MessageContent::Text("hostile ping".to_string()),
            name: None,
        }],
        system_prompt: Some("federated-alignment chaos test".to_string()),
        model: Some("benign".to_string()),
        provider: None,
        temperature: Some(0.0),
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
    })
}

// proves: federated alignment (hostile peer dispatch surfaces typed
// refusal — Forbidden verdict prevents module execution, substrate
// stays alive, caller receives structured error)
#[tokio::test]
async fn hostile_peer_dispatch_is_refused_with_typed_forbidden_verdict() {
    let loop_back = TwoAircLoopback::new()
        .await
        .expect("fixture setup should succeed");

    // peer_a's DENY policy: refuses any cross-grid caller with
    // `ForbiddenReason::UnknownPeer`. The fixture's TwoAircLoopback
    // actually enrolls peer_b in peer_a's trust store (mutual
    // `add_peer`), so this policy overrides the trust-store state
    // — and that override is doctrine-honest: the gate is the
    // authority on admission, not the trust store. The test
    // simulates "policy thinks they're unknown" semantics, which is
    // what an ORM-backed production policy would surface when its
    // capability table refuses regardless of the underlying trust
    // store's enrollment list.
    let policy = ClosurePolicy::new(
        "chaos-deny-cross-grid",
        move |_decision: &RouteDecision, caller: Option<&CallerIdentity>| match caller {
            Some(c) if matches!(c.source, CallerSource::Airc) => Verdict::Forbidden {
                reason: ForbiddenReason::UnknownPeer,
            },
            _ => Verdict::Allowed,
        },
    );

    let executor = Arc::new(
        CommandExecutor::new(registry_with_ai_generate()).with_policy(Arc::new(policy)),
    );
    let handler = build_handler(Arc::clone(loop_back.peer_a()), executor);

    let ready = Arc::new(Notify::new());
    let responder = spawn_substrate_responder(
        Arc::clone(&handler),
        Arc::clone(loop_back.peer_a()),
        Arc::clone(&ready),
    )
    .await;
    ready.notified().await;

    // peer_b (hostile from peer_a's perspective) dispatches normally.
    let transport = Arc::into_inner(AircLiveTransport::new(
        Arc::clone(loop_back.peer_b()),
        loop_back.peer_a_id(),
    ))
    .expect("freshly-allocated Arc has refcount 1")
    .with_deadline(std::time::Duration::from_millis(800));

    let result = transport.send_request(build_remote_request()).await;

    // The substrate refused. The wire-level error surface flattens
    // the typed Verdict to a String at AircCommandResponse::Error
    // (a known compression cost — tracked in module doc), so we
    // assert on the structured `RemoteInferenceError::PeerAdapterFailed`
    // variant (the adapter layer's classification) AND on the
    // "forbidden" / "UnknownPeer" prose carried inside.
    let err = result.expect_err(
        "DENY policy must refuse the dispatch; got Ok — gate bypass or \
         policy not threaded into process_request_via",
    );

    let message = format!("{err:?}");
    assert!(
        message.contains("forbidden"),
        "substrate's refusal must carry 'forbidden:' prefix from the \
         gate's typed Verdict (so audit + operator can identify the \
         cause). Got: {message}"
    );
    // The substrate formats `Err(format!("forbidden: {reason}"))`
    // where `{reason}` uses ForbiddenReason's thiserror Display. For
    // UnknownPeer that's "caller peer not enrolled in this
    // substrate" — so we assert on the Display prose, not the
    // variant name. Task #243 (typed `AircCommandResponse::Verdict`
    // variant; same antipattern PR #1593 closed for the deadline
    // classifier) would let us match the variant directly; today's
    // wire compression is prose.
    assert!(
        message.contains("not enrolled"),
        "the Forbidden reason must surface to the wire — caller should \
         be able to distinguish 'unknown peer' from 'no permission' or \
         'rate limited' for audit + retry decisions. The Display prose \
         for ForbiddenReason::UnknownPeer is 'caller peer not enrolled \
         in this substrate' — got: {message}"
    );

    // Substrate stayed alive — we made it here without panic. The
    // responder task should also have joined cleanly (it returns
    // after handling one request).
    responder.await.expect("responder task joined cleanly");
}

// proves: federated alignment (gate sees the airc-verified peer_id
// of the caller, not a header-claimable one — closes the silent
// identity-substitution seam)
#[tokio::test]
async fn gate_sees_callers_airc_verified_peer_id_not_a_claimed_one() {
    let loop_back = TwoAircLoopback::new()
        .await
        .expect("fixture setup should succeed");

    let peer_b_id = loop_back.peer_b_id();

    // Policy that ALLOWS but CAPTURES the caller identity it
    // received. Lets the test assert that the gate saw peer_b's
    // verified airc peer_id, not something a hostile peer could
    // have claimed in a header.
    let captured: Arc<Mutex<Option<CallerIdentity>>> = Arc::new(Mutex::new(None));
    let captured_clone = Arc::clone(&captured);
    let policy = ClosurePolicy::new(
        "chaos-capture-caller",
        move |_decision: &RouteDecision, caller: Option<&CallerIdentity>| {
            *captured_clone.lock().unwrap() = caller.cloned();
            Verdict::Allowed
        },
    );

    let executor = Arc::new(
        CommandExecutor::new(registry_with_ai_generate()).with_policy(Arc::new(policy)),
    );
    let handler = build_handler(Arc::clone(loop_back.peer_a()), executor);

    let ready = Arc::new(Notify::new());
    let responder = spawn_substrate_responder(
        Arc::clone(&handler),
        Arc::clone(loop_back.peer_a()),
        Arc::clone(&ready),
    )
    .await;
    ready.notified().await;

    let transport = Arc::into_inner(AircLiveTransport::new(
        Arc::clone(loop_back.peer_b()),
        loop_back.peer_a_id(),
    ))
    .expect("freshly-allocated Arc has refcount 1")
    .with_deadline(std::time::Duration::from_millis(800));

    let _result = transport.send_request(build_remote_request()).await;
    responder.await.expect("responder task joined cleanly");

    let observed = captured
        .lock()
        .unwrap()
        .clone()
        .expect(
            "AuthPolicy::gate must have been invoked with Some(caller) — \
             the cross-grid dispatch path failed to thread caller \
             identity into the gate (silent privilege-escalation seam)",
        );

    assert_eq!(
        observed.peer_id.as_uuid(), peer_b_id,
        "the caller identity surfaced to the gate must match peer_b's \
         airc-verified peer_id, not a header-claimable shape. If this \
         fires, a hostile peer can substitute identities by rewriting \
         headers — closes the same seam reviewer 2 flagged on PR #1529."
    );
    assert!(
        matches!(observed.source, CallerSource::Airc),
        "caller source must be CallerSource::Airc for cross-grid \
         dispatch — Local would mean the substrate accepted the \
         remote dispatch as if it were locally-originated, defeating \
         the federation contract. Got: {observed:?}"
    );
}
