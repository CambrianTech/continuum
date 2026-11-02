//! Integration test: `continuum-client::AircIpcTransport` (client side) round-trips an
//! `AircCommandRequest` against the substrate's actual command receiver
//! (`CommandRequestHandler::on_addressed`, fed by the substrate's addressed inbox) over
//! the `TwoAircLoopback` fixture.
//!
//! ## What this catches
//!
//! Drift between the client's request and what the substrate accepts: if the client
//! stops sending airc's typed `Command` frame, stops stamping the command path header
//! the ingress contract decides on, changes the `kind` discriminator, or the response
//! envelope shape changes on either end, the round trip fails. The substrate side is the
//! production receiver end to end (ingress contract, decode, executor, reply bound to the
//! request), not a parallel inline parser.

use std::sync::Arc;
use std::time::Duration;

use airc_test_fixtures::TwoAircLoopback;
use async_trait::async_trait;
use continuum_client::Connection;
use continuum_core::ipc::command_ingress::CommandRequestHandler;
use continuum_core::routing::addressed_inbox::{AddressedHandler, AddressedKind};
use continuum_core::runtime::command_executor::CommandExecutor;
use continuum_core::runtime::{
    CommandResult, ModuleConfig, ModulePriority, ModuleRegistry, ServiceModule,
};

/// Echoes the command path and params it ran, so the test asserts the wire round trip.
struct DebugPing;

impl DebugPing {
    const PREFIXES: &'static [&'static str] = &["debug/ping"];
}

#[async_trait]
impl ServiceModule for DebugPing {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "debug-ping",
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
        command: &str,
        params: serde_json::Value,
    ) -> Result<CommandResult, String> {
        Ok(CommandResult::Json(serde_json::json!({
            "ok": true,
            "echoed_path": command,
            "echoed_params": params,
        })))
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// The substrate on `peer_a`: the production receiver over an executor running `DebugPing`.
fn build_handler(peer_a: Arc<airc_lib::Airc>) -> Arc<CommandRequestHandler> {
    let registry = Arc::new(ModuleRegistry::new());
    registry.register(Arc::new(DebugPing));
    let executor = Arc::new(CommandExecutor::new(registry));
    CommandRequestHandler::new(peer_a, executor)
}

/// Read `peer_a`'s addressed inbox and hand the first command to the substrate's real
/// receiver, asserting it arrived as airc's typed `Command`.
async fn spawn_real_substrate_responder(
    handler: Arc<CommandRequestHandler>,
    peer_a: Arc<airc_lib::Airc>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut inbox = peer_a
            .addressed_inbox(tokio::time::Instant::now() + airc_ipc::client::DEFAULT_RPC_TIMEOUT)
            .await
            .expect("peer_a addressed inbox");
        let frame = inbox.next().await.expect("the client's command arrives");
        assert_eq!(
            frame.kind(),
            AddressedKind::Command,
            "the client sends airc's typed Command"
        );
        handler.on_addressed(frame).await;
    })
}

#[tokio::test]
async fn aircipctransport_round_trips_against_real_substrate_command_handler() {
    let loop_back = TwoAircLoopback::new()
        .await
        .expect("fixture setup should succeed");

    // peer_a = substrate; peer_b = client.
    let handler = build_handler(Arc::clone(loop_back.peer_a()));
    let responder =
        spawn_real_substrate_responder(Arc::clone(&handler), Arc::clone(loop_back.peer_a())).await;

    // Give the responder time to open its inbox.
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Build a Connection on peer_b targeted at peer_a.
    let conn = Connection::connect(Arc::clone(loop_back.peer_b()), loop_back.peer_a_id());

    // Send a typed (P, R) command. The substrate's receiver admits, decodes, runs
    // DebugPing and replies bound to the request.
    let params = serde_json::json!({"hello": "world", "count": 42});
    let result: serde_json::Value = conn
        .commands()
        .execute("debug/ping", params.clone())
        .await
        .expect("AircIpcTransport command roundtrip");

    // The substrate admitted and decoded our wire bytes, ran the command, and replied
    // through its real reply path. End-to-end wire shape is verified.
    assert_eq!(result["ok"], true, "ok flag, full response = {result}");
    assert_eq!(
        result["echoed_path"], "debug/ping",
        "path round-trips through substrate parser"
    );
    assert_eq!(
        result["echoed_params"], params,
        "params round-trip through substrate parser (full result = {result})"
    );

    responder.await.expect("responder task joined");
}
