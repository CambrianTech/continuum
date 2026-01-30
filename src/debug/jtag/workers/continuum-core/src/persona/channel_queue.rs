//! ChannelQueue — Generic queue container that delegates all decisions to items
//!
//! This module has ZERO item-type-specific logic. It asks items:
//! - How to sort? → item.effective_priority()
//! - Is this urgent? → item.is_urgent()
//! - Can this be dropped? → item.can_be_kicked() / item.kick_resistance()
//! - Should items merge? → item.should_consolidate_with()
//!
//! One ChannelQueue per ActivityDomain. The CNS iterates channels in priority order.

use super::channel_items::{ChatQueueItem, TaskQueueItem};
use super::channel_types::{ActivityDomain, ChannelStatus, QueueItemBehavior};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::debug;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Configuration for a channel queue
pub struct ChannelQueueConfig {
    pub domain: ActivityDomain,
    pub max_size: usize,
    pub name: String,
}

/// Generic queue container — delegates ALL behavioral decisions to items.
pub struct ChannelQueue {
    domain: ActivityDomain,
    name: String,
    max_size: usize,
    items: Vec<Box<dyn QueueItemBehavior>>,
}

impl ChannelQueue {
    pub fn new(config: ChannelQueueConfig) -> Self {
        Self {
            domain: config.domain,
            name: config.name,
            max_size: config.max_size,
            items: Vec::new(),
        }
    }

    // =========================================================================
    // ENQUEUE — Items decide their own kick policy
    // =========================================================================

    /// Add item to this channel's queue.
    /// Sorts by effective_priority. If over capacity, kicks items that allow it
    /// (lowest kick_resistance first).
    pub fn enqueue(&mut self, item: Box<dyn QueueItemBehavior>) {
        self.items.push(item);
        self.sort();

        // Capacity management: ASK ITEMS if they can be kicked
        while self.items.len() > self.max_size {
            let now = now_ms();
            // Find kickable items sorted by resistance (lowest first)
            let mut kickable_indices: Vec<(usize, f32)> = self.items.iter()
                .enumerate()
                .filter(|(_, item)| item.can_be_kicked())
                .map(|(i, item)| {
                    (i, item.kick_resistance(now, item.timestamp()))
                })
                .collect();

            if kickable_indices.is_empty() {
                break; // Nothing can be kicked — queue stays oversized
            }

            // Sort by resistance ascending (lowest kicked first)
            kickable_indices.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

            let kick_idx = kickable_indices[0].0;
            let kicked = self.items.remove(kick_idx);
            debug!(
                "Kicked item {} (type={}, resistance={:.2}) from {} channel (size={})",
                kicked.id(),
                kicked.item_type(),
                kickable_indices[0].1,
                self.name,
                self.items.len()
            );
        }
    }

    // =========================================================================
    // CONSOLIDATION — Items decide their own merge policy
    // =========================================================================

    /// Consolidate items in this channel.
    /// Items decide: should_consolidate_with() determines groups.
    /// Type-specific consolidation methods merge the groups.
    ///
    /// Called once per CNS service cycle before processing.
    pub fn consolidate(&mut self) {
        self.consolidate_rebuild();
    }

    /// Cleaner consolidation: collect groups, then rebuild in one pass.
    fn consolidate_rebuild(&mut self) {
        if self.items.len() <= 1 {
            return;
        }

        let mut consumed = vec![false; self.items.len()];
        let mut groups: Vec<(usize, Vec<usize>)> = Vec::new(); // (anchor_idx, group_member_indices)

        // Phase 1: identify groups
        for i in 0..self.items.len() {
            if consumed[i] {
                continue;
            }

            let mut group: Vec<usize> = Vec::new();
            #[allow(clippy::needless_range_loop)] // j indexes both consumed[] and self.items[]
            for j in (i + 1)..self.items.len() {
                if !consumed[j] && self.items[i].should_consolidate_with(self.items[j].as_ref()) {
                    group.push(j);
                    consumed[j] = true;
                }
            }

            if !group.is_empty() {
                consumed[i] = true;
                groups.push((i, group));
            }
        }

        if groups.is_empty() {
            return; // Nothing to consolidate
        }

        // Phase 2: build consolidated items
        let mut consolidated_items: Vec<Box<dyn QueueItemBehavior>> = Vec::new();
        let mut all_consumed: Vec<bool> = vec![false; self.items.len()];

        for (anchor_idx, group_indices) in &groups {
            all_consumed[*anchor_idx] = true;
            for &idx in group_indices {
                all_consumed[idx] = true;
            }

            let item_type = self.items[*anchor_idx].item_type();
            match item_type {
                "chat" => {
                    if let Some(c) = self.consolidate_chat_group(*anchor_idx, group_indices) {
                        consolidated_items.push(c);
                    }
                }
                "task" => {
                    if let Some(c) = self.consolidate_task_group(*anchor_idx, group_indices) {
                        consolidated_items.push(c);
                    }
                }
                _ => {
                    // Can't consolidate unknown types — they stay unconsumed
                    all_consumed[*anchor_idx] = false;
                    for &idx in group_indices {
                        all_consumed[idx] = false;
                    }
                }
            }
        }

        // Phase 3: rebuild items list
        let old_items = std::mem::take(&mut self.items);
        let mut new_items: Vec<Box<dyn QueueItemBehavior>> = Vec::new();

        // Add unconsumed items (singletons)
        for (i, item) in old_items.into_iter().enumerate() {
            if !all_consumed[i] {
                new_items.push(item);
            }
        }

        // Add consolidated items
        new_items.extend(consolidated_items);

        self.items = new_items;
        self.sort();
    }

    /// Consolidate a group of chat items
    fn consolidate_chat_group(
        &self,
        anchor_idx: usize,
        group_indices: &[usize],
    ) -> Option<Box<dyn QueueItemBehavior>> {
        let anchor = self.items[anchor_idx].as_any().downcast_ref::<ChatQueueItem>()?;
        let others: Vec<&ChatQueueItem> = group_indices.iter()
            .filter_map(|&idx| self.items[idx].as_any().downcast_ref::<ChatQueueItem>())
            .collect();

        Some(Box::new(anchor.consolidate_with_items(&others)))
    }

    /// Consolidate a group of task items
    fn consolidate_task_group(
        &self,
        anchor_idx: usize,
        group_indices: &[usize],
    ) -> Option<Box<dyn QueueItemBehavior>> {
        let anchor = self.items[anchor_idx].as_any().downcast_ref::<TaskQueueItem>()?;
        let others: Vec<&TaskQueueItem> = group_indices.iter()
            .filter_map(|&idx| self.items[idx].as_any().downcast_ref::<TaskQueueItem>())
            .collect();

        Some(Box::new(anchor.consolidate_with_items(&others)))
    }

    // =========================================================================
    // ACCESSORS — All delegate to item properties
    // =========================================================================

    /// Any item in this channel reports itself as urgent
    pub fn has_urgent_work(&self) -> bool {
        self.items.iter().any(|i| i.is_urgent())
    }

    /// Channel has any items at all
    pub fn has_work(&self) -> bool {
        !self.items.is_empty()
    }

    /// Number of items in this channel
    pub fn size(&self) -> usize {
        self.items.len()
    }

    /// Look at the highest-priority item without removing it
    pub fn peek(&self) -> Option<&dyn QueueItemBehavior> {
        self.items.first().map(|i| i.as_ref())
    }

    /// Get the priority of the highest-priority item (for state gating check)
    pub fn peek_priority(&self) -> f32 {
        let now = now_ms();
        self.items.first()
            .map(|i| i.effective_priority(now, i.timestamp()))
            .unwrap_or(0.0)
    }

    /// Remove and return the highest-priority item
    pub fn pop(&mut self) -> Option<Box<dyn QueueItemBehavior>> {
        if self.items.is_empty() {
            return None;
        }
        // Re-sort before popping (aging changes order)
        self.sort();
        Some(self.items.remove(0))
    }

    /// Get channel status snapshot
    pub fn status(&self) -> ChannelStatus {
        ChannelStatus {
            domain: self.domain,
            size: self.items.len() as u32,
            has_urgent: self.has_urgent_work(),
            has_work: self.has_work(),
        }
    }

    /// Channel domain
    pub fn domain(&self) -> ActivityDomain {
        self.domain
    }

    /// Clear all items
    pub fn clear(&mut self) {
        self.items.clear();
    }

    // =========================================================================
    // INTERNALS
    // =========================================================================

    fn sort(&mut self) {
        let now = now_ms();
        self.items.sort_by(|a, b| {
            // Use item timestamp as enqueued_at proxy (items set enqueued_at = now on construction)
            let pa = a.effective_priority(now, a.timestamp());
            let pb = b.effective_priority(now, b.timestamp());
            // Higher priority first
            pb.partial_cmp(&pa).unwrap_or(std::cmp::Ordering::Equal)
        });
    }
}

//=============================================================================
// TESTS
//=============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::channel_items::*;
    use super::super::types::SenderType;
    use uuid::Uuid;

    fn make_chat_queue() -> ChannelQueue {
        ChannelQueue::new(ChannelQueueConfig {
            domain: ActivityDomain::Chat,
            max_size: 10,
            name: "chat".into(),
        })
    }

    fn boxed_chat(room: Uuid, mentions: bool, priority: f32) -> Box<dyn QueueItemBehavior> {
        Box::new(ChatQueueItem {
            id: Uuid::new_v4(),
            room_id: room,
            content: format!("Message p={priority}"),
            sender_id: Uuid::new_v4(),
            sender_name: "User".into(),
            sender_type: SenderType::Human,
            mentions,
            timestamp: now_ms(),
            enqueued_at: now_ms(),
            priority,
            consolidated_context: Vec::new(),
        })
    }

    fn boxed_voice() -> Box<dyn QueueItemBehavior> {
        Box::new(VoiceQueueItem {
            id: Uuid::new_v4(),
            room_id: Uuid::new_v4(),
            content: "Voice".into(),
            sender_id: Uuid::new_v4(),
            sender_name: "Joel".into(),
            sender_type: SenderType::Human,
            voice_session_id: Uuid::new_v4(),
            timestamp: now_ms(),
            enqueued_at: now_ms(),
            priority: 1.0,
        })
    }

    #[test]
    fn test_enqueue_and_pop_priority_order() {
        let mut queue = make_chat_queue();
        let room = Uuid::new_v4();

        queue.enqueue(boxed_chat(room, false, 0.3));
        queue.enqueue(boxed_chat(room, false, 0.9));
        queue.enqueue(boxed_chat(room, false, 0.5));

        assert_eq!(queue.size(), 3);
        assert!(queue.has_work());

        // Should pop highest priority first
        let first = queue.pop().unwrap();
        assert!((first.base_priority() - 0.9).abs() < 0.01);

        let second = queue.pop().unwrap();
        assert!((second.base_priority() - 0.5).abs() < 0.01);

        let third = queue.pop().unwrap();
        assert!((third.base_priority() - 0.3).abs() < 0.01);

        assert!(!queue.has_work());
    }

    #[test]
    fn test_capacity_kick() {
        let mut queue = ChannelQueue::new(ChannelQueueConfig {
            domain: ActivityDomain::Chat,
            max_size: 3,
            name: "small-chat".into(),
        });
        let room = Uuid::new_v4();

        queue.enqueue(boxed_chat(room, false, 0.9));
        queue.enqueue(boxed_chat(room, false, 0.5));
        queue.enqueue(boxed_chat(room, false, 0.3));
        assert_eq!(queue.size(), 3);

        // Adding a 4th should kick the lowest priority
        queue.enqueue(boxed_chat(room, false, 0.7));
        assert_eq!(queue.size(), 3); // Still 3 after kick
    }

    #[test]
    fn test_voice_never_kicked() {
        let mut queue = ChannelQueue::new(ChannelQueueConfig {
            domain: ActivityDomain::Audio,
            max_size: 2,
            name: "audio".into(),
        });

        queue.enqueue(boxed_voice());
        queue.enqueue(boxed_voice());
        queue.enqueue(boxed_voice()); // Over capacity

        // Voice items can't be kicked, so queue stays oversized
        assert_eq!(queue.size(), 3);
    }

    #[test]
    fn test_has_urgent_work() {
        let mut queue = make_chat_queue();
        let room = Uuid::new_v4();

        queue.enqueue(boxed_chat(room, false, 0.5));
        assert!(!queue.has_urgent_work());

        queue.enqueue(boxed_chat(room, true, 0.8)); // mention = urgent
        assert!(queue.has_urgent_work());
    }

    #[test]
    fn test_chat_consolidation() {
        let mut queue = make_chat_queue();
        let room = Uuid::new_v4();
        let other_room = Uuid::new_v4();

        queue.enqueue(boxed_chat(room, false, 0.5));
        queue.enqueue(boxed_chat(room, false, 0.7));
        queue.enqueue(boxed_chat(room, false, 0.3));
        queue.enqueue(boxed_chat(other_room, false, 0.6));

        assert_eq!(queue.size(), 4);

        queue.consolidate();

        // 3 same-room messages → 1 consolidated + 1 other-room = 2
        assert_eq!(queue.size(), 2);
    }

    #[test]
    fn test_peek_priority() {
        let mut queue = make_chat_queue();
        let room = Uuid::new_v4();

        queue.enqueue(boxed_chat(room, false, 0.3));
        queue.enqueue(boxed_chat(room, false, 0.9));

        let p = queue.peek_priority();
        assert!((p - 0.9).abs() < 0.05, "Expected ~0.9, got {p}");
    }

    #[test]
    fn test_status_snapshot() {
        let mut queue = make_chat_queue();
        let room = Uuid::new_v4();

        let status = queue.status();
        assert_eq!(status.size, 0);
        assert!(!status.has_work);
        assert!(!status.has_urgent);

        queue.enqueue(boxed_chat(room, true, 0.8));
        let status = queue.status();
        assert_eq!(status.size, 1);
        assert!(status.has_work);
        assert!(status.has_urgent);
    }
}
