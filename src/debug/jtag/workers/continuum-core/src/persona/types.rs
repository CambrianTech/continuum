use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboxMessage {
    pub id: Uuid,
    pub room_id: Uuid,
    pub sender_id: Uuid,
    pub sender_name: String,
    pub content: String,
    pub timestamp: u64,
    pub priority: f32,
}

impl PartialEq for InboxMessage {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for InboxMessage {}

impl PartialOrd for InboxMessage {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

// Binary heap is max-heap, so reverse ordering for priority (higher priority first)
impl Ord for InboxMessage {
    fn cmp(&self, other: &Self) -> Ordering {
        self.priority.partial_cmp(&other.priority).unwrap_or(Ordering::Equal)
    }
}
