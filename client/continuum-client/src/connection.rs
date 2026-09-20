//! Connection — owns a `Transport`, hands out command + event clients.

use std::sync::Arc;

use uuid::Uuid;

use crate::airc_ipc::AircIpcTransport;
use crate::command::CommandClient;
use crate::error::ClientError;
use crate::event::EventSubscriber;
use crate::session::SessionIdentity;
use crate::transport::{ServeHandler, Transport};

/// One session against a continuum substrate. Generic over the wire so
/// the same code drives local airc IPC, remote airc grid, and the
/// `MockTransport` used in downstream tests.
///
/// A connection carries WHO it acts as (`identity`: userId + sessionId) and,
/// optionally, WHERE — the conversation/room `context_id` a [`scoped`] view is
/// bound to. The 4 verbs hang off this one object; `scoped(ctx)` returns another
/// connection over the SAME transport with the context set, so every client —
/// UI, CLI, persona — uses the identical `connect → session → scoped(context)`
/// shape (`[[persona-is-a-client]]`).
///
/// [`scoped`]: Connection::scoped
pub struct Connection<T: Transport> {
    transport: Arc<T>,
    identity: SessionIdentity,
    context_id: Option<Uuid>,
}

// Manual `Clone` (not derived): a connection is cheap to clone — it shares the
// one `Arc<T>` transport. Derived `Clone` would wrongly require `T: Clone`;
// `Arc<T>` is `Clone` regardless. Cloning lets a `provide` registration hold a
// handle to revoke on drop without re-establishing the session, and is how
// `scoped` returns a context-bound view over the same wire.
impl<T: Transport> Clone for Connection<T> {
    fn clone(&self) -> Self {
        Self {
            transport: Arc::clone(&self.transport),
            identity: self.identity.clone(),
            context_id: self.context_id,
        }
    }
}

impl<T: Transport> Connection<T> {
    /// Build a connection over an already-established transport. Higher-
    /// level constructors (e.g. `connect_local`, `connect_remote`) will
    /// wrap this once the airc transport impls land. Identity is unknown
    /// until the establishing layer sets it via [`with_identity`].
    ///
    /// [`with_identity`]: Connection::with_identity
    pub fn new(transport: T) -> Self {
        Self {
            transport: Arc::new(transport),
            identity: SessionIdentity::unknown(),
            context_id: None,
        }
    }

    /// Set the identity this connection acts as — called by the establishing
    /// layer (airc pairing / substrate handshake for a UI client; the spawn
    /// path for a persona, which knows its own citizen id). The SDK surfaces
    /// what's established; it never fabricates identity.
    pub fn with_identity(mut self, identity: SessionIdentity) -> Self {
        self.identity = identity;
        self
    }

    /// WHO this connection acts as — citizen (`userId`) + session instance
    /// (`sessionId`). Surfaced uniformly across every client + persona.
    pub fn session(&self) -> SessionIdentity {
        self.identity.clone()
    }

    /// The conversation/room this connection is scoped to, if any (the third
    /// ID tier). `None` on an unscoped connection.
    pub fn context_id(&self) -> Option<Uuid> {
        self.context_id
    }

    /// Return a view of this connection SCOPED to a conversation/room — its
    /// command + event verbs auto-stamp `contextId` (the third ID tier) so
    /// callers never re-thread the scope. Shares the same transport + identity;
    /// only the context differs. This is how a persona services a room (scoped
    /// to that room's contextId) the same way a browser tab does.
    pub fn scoped(&self, context_id: Uuid) -> Self {
        Self {
            transport: Arc::clone(&self.transport),
            identity: self.identity.clone(),
            context_id: Some(context_id),
        }
    }

    /// Typed command dispatcher for this connection, scoped to its context.
    pub fn commands(&self) -> CommandClient<T> {
        CommandClient::with_context(Arc::clone(&self.transport), self.context_id)
    }

    /// Typed event subscriber for this connection.
    pub fn events(&self) -> EventSubscriber<T> {
        EventSubscriber::new(Arc::clone(&self.transport))
    }

    /// Publish an event to `class` — the publish side of the Event primitive
    /// (the emit twin of `events().subscribe`). When this connection is scoped,
    /// `contextId` is stamped into the payload (the same third-tier scope the
    /// command path stamps), so an emitted event carries the conversation it
    /// belongs to.
    pub async fn emit(&self, class: &str, mut payload: serde_json::Value) -> Result<(), ClientError> {
        if let Some(context_id) = self.context_id {
            if let serde_json::Value::Object(map) = &mut payload {
                map.insert(
                    "contextId".to_string(),
                    serde_json::Value::String(context_id.to_string()),
                );
            }
        }
        self.transport.emit(class, payload).await
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockTransport;
    use futures::StreamExt;
    use serde_json::json;
    use std::sync::Mutex;

    // what this catches: a SCOPED connection stamps contextId (the third ID
    // tier) into the command envelope as a sibling of params — the mechanism the
    // substrate reads it from (command_envelope.rs). Regression here un-scopes
    // every per-context handler silently.
    #[tokio::test]
    async fn scoped_connection_stamps_context_id_into_command_envelope() {
        let captured: Arc<Mutex<Option<serde_json::Value>>> = Arc::new(Mutex::new(None));
        let cap = Arc::clone(&captured);
        let mock = MockTransport::new();
        mock.respond_to("chat/send", move |params| {
            *cap.lock().unwrap() = Some(params);
            Ok(json!({ "ok": true }))
        });
        let ctx = Uuid::new_v4();

        let conn = Connection::new(mock).scoped(ctx);
        let _: serde_json::Value = conn
            .commands()
            .execute("chat/send", json!({ "text": "hi" }))
            .await
            .expect("execute");

        let sent = captured.lock().unwrap().clone().expect("handler saw params");
        assert_eq!(sent["text"], "hi", "command params survive intact");
        assert_eq!(
            sent["contextId"],
            json!(ctx.to_string()),
            "scoped connection stamps contextId as an envelope sibling"
        );
    }

    // what this catches: an UNSCOPED connection sends NO contextId — context is
    // opt-in via scoped(), never leaked onto a bare connection.
    #[tokio::test]
    async fn unscoped_connection_omits_context_id() {
        let captured: Arc<Mutex<Option<serde_json::Value>>> = Arc::new(Mutex::new(None));
        let cap = Arc::clone(&captured);
        let mock = MockTransport::new();
        mock.respond_to("ping", move |params| {
            *cap.lock().unwrap() = Some(params);
            Ok(json!({}))
        });

        let conn = Connection::new(mock); // not scoped
        let _: serde_json::Value = conn.commands().execute("ping", json!({})).await.expect("execute");

        let sent = captured.lock().unwrap().clone().expect("handler saw params");
        assert!(
            sent.get("contextId").is_none(),
            "unscoped connection must not stamp contextId"
        );
    }

    // what this catches: session() surfaces the established identity; an
    // un-established connection is honestly unknown (None/None), never fabricated.
    #[tokio::test]
    async fn session_surfaces_established_identity() {
        let unknown = Connection::new(MockTransport::new());
        assert_eq!(unknown.session(), SessionIdentity::unknown());
        assert_eq!(unknown.context_id(), None);

        let id = SessionIdentity::new(Uuid::new_v4(), Uuid::new_v4());
        let known = Connection::new(MockTransport::new()).with_identity(id.clone());
        assert_eq!(known.session(), id);
    }

    // what this catches: scoped() carries identity forward + sets context, and
    // does NOT mutate the parent — a persona scoped to a room is the same citizen.
    #[tokio::test]
    async fn scoped_preserves_identity_and_leaves_parent_unscoped() {
        let id = SessionIdentity::new(Uuid::new_v4(), Uuid::new_v4());
        let ctx = Uuid::new_v4();
        let conn = Connection::new(MockTransport::new()).with_identity(id.clone());

        let scoped = conn.scoped(ctx);
        assert_eq!(scoped.session(), id, "same citizen, now scoped");
        assert_eq!(scoped.context_id(), Some(ctx));
        assert_eq!(conn.context_id(), None, "parent connection stays unscoped");
    }

    // what this catches: an emitted event from a scoped connection carries its
    // contextId in the payload — the event knows the conversation it belongs to.
    #[tokio::test]
    async fn scoped_connection_stamps_context_id_into_emitted_event() {
        let mock = MockTransport::new();
        let ctx = Uuid::new_v4();
        let conn = Connection::new(mock.clone()).scoped(ctx);

        let mut stream = mock.subscribe("room:msg").await.expect("subscribe");
        conn.emit("room:msg", json!({ "text": "hi" })).await.expect("emit");

        let ev = stream.next().await.expect("event").expect("ok");
        assert_eq!(ev["text"], "hi");
        assert_eq!(ev["contextId"], json!(ctx.to_string()));
    }
}
