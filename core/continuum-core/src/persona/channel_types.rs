//! Channel Queue Types — ActivityDomain + QueueItemBehavior trait
//!
//! Mirrors the TypeScript BaseQueueItem abstract class as a Rust trait.
//! Items control their own behavior: urgency, consolidation, kick resistance, aging.
//! The queue is a generic container that delegates all decisions to items.
//!
//! Pattern: Template method via default trait implementations.
//! Subclasses (VoiceQueueItem, ChatQueueItem, TaskQueueItem) override only what differs.

use serde::{Deserialize, Serialize};
use std::any::Any;
use ts_rs::TS;
use uuid::Uuid;

//=============================================================================
// ACTIVITY DOMAIN — Which channel an item routes to
//=============================================================================

/// Activity domain for channel routing.
/// Each domain has one ChannelQueue. Items route to their domain's queue.
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/ActivityDomain.ts"
)]
pub enum ActivityDomain {
    /// Voice/audio: always urgent, never kicked, no consolidation
    Audio,
    /// Chat messages: per-room consolidation, mention urgency, RTOS aging
    Chat,
    /// Code: workspace-scoped coding tasks, not urgent, never kicked, slow aging
    Code,
    /// Background tasks: dependency-aware, overdue urgency
    Background,
}

/// All currently registered domains in priority order (highest first).
/// Used by service_cycle() to iterate channels.
pub const DOMAIN_PRIORITY_ORDER: &[ActivityDomain] = &[
    ActivityDomain::Audio,
    ActivityDomain::Chat,
    ActivityDomain::Code,
    ActivityDomain::Background,
];

//=============================================================================
// QUEUE ITEM BEHAVIOR — Trait replacing TS BaseQueueItem abstract class
//=============================================================================

/// Core trait for queue items. Items control their own destiny.
///
/// The queue/channel is a generic container that asks items:
/// - How to sort? → effective_priority()
/// - Is this urgent? → is_urgent()
/// - Can it be dropped? → can_be_kicked() / kick_resistance()
/// - Should items merge? → should_consolidate_with()
///
/// Default implementations provide sensible RTOS-style behavior.
/// Subclasses override only what differs (e.g., Voice: always urgent, never kicked).
pub trait QueueItemBehavior: Send + Sync + Any + std::fmt::Debug {
    /// Runtime type discriminator (e.g., "voice", "chat", "task")
    fn item_type(&self) -> &'static str;

    /// Which activity domain this item belongs to
    fn domain(&self) -> ActivityDomain;

    /// Unique identifier for this item
    fn id(&self) -> Uuid;

    /// Creation timestamp (Unix ms)
    fn timestamp(&self) -> u64;

    /// Base priority (0.0-1.0). Subclasses define their own scale.
    fn base_priority(&self) -> f32;

    // =========================================================================
    // RTOS AGING (Template Method Pattern)
    // =========================================================================

    /// Time in milliseconds for aging boost to reach maximum.
    /// Override to change aging speed. Set very high to effectively disable.
    /// Default: 30,000ms (30 seconds)
    fn aging_boost_ms(&self) -> f32 {
        30_000.0
    }

    /// Maximum priority boost from queue aging (0.0-1.0).
    /// Override to 0 to disable aging entirely (e.g., voice).
    /// Default: 0.5
    fn max_aging_boost(&self) -> f32 {
        0.5
    }

    /// Effective priority = base_priority + aging boost.
    /// RTOS-style: items waiting longer get higher effective priority.
    /// This prevents starvation — every item eventually gets serviced.
    ///
    /// Subclasses rarely override this; instead override aging_boost_ms/max_aging_boost.
    fn effective_priority(&self, now_ms: u64, enqueued_at_ms: u64) -> f32 {
        let wait_ms = now_ms.saturating_sub(enqueued_at_ms) as f32;
        let aging_ms = self.aging_boost_ms();
        if aging_ms <= 0.0 {
            return self.base_priority().min(1.0);
        }
        let boost = (wait_ms / aging_ms * self.max_aging_boost()).min(self.max_aging_boost());
        (self.base_priority() + boost).min(1.0)
    }

    // =========================================================================
    // URGENCY
    // =========================================================================

    /// Is this item time-critical? Urgent items bypass the cognitive scheduler.
    /// Default: false. Voice overrides to true. Chat overrides for mentions.
    fn is_urgent(&self) -> bool {
        false
    }

    // =========================================================================
    // QUEUE MANAGEMENT (KICKING)
    // =========================================================================

    /// Can this item be dropped when the queue is at capacity?
    /// Default: true. Voice overrides to false (never drop voice).
    fn can_be_kicked(&self) -> bool {
        true
    }

    /// Resistance to being kicked. Lower values are kicked first.
    /// Default: effective_priority (low priority items kicked first).
    /// Voice overrides to f32::INFINITY (never kicked).
    fn kick_resistance(&self, now_ms: u64, enqueued_at_ms: u64) -> f32 {
        self.effective_priority(now_ms, enqueued_at_ms)
    }

    // =========================================================================
    // ROUTING
    // =========================================================================

    /// Which channel should this item be routed to?
    /// Default: self.domain(). Override for items that belong to a different
    /// channel than their logical domain.
    fn routing_domain(&self) -> ActivityDomain {
        self.domain()
    }

    // =========================================================================
    // CONSOLIDATION
    // =========================================================================

    /// Opaque consolidation key — items returning the SAME key
    /// consolidate together. `None` means "this item is always a
    /// singleton" (matches the default `should_consolidate_with`
    /// shape: never).
    ///
    /// Replaces the legacy O(N²) `should_consolidate_with` pairwise
    /// check with a single HashMap pass in `ChannelQueue::consolidate`:
    /// identical keys land in the same bucket. The trait now expresses
    /// the consolidation rule as a key the queue can hash, not as a
    /// pairwise predicate the queue has to N²-poll.
    ///
    /// ## Contract
    ///
    /// `a.consolidation_key() == b.consolidation_key()` iff
    /// `a.should_consolidate_with(b)` would have returned `true`.
    /// The default `should_consolidate_with` implementation below
    /// reads `consolidation_key` so implementing one for free gives
    /// the other; concrete impls should override `consolidation_key`
    /// only (and let `should_consolidate_with` default-derive).
    ///
    /// ## Why u64 (not String / typed enum)
    ///
    /// - Zero allocation per call (substring of the hot path).
    /// - No enum sprawl across the trait — each item folds its
    ///   criteria through a stable hasher.
    /// - HashMap<u64, _> is the substrate's idiomatic key-group shape.
    ///
    /// Stable-hash by feeding the item's `item_type()` + its
    /// consolidation criteria into a fresh `DefaultHasher`. Mixing
    /// `item_type` first prevents cross-type collisions (a chat with
    /// room_id=X must NOT key-match a task with context_id=X).
    fn consolidation_key(&self) -> Option<u64> {
        None
    }

    /// Can this item be merged with another item in the same channel?
    /// Default implementation derives from `consolidation_key`: items
    /// merge iff their keys are equal AND non-None.
    ///
    /// Concrete impls should override `consolidation_key` rather than
    /// this method — the queue's hot path uses `consolidation_key`
    /// directly for its O(N) HashMap grouping; `should_consolidate_with`
    /// stays for ad-hoc predicate checks (tests, ext consumers).
    fn should_consolidate_with(&self, other: &dyn QueueItemBehavior) -> bool {
        match (self.consolidation_key(), other.consolidation_key()) {
            (Some(a), Some(b)) => a == b,
            _ => false,
        }
    }

    /// Downcast to Any for type-specific consolidation checks
    fn as_any(&self) -> &dyn Any;

    // =========================================================================
    // SERIALIZATION — For IPC transport back to TypeScript
    // =========================================================================

    /// Serialize this item to JSON for IPC transport.
    /// Each item type includes its discriminator and all fields.
    fn to_json(&self) -> serde_json::Value;
}

//=============================================================================
// CHANNEL STATUS — Returned by IPC for monitoring
//=============================================================================

/// Per-channel status snapshot
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/ChannelStatus.ts"
)]
pub struct ChannelStatus {
    pub domain: ActivityDomain,
    pub size: u32,
    pub has_urgent: bool,
    pub has_work: bool,
}

/// Full channel registry status
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/ChannelRegistryStatus.ts"
)]
pub struct ChannelRegistryStatus {
    pub channels: Vec<ChannelStatus>,
    pub total_size: u32,
    pub has_urgent_work: bool,
    pub has_work: bool,
}

//=============================================================================
// COHERENT UNIT — one batch per channel-tick, the per-channel-typed shape
// cognition's analyze() consumes
//=============================================================================

/// One coherent batch drained from a channel per service cycle. Each
/// variant carries the domain-specific metadata cognition needs to
/// reason about it WITHOUT re-traversing the items.
///
/// Items are held as `Arc<dyn QueueItemBehavior>` per
/// `[[pass-by-reference-lazy-metadata-with-data]]`: lazy-cached derived
/// state (e.g. `ChatQueueItem::embedding()`) lives on each Arc-shared
/// item, so consumers across N personas share compute. Consumers that
/// need domain-specific access downcast individual items via
/// `as_any().downcast_ref::<ChatQueueItem>()`; `&self` is enough to
/// drive the lazy cells, no `Arc<ChatQueueItem>` needed.
///
/// Per `[[cognition-batches-per-channel-adapter]]`: cognition fires
/// `analyze()` ONCE per cycle with a `Vec<CoherentUnit>` (one per
/// channel-with-work), not per item. CBAR catch-up doesn't compound;
/// the slowest persona on the LCD tier sees the CURRENT batched state
/// per cycle, not a backlog of every missed item.
///
/// Per `[[strong-typing-across-boundaries]]`: variants discriminate on
/// `ActivityDomain` at compile-time so cognition's match is exhaustive.
/// When the next channel type lands (Code, future Video) a new variant
/// is added and every consumer's match is forced to extend coverage.
#[derive(Debug, Clone)]
pub enum CoherentUnit {
    /// A burst of chat items from one room within a temporal window.
    /// `primary_room` is the room shared by all items in this burst
    /// (the queue's consolidation enforces same-room grouping).
    Chat {
        items: Vec<std::sync::Arc<dyn QueueItemBehavior>>,
        window_span_ms: u64,
        primary_room: uuid::Uuid,
    },
    /// A voice clip ready for processing. Voice never consolidates
    /// (one item per burst); the variant still exists so cognition
    /// dispatches uniformly across domains.
    Voice {
        items: Vec<std::sync::Arc<dyn QueueItemBehavior>>,
        window_span_ms: u64,
    },
    /// A task batch — related work items consolidated by the
    /// `should_consolidate_with` predicate.
    Task {
        items: Vec<std::sync::Arc<dyn QueueItemBehavior>>,
        window_span_ms: u64,
    },
    /// Background work — periodic checks, low-urgency maintenance.
    Background {
        items: Vec<std::sync::Arc<dyn QueueItemBehavior>>,
        window_span_ms: u64,
    },
}

impl CoherentUnit {
    /// Which `ActivityDomain` this batch came from.
    pub fn domain(&self) -> ActivityDomain {
        match self {
            CoherentUnit::Chat { .. } => ActivityDomain::Chat,
            CoherentUnit::Voice { .. } => ActivityDomain::Audio,
            CoherentUnit::Task { .. } => ActivityDomain::Code,
            CoherentUnit::Background { .. } => ActivityDomain::Background,
        }
    }

    /// Number of items in this batch. Useful for the demand-pull
    /// architecture proof: cognition's `analyze()` is called ONCE
    /// per batch, regardless of `len()`.
    pub fn len(&self) -> usize {
        match self {
            CoherentUnit::Chat { items, .. }
            | CoherentUnit::Voice { items, .. }
            | CoherentUnit::Task { items, .. }
            | CoherentUnit::Background { items, .. } => items.len(),
        }
    }

    /// `true` iff this batch has no items. `drain_batch` returns `None`
    /// for empty queues so this is rarely true in practice, but the
    /// predicate keeps callers' match arms honest.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Temporal span of the burst (largest `item.timestamp - anchor`).
    /// Lifted into a method so per-domain dispatch tables can construct
    /// `CoherentInput::Other` for un-typed-view domains without
    /// re-matching the enum.
    pub fn window_span_ms(&self) -> u64 {
        match self {
            CoherentUnit::Chat { window_span_ms, .. }
            | CoherentUnit::Voice { window_span_ms, .. }
            | CoherentUnit::Task { window_span_ms, .. }
            | CoherentUnit::Background { window_span_ms, .. } => *window_span_ms,
        }
    }
}

/// Result from service_cycle() — what the TS loop should do next
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/ServiceCycleResult.ts"
)]
pub struct ServiceCycleResult {
    /// Should TS process an item?
    pub should_process: bool,
    /// The item to process (serialized). Null if should_process is false.
    #[ts(optional, type = "any")]
    pub item: Option<serde_json::Value>,
    /// Which domain the item came from
    #[ts(optional)]
    pub channel: Option<ActivityDomain>,
    /// How long TS should sleep if no work (adaptive cadence from PersonaState)
    pub wait_ms: u64,
    /// Current channel sizes for monitoring
    pub stats: ChannelRegistryStatus,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_activity_domain_serde() {
        let json = serde_json::to_string(&ActivityDomain::Audio).unwrap();
        assert_eq!(json, "\"AUDIO\"");

        let parsed: ActivityDomain = serde_json::from_str("\"CHAT\"").unwrap();
        assert_eq!(parsed, ActivityDomain::Chat);
    }

    #[test]
    fn test_domain_priority_order() {
        assert_eq!(DOMAIN_PRIORITY_ORDER[0], ActivityDomain::Audio);
        assert_eq!(DOMAIN_PRIORITY_ORDER[1], ActivityDomain::Chat);
        assert_eq!(DOMAIN_PRIORITY_ORDER[2], ActivityDomain::Code);
        assert_eq!(DOMAIN_PRIORITY_ORDER[3], ActivityDomain::Background);
    }
}
