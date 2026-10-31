//! The core's seam onto airc's addressed command bus.
//!
//! A peer command reaches this node ONLY through its identity's addressed inbox
//! (`airc_lib::AddressedInbox`): frames airc has verified (signature, sender, addressee)
//! and typed (`AddressedKind::Command` / `CommandResult`), never a room event classified
//! by a header string or a body. Room subscriptions feed perception and carry no command
//! handling at all (incident 2026-10-30: an untrusted peer's commands published into
//! #general were executed and fed to residents as speech).
//!
//! The core owns these traits so the ingress contract is testable without a daemon: the
//! production impls below delegate to airc; a test supplies a fake inbox and port.
//!
//! Progress and the settled result are separate sealed frame kinds. The library
//! retains the original request deadline for both, and inbox loss/expiry remains typed.

use std::any::Any;
use std::sync::Arc;

use airc_core::{Body, Headers, PeerId};
use airc_lib::{Airc, VerifiedAddressed};
use async_trait::async_trait;
use uuid::Uuid;

pub use airc_lib::AddressedKind;

/// One verified, addressed frame, as the ingress contract reads it.
pub trait AddressedFrame: Any + Send + Sync + std::fmt::Debug {
    /// The transport-verified sender.
    fn from(&self) -> PeerId;
    /// The peer it is addressed to.
    fn to(&self) -> PeerId;
    fn kind(&self) -> AddressedKind;
    /// The request/reply pairing airc stamped.
    fn correlation(&self) -> Uuid;
    fn headers(&self) -> &Headers;
    fn deadline(&self) -> tokio::time::Instant;
    /// Opaque until the ingress contract has admitted the frame.
    fn body(&self) -> &Body;
    fn as_any(&self) -> &dyn Any;
}

impl AddressedFrame for VerifiedAddressed {
    fn from(&self) -> PeerId {
        VerifiedAddressed::from(self)
    }
    fn to(&self) -> PeerId {
        VerifiedAddressed::to(self)
    }
    fn kind(&self) -> AddressedKind {
        VerifiedAddressed::kind(self)
    }
    fn correlation(&self) -> Uuid {
        VerifiedAddressed::correlation(self)
    }
    fn headers(&self) -> &Headers {
        VerifiedAddressed::headers(self)
    }
    fn deadline(&self) -> tokio::time::Instant {
        VerifiedAddressed::deadline(self)
    }
    fn body(&self) -> &Body {
        VerifiedAddressed::body(self)
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Where addressed frames arrive; closure, loss and expiry stay distinguishable.
#[async_trait]
pub trait AddressedInbox: Send {
    async fn next(&mut self) -> Result<Arc<dyn AddressedFrame>, airc_lib::InboxError>;
}

#[async_trait]
impl AddressedInbox for airc_lib::AddressedInbox {
    async fn next(&mut self) -> Result<Arc<dyn AddressedFrame>, airc_lib::InboxError> {
        airc_lib::AddressedInbox::next(self)
            .await
            .map(|frame| frame as Arc<dyn AddressedFrame>)
    }
}

/// What the ingress needs from this node's airc identity: who it is, a peer's enrolled
/// key (to verify a presented grant), and the reply + token stream bound to a request.
#[async_trait]
pub trait AddressedPort: Send + Sync {
    fn me(&self) -> PeerId;
    fn peer_public_key(&self, peer: PeerId) -> Option<[u8; 32]>;
    async fn reply(
        &self,
        request: &dyn AddressedFrame,
        headers: Headers,
        body: Body,
    ) -> Result<(), String>;
    async fn stream(
        &self,
        request: &dyn AddressedFrame,
        headers: Headers,
        body: Body,
    ) -> Result<(), String>;
}

/// The library binds a reply to the verified request it answers; a frame it did not
/// verify (a test fake handed to the production port) has nothing to bind to.
fn verified(request: &dyn AddressedFrame) -> Result<&VerifiedAddressed, String> {
    request
        .as_any()
        .downcast_ref::<VerifiedAddressed>()
        .ok_or_else(|| "a reply is bound to a frame airc verified; this frame was not".to_string())
}

#[async_trait]
impl AddressedPort for Airc {
    fn me(&self) -> PeerId {
        self.peer_id()
    }
    fn peer_public_key(&self, peer: PeerId) -> Option<[u8; 32]> {
        Airc::peer_public_key(self, peer)
    }
    async fn reply(
        &self,
        request: &dyn AddressedFrame,
        headers: Headers,
        body: Body,
    ) -> Result<(), String> {
        Airc::reply(self, verified(request)?, headers, body)
            .await
            .map_err(|e| format!("airc reply: {e}"))
    }
    async fn stream(
        &self,
        request: &dyn AddressedFrame,
        headers: Headers,
        body: Body,
    ) -> Result<(), String> {
        Airc::reply_progress(self, verified(request)?, headers, body)
            .await
            .map_err(|e| format!("airc stream chunk: {e}"))
    }
}

/// The node's command receiver: the pump hands it each addressed frame.
#[async_trait]
pub trait AddressedHandler: Send + Sync {
    async fn on_addressed(&self, frame: Arc<dyn AddressedFrame>);
}

#[cfg(any(test, feature = "test-fixtures"))]
pub mod fixtures;
