//! Session identity — WHO a connection is acting as.
//!
//! The first two tiers of the ID hierarchy (CLAUDE.md): `userId` (the permanent
//! citizen) and `sessionId` (this connection instance). The third tier —
//! `contextId` (the conversation/room scope) — is NOT here: it's per-scope, not
//! per-connection, so it lives on a scoped [`Connection`](crate::Connection),
//! not in the identity.
//!
//! Both fields are `Option` because identity is ESTABLISHED, not assumed: a
//! freshly-built connection may not yet know its identity until the establishing
//! layer sets it (`Connection::with_identity`) — the airc pairing / substrate
//! handshake for a UI client, or the spawn path for a persona (which knows its
//! own citizen id). The SDK surfaces what's known; it never fabricates identity.
//!
//! This is uniform across EVERY client — UI, CLI, and personas alike. A persona
//! is a citizen with a `SessionIdentity` just like a browser tab
//! (`[[persona-is-a-client]]`).

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The identity a [`Connection`](crate::Connection) acts as — citizen + session
/// instance. Surfaced by `Connection::session()`; the FFI facade re-exports it
/// as a record so every language SDK reads the same `{ userId?, sessionId? }`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionIdentity {
    /// The permanent citizen — set from the airc pairing / substrate mapping
    /// (or the persona's own id). `None` until established.
    pub user_id: Option<Uuid>,
    /// This connection instance (the "browser tab" tier). `None` until
    /// established by the connecting layer.
    pub session_id: Option<Uuid>,
}

impl SessionIdentity {
    /// An unestablished identity — the connection doesn't yet know who it is.
    pub fn unknown() -> Self {
        Self::default()
    }

    /// Build a fully-known identity (citizen + session instance).
    pub fn new(user_id: Uuid, session_id: Uuid) -> Self {
        Self {
            user_id: Some(user_id),
            session_id: Some(session_id),
        }
    }
}
