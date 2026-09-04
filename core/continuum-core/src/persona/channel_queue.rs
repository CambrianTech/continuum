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
use super::channel_types::{ActivityDomain, ChannelStatus, CoherentUnit, QueueItemBehavior};
use std::sync::Arc;
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
///
/// Items are held as `Arc<dyn QueueItemBehavior>` so multiple consumers
/// (cognition + observers + future per-persona channel views) can share
/// references to the same item. Per
/// `[[pass-by-reference-lazy-metadata-with-data]]`: items are
/// immutable after enqueue; lazy-cached derived state (embedding,
/// RAG chunks, future STT/video decode) rides on the item itself as
/// `OnceLock<Arc<T>>` cells, so the first consumer that demands the
/// decoded form triggers compute and every subsequent consumer gets
/// the cached Arc clone.
///
/// Consolidation produces NEW Arc'd items (see `consolidate_chat_group`
/// / `consolidate_task_group`); originals are dropped along with the
/// old `items` vec when `consolidate_rebuild` swaps in the new list,
/// which is sound because Arc refcounts drop only when the LAST
/// consumer releases.
pub struct ChannelQueue {
    domain: ActivityDomain,
    name: String,
    max_size: usize,
    items: Vec<Arc<dyn QueueItemBehavior>>,
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
    pub fn enqueue(&mut self, item: Arc<dyn QueueItemBehavior>) {
        self.items.push(item);
        self.sort();

        // Capacity management: ASK ITEMS if they can be kicked
        while self.items.len() > self.max_size {
            let now = now_ms();
            // Find kickable items sorted by resistance (lowest first)
            let mut kickable_indices: Vec<(usize, f32)> = self
                .items
                .iter()
                .enumerate()
                .filter(|(_, item)| item.can_be_kicked())
                .map(|(i, item)| (i, item.kick_resistance(now, item.timestamp())))
                .collect();

            if kickable_indices.is_empty() {
                break; // Nothing can be kicked — queue stays oversized
            }

            // Sort by resistance ascending (lowest kicked first)
            kickable_indices
                .sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

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

    /// O(N) consolidation via single HashMap-by-key pass.
    ///
    /// Replaces the prior O(N²) pairwise `should_consolidate_with`
    /// check (task #246). The old shape ran N(N-1)/2 vtable calls per
    /// service tick; on N=500 that's 124,750 calls per tick — the
    /// dominant cost in the demand-pull architecture proof's wall-clock
    /// measurement (architecture_demand_pull_cognition.rs).
    ///
    /// ## Algorithm
    ///
    /// 1. **Single pass**: bucket items by `item.consolidation_key()`.
    ///    `None` → singleton bucket (kept as-is). `Some(k)` → group
    ///    bucket keyed by k. O(N) work, O(N) memory.
    /// 2. **Per-bucket consolidation**: groups with ≥2 items merge via
    ///    the item-type's typed consolidator. Singletons and lone-key
    ///    groups pass through.
    /// 3. **Rebuild**: assemble `new_items` from singletons +
    ///    consolidated anchors. Sort restores priority order.
    ///
    /// Total cost: O(N) hash inserts + O(K) per-group consolidation
    /// where K is the largest bucket size. Realistic K is bounded by
    /// arrival burst size (e.g. ~50 same-room messages between ticks),
    /// not by total inbox.
    ///
    /// ## Contract preservation
    ///
    /// The legacy `should_consolidate_with` trait method now defaults
    /// to `a.key() == b.key() && key.is_some()` so existing tests
    /// asserting the predicate keep passing. New per-type impls
    /// override `consolidation_key`; the predicate derives for free.
    ///
    /// ## Anchor selection
    ///
    /// Within each group, the anchor is the LOWEST-INDEX item — same
    /// semantics as the legacy O(N²) impl's outer-loop-wins behavior.
    /// Consolidators are called with `(anchor_idx, &member_indices)`
    /// matching the prior signature.
    fn consolidate_rebuild(&mut self) {
        if self.items.len() <= 1 {
            return;
        }

        use std::collections::HashMap;

        // Phase 1: single-pass group-by-key (O(N) hash inserts).
        // Items with `None` keys are singletons and skip the bucket
        // dance entirely. For grouped items, the FIRST item per key
        // becomes the anchor (lowest-index = same anchor selection
        // semantics as the legacy O(N²) impl's outer-loop).
        struct Bucket {
            anchor_idx: usize,
            members: Vec<usize>,
        }
        let mut buckets: HashMap<u64, Bucket> = HashMap::new();
        for (i, item) in self.items.iter().enumerate() {
            let Some(key) = item.consolidation_key() else {
                continue;
            };
            buckets
                .entry(key)
                .and_modify(|b| b.members.push(i))
                .or_insert(Bucket {
                    anchor_idx: i,
                    members: Vec::new(),
                });
        }

        // Phase 2: per-bucket consolidation (only buckets with ≥1
        // member beyond the anchor actually merge). The same per-type
        // dispatch as the legacy impl — chat / task / unknown.
        let mut consolidated_items: Vec<Arc<dyn QueueItemBehavior>> = Vec::new();
        let mut all_consumed: Vec<bool> = vec![false; self.items.len()];

        for bucket in buckets.values() {
            if bucket.members.is_empty() {
                // Lone item with a key — no one to consolidate with;
                // pass through as a singleton. Don't mark consumed so
                // it flows through Phase 3's pass-through path.
                continue;
            }

            // Tentatively mark the whole bucket consumed; if the
            // per-type consolidator declines (unknown item type), we
            // un-mark them below so they pass through as singletons.
            all_consumed[bucket.anchor_idx] = true;
            for &idx in &bucket.members {
                all_consumed[idx] = true;
            }

            let item_type = self.items[bucket.anchor_idx].item_type();
            let consolidated = match item_type {
                "chat" => self.consolidate_chat_group(bucket.anchor_idx, &bucket.members),
                "task" => self.consolidate_task_group(bucket.anchor_idx, &bucket.members),
                _ => None,
            };
            match consolidated {
                Some(c) => consolidated_items.push(c),
                None => {
                    // Per-type consolidator declined — un-consume the
                    // bucket so its items pass through as singletons.
                    all_consumed[bucket.anchor_idx] = false;
                    for &idx in &bucket.members {
                        all_consumed[idx] = false;
                    }
                }
            }
        }

        if consolidated_items.is_empty() {
            // No groups merged — nothing to rebuild.
            return;
        }

        // Phase 3: rebuild items list — singletons + consolidated.
        let old_items = std::mem::take(&mut self.items);
        let mut new_items: Vec<Arc<dyn QueueItemBehavior>> = Vec::with_capacity(
            old_items.len() - consolidated_items.len() + consolidated_items.len(),
        );
        for (i, item) in old_items.into_iter().enumerate() {
            if !all_consumed[i] {
                new_items.push(item);
            }
        }
        new_items.extend(consolidated_items);

        self.items = new_items;
        self.sort();
    }

    /// Consolidate a group of chat items
    fn consolidate_chat_group(
        &self,
        anchor_idx: usize,
        group_indices: &[usize],
    ) -> Option<Arc<dyn QueueItemBehavior>> {
        let anchor = self.items[anchor_idx]
            .as_any()
            .downcast_ref::<ChatQueueItem>()?;
        let others: Vec<&ChatQueueItem> = group_indices
            .iter()
            .filter_map(|&idx| self.items[idx].as_any().downcast_ref::<ChatQueueItem>())
            .collect();

        Some(Arc::new(anchor.consolidate_with_items(&others)))
    }

    /// Consolidate a group of task items
    fn consolidate_task_group(
        &self,
        anchor_idx: usize,
        group_indices: &[usize],
    ) -> Option<Arc<dyn QueueItemBehavior>> {
        let anchor = self.items[anchor_idx]
            .as_any()
            .downcast_ref::<TaskQueueItem>()?;
        let others: Vec<&TaskQueueItem> = group_indices
            .iter()
            .filter_map(|&idx| self.items[idx].as_any().downcast_ref::<TaskQueueItem>())
            .collect();

        Some(Arc::new(anchor.consolidate_with_items(&others)))
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

    /// Look at the highest-priority item without removing it.
    /// Returns `&Arc<dyn>` so callers can clone for shared reference if
    /// needed without removing from the queue.
    pub fn peek(&self) -> Option<&Arc<dyn QueueItemBehavior>> {
        self.items.first()
    }

    /// Look at the highest-priority item as a trait-object reference
    /// (no Arc handle). Cheap accessor for callers that only need to
    /// read item state.
    pub fn peek_ref(&self) -> Option<&dyn QueueItemBehavior> {
        self.items.first().map(|i| i.as_ref())
    }

    /// Get the priority of the highest-priority item (for state gating check)
    pub fn peek_priority(&self) -> f32 {
        let now = now_ms();
        self.items
            .first()
            .map(|i| i.effective_priority(now, i.timestamp()))
            .unwrap_or(0.0)
    }

    /// Remove and return the highest-priority item
    pub fn pop(&mut self) -> Option<Arc<dyn QueueItemBehavior>> {
        if self.items.is_empty() {
            return None;
        }
        // Re-sort before popping (aging changes order)
        self.sort();
        Some(self.items.remove(0))
    }

    /// Drain a coherent batch from this channel: run consolidation,
    /// then pull items within `window_ms` of the highest-priority
    /// anchor into a typed `CoherentUnit`. Returns `None` when the
    /// queue is empty.
    ///
    /// This is the cognition-side consumption primitive per
    /// `[[cognition-batches-per-channel-adapter]]`: one drain per
    /// channel-tick yields one `CoherentUnit` that cognition's
    /// `analyze()` consumes ONCE — not N times for N items. Items
    /// within the window come out together; items outside it stay
    /// in the queue for the next cycle (RTOS catch-up doesn't
    /// compound — slow personas see CURRENT state, not a backlog).
    ///
    /// Items are returned as `Vec<Arc<dyn QueueItemBehavior>>` to
    /// preserve the lazy-cell sharing per
    /// `[[pass-by-reference-lazy-metadata-with-data]]` — consumers
    /// across multiple personas in the same room share the same
    /// items and thus the same lazy-cached derived state (embedding,
    /// future STT, etc.).
    ///
    /// The `window_ms` is anchored to the highest-priority item's
    /// timestamp; this mirrors `PersonaInbox::drain_frame`'s shape so
    /// the cognition layer sees one consistent "burst boundary"
    /// semantic across both the legacy inbox and the per-channel
    /// queues.
    ///
    /// **Window is BIDIRECTIONAL** — items with timestamp in
    /// `[anchor_ts.saturating_sub(window_ms), anchor_ts.saturating_add(window_ms)]`
    /// come out as the burst; items outside that range stay in the
    /// queue for the next tick. The range is INCLUSIVE on both ends.
    ///
    /// Edge cases:
    /// - Items with `ts == anchor_ts ± window_ms` are IN the burst.
    /// - Items with `ts == anchor_ts ± (window_ms + 1)` are deferred.
    /// - With `window_ms == 0`, only items sharing the anchor's exact
    ///   timestamp are drained.
    /// - `saturating_sub` / `saturating_add` prevent overflow at
    ///   extreme timestamps.
    pub fn drain_batch(&mut self, window_ms: u64) -> Option<CoherentUnit> {
        // Run the existing item-driven consolidation FIRST so the
        // drain operates on already-merged items. Items that decided
        // to merge (per `should_consolidate_with`) collapse into new
        // Arc'd consolidated items before the window-based pull.
        self.consolidate();

        if self.items.is_empty() {
            return None;
        }

        // Sort by current effective priority (aging matters — items
        // that have waited get boosted toward the front).
        self.sort();

        // Anchor = highest-priority item. Its timestamp defines the
        // window's pivot.
        let anchor_ts = self.items[0].timestamp();
        let window_lo = anchor_ts.saturating_sub(window_ms);
        let window_hi = anchor_ts.saturating_add(window_ms);

        // Partition: items inside the window come out as the batch;
        // items outside stay in the queue.
        let mut in_window: Vec<Arc<dyn QueueItemBehavior>> = Vec::new();
        let mut retained: Vec<Arc<dyn QueueItemBehavior>> = Vec::new();
        for item in self.items.drain(..) {
            let ts = item.timestamp();
            if ts >= window_lo && ts <= window_hi {
                in_window.push(item);
            } else {
                retained.push(item);
            }
        }
        self.items = retained;

        if in_window.is_empty() {
            return None;
        }

        // Compute window span. anchor is at index 0 by definition; the
        // others are bounded by ±window_ms around it.
        let window_span_ms = in_window
            .iter()
            .map(|i| i.timestamp().abs_diff(anchor_ts))
            .max()
            .unwrap_or(0);

        // Build the typed CoherentUnit. Each queue is per-domain
        // (routing enforces it), so we discriminate on `self.domain`.
        let unit = match self.domain {
            ActivityDomain::Chat => {
                // Pull the primary_room from the anchor. Consolidation
                // guarantees same-room grouping for chat items, so the
                // anchor's room is the burst's room.
                let primary_room = in_window
                    .first()
                    .and_then(|i| i.as_any().downcast_ref::<ChatQueueItem>())
                    .map(|c| c.room_id)
                    .unwrap_or_else(uuid::Uuid::nil);
                CoherentUnit::Chat {
                    items: in_window,
                    window_span_ms,
                    primary_room,
                }
            }
            ActivityDomain::Audio => CoherentUnit::Voice {
                items: in_window,
                window_span_ms,
            },
            ActivityDomain::Code => CoherentUnit::Task {
                items: in_window,
                window_span_ms,
            },
            ActivityDomain::Background => CoherentUnit::Background {
                items: in_window,
                window_span_ms,
            },
        };

        Some(unit)
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
    use super::super::channel_items::*;
    use super::super::types::SenderType;
    use super::*;
    use uuid::Uuid;

    fn make_chat_queue() -> ChannelQueue {
        ChannelQueue::new(ChannelQueueConfig {
            domain: ActivityDomain::Chat,
            max_size: 10,
            name: "chat".into(),
        })
    }

    fn arc_chat(room: Uuid, mentions: bool, priority: f32) -> Arc<dyn QueueItemBehavior> {
        Arc::new(ChatQueueItem {
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
            media: Vec::new(),
            embedding_cell: std::sync::OnceLock::new(),
            #[cfg(any(test, feature = "test-fixtures"))]
            compute_calls: std::sync::atomic::AtomicUsize::new(0),
        })
    }

    fn arc_voice() -> Arc<dyn QueueItemBehavior> {
        Arc::new(VoiceQueueItem {
            id: Uuid::new_v4(),
            room_id: Uuid::new_v4(),
            content: "Voice".into(),
            sender_id: Uuid::new_v4(),
            sender_name: "test-user".into(),
            sender_type: SenderType::Human,
            voice_session_id: Uuid::new_v4(),
            timestamp: now_ms(),
            enqueued_at: now_ms(),
            priority: 1.0,
            media: Vec::new(),
        })
    }

    #[test]
    fn test_enqueue_and_pop_priority_order() {
        let mut queue = make_chat_queue();
        let room = Uuid::new_v4();

        queue.enqueue(arc_chat(room, false, 0.3));
        queue.enqueue(arc_chat(room, false, 0.9));
        queue.enqueue(arc_chat(room, false, 0.5));

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

        queue.enqueue(arc_chat(room, false, 0.9));
        queue.enqueue(arc_chat(room, false, 0.5));
        queue.enqueue(arc_chat(room, false, 0.3));
        assert_eq!(queue.size(), 3);

        // Adding a 4th should kick the lowest priority
        queue.enqueue(arc_chat(room, false, 0.7));
        assert_eq!(queue.size(), 3); // Still 3 after kick
    }

    /// proves: cognition-batches-per-channel — one drain returns one
    /// `CoherentUnit` regardless of how many items match the window.
    ///
    /// Enqueue N chat items in the same room within a tight window;
    /// `drain_batch` must return ONE `CoherentUnit::Chat` containing
    /// the consolidated post-merge items. Cognition's `analyze()`
    /// then fires ONCE per cycle on this batch — not N times.
    #[test]
    fn drain_batch_returns_one_coherent_unit_for_n_arrivals() {
        let mut queue = make_chat_queue();
        let room = Uuid::new_v4();

        // 5 chat items in the same room. ChatQueueItem's
        // should_consolidate_with returns true for same-room chat,
        // so consolidate() will collapse them.
        for priority in [0.3_f32, 0.5, 0.7, 0.4, 0.6] {
            queue.enqueue(arc_chat(room, false, priority));
        }
        assert_eq!(queue.size(), 5);

        // 1-second window — generous enough to catch all items
        // enqueued back-to-back in the test.
        let unit = queue.drain_batch(1_000).expect("drain returns Some");

        // The batch carries the typed Chat variant.
        match unit {
            CoherentUnit::Chat {
                ref items,
                primary_room,
                ..
            } => {
                // After consolidation, the items collapse into the
                // anchor's consolidated form — 5 originals become 1
                // post-merge item carrying the others as
                // `consolidated_context`. The cognition layer
                // analyzes ONE coherent unit even though 5 raw
                // messages arrived.
                assert!(
                    !items.is_empty(),
                    "drain returned an empty batch — consolidation should preserve at least the anchor"
                );
                assert_eq!(primary_room, room, "primary_room must equal the chat room");
            }
            other => panic!("expected CoherentUnit::Chat, got {other:?}"),
        }
    }

    /// proves: cognition-batches-per-channel — empty queue returns None
    #[test]
    fn drain_batch_on_empty_queue_returns_none() {
        let mut queue = make_chat_queue();
        assert!(queue.drain_batch(1_000).is_none());
    }

    #[test]
    fn test_voice_never_kicked() {
        let mut queue = ChannelQueue::new(ChannelQueueConfig {
            domain: ActivityDomain::Audio,
            max_size: 2,
            name: "audio".into(),
        });

        queue.enqueue(arc_voice());
        queue.enqueue(arc_voice());
        queue.enqueue(arc_voice()); // Over capacity

        // Voice items can't be kicked, so queue stays oversized
        assert_eq!(queue.size(), 3);
    }

    #[test]
    fn test_has_urgent_work() {
        let mut queue = make_chat_queue();
        let room = Uuid::new_v4();

        queue.enqueue(arc_chat(room, false, 0.5));
        assert!(!queue.has_urgent_work());

        queue.enqueue(arc_chat(room, true, 0.8)); // mention = urgent
        assert!(queue.has_urgent_work());
    }

    #[test]
    fn test_chat_consolidation() {
        let mut queue = make_chat_queue();
        let room = Uuid::new_v4();
        let other_room = Uuid::new_v4();

        queue.enqueue(arc_chat(room, false, 0.5));
        queue.enqueue(arc_chat(room, false, 0.7));
        queue.enqueue(arc_chat(room, false, 0.3));
        queue.enqueue(arc_chat(other_room, false, 0.6));

        assert_eq!(queue.size(), 4);

        queue.consolidate();

        // 3 same-room messages → 1 consolidated + 1 other-room = 2
        assert_eq!(queue.size(), 2);
    }

    #[test]
    fn test_peek_priority() {
        let mut queue = make_chat_queue();
        let room = Uuid::new_v4();

        queue.enqueue(arc_chat(room, false, 0.3));
        queue.enqueue(arc_chat(room, false, 0.9));

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

        queue.enqueue(arc_chat(room, true, 0.8));
        let status = queue.status();
        assert_eq!(status.size, 1);
        assert!(status.has_work);
        assert!(status.has_urgent);
    }
}
