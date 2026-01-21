//! Event System
//!
//! Events flow through the system correlated by Handle.
//! Pull-based: subscribers poll, never pushed.

use crate::handle::Handle;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::RwLock;
use tokio::sync::broadcast;

/// Event types that flow through the system
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum StreamEvent {
    /// Stream started
    Started { handle: Handle },

    /// Progress update (0.0 - 1.0)
    Progress { handle: Handle, progress: f32, message: Option<String> },

    /// Frame available (use SlotRef to access)
    FrameReady { handle: Handle, frame_type: FrameType, slot: u16 },

    /// Stream completed successfully
    Completed { handle: Handle },

    /// Stream failed
    Failed { handle: Handle, error: String },

    /// Stream cancelled by user
    Cancelled { handle: Handle },
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum FrameType {
    Audio,
    Video,
    Text,
    Image,
}

impl StreamEvent {
    /// Get the handle this event is correlated with
    pub fn handle(&self) -> Handle {
        match self {
            StreamEvent::Started { handle } => *handle,
            StreamEvent::Progress { handle, .. } => *handle,
            StreamEvent::FrameReady { handle, .. } => *handle,
            StreamEvent::Completed { handle } => *handle,
            StreamEvent::Failed { handle, .. } => *handle,
            StreamEvent::Cancelled { handle } => *handle,
        }
    }

    /// Check if this is a terminal event
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            StreamEvent::Completed { .. }
                | StreamEvent::Failed { .. }
                | StreamEvent::Cancelled { .. }
        )
    }
}

/// Event bus for publishing and subscribing to events
///
/// Uses broadcast channels - multiple subscribers can receive same events.
/// Events are correlated by Handle for filtering.
pub struct EventBus {
    /// Global broadcast channel
    sender: broadcast::Sender<StreamEvent>,

    /// Per-handle subscriptions for efficient filtering
    handle_senders: Arc<RwLock<HashMap<Handle, broadcast::Sender<StreamEvent>>>>,
}

impl EventBus {
    /// Create new event bus
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self {
            sender,
            handle_senders: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Publish an event
    pub fn publish(&self, event: StreamEvent) {
        // Send to global channel (ignore error if no subscribers)
        let _ = self.sender.send(event.clone());

        // Send to handle-specific channel if exists
        let handle = event.handle();
        if let Some(sender) = self.handle_senders.read().get(&handle) {
            let _ = sender.send(event);
        }
    }

    /// Subscribe to all events
    pub fn subscribe_all(&self) -> broadcast::Receiver<StreamEvent> {
        self.sender.subscribe()
    }

    /// Subscribe to events for a specific handle
    pub fn subscribe_handle(&self, handle: Handle) -> broadcast::Receiver<StreamEvent> {
        let mut senders = self.handle_senders.write();

        if let Some(sender) = senders.get(&handle) {
            sender.subscribe()
        } else {
            let (sender, receiver) = broadcast::channel(64);
            senders.insert(handle, sender);
            receiver
        }
    }

    /// Unsubscribe handle (cleanup after stream ends)
    pub fn unsubscribe_handle(&self, handle: Handle) {
        self.handle_senders.write().remove(&handle);
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new(1024)
    }
}

// EventBus is Send + Sync
unsafe impl Send for EventBus {}
unsafe impl Sync for EventBus {}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_event_publish_subscribe() {
        let bus = EventBus::new(16);
        let handle = Handle::new();

        let mut receiver = bus.subscribe_handle(handle);

        bus.publish(StreamEvent::Started { handle });
        bus.publish(StreamEvent::Progress {
            handle,
            progress: 0.5,
            message: Some("Halfway".to_string()),
        });
        bus.publish(StreamEvent::Completed { handle });

        let event1 = receiver.recv().await.unwrap();
        assert!(matches!(event1, StreamEvent::Started { .. }));

        let event2 = receiver.recv().await.unwrap();
        assert!(matches!(event2, StreamEvent::Progress { progress, .. } if progress == 0.5));

        let event3 = receiver.recv().await.unwrap();
        assert!(event3.is_terminal());
    }
}
