//! Typed event subscription over a `Transport`.

use std::pin::Pin;
use std::sync::Arc;

use futures::Stream;
use serde_json::Value;

use crate::error::ClientError;
use crate::transport::Transport;

/// An open stream of substrate events as raw JSON values. Consumers
/// typically wrap this in `EventSubscriber::subscribe::<E>` to get
/// typed events.
pub type EventStream = Pin<Box<dyn Stream<Item = Result<Value, ClientError>> + Send>>;

/// Subscribes to substrate events over `T`.
pub struct EventSubscriber<T: Transport> {
    transport: Arc<T>,
}

impl<T: Transport> EventSubscriber<T> {
    pub(crate) fn new(transport: Arc<T>) -> Self {
        Self { transport }
    }

    /// Open an event stream for a class. Class strings follow the
    /// substrate's URI convention (e.g. `"persona.response.*"`).
    pub async fn subscribe(&self, class: &str) -> Result<EventStream, ClientError> {
        self.transport.subscribe(class).await
    }
}
