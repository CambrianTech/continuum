use super::types::InboxMessage;
use std::collections::BinaryHeap;
use tokio::sync::{mpsc, Notify};
use std::sync::Arc;
use uuid::Uuid;

/// Concurrent persona inbox with priority queue
///
/// Pattern: Message passing via Tokio channels (no locks)
/// - enqueue() sends to channel (non-blocking)
/// - Worker task drains channel into BinaryHeap
/// - dequeue() pulls from heap (lock-free via channel)
pub struct PersonaInbox {
    persona_id: Uuid,
    enqueue_tx: mpsc::UnboundedSender<InboxMessage>,
    dequeue_rx: mpsc::UnboundedReceiver<InboxMessage>,
    signal: Arc<Notify>,
}

impl PersonaInbox {
    pub fn new(persona_id: Uuid) -> Self {
        let (enqueue_tx, mut enqueue_rx) = mpsc::unbounded_channel::<InboxMessage>();
        let (dequeue_tx, dequeue_rx) = mpsc::unbounded_channel::<InboxMessage>();
        let signal = Arc::new(Notify::new());
        let signal_clone = signal.clone();

        // Spawn worker task to manage priority queue
        tokio::spawn(async move {
            let mut heap: BinaryHeap<InboxMessage> = BinaryHeap::new();

            loop {
                tokio::select! {
                    // Receive new messages from enqueue channel
                    Some(msg) = enqueue_rx.recv() => {
                        heap.push(msg);
                        // Don't notify here - let dequeue() trigger the pop
                        // This ensures priority ordering is preserved across batches
                    }

                    // Send highest priority message to dequeue channel
                    // Only triggered when dequeue() calls notify_one()
                    _ = signal_clone.notified(), if !heap.is_empty() => {
                        if let Some(msg) = heap.pop() {
                            let _ = dequeue_tx.send(msg);
                        }
                    }
                }
            }
        });

        Self {
            persona_id,
            enqueue_tx,
            dequeue_rx,
            signal,
        }
    }

    /// Enqueue message (non-blocking)
    pub fn enqueue(&self, message: InboxMessage) {
        let _ = self.enqueue_tx.send(message);
    }

    /// Dequeue highest priority message (async)
    pub async fn dequeue(&mut self) -> Option<InboxMessage> {
        self.signal.notify_one();
        self.dequeue_rx.recv().await
    }

    /// Wait for work available signal
    pub async fn wait_for_work(&self) {
        self.signal.notified().await;
    }

    pub fn persona_id(&self) -> Uuid {
        self.persona_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_priority_ordering() {
        let persona_id = Uuid::new_v4();
        let mut inbox = PersonaInbox::new(persona_id);

        // Enqueue messages with different priorities
        let low_msg = InboxMessage {
            id: Uuid::new_v4(),
            room_id: Uuid::new_v4(),
            sender_id: Uuid::new_v4(),
            sender_name: "Test".to_string(),
            content: "Low priority".to_string(),
            timestamp: 1000,
            priority: 0.3,
        };

        let high_msg = InboxMessage {
            id: Uuid::new_v4(),
            room_id: Uuid::new_v4(),
            sender_id: Uuid::new_v4(),
            sender_name: "Test".to_string(),
            content: "High priority".to_string(),
            timestamp: 2000,
            priority: 0.9,
        };

        inbox.enqueue(low_msg.clone());
        inbox.enqueue(high_msg.clone());

        // Wait for worker task to process
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Should get high priority first
        let first = inbox.dequeue().await.unwrap();
        assert_eq!(first.priority, 0.9, "First message should be high priority");

        let second = inbox.dequeue().await.unwrap();
        assert_eq!(second.priority, 0.3, "Second message should be low priority");
    }
}
