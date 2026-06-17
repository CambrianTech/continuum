//! Connection — owns a `Transport`, hands out command + event clients.

use std::sync::Arc;

use uuid::Uuid;

use crate::airc_ipc::AircIpcTransport;
use crate::command::CommandClient;
use crate::error::ClientError;
use crate::event::EventSubscriber;
use crate::transport::{ServeHandler, Transport};

/// One session against a continuum substrate. Generic over the wire so
/// the same code drives local airc IPC, remote airc grid, and the
/// `MockTransport` used in downstream tests.
pub struct Connection<T: Transport> {
    transport: Arc<T>,
}

// Manual `Clone` (not derived): a connection is cheap to clone — it shares the
// one `Arc<T>` transport. Derived `Clone` would wrongly require `T: Clone`;
// `Arc<T>` is `Clone` regardless. Cloning lets a `provide` registration hold a
// handle to revoke on drop without re-establishing the session.
impl<T: Transport> Clone for Connection<T> {
    fn clone(&self) -> Self {
        Self {
            transport: Arc::clone(&self.transport),
        }
    }
}

impl<T: Transport> Connection<T> {
    /// Build a connection over an already-established transport. Higher-
    /// level constructors (e.g. `connect_local`, `connect_remote`) will
    /// wrap this once the airc transport impls land.
    pub fn new(transport: T) -> Self {
        Self {
            transport: Arc::new(transport),
        }
    }

    /// Typed command dispatcher for this connection.
    pub fn commands(&self) -> CommandClient<T> {
        CommandClient::new(Arc::clone(&self.transport))
    }

    /// Typed event subscriber for this connection.
    pub fn events(&self) -> EventSubscriber<T> {
        EventSubscriber::new(Arc::clone(&self.transport))
    }

    /// Register a handler to SERVE `command` — the client-provided side of the
    /// Command primitive (the substrate routes matching commands here). Twin of
    /// `commands().execute`; stop with [`Connection::revoke`].
    pub async fn provide(
        &self,
        command: &str,
        handler: Arc<dyn ServeHandler>,
    ) -> Result<(), ClientError> {
        self.transport.provide(command, handler).await
    }

    /// Stop serving `command`. Idempotent.
    pub async fn revoke(&self, command: &str) -> Result<(), ClientError> {
        self.transport.revoke(command).await
    }

    /// Close the underlying transport. After this, further command /
    /// subscribe calls return `ClientError::Closed`.
    pub async fn close(self) -> Result<(), ClientError> {
        self.transport.close().await
    }
}

impl Connection<AircIpcTransport> {
    /// Connect to a local continuum-core-server via airc IPC.
    ///
    /// `airc` is the caller's airc handle (typically built from
    /// `airc_lib::Airc::join(home)` or similar). `target_peer` is the
    /// substrate's peer UUID — operators get it from `airc status` on
    /// the running server.
    pub fn connect(airc: Arc<airc_lib::Airc>, target_peer: Uuid) -> Self {
        Self::new(AircIpcTransport::new(airc, target_peer))
    }
}
