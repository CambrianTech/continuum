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
#[ts(export, export_to = "../../../shared/generated/persona/ActivityDomain.ts")]
pub enum ActivityDomain {
    /// Voice/audio: always urgent, never kicked, no consolidation
    Audio,
    /// Chat messages: per-room consolidation, mention urgency, RTOS aging
    Chat,
    /// Background tasks: dependency-aware, overdue urgency
    Background,
    // Future domains:
    // RealtimeGame,
    // Code,
    // Music,
    // RobotControl,
}

/// All currently registered domains in priority order (highest first).
/// Used by service_cycle() to iterate channels.
pub const DOMAIN_PRIORITY_ORDER: &[ActivityDomain] = &[
    ActivityDomain::Audio,
    ActivityDomain::Chat,
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
pub trait QueueItemBehavior: Send + Sync + Any {
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

    /// Can this item be merged with another item in the same channel?
    /// Items decide their own consolidation rules.
    ///
    /// Default: false (no consolidation).
    /// Chat overrides to consolidate same-room messages.
    /// Task overrides to consolidate related tasks.
    fn should_consolidate_with(&self, _other: &dyn QueueItemBehavior) -> bool {
        false
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
#[ts(export, export_to = "../../../shared/generated/persona/ChannelStatus.ts")]
pub struct ChannelStatus {
    pub domain: ActivityDomain,
    pub size: u32,
    pub has_urgent: bool,
    pub has_work: bool,
}

/// Full channel registry status
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/persona/ChannelRegistryStatus.ts")]
pub struct ChannelRegistryStatus {
    pub channels: Vec<ChannelStatus>,
    pub total_size: u32,
    pub has_urgent_work: bool,
    pub has_work: bool,
}

/// Result from service_cycle() — what the TS loop should do next
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/persona/ServiceCycleResult.ts")]
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
        assert_eq!(DOMAIN_PRIORITY_ORDER[2], ActivityDomain::Background);
    }
}
