//! Transport — the seam between a `Connection` and the wire.

use async_trait::async_trait;
use serde_json::Value;

use crate::error::ClientError;
use crate::event::EventStream;

/// Pluggable wire for a `Connection`. Implementations: local airc IPC
/// (a continuum-core-server on the same machine), remote airc grid
/// (a substrate on another peer), and a `MockTransport` (under
/// `test-fixtures`) for downstream unit tests.
#[async_trait]
pub trait Transport: Send + Sync + 'static {
    /// Round-trip one command. Caller gives JSON params, gets JSON result
    /// or a typed error.
    async fn request(&self, command: &str, params: Value) -> Result<Value, ClientError>;

    /// Open an event stream for the given class. Class strings follow the
    /// substrate's URI convention (e.g. `"persona.response.*"`).
    async fn subscribe(&self, class: &str) -> Result<EventStream, ClientError>;

    /// Close the underlying connection. Idempotent; later calls on the
    /// same transport return `ClientError::Closed`.
    async fn close(&self) -> Result<(), ClientError>;
}
