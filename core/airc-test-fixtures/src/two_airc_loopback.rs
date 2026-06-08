//! `TwoAircLoopback` — two `airc_lib::Airc` peers wired over a real
//! loopback transport, ready for cross-grid integration tests.
//!
//! ## Status: skeleton
//!
//! This module defines the public API the fixture SHOULD expose. The
//! method bodies are `todo!()` placeholders pending the airc-lib study
//! slice that fills them in. Locking the shape here lets consumers
//! (the substrate's integration tests + continuum-client's roundtrip
//! tests) write against the final API today and exercise it once the
//! impl lands.
//!
//! ## What the real impl needs to do
//!
//! - Allocate two `tempfile::TempDir` homes (one per peer).
//! - Spawn or attach two `airc_lib::Airc` instances configured to talk
//!   to each other. Options under investigation:
//!     a) Spawn two real `airc` daemon child processes bound to
//!        distinct unix sockets; have peer A connect to peer B's
//!        socket as a remote.
//!     b) Use airc-lib's in-process / loopback test mode if one
//!        exists (check `airc_lib::Airc::*` test constructors).
//! - Wait for the peer registration to complete so the test can
//!   immediately `peer_a.request(MentionTarget::Peer(b_id), ...)` and
//!   get a reply.
//! - On `Drop`, tear down both Airc instances cleanly and remove the
//!   temp homes.
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

use std::sync::Arc;

use airc_lib::Airc;
use thiserror::Error;
use uuid::Uuid;

/// Typed errors the fixture surfaces. Each variant names the specific
/// piece that failed so a test failure points straight at the issue
/// instead of bubbling a generic panic.
#[derive(Debug, Error)]
pub enum LoopbackError {
    /// Could not allocate a tempdir for one of the peer homes.
    #[error("temp dir allocation: {0}")]
    TempDir(#[from] std::io::Error),

    /// One of the airc-lib spawn/attach calls returned an error.
    #[error("airc spawn: {0}")]
    AircSpawn(String),

    /// The peers were spun up but didn't discover each other before
    /// the registration deadline.
    #[error("peer registration timeout — peers never saw each other")]
    PeerRegistrationTimeout,

    /// Catch-all for the skeleton phase. Real impl decomposes this
    /// into more specific variants.
    #[error("not yet implemented in skeleton: {0}")]
    NotImplemented(&'static str),
}

/// Two `airc_lib::Airc` peers wired over a real loopback transport.
///
/// Clone is intentionally NOT derived; the fixture owns the two Airc
/// instances and the tempdirs. Consumers `Arc`-clone the peer handles
/// they want to use.
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
    /// Returns once both peers have registered with each other and are
    /// ready to round-trip commands.
    pub async fn new() -> Result<Self, LoopbackError> {
        Err(LoopbackError::NotImplemented(
            "TwoAircLoopback::new — spawn airc peers + wire loopback transport",
        ))
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
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Skeleton-phase smoke test: the constructor surfaces a typed
    /// NotImplemented error rather than panicking. When the real impl
    /// lands this test gets replaced by one that asserts a successful
    /// build + a round-trip.
    #[tokio::test]
    async fn skeleton_constructor_surfaces_typed_not_implemented() {
        match TwoAircLoopback::new().await {
            Err(LoopbackError::NotImplemented(_)) => { /* expected */ }
            Err(other) => panic!("expected NotImplemented in skeleton phase, got {other:?}"),
            Ok(_) => panic!("skeleton constructor should not return Ok"),
        }
    }
}
