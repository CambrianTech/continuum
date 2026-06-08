//! Integration test: `AircIpcTransport` (client side) round-trips an
//! `AircCommandRequest` against a substrate-equivalent responder (server
//! side) over the `TwoAircLoopback` fixture.
//!
//! This closes the wire-shape verification gap that adversarial reviewer
//! 1 flagged on PR #1557: the unit tests of `AircIpcTransport::request`
//! and the substrate's `CommandRequestHandler` each prove their PARSING
//! surface in isolation. They do NOT prove that an envelope serialized
//! by the client passes through airc-lib's CBOR framing + LAN transport
//! + correlation_id stamping + header rewrites + body unwrap correctly
//! at the server end.
//!
//! The fixture's `peer_a` plays the SUBSTRATE side: subscribes to airc,
//! filters incoming events on `continuum.body_hint =
//! continuum.command.request.v1`, parses with `AircCommandRequest`,
//! constructs an `AircCommandResponse::Ok`, replies via `airc.reply`.
//!
//! The fixture's `peer_b` plays the CLIENT side: hands its Airc to a
//! `Connection<AircIpcTransport>::connect(...)`, calls
//! `commands().execute()`, expects the typed result.
//!
//! If at any point the wire shapes drift (a renamed header, a changed
//! body hint, an envelope field added/dropped without a v1 migration),
//! this test fails — caught BEFORE the substrate ships.

use std::sync::Arc;
use std::time::Duration;

use airc_core::{Body, PeerId};
use airc_protocol::{HEADER_AIRC_CORRELATION_ID, HEADER_AIRC_REPLY_TO};
use airc_test_fixtures::TwoAircLoopback;
use continuum_airc_protocol::{
    AircCommandRequest, AircCommandResponse, COMMAND_REQUEST_BODY_HINT, COMMAND_RESPONSE_BODY_HINT,
    HEADER_COMMAND_STATUS, HEADER_CONTINUUM_BODY_HINT,
};
use continuum_client::Connection;
use futures::stream::StreamExt;
use uuid::Uuid;

/// Spawn a minimal substrate-equivalent responder on `peer_a`. It
/// answers any incoming `AircCommandRequest` with a fixed
/// `AircCommandResponse::Ok({"path": <echoed>, "params": <echoed>})`.
/// Real substrate runs the request through a `CommandExecutor`; this
/// test pins the WIRE SHAPE, not the executor's policy gate.
async fn spawn_substrate_equivalent_responder(
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
            // Skip our own emissions (including replies we sent).
            if event.peer_id == self_id {
                continue;
            }
            // Only commands — let other traffic flow past.
            let hint = match event.headers.get(HEADER_CONTINUUM_BODY_HINT) {
                Some(h) => h,
                None => continue,
            };
            if hint != COMMAND_REQUEST_BODY_HINT {
                continue;
            }

            // Pull correlation + reply_to off the airc-lib headers.
            let correlation_id = event
                .headers
                .get(HEADER_AIRC_CORRELATION_ID)
                .and_then(|s| Uuid::parse_str(s).ok())
                .expect("correlation_id header");
            let reply_to_uuid = event
                .headers
                .get(HEADER_AIRC_REPLY_TO)
                .and_then(|s| Uuid::parse_str(s).ok())
                .expect("reply_to header");

            // Decode the body as AircCommandRequest — same shape both
            // ends import from continuum-airc-protocol.
            let request: AircCommandRequest = match &event.body {
                Some(Body::Json(v)) => {
                    serde_json::from_value(v.clone()).expect("decode request")
                }
                _ => panic!("expected Json body with AircCommandRequest"),
            };

            // Build the canonical Ok response that echoes path + params.
            let response = AircCommandResponse::ok(serde_json::json!({
                "ok": true,
                "echoed_path": request.path,
                "echoed_params": request.params,
            }));

            // Serialize + stamp the substrate's reply headers.
            let body_value = serde_json::to_value(&response).expect("serialize response");
            let mut reply_headers = airc_core::Headers::new();
            reply_headers.insert(
                HEADER_COMMAND_STATUS.to_string(),
                response.status_header_value().to_string(),
            );
            reply_headers.insert(
                HEADER_CONTINUUM_BODY_HINT.to_string(),
                COMMAND_RESPONSE_BODY_HINT.to_string(),
            );

            peer_a
                .reply(
                    PeerId::from_uuid(reply_to_uuid),
                    correlation_id,
                    reply_headers,
                    Body::Json(body_value),
                )
                .await
                .expect("peer_a reply");
            return;
        }
    })
}

#[tokio::test]
async fn aircipctransport_round_trips_against_substrate_equivalent_responder() {
    let loop_back = TwoAircLoopback::new()
        .await
        .expect("fixture setup should succeed");

    // peer_a = substrate; peer_b = client.
    let responder = spawn_substrate_equivalent_responder(Arc::clone(loop_back.peer_a())).await;

    // Give the responder time to install its subscribe filter.
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Build a Connection on peer_b targeted at peer_a.
    let conn = Connection::connect(Arc::clone(loop_back.peer_b()), loop_back.peer_a_id());

    // Send the kind of command continuum-client consumers will send —
    // a typed (P, R) execute call. The responder echoes path + params,
    // so we decode the result as a serde_json::Value.
    let params = serde_json::json!({"hello": "world", "count": 42});
    let result: serde_json::Value = conn
        .commands()
        .execute("debug/ping", params.clone())
        .await
        .expect("AircIpcTransport command roundtrip");

    // Assert the response shape. If any wire bytes drift, the
    // assertions fail with a clear signal.
    assert_eq!(result["ok"], true, "ok flag, full response = {result}");
    assert_eq!(
        result["echoed_path"], "debug/ping",
        "path round-trips through wire"
    );
    assert_eq!(
        result["echoed_params"], params,
        "params round-trip through wire (full result = {result})"
    );

    responder.await.expect("responder task joined");
}
