//! Connection — owns a `Transport`, hands out command + event clients.

use std::sync::Arc;

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
