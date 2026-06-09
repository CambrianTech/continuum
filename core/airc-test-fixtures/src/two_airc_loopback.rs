//! `TwoAircLoopback` — two `airc_lib::Airc` peers wired over a real
//! LAN loopback transport, ready for cross-grid integration tests.
//!
//! ## What this fixture proves
//!
//! The substrate's `command_handler` (server side) and
//! `continuum-client::AircIpcTransport` (client side) speak the same
//! `continuum-airc-protocol` envelopes. Unit tests of each end exercise
//! the parsing surface in isolation. They do NOT prove that an envelope
//! serialized by the client end-to-end deserializes correctly at the
//! server end after airc-lib's CBOR framing, header rewrites,
//! correlation_id stamping, deadline negotiation, and LAN-transport
//! round-trip. That gap was flagged by adversarial reviewer 1 on
//! PR #1557. This fixture closes it.
//!
//! ## Topology
//!
//! ```text
//!   peer_a ───── add_peer(b) ─────► peer_b
//!     │                                │
//!     │ ◄──── add_peer(a) ─────────────┤
//!     │                                │
//!     │ join("...")          join("...")
//!     │                                │
//!     │ ── connect_lan(b_addr, b_id) ─►│
//!     │                                │ ── listen_lan(127.0.0.1:0)
//! ```
//!
//! Both peers live in the same process, in distinct `TempDir` homes.
//! Drop the fixture and both homes get cleaned up.
//!
//! ## Intended consumer shape
//!
//! ```ignore
//! use airc_test_fixtures::TwoAircLoopback;
//!
//! #[tokio::test]
//! async fn client_to_substrate_roundtrip() -> anyhow::Result<()> {
//!     let loop_back = TwoAircLoopback::new().await?;
//!
//!     // Stand up substrate command_handler on peer A.
//!     let _server = substrate_command_handler::spawn(loop_back.peer_a().clone()).await?;
//!
//!     // Build a continuum-client transport on peer B targeted at peer A.
//!     let transport = continuum_client::AircIpcTransport::new(
//!         loop_back.peer_b().clone(),
//!         loop_back.peer_a_id(),
//!     );
//!
//!     // Dispatch a real command and assert the typed result.
//!     let conn = continuum_client::Connection::new(transport);
//!     let result: serde_json::Value = conn.commands()
//!         .execute("debug/ping", serde_json::json!({})).await?;
//!     assert_eq!(result["ok"], true);
//!     Ok(())
//! }
//! ```

use std::net::SocketAddr;
use std::sync::Arc;

use airc_lib::Airc;
use thiserror::Error;
use uuid::Uuid;

/// Default room both peers join during fixture setup. Callers don't
/// usually need to know the room — they target the other peer by
/// `peer_id` directly via `MentionTarget::Peer(...)`.
const FIXTURE_ROOM: &str = "two-airc-loopback";

/// Default bind address for peer_b's LAN listen. `:0` lets the kernel
/// pick a free port so parallel test runs don't collide.
const LOOPBACK_BIND: &str = "127.0.0.1:0";

/// Typed errors the fixture surfaces. Each variant names the specific
/// piece that failed so a test failure points straight at the issue
/// instead of bubbling a generic panic.
#[derive(Debug, Error)]
pub enum LoopbackError {
    /// Could not allocate a tempdir for one of the peer homes.
    #[error("temp dir allocation: {0}")]
    TempDir(#[from] std::io::Error),

    /// One of the airc-lib spawn/attach calls returned an error. The
    /// message names which peer + which step (open, peer_spec parse,
    /// add_peer, join, listen_lan, connect_lan).
    #[error("airc spawn: {0}")]
    AircSpawn(String),
}

/// Two `airc_lib::Airc` peers wired over a real LAN loopback.
///
/// Clone is intentionally NOT derived; the fixture owns the two Airc
/// instances and the temp homes. Consumers `Arc`-clone the peer handles
/// via `peer_a()` / `peer_b()` for the side(s) they want to use.
pub struct TwoAircLoopback {
    peer_a: Arc<Airc>,
    peer_b: Arc<Airc>,
    peer_a_id: Uuid,
    peer_b_id: Uuid,
    // Tempdirs are dropped (and removed) when the fixture goes out of
    // scope. Kept here so they outlive the Airc instances they back.
    _peer_a_home: tempfile::TempDir,
    _peer_b_home: tempfile::TempDir,
}

impl TwoAircLoopback {
    /// Build a fresh loopback fixture with two freshly-spawned Airc peers.
    /// Returns once both peers have:
    /// 1. opened with strict verification on their own temp homes
    /// 2. learned each other's `PeerSpec` via mutual `add_peer`
    /// 3. joined the shared fixture room
    /// 4. wired a LAN transport (peer_b listens; peer_a connects)
    ///
    /// After this, `peer_a.request(MentionTarget::Peer(peer_b_id), ...)`
    /// reaches peer_b's `subscribe()` stream, and vice versa.
    pub async fn new() -> Result<Self, LoopbackError> {
        let peer_a_home = tempfile::TempDir::new()?;
        let peer_b_home = tempfile::TempDir::new()?;

        let peer_a = Airc::open(peer_a_home.path())
            .await
            .map_err(|e| LoopbackError::AircSpawn(format!("peer_a open: {e}")))?;
        let peer_b = Airc::open(peer_b_home.path())
            .await
            .map_err(|e| LoopbackError::AircSpawn(format!("peer_b open: {e}")))?;

        let peer_a_id = peer_a.peer_id().as_uuid();
        let peer_b_id = peer_b.peer_id().as_uuid();

        // Mutual trust: each peer parses the other's spec + adds it.
        let a_spec = peer_a
            .peer_spec()
            .parse()
            .map_err(|e| LoopbackError::AircSpawn(format!("parse peer_a spec: {e:?}")))?;
        let b_spec = peer_b
            .peer_spec()
            .parse()
            .map_err(|e| LoopbackError::AircSpawn(format!("parse peer_b spec: {e:?}")))?;
        peer_a
            .add_peer(b_spec)
            .await
            .map_err(|e| LoopbackError::AircSpawn(format!("peer_a trust peer_b: {e}")))?;
        peer_b
            .add_peer(a_spec)
            .await
            .map_err(|e| LoopbackError::AircSpawn(format!("peer_b trust peer_a: {e}")))?;

        // Join the shared fixture room so room-scoped subscribe lands.
        peer_a
            .join(FIXTURE_ROOM)
            .await
            .map_err(|e| LoopbackError::AircSpawn(format!("peer_a join({FIXTURE_ROOM}): {e}")))?;
        peer_b
            .join(FIXTURE_ROOM)
            .await
            .map_err(|e| LoopbackError::AircSpawn(format!("peer_b join({FIXTURE_ROOM}): {e}")))?;

        // Wire the LAN: peer_b listens on a kernel-picked loopback port,
        // peer_a dials in with peer_b's peer_id for auth.
        let bind: SocketAddr = LOOPBACK_BIND
            .parse()
            .expect("LOOPBACK_BIND is a valid SocketAddr");
        let peer_b_addr = peer_b
            .listen_lan(bind)
            .await
            .map_err(|e| LoopbackError::AircSpawn(format!("peer_b listen_lan({bind}): {e}")))?;
        peer_a
            .connect_lan(peer_b_addr, peer_b.peer_id())
            .await
            .map_err(|e| {
                LoopbackError::AircSpawn(format!(
                    "peer_a connect_lan({peer_b_addr}, peer_b_id): {e}"
                ))
            })?;

        Ok(Self {
            peer_a: Arc::new(peer_a),
            peer_b: Arc::new(peer_b),
            peer_a_id,
            peer_b_id,
            _peer_a_home: peer_a_home,
            _peer_b_home: peer_b_home,
        })
    }

    /// The first peer's `Arc<Airc>` handle. Test code clones this and
    /// hands it to the SERVER side of whatever it's testing (typically
    /// the substrate's command_handler).
    pub fn peer_a(&self) -> &Arc<Airc> {
        &self.peer_a
    }

    /// The second peer's `Arc<Airc>` handle. Test code clones this and
    /// hands it to the CLIENT side (typically continuum-client's
    /// AircIpcTransport).
    pub fn peer_b(&self) -> &Arc<Airc> {
        &self.peer_b
    }

    /// The first peer's UUID — what peer_b targets when it dispatches.
    pub fn peer_a_id(&self) -> Uuid {
        self.peer_a_id
    }

    /// The second peer's UUID — symmetric, for tests that dispatch the
    /// other direction.
    pub fn peer_b_id(&self) -> Uuid {
        self.peer_b_id
    }

    /// The room both peers joined during setup. Mostly useful for tests
    /// that want to publish to a room rather than dispatch a peer-
    /// targeted command.
    pub fn shared_room(&self) -> &'static str {
        FIXTURE_ROOM
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    use airc_core::{Body, Headers, MentionTarget, PeerId};
    use futures::stream::StreamExt;

    /// Bare-airc roundtrip smoke: peer_a sends a request to peer_b,
    /// peer_b's subscribe stream sees it and replies, peer_a's
    /// await_reply resolves. Proves the fixture's wire is alive
    /// end-to-end without any continuum-specific protocol on top.
    #[tokio::test]
    async fn bare_request_reply_round_trips_over_loopback() {
        let loop_back = TwoAircLoopback::new()
            .await
            .expect("fixture setup should succeed");

        let peer_b_handle = Arc::clone(loop_back.peer_b());
        let peer_b_self_id = peer_b_handle.peer_id();
        let responder = tokio::spawn(async move {
            let mut stream = peer_b_handle
                .subscribe()
                .await
                .expect("peer_b subscribe");
            while let Some(event) = stream.next().await {
                let event = match event {
                    Ok(e) => e,
                    Err(_) => continue,
                };
                // Skip our own emissions.
                if event.peer_id == peer_b_self_id {
                    continue;
                }
                let Some(correlation) =
                    event.headers.get(airc_protocol::HEADER_AIRC_CORRELATION_ID)
                else {
                    continue;
                };
                let Some(reply_to) = event.headers.get(airc_protocol::HEADER_AIRC_REPLY_TO) else {
                    continue;
                };
                let correlation_id =
                    Uuid::parse_str(correlation).expect("valid correlation uuid");
                let reply_to_peer = PeerId::from_uuid(
                    Uuid::parse_str(reply_to).expect("valid reply_to uuid"),
                );
                let mut reply_headers = Headers::new();
                reply_headers.insert("test.body_hint".into(), "loopback.pong".into());
                peer_b_handle
                    .reply(
                        reply_to_peer,
                        correlation_id,
                        reply_headers,
                        Body::text("pong"),
                    )
                    .await
                    .expect("peer_b reply");
                return;
            }
        });

        // Give peer_b time to install the subscribe filter before peer_a
        // emits the request. airc_lib's request() arms the reply stream
        // before sending, but the responder above is in a separate task
        // that needs to call subscribe first.
        tokio::time::sleep(Duration::from_millis(50)).await;

        let mut headers = Headers::new();
        headers.insert("airc.command_kind".into(), "test.loopback.ping".into());
        let pending = loop_back
            .peer_a()
            .request(
                MentionTarget::Peer(PeerId::from_uuid(loop_back.peer_b_id())),
                headers,
                Body::text("ping"),
                Duration::from_secs(5),
            )
            .await
            .expect("peer_a request");
        let reply = loop_back
            .peer_a()
            .await_reply(pending)
            .await
            .expect("peer_a await_reply");

        // The body should be our pong. `Body::text("pong")` constructs
        // `Body::Json({"text": "pong"})` so we extract the `text` field.
        match reply.body {
            Some(Body::Json(v)) => {
                assert_eq!(v["text"], "pong", "got body json {v}");
            }
            other => panic!("expected Json pong body, got {other:?}"),
        }

        responder.await.expect("responder task joined");
    }
}
