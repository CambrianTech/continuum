//! ChannelRegistry — Routes queue items to per-domain ChannelQueues
//!
//! The registry doesn't know item types — it routes by item.routing_domain().
//! Each ActivityDomain has at most one ChannelQueue.
//!
//! Pattern: HashMap<ActivityDomain, ChannelQueue> with global Notify signal.
//! When any channel receives work, the global signal wakes the service loop.

use super::channel_queue::{ChannelQueue, ChannelQueueConfig};
use super::channel_types::{
    ActivityDomain, ChannelRegistryStatus, CoherentUnit, QueueItemBehavior, ServiceCycleResult,
    DOMAIN_PRIORITY_ORDER,
};
use super::channel_view::{ChatChannelView, CoherentInput, PersonaChannelView};
use super::persona_identity::PersonaIdentity;
use super::types::PersonaState;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info};
use uuid::Uuid;

/// Default burst window for [`ChannelRegistry::service_cycle_batched`].
///
/// **Window is BIDIRECTIONAL** — drain_batch pulls items within
/// `[anchor_ts - 5_000, anchor_ts + 5_000]` ms, i.e. a 10-second total
/// span centered on the highest-priority item's timestamp. Future
/// arrivals (timestamps after the anchor) AND prior arrivals within
/// the same window all collapse into the same burst.
///
/// Why bidirectional: anchor-defined ranges should be symmetric so the
/// burst boundary doesn't depend on which item happened to win the
/// priority tiebreak. The 5s value targets typical conversational
/// latency — bursts arriving within ~5s of an anchor get collapsed into
/// a single coherent input for cognition's `analyze()`. Wider windows
/// aggregate more (one fewer analyze per tick) but reduce reactivity.
///
/// PR D (audio) is where this gets mood-tuned via `PersonaState`. PR A
/// stays with a const so the seam is testable without state plumbing.
///
/// Per Reviewer 1 C6: the doc previously said "5 seconds" without
/// disambiguating one-sided vs bidirectional; this docstring resolves
/// that as bidirectional matching the `drain_batch` implementation in
/// `channel_queue.rs`.
pub const DEFAULT_BURST_WINDOW_MS: u64 = 5_000;

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
    ///
    /// Items are Arc-shared per `[[pass-by-reference-lazy-metadata-with-data]]`
    /// so multiple consumers (cognition + observers + future per-persona
    /// channel views) can hold references to the same item; lazy-cached
    /// derived state on the item is shared across all consumers.
    pub fn route(&mut self, item: Arc<dyn QueueItemBehavior>) -> Result<ActivityDomain, String> {
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

    /// Get full status snapshot.
    ///
    /// Single-pass aggregation: builds the per-channel status Vec AND the
    /// rollup fields (total_size / has_urgent_work / has_work) in one
    /// walk over DOMAIN_PRIORITY_ORDER. Previously did 1 walk to build
    /// the Vec then 3 more walks to sum/any/any over the result, plus
    /// Vec growth from an unsized `.collect()`. service_cycle() calls
    /// this every tick (per persona, every 3-10s); the per-tick savings
    /// compound across the active persona fleet.
    pub fn status(&self) -> ChannelRegistryStatus {
        let mut channels = Vec::with_capacity(DOMAIN_PRIORITY_ORDER.len());
        let mut total_size: u32 = 0;
        let mut has_urgent_work = false;
        let mut has_work = false;
        for &domain in DOMAIN_PRIORITY_ORDER {
            if let Some(channel) = self.channels.get(&domain) {
                let s = channel.status();
                total_size += s.size;
                has_urgent_work |= s.has_urgent;
                has_work |= s.has_work;
                channels.push(s);
            }
        }
        ChannelRegistryStatus {
            channels,
            total_size,
            has_urgent_work,
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

        // 3. Check urgent channels first (priority order). Single get_mut
        //    per domain — the previous pattern did get() to check
        //    has_urgent_work() then get_mut() to pop, doubling the
        //    HashMap probes per tick. NLL handles the borrow reuse
        //    cleanly without the double-lookup workaround.
        for &domain in DOMAIN_PRIORITY_ORDER {
            if let Some(channel) = self.channels.get_mut(&domain) {
                if channel.has_urgent_work() {
                    if let Some(item) = channel.pop() {
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

        // 4. Non-urgent: check with state gating (skip Audio — already
        //    checked for urgent). Same single-lookup pattern as the
        //    urgent loop above.
        for &domain in &DOMAIN_PRIORITY_ORDER[1..] {
            if let Some(channel) = self.channels.get_mut(&domain) {
                if channel.has_work() {
                    let peek_priority = channel.peek_priority();
                    if state.should_engage(peek_priority) {
                        if let Some(item) = channel.pop() {
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

    /// Demand-pull service cycle — the batched cognition entry point.
    ///
    /// One service tick = at most one [`CoherentInput`] per channel-with-work,
    /// independent of how many items each channel drained. The caller passes
    /// the Vec to cognition's `analyze()` ONCE, witnessing
    /// `[[cognition-batches-per-channel-adapter]]`:
    ///
    /// > N inbox arrivals on one channel → 1 analyze, not N.
    /// > Cycle wall-clock bounded by inference + ε, regardless of arrival rate.
    ///
    /// Returns `Vec<CoherentInput>` (possibly empty) and updates `state.inbox_load`
    /// + mood as a side effect — same state-tracking semantics as the
    /// pop-style [`Self::service_cycle`].
    ///
    /// `persona_id` + `persona_name` thread through to each
    /// [`PersonaChannelView::interpret`] call so the cheap per-persona
    /// perspective (mention detection, identity-aware ranking) reads the
    /// SHARED lazy cells on each item per
    /// `[[shared-decode-per-persona-perspective]]` — the embedding
    /// `Arc<Vec<f32>>` is the same value across every persona viewing the
    /// same burst.
    ///
    /// Existing [`Self::service_cycle`] stays for the legacy single-pop
    /// surface (`modules/channel::channel/service-cycle{,-full}` + a
    /// handful of architecture-proof tests). Persona production has
    /// cut over to this batched path via
    /// `service_module::service_burst_for` (task #249).
    pub fn service_cycle_batched(
        &mut self,
        state: &mut PersonaState,
        identity: &PersonaIdentity,
        window_ms: u64,
    ) -> Vec<CoherentInput> {
        // 1. Consolidate (items decide how — same as single-pop path)
        self.consolidate_all();

        // 2. Track load (same state-update contract as the single-pop path
        //    so consumers swapping methods don't observe a mood delta)
        state.inbox_load = self.total_size() as u32;
        state.calculate_mood();

        let mut inputs: Vec<CoherentInput> = Vec::new();

        // 3. ONE drain + interpret per domain per tick — independent of
        //    how many items each channel carried. This is the
        //    load-bearing wall-clock-bounded property.
        for &domain in DOMAIN_PRIORITY_ORDER {
            let Some(channel) = self.channels.get_mut(&domain) else {
                continue;
            };
            if !channel.has_work() {
                continue;
            }
            let Some(unit) = channel.drain_batch(window_ms) else {
                continue;
            };
            let input = Self::interpret_for_domain(&unit, identity);
            inputs.push(input);
        }

        inputs
    }

    /// Per-domain view dispatch for `service_cycle_batched`.
    ///
    /// PR A ships `ChatChannelView` for the Chat domain. Non-Chat
    /// domains have no typed view yet — they drain into a
    /// `CoherentInput::Other` constructed DIRECTLY here, not by
    /// routing through `ChatChannelView` (which would be a silent
    /// fallthrough that hides dispatch bugs per `[[no-fallbacks-ever]]`).
    ///
    /// Adding a typed view for a new domain = replace its arm with the
    /// new view's call. The single match here is the dispatch table;
    /// `ChatChannelView::interpret` panics if called on a non-Chat unit
    /// (programmer-error guard) so a future migration can't silently
    /// regress.
    fn interpret_for_domain(unit: &CoherentUnit, identity: &PersonaIdentity) -> CoherentInput {
        match unit.domain() {
            ActivityDomain::Chat => ChatChannelView.interpret(unit, identity),
            domain
            @ (ActivityDomain::Audio | ActivityDomain::Code | ActivityDomain::Background) => {
                CoherentInput::Other {
                    domain,
                    item_count: unit.len(),
                    window_span_ms: unit.window_span_ms(),
                }
            }
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
    use super::super::channel_items::*;
    use super::super::types::SenderType;
    use super::*;
    use uuid::Uuid;

    fn now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
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

        let domain = registry.route(arc_chat(room, false, 0.5)).unwrap();
        assert_eq!(domain, ActivityDomain::Chat);
        assert_eq!(registry.get(ActivityDomain::Chat).unwrap().size(), 1);
        assert_eq!(registry.get(ActivityDomain::Audio).unwrap().size(), 0);

        let domain = registry.route(arc_voice()).unwrap();
        assert_eq!(domain, ActivityDomain::Audio);
        assert_eq!(registry.get(ActivityDomain::Audio).unwrap().size(), 1);
    }

    #[test]
    fn test_total_size() {
        let mut registry = ChannelRegistry::new();
        let room = Uuid::new_v4();

        registry.route(arc_chat(room, false, 0.5)).unwrap();
        registry.route(arc_chat(room, false, 0.7)).unwrap();
        registry.route(arc_voice()).unwrap();

        assert_eq!(registry.total_size(), 3);
    }

    #[test]
    fn test_has_urgent_work() {
        let mut registry = ChannelRegistry::new();
        let room = Uuid::new_v4();

        assert!(!registry.has_urgent_work());

        registry.route(arc_chat(room, false, 0.5)).unwrap();
        assert!(!registry.has_urgent_work()); // No mentions

        registry.route(arc_voice()).unwrap();
        assert!(registry.has_urgent_work()); // Voice is always urgent
    }

    #[test]
    fn test_status_snapshot() {
        let mut registry = ChannelRegistry::new();
        let room = Uuid::new_v4();

        registry.route(arc_chat(room, false, 0.5)).unwrap();
        registry.route(arc_voice()).unwrap();

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
        registry.route(arc_chat(room, false, 0.5)).unwrap();
        // Add voice (urgent)
        registry.route(arc_voice()).unwrap();

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
        registry.route(arc_chat(room, false, 0.3)).unwrap();

        // Active mood — should engage with everything
        let result = registry.service_cycle(&mut state);
        assert!(result.should_process);

        // Force overwhelmed: compute_budget < 0.2 triggers Overwhelmed in calculate_mood()
        // (can't just set mood directly since service_cycle calls calculate_mood)
        state.compute_budget = 0.1;
        registry.route(arc_chat(room, false, 0.3)).unwrap();

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
        registry.route(arc_chat(room, false, 0.5)).unwrap();
        registry.route(arc_chat(room, false, 0.7)).unwrap();
        registry.route(arc_chat(room, false, 0.3)).unwrap();

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

        registry.route(arc_chat(room, false, 0.5)).unwrap();
        registry.route(arc_voice()).unwrap();

        assert_eq!(registry.total_size(), 2);

        registry.clear_all();
        assert_eq!(registry.total_size(), 0);
    }

    //=========================================================================
    // service_cycle_batched — Delta 5 of task #244
    //
    // These tests pin the demand-pull doctrine concretely. They are the
    // foundation Delta 6's architecture proof builds on; if any of these
    // fail the doctrine claim itself is wrong, not the test.
    //=========================================================================

    use super::super::channel_view::CoherentInput;

    // Note: the N-arrivals-→-1-input doctrine pin lives in
    // `tests/architecture_demand_pull_cognition.rs::service_cycle_with_n_chat_messages_yields_one_input`
    // at N=500. Per CLAUDE.md "Tests must justify themselves" — the
    // unit-level duplicate at N=50 was removed to keep the test surface
    // honest (Reviewer 3 C3 / [[every-error-is-an-opportunity-to-battle-harden]]).
    //
    // Unit tests here cover the structural invariants the architecture
    // test doesn't pin directly: multi-channel ordering, empty-vec
    // honesty, state-side-effect contract, identity-aware perspective,
    // and the equivalence with the legacy service_cycle pop-path.

    /// proves: multi-channel work → one CoherentInput per channel-with-
    /// work, ordered by `DOMAIN_PRIORITY_ORDER` (Audio first). Demand-pull
    /// is per-channel, not per-tick-overall. A persona with audio + chat
    /// work gets two inputs in one tick — cognition's downstream analyze
    /// sees both at once instead of two sequential per-channel ticks,
    /// AND the urgency-first ordering means voice is at index 0 so the
    /// downstream caller can short-circuit on it without searching.
    #[test]
    fn batched_produces_one_input_per_channel_with_work_audio_first() {
        let mut registry = ChannelRegistry::new();
        let mut state = PersonaState::new();
        let room = Uuid::new_v4();
        let persona_id = Uuid::new_v4();

        // Enqueue chat FIRST, voice second. The doctrine claim is that
        // ordering of the returned Vec reflects DOMAIN_PRIORITY_ORDER
        // (Audio before Chat), NOT enqueue order — voice must still
        // be first in the Vec.
        registry.route(arc_chat(room, false, 0.5)).unwrap();
        registry.route(arc_chat(room, false, 0.6)).unwrap();
        registry.route(arc_voice()).unwrap();

        let inputs = registry.service_cycle_batched(
            &mut state,
            &PersonaIdentity::new(persona_id, "Helper"),
            DEFAULT_BURST_WINDOW_MS,
        );

        // Two channels had work (Audio + Chat) → two inputs.
        assert_eq!(inputs.len(), 2, "expected one input per channel-with-work");
        // Audio is FIRST per DOMAIN_PRIORITY_ORDER — pinned because
        // cognition's downstream urgency handling depends on it.
        assert_eq!(
            inputs[0].domain(),
            ActivityDomain::Audio,
            "Audio must be at index 0 per DOMAIN_PRIORITY_ORDER — cognition's \
             urgency short-circuit relies on this ordering"
        );
        assert_eq!(inputs[1].domain(), ActivityDomain::Chat);
    }

    /// proves: no work → empty Vec, not a sentinel. Per `[[no-fallbacks-
    /// ever]]`: empty queues return empty, not a phantom "Idle" burst
    /// that cognition would have to filter out.
    #[test]
    fn batched_empty_registry_returns_empty_vec() {
        let mut registry = ChannelRegistry::new();
        let mut state = PersonaState::new();
        let persona_id = Uuid::new_v4();

        let inputs = registry.service_cycle_batched(
            &mut state,
            &PersonaIdentity::new(persona_id, "Helper"),
            DEFAULT_BURST_WINDOW_MS,
        );

        assert!(
            inputs.is_empty(),
            "empty registry must return empty Vec, not a sentinel"
        );
    }

    /// proves: state.inbox_load + mood update side-effects survive the
    /// batched path with the SAME values the legacy `service_cycle`
    /// produces. Consumers swapping from `service_cycle` to
    /// `service_cycle_batched` must not see a mood or load delta — the
    /// state model is shared across both entry points.
    ///
    /// Strengthened per Reviewer 3 C5: exact-value assertions (was
    /// previously `inbox_load > 0` which any positive-write impl would
    /// pass) plus an explicit Mood::Overwhelmed pin so the mood
    /// transition is part of the contract.
    #[test]
    fn batched_updates_state_load_and_mood_with_exact_values() {
        let mut registry = ChannelRegistry::new();
        let mut state = PersonaState::new();
        let room = Uuid::new_v4();
        let persona_id = Uuid::new_v4();

        // 25 chat items with mentions=true → consolidation may merge
        // them since same room. The state.inbox_load is set BEFORE
        // drain to total_size(), AFTER consolidate_all() has run.
        // Consolidation collapses same-room messages into one anchor
        // (with consolidated_context Vec), so post-consolidation
        // total_size = 1 anchor. Pin that exactly.
        for _ in 0..25 {
            registry.route(arc_chat(room, false, 0.5)).unwrap();
        }

        // Sanity: initial state
        assert_eq!(state.inbox_load, 0);
        assert!(matches!(
            state.mood,
            crate::persona::types::Mood::Active | crate::persona::types::Mood::Idle
        ));

        let _inputs = registry.service_cycle_batched(
            &mut state,
            &PersonaIdentity::new(persona_id, "Helper"),
            DEFAULT_BURST_WINDOW_MS,
        );

        // After consolidation collapses 25 same-room items into 1
        // anchor, total_size == 1 — that's what gets written to
        // inbox_load (BEFORE the drain consumes it).
        assert_eq!(
            state.inbox_load, 1,
            "inbox_load must reflect post-consolidation total_size (25 items \
             → 1 same-room anchor → load=1), NOT pre-consolidation count or 0"
        );
        // Mood is calculated from inbox_load via `state.calculate_mood()`
        // (inbox_load > 20 → Overwhelmed; here load=1 so Active).
        assert_eq!(
            state.mood,
            crate::persona::types::Mood::Active,
            "mood must be Active after load=1 (the calculate_mood transition)"
        );
    }

    /// proves: `service_cycle` (legacy pop) and `service_cycle_batched`
    /// produce IDENTICAL state side-effects (inbox_load + mood) for the
    /// same inputs.
    ///
    /// This is the "consumers swapping should observe no delta" claim
    /// the prior `batched_updates_state_load_and_mood` test made in
    /// prose but never pinned. Per Reviewer 3 C7: persona production
    /// has migrated to the batched seam via
    /// `service_module::service_burst_for` (task #249), but the
    /// legacy single-pop `service_cycle` still ships for other
    /// callers; silent drift between them is the most likely
    /// regression. This test pins parity.
    #[test]
    fn service_cycle_and_batched_produce_identical_state_side_effects() {
        let room = Uuid::new_v4();

        let mut registry_legacy = ChannelRegistry::new();
        let mut registry_batched = ChannelRegistry::new();
        let mut state_legacy = PersonaState::new();
        let mut state_batched = PersonaState::new();

        // Same input set in both registries.
        for _ in 0..7 {
            registry_legacy.route(arc_chat(room, false, 0.5)).unwrap();
            registry_batched.route(arc_chat(room, false, 0.5)).unwrap();
        }

        let _ = registry_legacy.service_cycle(&mut state_legacy);
        let _ = registry_batched.service_cycle_batched(
            &mut state_batched,
            &PersonaIdentity::new(Uuid::new_v4(), "Helper"),
            DEFAULT_BURST_WINDOW_MS,
        );

        assert_eq!(
            state_legacy.inbox_load, state_batched.inbox_load,
            "inbox_load drift between service_cycle ({}) and service_cycle_batched ({})",
            state_legacy.inbox_load, state_batched.inbox_load,
        );
        assert_eq!(
            state_legacy.mood, state_batched.mood,
            "mood drift between service_cycle ({:?}) and service_cycle_batched ({:?})",
            state_legacy.mood, state_batched.mood,
        );
    }

    /// proves: identity-aware perspective extends to the batched path
    /// — same registry, two personas, different `anyone_mentioned_persona`
    /// flags on the same underlying burst.
    #[test]
    fn batched_perspective_is_identity_aware() {
        let mut registry = ChannelRegistry::new();
        let mut state_maya = PersonaState::new();
        let mut state_helper = PersonaState::new();
        let room = Uuid::new_v4();

        // Send a message mentioning Maya by name
        let mention = Arc::new(ChatQueueItem {
            id: Uuid::new_v4(),
            room_id: room,
            content: "hey Maya, can you take a look?".into(),
            sender_id: Uuid::new_v4(),
            sender_name: "Operator".into(),
            sender_type: SenderType::Human,
            mentions: false,
            timestamp: now_ms(),
            enqueued_at: now_ms(),
            priority: 0.5,
            consolidated_context: Vec::new(),
            media: Vec::new(),
            embedding_cell: std::sync::OnceLock::new(),
            #[cfg(any(test, feature = "test-fixtures"))]
            compute_calls: std::sync::atomic::AtomicUsize::new(0),
        });
        registry
            .route(mention.clone() as Arc<dyn QueueItemBehavior>)
            .unwrap();

        // Maya's perspective — she should see herself mentioned
        let inputs_maya = registry.service_cycle_batched(
            &mut state_maya,
            &PersonaIdentity::new(Uuid::new_v4(), "Maya"),
            DEFAULT_BURST_WINDOW_MS,
        );
        let maya_mentioned = inputs_maya
            .iter()
            .find_map(|i| match i {
                CoherentInput::Chat(c) => Some(c.anyone_mentioned_persona),
                _ => None,
            })
            .expect("Maya should have received a chat input");
        assert!(
            maya_mentioned,
            "Maya should see herself mentioned in 'hey Maya'"
        );

        // Re-route the same item for a fresh tick (drain consumed it)
        registry
            .route(mention as Arc<dyn QueueItemBehavior>)
            .unwrap();

        // Helper's perspective — he should NOT see himself mentioned
        let inputs_helper = registry.service_cycle_batched(
            &mut state_helper,
            &PersonaIdentity::new(Uuid::new_v4(), "Helper"),
            DEFAULT_BURST_WINDOW_MS,
        );
        let helper_mentioned = inputs_helper
            .iter()
            .find_map(|i| match i {
                CoherentInput::Chat(c) => Some(c.anyone_mentioned_persona),
                _ => None,
            })
            .expect("Helper should have received a chat input");
        assert!(
            !helper_mentioned,
            "Helper should NOT see himself mentioned — Maya was named, not Helper"
        );
    }
}
