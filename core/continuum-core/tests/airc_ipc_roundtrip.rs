//! Integration test: `continuum-client::AircIpcTransport` (client side)
//! round-trips an `AircCommandRequest` against the substrate's actual
//! `CommandRequestHandler::parse_envelope` + `send_reply` (server side)
//! over the `TwoAircLoopback` fixture.
//!
//! ## What this catches
//!
//! Reviewer 3 on PR #1558 round 1 flagged that the earlier integration
//! test inlined `serde_json::from_value` instead of calling the
//! substrate's real parser. That meant drift between the client's
//! envelope and `CommandRequestHandler::parse_envelope`'s expectations
//! would NOT fail the test. This test fixes that by driving the
//! SUBSTRATE'S OWN `pub` parse + reply functions from the responder.
//!
//! If at any point the substrate parser starts requiring a new header,
//! drops a required header, changes the `kind` discriminator, or
//! changes the response envelope shape, this test fails — the wire
//! shape is now actually verified end-to-end against the real
//! substrate code, not a parallel inline parser.

use std::sync::Arc;
use std::time::Duration;

use airc_test_fixtures::TwoAircLoopback;
use continuum_airc_protocol::{
    AircCommandResponse, COMMAND_REQUEST_BODY_HINT, HEADER_CONTINUUM_BODY_HINT, KIND_PEER,
};
use continuum_client::Connection;
use continuum_core::routing::CommandRequestHandler;
use continuum_core::runtime::command_executor::CommandExecutor;
use continuum_core::runtime::ModuleRegistry;
use futures::stream::StreamExt;

/// Build a `CommandRequestHandler` against the loopback's peer_a.
/// Constructs a bare `ModuleRegistry` + `CommandExecutor` — we don't
/// register any modules because the responder bypasses `process_request`
/// (it crafts its own response shape to assert the wire, not the
/// executor's policy gate).
fn build_handler(peer_a: Arc<airc_lib::Airc>) -> Arc<CommandRequestHandler> {
    let registry = Arc::new(ModuleRegistry::new());
    let executor = Arc::new(CommandExecutor::new(registry));
    CommandRequestHandler::new(peer_a, executor)
}

/// Spawn a substrate-equivalent responder on `peer_a` using the SUBSTRATE'S
/// REAL parse + reply functions. The responder controls the response
/// payload directly so the test can assert the wire bytes; it does NOT
/// route through `CommandExecutor::execute` because that would require
/// registering a full module and isn't what this test pins.
async fn spawn_real_substrate_responder(
    handler: Arc<CommandRequestHandler>,
    peer_a: Arc<airc_lib::Airc>,
) -> tokio::task::JoinHandle<()> {
    let self_id = peer_a.peer_id();
    tokio::spawn(async move {
        let mut stream = peer_a.subscribe().await.expect("peer_a subscribe");
        while let Some(event) = stream.next().await {
            let event = match event {
                Ok(e) => e,
                Err(_) => continue,
            };
            if event.peer_id == self_id {
                continue;
            }
            // Body-hint filter — mirror the substrate's adapter dispatch
            // discriminator. Non-command frames (chat, presence, etc)
            // flow past.
            let hint = match event.headers.get(HEADER_CONTINUUM_BODY_HINT) {
                Some(h) => h,
                None => continue,
            };
            if hint != COMMAND_REQUEST_BODY_HINT {
                continue;
            }

            // Drive the SUBSTRATE's actual parser. If the client's wire
            // envelope drifts from what the substrate accepts, this
            // call returns Err — caught.
            let parsed = match CommandRequestHandler::parse_envelope(&event) {
                Ok(p) => p,
                Err(e) => panic!("substrate parse_envelope rejected client envelope: {e:?}"),
            };

            // Per-field assertions on the substrate's view of the
            // parsed request. KIND_PEER is wire-stable; a drift here
            // would be a substantive protocol bug.
            assert_eq!(
                parsed.request.kind, KIND_PEER,
                "client must dispatch with kind=\"peer\"; got {:?}",
                parsed.request.kind
            );
            assert_eq!(
                parsed.request.path, "debug/ping",
                "client wire-out preserves path"
            );
            assert_eq!(
                parsed.request.env, None,
                "no env was specified by the client"
            );

            // Construct the canonical Ok response. The substrate's own
            // `send_reply` serializes via the protocol crate's enum + stamps
            // both reply headers — so a serde-tag change OR a header
            // rename on EITHER end would break this roundtrip.
            let response = AircCommandResponse::ok(serde_json::json!({
                "ok": true,
                "echoed_path": parsed.request.path,
                "echoed_params": parsed.request.params,
            }));

            // Drive the SUBSTRATE's actual reply path.
            handler
                .send_reply(&parsed, &response)
                .await
                .expect("substrate send_reply");
            return;
        }
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

    // Give the responder time to install its subscribe filter.
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Build a Connection on peer_b targeted at peer_a.
    let conn = Connection::connect(Arc::clone(loop_back.peer_b()), loop_back.peer_a_id());

    // Send a typed (P, R) command. The substrate parses + the responder
    // echoes path + params back via the real send_reply.
    let params = serde_json::json!({"hello": "world", "count": 42});
    let result: serde_json::Value = conn
        .commands()
        .execute("debug/ping", params.clone())
        .await
        .expect("AircIpcTransport command roundtrip");

    // The substrate parsed our wire bytes, echoed path + params, and
    // serialized the response through its real send_reply. End-to-end
    // wire shape is verified.
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
