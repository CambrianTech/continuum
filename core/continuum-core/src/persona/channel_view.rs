//! `PersonaChannelView` — per-persona perspective on a shared `CoherentUnit`.
//!
//! Per `[[shared-decode-per-persona-perspective]]`: the substrate-shared
//! expensive decode (embedding computation, future STT, video frame
//! decode) lives on the channel item as `OnceLock<Arc<T>>` cells per
//! `[[pass-by-reference-lazy-metadata-with-data]]`. THIS layer is the
//! cheap per-persona perspective above it — identity-aware filtering,
//! ranking, mention detection, perspective summarization.
//!
//! The cost model the substrate earns:
//!
//! - shared decode (item-level): runs ONCE per item, amortized across N
//!   personas — the lazy cell on each `Arc<dyn QueueItemBehavior>` is
//!   populated by whichever persona demands the embedding first; every
//!   subsequent persona gets the cached `Arc<Vec<f32>>` clone
//! - per-persona perspective (this layer): runs N times per cycle but
//!   each pass is cheap — string scans, sender filters, identity-aware
//!   attention. NO inference calls, NO heavy compute.
//!
//! Combined: N personas in a room sharing M arrivals costs
//! `M × decode_cost + N × M × cheap_perspective_cost` per cycle, NOT
//! `N × M × decode_cost`. The flow-geometric proof's persona-day-one
//! delivery rests on this split.

use crate::persona::channel_items::ChatQueueItem;
use crate::persona::channel_types::{ActivityDomain, CoherentUnit};
use crate::persona::persona_identity::PersonaIdentity;
use std::sync::Arc;
use uuid::Uuid;

//=============================================================================
// COHERENT INPUT — what cognition's analyze() consumes per channel-tick
//=============================================================================

/// Per-channel typed input cognition's `analyze()` receives.
///
/// One `CoherentInput` per channel-with-work per service cycle, per
/// `[[cognition-batches-per-channel-adapter]]`. Production callers go
/// through `evaluator::analyze_burst` (task #248) which takes a single
/// `CoherentInput` and fires the gate ONCE per burst; the service-loop
/// integration that drives this from
/// `service_module::service_burst_for` is task #249.
///
/// Per `[[strong-typing-across-boundaries]]`: each variant carries the
/// per-domain shape cognition needs. Adding a new channel adds a new
/// variant; every consumer's match is forced to extend coverage.
#[derive(Debug, Clone)]
pub enum CoherentInput {
    /// Aggregated chat burst — multiple messages from one room
    /// collapsed into a single coherent input for cognition.
    Chat(ChatCoherentInput),

    /// Catch-all for domains whose per-persona interpret layer hasn't
    /// landed yet (Voice/Task/Background). PR A ships ChatChannelView;
    /// subsequent PRs add domain-specific views and replace this
    /// branch with typed variants per `[[strong-typing-across-boundaries]]`.
    Other {
        domain: ActivityDomain,
        item_count: usize,
        window_span_ms: u64,
    },
}

impl CoherentInput {
    pub fn domain(&self) -> ActivityDomain {
        match self {
            CoherentInput::Chat(_) => ActivityDomain::Chat,
            CoherentInput::Other { domain, .. } => *domain,
        }
    }
}

/// Chat-specific aggregated input. Fields the cognition layer needs
/// to reason about a burst of chat messages WITHOUT re-traversing the
/// individual items.
#[derive(Debug, Clone)]
pub struct ChatCoherentInput {
    /// The room all items in this burst came from. Consolidation
    /// enforces single-room grouping per the existing
    /// `ChatQueueItem::should_consolidate_with` semantics.
    pub primary_room: Uuid,

    /// How many raw messages collapsed into this burst.
    pub burst_message_count: usize,

    /// Temporal span of the burst (largest item.timestamp - anchor).
    pub window_span_ms: u64,

    /// Concatenated "Sender: message" lines across the burst. Newest
    /// last; cognition's prompt assembly uses this as a recent-room-
    /// transcript chunk.
    pub aggregated_content: String,

    /// Name of the most recent sender in the burst (the one who
    /// effectively triggered the persona's attention). Empty when the
    /// burst has no chat items (shouldn't happen if drain_batch is
    /// honest, but kept defensive).
    pub last_sender_name: String,

    /// `true` iff any item in the burst named THIS persona (case-
    /// insensitive substring match on the persona's name). The
    /// identity-aware perspective: same burst can be "mentions
    /// Maya" from Maya's view AND "mentions nobody" from Helper's
    /// view, even though the underlying items are identical.
    pub anyone_mentioned_persona: bool,

    /// Shared embedding of the burst's anchor item. SAME `Arc<Vec<f32>>`
    /// across all personas viewing the same burst — proves the
    /// `[[shared-decode-per-persona-perspective]]` doctrine concretely:
    /// the expensive compute happens ONCE on the item; every persona's
    /// `ChatChannelView::interpret` reads the same cached cell.
    pub burst_embedding: Arc<Vec<f32>>,
}

//=============================================================================
// PERSONA CHANNEL VIEW — the per-persona interpret trait
//=============================================================================

/// Per-channel, per-persona perspective layer on a `CoherentUnit`.
///
/// One impl per channel type. The trait stays narrow on purpose: the
/// substrate-shared expensive decode lives on the channel item (lazy
/// cells); this layer is the CHEAP per-persona transform above it.
///
/// `identity: &PersonaIdentity` is the identity-aware seam — chat's
/// mention detection routes through `identity.mentions(text)` (word-
/// boundary, not substring; see [`PersonaIdentity::mentions`]
/// docstring). Future video views can attribute gaze/attention by
/// identity, etc. Threaded explicitly so the substrate's per-persona
/// perspective is a pure-function dependency, not a runtime side-
/// channel.
///
/// Per `[[strong-typing-across-boundaries]]` (task #247): the identity
/// is a TYPE, not a name string + id pair, so callers can't
/// accidentally re-introduce the substring-match bug class by swapping
/// out the helper.
pub trait PersonaChannelView: Send + Sync {
    fn interpret(&self, unit: &CoherentUnit, identity: &PersonaIdentity) -> CoherentInput;
}

//=============================================================================
// CHAT CHANNEL VIEW — the first instantiation
//=============================================================================

/// Per-persona view of a `CoherentUnit::Chat` burst. Cheap: walks the
/// items, downcasts to `ChatQueueItem`, aggregates into a typed
/// `ChatCoherentInput`. Calls `item.embedding()` to read the shared
/// lazy cell on the burst's anchor — the SAME Arc every persona sees.
///
/// Identity-aware: `persona_name` controls the `anyone_mentioned_persona`
/// flag. Two personas viewing the SAME burst can get different
/// CoherentInput values for that field — the per-persona perspective
/// in action.
///
/// ## Trait contract: only handles `CoherentUnit::Chat`
///
/// Per `[[no-fallbacks-ever]]`: this view is responsible ONLY for the
/// Chat domain. The dispatch layer (`ChannelRegistry::interpret_for_domain`)
/// MUST route Voice/Task/Background units to a different view (or to
/// the typed `CoherentInput::Other` construction inline). Calling
/// `ChatChannelView::interpret` on a non-Chat unit is a programmer
/// error and panics — silent fallthrough to `Other` was the original
/// shape but adversarial review flagged it as a fallback that hides
/// dispatch bugs.
pub struct ChatChannelView;

impl PersonaChannelView for ChatChannelView {
    fn interpret(&self, unit: &CoherentUnit, identity: &PersonaIdentity) -> CoherentInput {
        match unit {
            CoherentUnit::Chat {
                items,
                window_span_ms,
                primary_room,
            } => {
                let mut aggregated = String::new();
                let mut last_sender = String::new();
                let mut anyone_mentioned = false;
                let mut burst_embedding: Option<Arc<Vec<f32>>> = None;
                // Sum across consolidated anchors — an item carrying
                // `consolidated_context: Vec<ConsolidatedContext>` represents
                // itself PLUS all the prior messages it absorbed. The doctrine's
                // load-bearing count is the underlying message count, not the
                // post-consolidation Vec length.
                let mut total_messages: usize = 0;

                for item in items {
                    let Some(chat) = item.as_any().downcast_ref::<ChatQueueItem>() else {
                        continue;
                    };

                    // Prior consolidated messages first, then the anchor itself —
                    // matches the order ChatQueueItem.consolidate_with_items
                    // produces the absorption in.
                    for prior in &chat.consolidated_context {
                        if !aggregated.is_empty() {
                            aggregated.push('\n');
                        }
                        aggregated.push_str(&prior.sender_name);
                        aggregated.push_str(": ");
                        aggregated.push_str(&prior.content);
                        if identity.mentions(&prior.content) {
                            anyone_mentioned = true;
                        }
                    }
                    total_messages += chat.consolidated_context.len();

                    if !aggregated.is_empty() {
                        aggregated.push('\n');
                    }
                    aggregated.push_str(&chat.sender_name);
                    aggregated.push_str(": ");
                    aggregated.push_str(&chat.content);
                    total_messages += 1;

                    last_sender = chat.sender_name.clone();

                    if identity.mentions(&chat.content) {
                        anyone_mentioned = true;
                    }

                    // Read the shared embedding lazy cell on the FIRST
                    // item — same Arc every persona sees. Read-once
                    // keeps the per-persona perspective cheap.
                    if burst_embedding.is_none() {
                        burst_embedding = Some(chat.embedding());
                    }
                }

                CoherentInput::Chat(ChatCoherentInput {
                    primary_room: *primary_room,
                    burst_message_count: total_messages,
                    window_span_ms: *window_span_ms,
                    aggregated_content: aggregated,
                    last_sender_name: last_sender,
                    anyone_mentioned_persona: anyone_mentioned,
                    // Empty Arc<Vec<f32>> sentinel only when the burst
                    // had no chat items (defensive — drain_batch should
                    // never produce such a burst for ActivityDomain::Chat).
                    burst_embedding: burst_embedding.unwrap_or_else(|| Arc::new(Vec::new())),
                })
            }
            // Programmer-error guard, NOT a fallback. The dispatch
            // contract (channel_registry::interpret_for_domain) MUST
            // route non-Chat units away from this view. Reaching this
            // branch means the registry's match is broken — silent
            // construction of `CoherentInput::Other` here would mask
            // the dispatch bug.
            other => unreachable!(
                "ChatChannelView::interpret called on non-Chat unit ({:?}). \
                 This is a registry dispatch bug — check \
                 ChannelRegistry::interpret_for_domain. \
                 Per [[no-fallbacks-ever]], the Other variant must be \
                 constructed directly by the dispatcher, not via this \
                 view falling through.",
                other.domain()
            ),
        }
    }
}

//=============================================================================
// TESTS
//=============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persona::channel_items::ChatQueueItem;
    use crate::persona::types::SenderType;
    use std::sync::OnceLock;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn now_ms() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }

    fn make_chat_arc(
        content: &str,
        sender: &str,
        room: Uuid,
    ) -> Arc<dyn crate::persona::channel_types::QueueItemBehavior> {
        Arc::new(ChatQueueItem {
            id: Uuid::new_v4(),
            room_id: room,
            content: content.into(),
            sender_id: Uuid::new_v4(),
            sender_name: sender.into(),
            sender_type: SenderType::Human,
            mentions: false,
            timestamp: now_ms(),
            enqueued_at: now_ms(),
            priority: 0.5,
            consolidated_context: Vec::new(),
            media: Vec::new(),
            embedding_cell: OnceLock::new(),
            #[cfg(any(test, feature = "test-fixtures"))]
            compute_calls: std::sync::atomic::AtomicUsize::new(0),
        })
    }

    /// proves: ChatChannelView::interpret aggregates a burst into a
    /// typed ChatCoherentInput — one input per burst, all items
    /// merged into the aggregated_content field
    #[test]
    fn chat_view_aggregates_burst_into_typed_input() {
        let room = Uuid::new_v4();
        let burst = CoherentUnit::Chat {
            items: vec![
                make_chat_arc("hello team", "Operator", room),
                make_chat_arc("hi there", "Maya", room),
                make_chat_arc("good morning", "Operator", room),
            ],
            window_span_ms: 500,
            primary_room: room,
        };

        let view = ChatChannelView;
        let input = view.interpret(&burst, &PersonaIdentity::new(Uuid::new_v4(), "Helper"));

        match input {
            CoherentInput::Chat(chat) => {
                assert_eq!(chat.primary_room, room);
                assert_eq!(chat.burst_message_count, 3);
                assert_eq!(chat.window_span_ms, 500);
                assert_eq!(chat.last_sender_name, "Operator");
                assert!(chat.aggregated_content.contains("Operator: hello team"));
                assert!(chat.aggregated_content.contains("Maya: hi there"));
                assert!(chat.aggregated_content.contains("Operator: good morning"));
                // "Helper" was never mentioned in any item
                assert!(!chat.anyone_mentioned_persona);
            }
            other => panic!("expected CoherentInput::Chat, got {other:?}"),
        }
    }

    /// proves: identity-aware perspective — two personas viewing the
    /// SAME burst get DIFFERENT `anyone_mentioned_persona` values
    /// based on their identity. Same shared items, different
    /// per-persona perspective. The doctrine concretely demonstrated.
    #[test]
    fn chat_view_mention_detection_is_identity_aware() {
        let room = Uuid::new_v4();
        let burst = CoherentUnit::Chat {
            items: vec![
                make_chat_arc("hey Maya can you review this?", "Operator", room),
                make_chat_arc("on it", "Maya", room),
            ],
            window_span_ms: 500,
            primary_room: room,
        };

        let view = ChatChannelView;

        // Maya sees herself mentioned
        let maya_input = view.interpret(&burst, &PersonaIdentity::new(Uuid::new_v4(), "Maya"));
        let maya_mentioned = match &maya_input {
            CoherentInput::Chat(c) => c.anyone_mentioned_persona,
            _ => panic!("expected Chat input"),
        };
        assert!(
            maya_mentioned,
            "Maya should see herself mentioned in 'hey Maya can you review this?'"
        );

        // Helper does NOT see itself mentioned
        let helper_input = view.interpret(&burst, &PersonaIdentity::new(Uuid::new_v4(), "Helper"));
        let helper_mentioned = match &helper_input {
            CoherentInput::Chat(c) => c.anyone_mentioned_persona,
            _ => panic!("expected Chat input"),
        };
        assert!(
            !helper_mentioned,
            "Helper should NOT see itself mentioned — Maya was named, not Helper"
        );
    }

    /// proves: shared-decode property survives the interpret layer —
    /// two personas calling interpret on the same burst read the SAME
    /// `Arc<Vec<f32>>` for `burst_embedding`. The embedding compute
    /// fires ONCE on the underlying item (whoever called first); both
    /// CoherentInput values carry handles to the same cached Arc.
    ///
    /// This is the doctrine `[[shared-decode-per-persona-perspective]]`
    /// concretely witnessed: the per-persona perspective layer reads
    /// the substrate-shared decode, doesn't re-compute it.
    #[test]
    fn chat_view_burst_embedding_is_arc_shared_across_personas() {
        let room = Uuid::new_v4();
        let items: Vec<Arc<dyn crate::persona::channel_types::QueueItemBehavior>> =
            vec![make_chat_arc(
                "the shared content for this test",
                "Operator",
                room,
            )];
        let burst = CoherentUnit::Chat {
            items,
            window_span_ms: 0,
            primary_room: room,
        };

        let view = ChatChannelView;

        // Maya's perspective
        let maya = match view.interpret(&burst, &PersonaIdentity::new(Uuid::new_v4(), "Maya")) {
            CoherentInput::Chat(c) => c,
            _ => panic!("expected Chat"),
        };

        // Helper's perspective (different persona, SAME burst)
        let helper = match view.interpret(&burst, &PersonaIdentity::new(Uuid::new_v4(), "Helper")) {
            CoherentInput::Chat(c) => c,
            _ => panic!("expected Chat"),
        };

        // Same underlying Arc — the embedding compute fired ONCE on
        // the item's lazy cell; both perspective passes read the
        // cached share.
        assert!(
            Arc::ptr_eq(&maya.burst_embedding, &helper.burst_embedding),
            "burst_embedding must be Arc-shared across personas — \
             interpret() should read the item's cached cell, not recompute"
        );
    }

    /// proves: ChatChannelView is responsible ONLY for Chat units.
    /// Calling it on a non-Chat unit panics with a programmer-error
    /// message — adversarial review (Reviewer 1 C4 / Reviewer 3 C6)
    /// flagged the prior silent-fallthrough-to-Other shape as a
    /// `[[no-fallbacks-ever]]` violation. The dispatch layer
    /// (`ChannelRegistry::interpret_for_domain`) now constructs
    /// `CoherentInput::Other` directly for non-Chat domains.
    #[test]
    #[should_panic(expected = "ChatChannelView::interpret called on non-Chat unit")]
    fn chat_view_panics_on_non_chat_unit() {
        let burst = CoherentUnit::Voice {
            items: Vec::new(),
            window_span_ms: 0,
        };

        let view = ChatChannelView;
        let _ = view.interpret(&burst, &PersonaIdentity::new(Uuid::new_v4(), "Maya"));
    }
}
