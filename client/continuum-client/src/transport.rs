//! Transport — the seam between a `Connection` and the wire.
//!
//! The two universal primitives are each BIDIRECTIONAL. Commands = `request`
//! (call out) + [`provide`](Transport::provide) (serve a command the substrate
//! / a peer routes here — the client's display / sensors / renderer that the
//! core can't reach). Events = `subscribe` (listen) + emit (publish; the
//! publish verb lands once the substrate's client→publisher receive-door
//! exists — see SDK-API-SURFACE.md). This trait carries the call/listen +
//! serve halves; the publish half is a fast-follow.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use crate::error::ClientError;
use crate::event::EventStream;

/// A handler the client REGISTERS to serve a command it provides — the serve
/// side of the Command primitive. When the substrate (or a peer) routes a
/// command this client provides, the transport's serve loop calls this with the
/// request params and replies with the result. JSON in / JSON out; the typed
/// SDK layer wraps it, the FFI facade exposes it as a foreign callback.
/// OpenCV-style adapter polymorphism: one command identity, N platform handlers
/// (web = DOM/canvas, desktop = OS, AR/VR = renderer capture).
#[async_trait]
pub trait ServeHandler: Send + Sync {
    /// Run the provided command. `Ok` ships as the command result; `Err`
    /// surfaces to the caller as a refusal — never a silent drop.
    async fn handle(&self, params: Value) -> Result<Value, ClientError>;
}

/// Pluggable wire for a `Connection`. Implementations: local airc IPC
/// (a continuum-core-server on the same machine), remote airc grid
/// (a substrate on another peer), and a `MockTransport` (under
/// `test-fixtures`) for downstream unit tests.
#[async_trait]
pub trait Transport: Send + Sync + 'static {
    /// Round-trip one command. Caller gives JSON params, gets JSON result
    /// or a typed error. (Command: CALL.)
    async fn request(&self, command: &str, params: Value) -> Result<Value, ClientError>;

    /// Open an event stream for the given class. Class strings follow the
    /// substrate's URI convention (e.g. `"persona.response.*"`). (Event: LISTEN.)
    async fn subscribe(&self, class: &str) -> Result<EventStream, ClientError>;

    /// Publish an event to `class` — the publish twin of `subscribe`, routed to
    /// the substrate's event fan-out. (Event: PUBLISH.)
    async fn emit(&self, class: &str, payload: Value) -> Result<(), ClientError>;

    /// Register `handler` to serve `command`: inbound requests the substrate
    /// routes here are dispatched to it and the result replied automatically.
    /// The serve twin of `request`. Idempotent per command (re-registering
    /// replaces). (Command: SERVE.)
    async fn provide(
        &self,
        command: &str,
        handler: Arc<dyn ServeHandler>,
    ) -> Result<(), ClientError>;

    /// Stop serving `command` (deregister its handler). Idempotent — revoking a
    /// command that was never provided is a no-op, not an error.
    async fn revoke(&self, command: &str) -> Result<(), ClientError>;

    /// Close the underlying connection. Idempotent; later calls on the
    /// same transport return `ClientError::Closed`.
    async fn close(&self) -> Result<(), ClientError>;
}
