//! ChannelRegistry — Routes queue items to per-domain ChannelQueues
//!
//! The registry doesn't know item types — it routes by item.routing_domain().
//! Each ActivityDomain has at most one ChannelQueue.
//!
//! Pattern: HashMap<ActivityDomain, ChannelQueue> with global Notify signal.
//! When any channel receives work, the global signal wakes the service loop.

use super::channel_queue::{ChannelQueue, ChannelQueueConfig};
use super::channel_types::{
    ActivityDomain, ChannelRegistryStatus, QueueItemBehavior, ServiceCycleResult,
    DOMAIN_PRIORITY_ORDER,
};
use super::types::PersonaState;
use std::collections::HashMap;
use tracing::{debug, info};

/// Channel registry — routes items to per-domain queues.
/// Owns all channel queues and provides the service_cycle() entry point.
pub struct ChannelRegistry {
    channels: HashMap<ActivityDomain, ChannelQueue>,
}

impl ChannelRegistry {
    /// Create a new registry with default channels
    pub fn new() -> Self {
        let mut registry = Self {
            channels: HashMap::new(),
        };

        // Register default channels with sizes matching TS implementation
        registry.register(ChannelQueue::new(ChannelQueueConfig {
            domain: ActivityDomain::Audio,
            max_size: 50,
            name: "AUDIO".into(),
        }));
        registry.register(ChannelQueue::new(ChannelQueueConfig {
            domain: ActivityDomain::Chat,
            max_size: 500,
            name: "CHAT".into(),
        }));
        registry.register(ChannelQueue::new(ChannelQueueConfig {
            domain: ActivityDomain::Code,
            max_size: 100,
            name: "CODE".into(),
        }));
        registry.register(ChannelQueue::new(ChannelQueueConfig {
            domain: ActivityDomain::Background,
            max_size: 200,
            name: "BACKGROUND".into(),
        }));

        info!(
            "ChannelRegistry initialized with {} channels: {:?}",
            registry.channels.len(),
            registry.channels.keys().collect::<Vec<_>>()
        );

        registry
    }

    /// Register a channel queue for its domain
    pub fn register(&mut self, queue: ChannelQueue) {
        let domain = queue.domain();
        self.channels.insert(domain, queue);
    }

    /// Route an item to its channel based on item.routing_domain().
    /// Returns Ok(domain) on success, Err if no channel registered.
    pub fn route(&mut self, item: Box<dyn QueueItemBehavior>) -> Result<ActivityDomain, String> {
        let domain = item.routing_domain();
        match self.channels.get_mut(&domain) {
            Some(queue) => {
                debug!(
                    "Routing {} item {} to {} channel",
                    item.item_type(),
                    item.id(),
                    domain_name(domain)
                );
                queue.enqueue(item);
                Ok(domain)
            }
            None => Err(format!("No channel registered for domain {domain:?}")),
        }
    }

    /// Get channel by domain (immutable)
    pub fn get(&self, domain: ActivityDomain) -> Option<&ChannelQueue> {
        self.channels.get(&domain)
    }

    /// Get channel by domain (mutable — for pop/consolidate)
    pub fn get_mut(&mut self, domain: ActivityDomain) -> Option<&mut ChannelQueue> {
        self.channels.get_mut(&domain)
    }

    /// Does ANY channel have urgent work?
    pub fn has_urgent_work(&self) -> bool {
        self.channels.values().any(|c| c.has_urgent_work())
    }

    /// Does ANY channel have work?
    pub fn has_work(&self) -> bool {
        self.channels.values().any(|c| c.has_work())
    }

    /// Total items across all channels
    pub fn total_size(&self) -> usize {
        self.channels.values().map(|c| c.size()).sum()
    }

    /// Consolidate all channels (items decide how)
    pub fn consolidate_all(&mut self) {
        for channel in self.channels.values_mut() {
            channel.consolidate();
        }
    }

    /// Get full status snapshot
    pub fn status(&self) -> ChannelRegistryStatus {
        let channels: Vec<_> = DOMAIN_PRIORITY_ORDER
            .iter()
            .filter_map(|domain| self.channels.get(domain).map(|c| c.status()))
            .collect();

        let total_size: u32 = channels.iter().map(|c| c.size).sum();
        let has_urgent = channels.iter().any(|c| c.has_urgent);
        let has_work = channels.iter().any(|c| c.has_work);

        ChannelRegistryStatus {
            channels,
            total_size,
            has_urgent_work: has_urgent,
            has_work,
        }
    }

    /// Clear all channels
    pub fn clear_all(&mut self) {
        for channel in self.channels.values_mut() {
            channel.clear();
        }
    }

    // =========================================================================
    // SERVICE CYCLE — The main scheduling entry point
    // =========================================================================

    /// Execute one service cycle.
    ///
    /// 1. Consolidate all channels (items decide how)
    /// 2. Update PersonaState (inbox_load, mood)
    /// 3. Check urgent channels first (AUDIO → CHAT → BACKGROUND)
    /// 4. Check non-urgent channels with state gating
    /// 5. Return next item to process, or idle cadence
    ///
    /// This is the Rust equivalent of the TS CNS.serviceChannels() method.
    pub fn service_cycle(&mut self, state: &mut PersonaState) -> ServiceCycleResult {
        // 1. Consolidate all channels
        self.consolidate_all();

        // 2. Update state
        state.inbox_load = self.total_size() as u32;
        state.calculate_mood();

        let stats = self.status();

        // 3. Check urgent channels first (priority order)
        for &domain in DOMAIN_PRIORITY_ORDER {
            if let Some(channel) = self.channels.get(&domain) {
                if channel.has_urgent_work() {
                    if let Some(item) = self.channels.get_mut(&domain).and_then(|c| c.pop()) {
                        debug!(
                            "Service cycle: urgent {} item from {:?} channel",
                            item.item_type(),
                            domain
                        );
                        return ServiceCycleResult {
                            should_process: true,
                            item: Some(item.to_json()),
                            channel: Some(domain),
                            wait_ms: 0,
                            stats,
                        };
                    }
                }
            }
        }

        // 4. Non-urgent: check with state gating (skip Audio — already checked for urgent)
        for &domain in &DOMAIN_PRIORITY_ORDER[1..] {
            if let Some(channel) = self.channels.get(&domain) {
                if channel.has_work() {
                    let peek_priority = channel.peek_priority();
                    if state.should_engage(peek_priority) {
                        if let Some(item) = self.channels.get_mut(&domain).and_then(|c| c.pop()) {
                            debug!(
                                "Service cycle: non-urgent {} item from {:?} channel (priority {:.2})",
                                item.item_type(),
                                domain,
                                peek_priority
                            );
                            return ServiceCycleResult {
                                should_process: true,
                                item: Some(item.to_json()),
                                channel: Some(domain),
                                wait_ms: 0,
                                stats,
                            };
                        }
                    }
                }
            }
        }

        // 5. No work — return adaptive cadence
        ServiceCycleResult {
            should_process: false,
            item: None,
            channel: None,
            wait_ms: state.service_cadence_ms(),
            stats,
        }
    }
}

impl Default for ChannelRegistry {
    fn default() -> Self {
        Self::new()
    }
}

fn domain_name(domain: ActivityDomain) -> &'static str {
    match domain {
        ActivityDomain::Audio => "AUDIO",
        ActivityDomain::Chat => "CHAT",
        ActivityDomain::Code => "CODE",
        ActivityDomain::Background => "BACKGROUND",
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

    fn now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
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
    fn test_registry_default_channels() {
        let registry = ChannelRegistry::new();
        assert!(registry.get(ActivityDomain::Audio).is_some());
        assert!(registry.get(ActivityDomain::Chat).is_some());
        assert!(registry.get(ActivityDomain::Code).is_some());
        assert!(registry.get(ActivityDomain::Background).is_some());
    }

    #[test]
    fn test_route_to_correct_channel() {
        let mut registry = ChannelRegistry::new();
        let room = Uuid::new_v4();

        let domain = registry.route(boxed_chat(room, false, 0.5)).unwrap();
        assert_eq!(domain, ActivityDomain::Chat);
        assert_eq!(registry.get(ActivityDomain::Chat).unwrap().size(), 1);
        assert_eq!(registry.get(ActivityDomain::Audio).unwrap().size(), 0);

        let domain = registry.route(boxed_voice()).unwrap();
        assert_eq!(domain, ActivityDomain::Audio);
        assert_eq!(registry.get(ActivityDomain::Audio).unwrap().size(), 1);
    }

    #[test]
    fn test_total_size() {
        let mut registry = ChannelRegistry::new();
        let room = Uuid::new_v4();

        registry.route(boxed_chat(room, false, 0.5)).unwrap();
        registry.route(boxed_chat(room, false, 0.7)).unwrap();
        registry.route(boxed_voice()).unwrap();

        assert_eq!(registry.total_size(), 3);
    }

    #[test]
    fn test_has_urgent_work() {
        let mut registry = ChannelRegistry::new();
        let room = Uuid::new_v4();

        assert!(!registry.has_urgent_work());

        registry.route(boxed_chat(room, false, 0.5)).unwrap();
        assert!(!registry.has_urgent_work()); // No mentions

        registry.route(boxed_voice()).unwrap();
        assert!(registry.has_urgent_work()); // Voice is always urgent
    }

    #[test]
    fn test_status_snapshot() {
        let mut registry = ChannelRegistry::new();
        let room = Uuid::new_v4();

        registry.route(boxed_chat(room, false, 0.5)).unwrap();
        registry.route(boxed_voice()).unwrap();

        let status = registry.status();
        assert_eq!(status.total_size, 2);
        assert!(status.has_urgent_work);
        assert!(status.has_work);
        assert_eq!(status.channels.len(), 4); // All domains reported
    }

    #[test]
    fn test_service_cycle_urgent_first() {
        let mut registry = ChannelRegistry::new();
        let mut state = PersonaState::new();
        let room = Uuid::new_v4();

        // Add chat first (non-urgent)
        registry.route(boxed_chat(room, false, 0.5)).unwrap();
        // Add voice (urgent)
        registry.route(boxed_voice()).unwrap();

        // Service cycle should return voice first (urgent)
        let result = registry.service_cycle(&mut state);
        assert!(result.should_process);
        assert_eq!(result.channel, Some(ActivityDomain::Audio));

        // Next cycle returns chat
        let result = registry.service_cycle(&mut state);
        assert!(result.should_process);
        assert_eq!(result.channel, Some(ActivityDomain::Chat));

        // Empty — idle
        let result = registry.service_cycle(&mut state);
        assert!(!result.should_process);
        assert!(result.wait_ms > 0);
    }

    #[test]
    fn test_service_cycle_state_gating() {
        let mut registry = ChannelRegistry::new();
        let mut state = PersonaState::new();
        let room = Uuid::new_v4();

        // Low priority chat
        registry.route(boxed_chat(room, false, 0.3)).unwrap();

        // Active mood — should engage with everything
        let result = registry.service_cycle(&mut state);
        assert!(result.should_process);

        // Force overwhelmed: compute_budget < 0.2 triggers Overwhelmed in calculate_mood()
        // (can't just set mood directly since service_cycle calls calculate_mood)
        state.compute_budget = 0.1;
        registry.route(boxed_chat(room, false, 0.3)).unwrap();

        let result = registry.service_cycle(&mut state);
        // Overwhelmed skips low priority (0.3 < 0.8)
        assert!(!result.should_process);
    }

    #[test]
    fn test_service_cycle_consolidates() {
        let mut registry = ChannelRegistry::new();
        let mut state = PersonaState::new();
        let room = Uuid::new_v4();

        // 3 messages from same room
        registry.route(boxed_chat(room, false, 0.5)).unwrap();
        registry.route(boxed_chat(room, false, 0.7)).unwrap();
        registry.route(boxed_chat(room, false, 0.3)).unwrap();

        assert_eq!(registry.total_size(), 3);

        // Service cycle consolidates before processing
        let result = registry.service_cycle(&mut state);
        assert!(result.should_process);

        // After consolidation + pop, should have fewer items
        assert!(registry.total_size() < 3);
    }

    #[test]
    fn test_clear_all() {
        let mut registry = ChannelRegistry::new();
        let room = Uuid::new_v4();

        registry.route(boxed_chat(room, false, 0.5)).unwrap();
        registry.route(boxed_voice()).unwrap();

        assert_eq!(registry.total_size(), 2);

        registry.clear_all();
        assert_eq!(registry.total_size(), 0);
    }
}
