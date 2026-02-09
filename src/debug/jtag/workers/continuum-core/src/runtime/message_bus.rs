//! MessageBus — inter-module event pub/sub with glob pattern subscriptions.
//!
//! Two-tier delivery (like CBAR's frame broadcasting):
//! - Synchronous: real-time handlers called inline during publish
//! - Asynchronous: deferred handlers receive via broadcast channel
//!
//! Modules subscribe via their config().event_subscriptions.

use dashmap::DashMap;
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

/// Event payload sent through the bus.
#[derive(Debug, Clone)]
pub struct BusEvent {
    pub name: String,
    pub payload: serde_json::Value,
}

pub struct MessageBus {
    /// Subscriptions grouped by module name
    subscriptions: DashMap<&'static str, Vec<Subscription>>,

    /// Broadcast channel for async (deferred) event delivery
    sender: broadcast::Sender<BusEvent>,
}

impl MessageBus {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(1024);
        Self {
            subscriptions: DashMap::new(),
            sender,
        }
    }

    /// Subscribe to events matching a glob pattern.
    ///
    /// synchronous=true: handle_event() called inline during publish (real-time tier)
    /// synchronous=false: event queued for async delivery (deferred tier)
    pub fn subscribe(&self, pattern: &str, module_name: &'static str, synchronous: bool) {
        let sub = Subscription {
            pattern: pattern.to_string(),
            module_name,
            synchronous,
        };
        self.subscriptions
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
    pub async fn publish(
        &self,
        event_name: &str,
        payload: serde_json::Value,
        registry: &super::ModuleRegistry,
    ) {
        // Synchronous tier: call matching handlers inline
        for entry in self.subscriptions.iter() {
            for sub in entry.value().iter() {
                if sub.synchronous && glob_matches(&sub.pattern, event_name) {
                    if let Some(module) = registry.get_by_name(sub.module_name) {
                        if let Err(e) = module.handle_event(event_name, payload.clone()).await {
                            warn!(
                                "Event handler error: module={}, event={}, error={}",
                                sub.module_name, event_name, e
                            );
                        }
                    }
                }
            }
        }

        // Deferred tier: broadcast for async consumers
        let event = BusEvent {
            name: event_name.to_string(),
            payload,
        };
        // Ignore send error (no receivers is fine)
        let _ = self.sender.send(event);
    }

    /// Publish without async (for use from sync code).
    /// Only broadcasts to deferred tier — synchronous handlers are skipped.
    pub fn publish_async_only(&self, event_name: &str, payload: serde_json::Value) {
        let event = BusEvent {
            name: event_name.to_string(),
            payload,
        };
        let _ = self.sender.send(event);
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
}
