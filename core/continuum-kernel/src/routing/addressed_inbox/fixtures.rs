//! A fake addressed inbox and port: the ingress contract and the command receiver are
//! exercised without an airc daemon. Production cannot link these (feature-gated).

use std::any::Any;
use std::sync::Arc;

use airc_core::{Body, Headers, PeerId};
use async_trait::async_trait;
use tokio::sync::mpsc;
use uuid::Uuid;

use super::{AddressedFrame, AddressedInbox, AddressedKind, AddressedPort};
use crate::routing::{AircCommandRequest, HEADER_COMMAND_PATH};

/// A frame as if airc had verified it.
#[derive(Debug, Clone)]
pub struct FakeFrame {
    pub from: PeerId,
    pub to: PeerId,
    pub kind: AddressedKind,
    pub correlation: Uuid,
    pub headers: Headers,
    pub body: Option<Body>,
}

impl FakeFrame {
    /// A `Command` from `from` to `to` naming `header_path`, carrying a request for `body_path`.
    pub fn command(
        from: PeerId,
        to: PeerId,
        header_path: Option<&str>,
        body_path: &str,
        params: serde_json::Value,
    ) -> Self {
        let mut headers = Headers::new();
        if let Some(path) = header_path {
            headers.insert(HEADER_COMMAND_PATH.to_string(), path.to_string());
        }
        let request =
            AircCommandRequest::new(body_path.to_string(), "peer".to_string(), None, params);
        Self {
            from,
            to,
            kind: AddressedKind::Command,
            correlation: Uuid::new_v4(),
            headers,
            body: serde_json::to_value(&request).ok().map(Body::Json), // ok(): a request of strings and a JSON value always serializes; None would surface as a malformed-request refusal
        }
    }
}

impl AddressedFrame for FakeFrame {
    fn from(&self) -> PeerId {
        self.from
    }
    fn to(&self) -> PeerId {
        self.to
    }
    fn kind(&self) -> AddressedKind {
        self.kind
    }
    fn correlation(&self) -> Uuid {
        self.correlation
    }
    fn headers(&self) -> &Headers {
        &self.headers
    }
    fn body(&self) -> Option<&Body> {
        self.body.as_ref()
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// An inbox fed by the test.
pub struct FakeInbox(mpsc::UnboundedReceiver<Arc<dyn AddressedFrame>>);

impl FakeInbox {
    pub fn channel() -> (mpsc::UnboundedSender<Arc<dyn AddressedFrame>>, Self) {
        let (tx, rx) = mpsc::unbounded_channel();
        (tx, Self(rx))
    }
}

#[async_trait]
impl AddressedInbox for FakeInbox {
    async fn next(&mut self) -> Option<Arc<dyn AddressedFrame>> {
        self.0.recv().await
    }
}

/// What the port was asked to send.
#[derive(Debug, Clone)]
pub struct Sent {
    /// The correlation of the request it answers.
    pub correlation: Uuid,
    pub headers: Headers,
    pub body: Body,
    /// A token-stream chunk rather than the settled reply.
    pub stream: bool,
}

/// This node's identity, as the test sees it: replies and stream chunks land on a channel.
pub struct FakePort {
    me: PeerId,
    sent: mpsc::UnboundedSender<Sent>,
}

impl FakePort {
    pub fn new(me: PeerId) -> (Arc<Self>, mpsc::UnboundedReceiver<Sent>) {
        let (sent, rx) = mpsc::unbounded_channel();
        (Arc::new(Self { me, sent }), rx)
    }

    fn send(
        &self,
        request: &dyn AddressedFrame,
        headers: Headers,
        body: Body,
        stream: bool,
    ) -> Result<(), String> {
        self.sent
            .send(Sent {
                correlation: request.correlation(),
                headers,
                body,
                stream,
            })
            .map_err(|_| "the test stopped listening".to_string())
    }
}

#[async_trait]
impl AddressedPort for FakePort {
    fn me(&self) -> PeerId {
        self.me
    }
    fn peer_public_key(&self, _peer: PeerId) -> Option<[u8; 32]> {
        None
    }
    async fn reply(
        &self,
        request: &dyn AddressedFrame,
        headers: Headers,
        body: Body,
    ) -> Result<(), String> {
        self.send(request, headers, body, false)
    }
    async fn stream(
        &self,
        request: &dyn AddressedFrame,
        headers: Headers,
        body: Body,
    ) -> Result<(), String> {
        self.send(request, headers, body, true)
    }
}
