use super::types::InboxMessage;
use serde::{Deserialize, Serialize};
use std::collections::BinaryHeap;
use std::sync::Mutex;
use std::time::Instant;
use ts_rs::TS;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../shared/generated/persona/PersonaInboxFrameMetrics.ts"
)]
pub struct PersonaInboxFrameMetrics {
    pub queue_depth_before: usize,
    pub queue_depth_after: usize,
    pub messages_drained: usize,
    #[ts(type = "number")]
    pub oldest_timestamp: u64,
    #[ts(type = "number")]
    pub newest_timestamp: u64,
    #[ts(type = "number")]
    pub frame_span_ms: u64,
    #[ts(type = "number")]
    pub drain_duration_us: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../shared/generated/persona/PersonaInboxFrame.ts"
)]
pub struct PersonaInboxFrame {
    #[ts(type = "string")]
    pub persona_id: Uuid,
    #[ts(type = "string")]
    pub room_id: Uuid,
    pub messages: Vec<InboxMessage>,
    pub metrics: PersonaInboxFrameMetrics,
}

/// Concurrent persona inbox with a priority queue and frame drain.
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

    /// Drain a bounded, same-room work frame around the highest-priority trigger.
    ///
    /// This is the persona equivalent of a computer-vision frame: collect the
    /// coherent work available now, process it once, and leave unrelated work in
    /// the queue. Callers get timing/depth metrics without inventing logging in
    /// the TypeScript wrapper.
    pub fn drain_frame(&self, window_ms: u64, max_items: usize) -> Option<PersonaInboxFrame> {
        if max_items == 0 {
            return None;
        }

        let start = Instant::now();
        let mut heap = self.heap.lock().ok()?;
        let queue_depth_before = heap.len();
        let anchor = heap.pop()?;
        let room_id = anchor.room_id;
        let anchor_timestamp = anchor.timestamp;

        let mut messages = Vec::with_capacity(max_items.min(queue_depth_before));
        messages.push(anchor);

        let mut retained = Vec::with_capacity(heap.len());
        while let Some(message) = heap.pop() {
            if messages.len() < max_items
                && message.room_id == room_id
                && message.timestamp.abs_diff(anchor_timestamp) <= window_ms
            {
                messages.push(message);
            } else {
                retained.push(message);
            }
        }

        heap.extend(retained);
        let queue_depth_after = heap.len();
        drop(heap);

        messages.sort_by_key(|message| message.timestamp);
        let oldest_timestamp = messages
            .first()
            .map(|message| message.timestamp)
            .unwrap_or(0);
        let newest_timestamp = messages
            .last()
            .map(|message| message.timestamp)
            .unwrap_or(0);

        Some(PersonaInboxFrame {
            persona_id: self.persona_id,
            room_id,
            metrics: PersonaInboxFrameMetrics {
                queue_depth_before,
                queue_depth_after,
                messages_drained: messages.len(),
                oldest_timestamp,
                newest_timestamp,
                frame_span_ms: newest_timestamp.saturating_sub(oldest_timestamp),
                drain_duration_us: u64::try_from(start.elapsed().as_micros()).unwrap_or(u64::MAX),
            },
            messages,
        })
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
    use crate::persona::{Modality, SenderType};

    fn message(
        room_id: Uuid,
        content: &str,
        timestamp: u64,
        priority: f32,
        source_modality: Option<Modality>,
    ) -> InboxMessage {
        InboxMessage {
            id: Uuid::new_v4(),
            room_id,
            sender_id: Uuid::new_v4(),
            sender_name: "Test".to_string(),
            sender_type: SenderType::Human,
            content: content.to_string(),
            timestamp,
            priority,
            source_modality,
            voice_session_id: None,
        }
    }

    #[test]
    fn test_priority_ordering() {
        let persona_id = Uuid::new_v4();
        let inbox = PersonaInbox::new(persona_id);

        let room_id = Uuid::new_v4();
        let low_msg = message(room_id, "Low priority", 1000, 0.3, None);
        let high_msg = message(room_id, "High priority", 2000, 0.9, None);

        inbox.enqueue(low_msg.clone());
        inbox.enqueue(high_msg.clone());

        // BinaryHeap is max-heap, so high priority should come first
        let first = inbox.dequeue().unwrap();
        assert_eq!(first.priority, 0.9, "First message should be high priority");

        let second = inbox.dequeue().unwrap();
        assert_eq!(
            second.priority, 0.3,
            "Second message should be low priority"
        );

        // Third should be None
        assert!(inbox.dequeue().is_none(), "Should be empty now");
    }

    #[test]
    fn test_drain_frame_batches_same_room_window_and_keeps_others() {
        let persona_id = Uuid::new_v4();
        let inbox = PersonaInbox::new(persona_id);
        let room_a = Uuid::new_v4();
        let room_b = Uuid::new_v4();

        inbox.enqueue(message(room_a, "earlier", 1_000, 0.4, Some(Modality::Chat)));
        inbox.enqueue(message(
            room_a,
            "trigger",
            1_030,
            0.9,
            Some(Modality::Voice),
        ));
        inbox.enqueue(message(room_a, "later", 1_070, 0.5, Some(Modality::Chat)));
        inbox.enqueue(message(room_a, "outside window", 1_500, 0.6, None));
        inbox.enqueue(message(room_b, "other room", 1_035, 0.8, None));

        let frame = inbox.drain_frame(100, 8).expect("frame should drain");

        assert_eq!(frame.persona_id, persona_id);
        assert_eq!(frame.room_id, room_a);
        assert_eq!(frame.messages.len(), 3);
        assert_eq!(
            frame
                .messages
                .iter()
                .map(|message| message.content.as_str())
                .collect::<Vec<_>>(),
            vec!["earlier", "trigger", "later"]
        );
        assert_eq!(frame.metrics.queue_depth_before, 5);
        assert_eq!(frame.metrics.queue_depth_after, 2);
        assert_eq!(frame.metrics.messages_drained, 3);
        assert_eq!(frame.metrics.oldest_timestamp, 1_000);
        assert_eq!(frame.metrics.newest_timestamp, 1_070);
        assert_eq!(frame.metrics.frame_span_ms, 70);

        let remaining_first = inbox.dequeue().expect("other room should remain");
        assert_eq!(remaining_first.content, "other room");
        let remaining_second = inbox.dequeue().expect("outside window should remain");
        assert_eq!(remaining_second.content, "outside window");
        assert!(inbox.dequeue().is_none());
    }

    #[test]
    fn test_drain_frame_respects_max_items_and_leaves_overflow() {
        let inbox = PersonaInbox::new(Uuid::new_v4());
        let room_id = Uuid::new_v4();

        inbox.enqueue(message(room_id, "first", 1_000, 0.9, None));
        inbox.enqueue(message(room_id, "second", 1_001, 0.8, None));
        inbox.enqueue(message(room_id, "third", 1_002, 0.7, None));

        let frame = inbox.drain_frame(100, 2).expect("frame should drain");

        assert_eq!(frame.messages.len(), 2);
        assert_eq!(frame.metrics.queue_depth_before, 3);
        assert_eq!(frame.metrics.queue_depth_after, 1);
        assert_eq!(inbox.len(), 1);
        assert_eq!(inbox.dequeue().expect("overflow remains").content, "third");
    }

    #[test]
    fn test_drain_frame_zero_max_items_is_noop() {
        let inbox = PersonaInbox::new(Uuid::new_v4());
        let room_id = Uuid::new_v4();
        inbox.enqueue(message(room_id, "kept", 1_000, 0.9, None));

        assert!(inbox.drain_frame(100, 0).is_none());
        assert_eq!(inbox.len(), 1);
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
