//! MessageBus — inter-module event pub/sub with glob pattern subscriptions.
//!
//! Two-tier delivery (like CBAR's frame broadcasting):
//! - Synchronous: real-time handlers called inline during publish
//! - Asynchronous: deferred handlers receive via broadcast channel
//!
//! Modules subscribe via their config().event_subscriptions.

use super::artifact_handle::{ArtifactKey, ArtifactSelector};
use dashmap::DashMap;
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Instant;
use tokio::sync::broadcast;
use tracing::warn;

/// A subscription record linking an event pattern to a module.
struct Subscription {
    /// Glob pattern: "voice:*", "data:users:created", "persona:state:*"
    pattern: String,
    /// The module name to notify
    module_name: &'static str,
    /// Whether delivery is synchronous (real-time tier) or async (deferred tier)
    synchronous: bool,
}

/// An artifact subscription record. Sibling to `Subscription` but uses
/// `ArtifactSelector::matches` (Exact / Prefix on the full
/// slash-convention key) instead of the colon-segmented `glob_matches`.
///
/// Why a separate path: `glob_matches` is built for the event-bus
/// convention `<a>:<b>:<c>` with `*` matching one segment. ArtifactKey
/// uses `<module>/<surface>.<event>` (slash + dot) and has its own
/// matcher already (`ArtifactSelector::matches`) that the producer +
/// consumer sides both agree on. Routing artifact events through
/// glob_matches forces a separator translation that doesn't exist
/// cleanly; routing them through their own matcher keeps both paths
/// honest. Event subscriptions and artifact subscriptions coexist on
/// the same MessageBus, share publish(), share record_recent — they
/// just walk different subscriber lists with different matchers.
struct ArtifactSubscription {
    selector: ArtifactSelector,
    module_name: &'static str,
}

/// Event payload sent through the bus.
#[derive(Debug, Clone)]
pub struct BusEvent {
    pub name: String,
    pub payload: serde_json::Value,
}

/// Timestamped event for the recent event buffer.
#[derive(Clone)]
struct TimestampedEvent {
    event: BusEvent,
    at: Instant,
}

/// Maximum number of recent events to buffer.
/// Sized for large pipeline sessions (e.g., 98-challenge RealClassEval with ~5 events/challenge).
const RECENT_EVENT_BUFFER_SIZE: usize = 1024;
/// How long recent events remain available for replay.
/// 5 minutes — enough for the student sentinel to consume events even when
/// the teacher runs far ahead (each challenge takes the student ~6s for LLM + grading).
const RECENT_EVENT_TTL_SECS: u64 = 300;

pub struct MessageBus {
    /// Subscriptions grouped by module name
    subscriptions: DashMap<&'static str, Vec<Subscription>>,

    /// Artifact subscriptions grouped by module name. Walked alongside
    /// `subscriptions` on every publish, but matched via
    /// `ArtifactSelector::matches` instead of `glob_matches`. PR-3 of
    /// CBAR-PIECE-2 introduces this path so Prefix selectors actually
    /// deliver — the prior approach of cramming ArtifactKeys through
    /// the colon-segmented glob matcher only worked for Exact.
    artifact_subscriptions: DashMap<&'static str, Vec<ArtifactSubscription>>,

    /// Broadcast channel for async (deferred) event delivery
    sender: broadcast::Sender<BusEvent>,

    /// Ring buffer of recent events for race-condition-safe watch steps.
    recent_events: Mutex<VecDeque<TimestampedEvent>>,

    /// Per-prefix coalescing: tracks last emit time per event prefix.
    /// Events matching a coalesced prefix are dropped if emitted within
    /// the coalescing window (50ms default). Prevents event floods from
    /// bulk operations like codebase indexing.
    coalesce_tracker: DashMap<String, Instant>,
}

impl Default for MessageBus {
    fn default() -> Self {
        Self::new()
    }
}

static GLOBAL_BUS: std::sync::OnceLock<std::sync::Arc<MessageBus>> = std::sync::OnceLock::new();

impl MessageBus {
    /// Minimum interval between events with the same prefix.
    /// Events arriving faster than this are dropped (coalesced).
    /// 50ms = max 20 events/sec per prefix — enough for UI updates,
    /// prevents flooding from bulk ops (indexer, ORM batch writes).
    const COALESCE_WINDOW_MS: u128 = 50;

    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(1024);
        Self {
            subscriptions: DashMap::new(),
            artifact_subscriptions: DashMap::new(),
            sender,
            recent_events: Mutex::new(VecDeque::with_capacity(RECENT_EVENT_BUFFER_SIZE)),
            coalesce_tracker: DashMap::new(),
        }
    }

    /// Find and consume a recent event matching the given pattern.
    /// Returns the event if found within the TTL window.
    /// Removes the matched event from the buffer to prevent double-matching
    /// across loop iterations that watch for the same event name.
    /// Uses the same glob matching as event subscriptions.
    pub fn find_recent_event(&self, pattern: &str) -> Option<BusEvent> {
        let now = Instant::now();
        let ttl = std::time::Duration::from_secs(RECENT_EVENT_TTL_SECS);
        let mut buf = self.recent_events.lock().unwrap();
        // Search from newest to oldest (VecDeque: back = newest)
        let found_idx = buf
            .iter()
            .enumerate()
            .rev()
            .find(|(_, te)| {
                now.duration_since(te.at) < ttl && glob_matches(pattern, &te.event.name)
            })
            .map(|(i, te)| (i, te.event.clone()));
        if let Some((idx, event)) = found_idx {
            buf.remove(idx); // O(min(idx, len-idx)) — acceptable for consumed matches
            Some(event)
        } else {
            None
        }
    }

    /// Record an event in the recent buffer (ring buffer with eviction).
    /// Sweeps expired events from the front before inserting.
    fn record_recent(&self, event: &BusEvent) {
        let now = Instant::now();
        let ttl = std::time::Duration::from_secs(RECENT_EVENT_TTL_SECS);
        let mut buf = self.recent_events.lock().unwrap();
        // Sweep expired events from the front (oldest first, O(1) each)
        while let Some(front) = buf.front() {
            if now.duration_since(front.at) >= ttl {
                buf.pop_front();
            } else {
                break;
            }
        }
        // Capacity guard (shouldn't trigger after TTL sweep, but safety net)
        if buf.len() >= RECENT_EVENT_BUFFER_SIZE {
            buf.pop_front();
        }
        buf.push_back(TimestampedEvent {
            event: event.clone(),
            at: now,
        });
    }

    /// Publish THE runtime bus process-globally (first writer wins — the boot path).
    /// Same precedent as `PersonaAircRuntimeRegistry::set_global`: host-independent
    /// bodies (the detached cognition/eval emitting `eval:progress`) publish without
    /// a threaded handle. Read with [`MessageBus::global`].
    pub fn set_global(bus: std::sync::Arc<MessageBus>) {
        let _ = GLOBAL_BUS.set(bus);
    }

    /// The process-global runtime bus, if boot has published it (None in bare unit
    /// tests — callers treat that as "no subscribers", never an error).
    pub fn global() -> Option<std::sync::Arc<MessageBus>> {
        GLOBAL_BUS.get().cloned()
    }

    /// Subscribe to events matching a glob pattern.
    ///
    /// synchronous=true: handle_event() called inline during `publish(..., registry)`.
    ///
    /// REALITY CHECK (#140 post-mortem, 2026-07-16): there is NO deferred tier.
    /// synchronous=false subscriptions are stored but never delivered — and
    /// `publish(..., registry)` (the only dispatching publisher) has no production
    /// callers today; live events flow through `publish_async_only`, which feeds
    /// ONLY the broadcast channel. A module that needs async bus events must run a
    /// bus-receiver task (`bus.receiver()` + `tokio::spawn` from `initialize`) —
    /// see `cognition::dispatch_listener::spawn` and
    /// `modules::chat::spawn_persist_listener` for the canonical shape. This doc
    /// used to promise "queued for async delivery (deferred tier)", which nearly
    /// shipped a silently-dead transcript writer.
    pub fn subscribe(&self, pattern: &str, module_name: &'static str, synchronous: bool) {
        let sub = Subscription {
            pattern: pattern.to_string(),
            module_name,
            synchronous,
        };
        self.subscriptions.entry(module_name).or_default().push(sub);
    }

    /// Subscribe to artifact events matching an ArtifactSelector.
    ///
    /// Sibling to `subscribe`, but routes via `ArtifactSelector::matches`
    /// (Exact / Prefix on the full slash-convention key) instead of
    /// colon-segmented glob_matches. Delivery is always synchronous —
    /// `on_artifact_available` is contract-bound to cheap-and-return,
    /// so inline dispatch from the publisher's task is safe and avoids
    /// the broadcast-channel detour that would force the runtime to
    /// route back to handle_event.
    ///
    /// Used by `Runtime::register` to wire `ServiceModule::
    /// artifact_subscriptions()`. The default `handle_event` impl on
    /// ServiceModule auto-forwards to `on_artifact_available` when
    /// the incoming event_name matches one of this module's selectors.
    pub fn subscribe_artifact(&self, selector: ArtifactSelector, module_name: &'static str) {
        let sub = ArtifactSubscription {
            selector,
            module_name,
        };
        self.artifact_subscriptions
            .entry(module_name)
            .or_default()
            .push(sub);
    }

    /// Get a receiver for async event delivery.
    /// Modules that need async events call this during initialize().
    pub fn receiver(&self) -> broadcast::Receiver<BusEvent> {
        self.sender.subscribe()
    }

    /// Publish an event. Synchronous handlers are called inline.
    /// Async handlers receive via the broadcast channel.
    ///
    /// registry is needed to look up module instances for synchronous delivery.
    ///
    /// Implementation note: both subscriber walks collect a
    /// `Vec<&'static str>` of matching module names BEFORE entering
    /// the async dispatch loop. This drops the DashMap borrow before
    /// any `.await`, which lets the publish future remain `Send` even
    /// when called from spawn contexts (e.g. genome PR-5's
    /// `tokio::spawn` of `publish_page_fault`). Without this, the
    /// DashMap iter borrow lives across the await and trips
    /// "implementation of `dashmap::Map` is not general enough"
    /// when the future is shipped to a Send-bounded task.
    pub async fn publish(
        &self,
        event_name: &str,
        payload: serde_json::Value,
        registry: &super::ModuleRegistry,
    ) {
        // Synchronous tier (glob-matched event_subscriptions): collect
        // matching module names, release the DashMap borrow, then
        // dispatch.
        let glob_matched: Vec<&'static str> = self
            .subscriptions
            .iter()
            .flat_map(|entry| {
                entry
                    .value()
                    .iter()
                    .filter(|sub| sub.synchronous && glob_matches(&sub.pattern, event_name))
                    .map(|sub| sub.module_name)
                    .collect::<Vec<_>>()
            })
            .collect();
        for module_name in glob_matched {
            if let Some(module) = registry.get_by_name(module_name) {
                if let Err(e) = module.handle_event(event_name, payload.clone()).await {
                    warn!(
                        "Event handler error: module={}, event={}, error={}",
                        module_name, event_name, e
                    );
                }
            }
        }

        // Artifact tier (ArtifactSelector-matched artifact_subscriptions):
        // walk the dedicated artifact subscriber list using the selector's
        // own matcher. Delivers via handle_event so the default impl on
        // ServiceModule (which forwards to on_artifact_available when
        // the key matches one of artifact_subscriptions()) closes the
        // loop. A module that overrides handle_event keeps full control;
        // it can call self.on_artifact_available(...).await from inside
        // its override.
        let key = ArtifactKey::from(event_name);
        let artifact_matched: Vec<&'static str> = self
            .artifact_subscriptions
            .iter()
            .flat_map(|entry| {
                entry
                    .value()
                    .iter()
                    .filter(|sub| sub.selector.matches(&key))
                    .map(|sub| sub.module_name)
                    .collect::<Vec<_>>()
            })
            .collect();
        for module_name in artifact_matched {
            if let Some(module) = registry.get_by_name(module_name) {
                if let Err(e) = module.handle_event(event_name, payload.clone()).await {
                    warn!(
                        "Artifact handler error: module={}, key={}, error={}",
                        module_name, event_name, e
                    );
                }
            }
        }

        // Deferred tier: broadcast for async consumers
        let event = BusEvent {
            name: event_name.to_string(),
            payload,
        };
        self.record_recent(&event);
        // Ignore send error (no receivers is fine)
        let _ = self.sender.send(event);
    }

    /// Publish without async (for use from sync code).
    /// Only broadcasts to deferred tier — synchronous handlers are skipped.
    /// Applies per-prefix coalescing to prevent event floods from bulk operations.
    pub fn publish_async_only(&self, event_name: &str, payload: serde_json::Value) {
        // Passthrough: sentinel/academy/chat events need real-time delivery
        let is_realtime = event_name.starts_with("sentinel:")
            || event_name.starts_with("academy:")
            || event_name.starts_with("chat:")
            || event_name.starts_with("command:")  // RTOS doctrine — every dispatch's completion event reaches the persona loop (see PERSONA-AS-DEVELOPER-GAP.md §P3)
            || event_name.starts_with("presence:")
            || event_name.starts_with("tool:")
            || event_name.starts_with("airc:bridge:")  // !continuum directive/reply control events — must not coalesce-drop (directive & reply share the airc:bridge prefix; coalescing would drop a reply emitted right after its directive)
            || event_name.contains("chat_messages")  // data:chat_messages:created must not be coalesced
            || event_name.contains("chat_rooms"); // room events are real-time too

        if !is_realtime {
            // Coalesce: extract prefix (first two segments) and rate-limit
            let prefix = Self::event_prefix(event_name);
            let now = Instant::now();

            if let Some(last) = self.coalesce_tracker.get(&prefix) {
                if now.duration_since(*last).as_millis() < Self::COALESCE_WINDOW_MS {
                    return; // Drop — too fast
                }
            }
            self.coalesce_tracker.insert(prefix, now);
        }

        let event = BusEvent {
            name: event_name.to_string(),
            payload,
        };
        self.record_recent(&event);
        let _ = self.sender.send(event);
    }

    /// Extract event prefix for coalescing (first two segments).
    /// "data:users:created" → "data:users"
    /// "code_index:created" → "code_index:created"
    fn event_prefix(event_name: &str) -> String {
        let parts: Vec<&str> = event_name.splitn(3, ':').collect();
        if parts.len() >= 2 {
            format!("{}:{}", parts[0], parts[1])
        } else {
            event_name.to_string()
        }
    }
}

/// Glob matching for event patterns.
/// Supports:
/// - Exact match: "data:users:created" matches "data:users:created"
/// - Wildcard segment: "data:*:created" matches "data:users:created"
/// - Trailing wildcard: "data:*" matches "data:users:created"
fn glob_matches(pattern: &str, event: &str) -> bool {
    let pat_parts: Vec<&str> = pattern.split(':').collect();
    let evt_parts: Vec<&str> = event.split(':').collect();

    let mut pi = 0;
    let mut ei = 0;

    while pi < pat_parts.len() && ei < evt_parts.len() {
        if pat_parts[pi] == "*" {
            // If this is the last pattern segment, match all remaining event segments
            if pi == pat_parts.len() - 1 {
                return true;
            }
            // Otherwise, match this one segment
            pi += 1;
            ei += 1;
        } else if pat_parts[pi] == evt_parts[ei] {
            pi += 1;
            ei += 1;
        } else {
            return false;
        }
    }

    pi == pat_parts.len() && ei == evt_parts.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exact_match() {
        assert!(glob_matches("data:users:created", "data:users:created"));
        assert!(!glob_matches("data:users:created", "data:users:deleted"));
        assert!(!glob_matches("data:users:created", "data:users"));
    }

    #[test]
    fn test_wildcard_segment() {
        assert!(glob_matches("data:*:created", "data:users:created"));
        assert!(glob_matches("data:*:created", "data:rooms:created"));
        assert!(!glob_matches("data:*:created", "data:users:deleted"));
    }

    #[test]
    fn test_trailing_wildcard() {
        assert!(glob_matches("data:*", "data:users:created"));
        assert!(glob_matches("data:*", "data:rooms"));
        assert!(glob_matches("data:*", "data:anything:here:deep"));
        assert!(!glob_matches("data:*", "voice:something"));
    }

    #[test]
    fn test_single_segment() {
        assert!(glob_matches("ping", "ping"));
        assert!(!glob_matches("ping", "pong"));
    }

    #[test]
    fn test_all_wildcard() {
        assert!(glob_matches("*", "anything"));
        assert!(glob_matches("*", "deep:nested:event"));
    }

    #[test]
    fn test_recent_event_buffer_is_ring() {
        let bus = MessageBus::new();
        // Fill beyond capacity
        for i in 0..RECENT_EVENT_BUFFER_SIZE + 10 {
            bus.publish_async_only(&format!("test:{i}"), serde_json::Value::Null);
        }
        let buf = bus.recent_events.lock().unwrap();
        assert_eq!(buf.len(), RECENT_EVENT_BUFFER_SIZE);
        // Oldest surviving event should be #10 (first 10 evicted)
        assert_eq!(buf.front().unwrap().event.name, "test:10");
        assert_eq!(
            buf.back().unwrap().event.name,
            format!("test:{}", RECENT_EVENT_BUFFER_SIZE + 9)
        );
    }

    #[test]
    fn test_find_recent_event_consumes() {
        let bus = MessageBus::new();
        bus.publish_async_only("foo:bar", serde_json::json!({"x": 1}));
        bus.publish_async_only("foo:baz", serde_json::json!({"x": 2}));

        // First find succeeds
        let found = bus.find_recent_event("foo:bar");
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "foo:bar");

        // Second find fails (consumed)
        assert!(bus.find_recent_event("foo:bar").is_none());

        // Other event still there
        assert!(bus.find_recent_event("foo:baz").is_some());
    }

    #[test]
    fn test_find_recent_event_glob() {
        let bus = MessageBus::new();
        bus.publish_async_only("academy:sess1:topic:ready:3", serde_json::Value::Null);

        // Glob match
        let found = bus.find_recent_event("academy:sess1:topic:ready:*");
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "academy:sess1:topic:ready:3");
    }
}
