use super::types::InboxMessage;
use std::collections::BinaryHeap;
use std::sync::Mutex;
use uuid::Uuid;

/// Concurrent persona inbox with priority queue
///
/// Pattern: Simple synchronous priority queue with mutex
/// - enqueue() adds to heap (with lock)
/// - dequeue() pops from heap (with lock)
/// - No Tokio runtime required (safe to use from std::thread)
///
/// NOTE: This is a simpler implementation that doesn't require Tokio.
/// For high-throughput async use cases, consider adding a Tokio-based
/// variant with channels and spawned worker tasks.
pub struct PersonaInbox {
    persona_id: Uuid,
    heap: Mutex<BinaryHeap<InboxMessage>>,
}

impl PersonaInbox {
    pub fn new(persona_id: Uuid) -> Self {
        Self {
            persona_id,
            heap: Mutex::new(BinaryHeap::new()),
        }
    }

    /// Enqueue message (non-blocking, uses mutex)
    pub fn enqueue(&self, message: InboxMessage) {
        if let Ok(mut heap) = self.heap.lock() {
            heap.push(message);
        }
    }

    /// Dequeue highest priority message (sync)
    pub fn dequeue(&self) -> Option<InboxMessage> {
        if let Ok(mut heap) = self.heap.lock() {
            heap.pop()
        } else {
            None
        }
    }

    /// Check if inbox has messages
    pub fn has_messages(&self) -> bool {
        if let Ok(heap) = self.heap.lock() {
            !heap.is_empty()
        } else {
            false
        }
    }

    /// Get message count
    pub fn len(&self) -> usize {
        if let Ok(heap) = self.heap.lock() {
            heap.len()
        } else {
            0
        }
    }

    /// Check if inbox is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn persona_id(&self) -> Uuid {
        self.persona_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persona::SenderType;

    #[test]
    fn test_priority_ordering() {
        let persona_id = Uuid::new_v4();
        let inbox = PersonaInbox::new(persona_id);

        // Enqueue messages with different priorities
        let low_msg = InboxMessage {
            id: Uuid::new_v4(),
            room_id: Uuid::new_v4(),
            sender_id: Uuid::new_v4(),
            sender_name: "Test".to_string(),
            sender_type: SenderType::Human,
            content: "Low priority".to_string(),
            timestamp: 1000,
            priority: 0.3,
            source_modality: None,
            voice_session_id: None,
        };

        let high_msg = InboxMessage {
            id: Uuid::new_v4(),
            room_id: Uuid::new_v4(),
            sender_id: Uuid::new_v4(),
            sender_name: "Test".to_string(),
            sender_type: SenderType::Human,
            content: "High priority".to_string(),
            timestamp: 2000,
            priority: 0.9,
            source_modality: None,
            voice_session_id: None,
        };

        inbox.enqueue(low_msg.clone());
        inbox.enqueue(high_msg.clone());

        // BinaryHeap is max-heap, so high priority should come first
        let first = inbox.dequeue().unwrap();
        assert_eq!(first.priority, 0.9, "First message should be high priority");

        let second = inbox.dequeue().unwrap();
        assert_eq!(second.priority, 0.3, "Second message should be low priority");

        // Third should be None
        assert!(inbox.dequeue().is_none(), "Should be empty now");
    }

    #[test]
    fn test_empty_inbox() {
        let persona_id = Uuid::new_v4();
        let inbox = PersonaInbox::new(persona_id);

        assert!(!inbox.has_messages());
        assert_eq!(inbox.len(), 0);
        assert!(inbox.dequeue().is_none());
    }
}
