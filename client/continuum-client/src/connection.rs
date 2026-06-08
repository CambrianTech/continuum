//! Connection — owns a `Transport`, hands out command + event clients.

use std::sync::Arc;

use uuid::Uuid;

use crate::airc_ipc::AircIpcTransport;
use crate::command::CommandClient;
use crate::error::ClientError;
use crate::event::EventSubscriber;
use crate::transport::Transport;

/// One session against a continuum substrate. Generic over the wire so
/// the same code drives local airc IPC, remote airc grid, and the
/// `MockTransport` used in downstream tests.
pub struct Connection<T: Transport> {
    transport: Arc<T>,
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
