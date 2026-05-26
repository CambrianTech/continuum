//! EventClassRegistry — process-global, thread-safe registry of declared
//! event classes.
//!
//! Roadmap item L1-1 (see docs/grid/GRID-MIGRATION-ROADMAP.md).
//! Spec: GRID-BUS-ARCHITECTURE §2.2 (continuum#1439).
//!
//! Module-singleton holding `name → ResolvedEventClassConfig`. Consulted by:
//!   - The IPC handler in `crate::modules::events` for declare/get/list
//!   - Future AircEventTransport (L1-2) for channel resolution
//!   - The TS-side cache, which hydrates via IPC on startup
//!
//! Registration is idempotent for identical re-declarations; conflicting
//! re-declarations throw — event classes are wire contracts.

use crate::events::event_class::{
    resolve_event_class_config, EventClassChannelStrategy, EventClassConfig,
    EventClassDeclareError, ResolvedEventClassConfig,
};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::OnceLock;
use thiserror::Error;

/// Errors raised when registering a class via the registry. Validation
/// errors from `resolve_event_class_config` are wrapped; the conflicting-
/// redeclaration check is registry-side.
#[derive(Debug, Error)]
pub enum EventClassRegistryError {
    #[error(transparent)]
    Declare(#[from] EventClassDeclareError),
}

/// Errors raised when resolving the airc channel for an event emission.
/// Happens at emit time (L1-2+), not at declare time.
#[derive(Debug, Error)]
pub enum EventClassChannelResolveError {
    #[error("EventClass '{0}' is not declared")]
    Undeclared(String),

    #[error("EventClass '{0}': declared with broadcast: false; airc channel resolution skipped")]
    NotBroadcast(String),

    #[error(
        "EventClass '{name}': channel: {channel:?} requires payload.{required_field} to be present and non-empty"
    )]
    MissingPayloadField {
        name: String,
        channel: EventClassChannelStrategy,
        required_field: &'static str,
    },

    #[error(
        "EventClass '{name}': channel: Custom requires a process-local resolver — \
         declared via Rust IPC but no Rust-side resolver wired. (TS-side custom \
         resolvers run in the TS process; the Rust registry only records the channel \
         strategy.)"
    )]
    CustomResolverUnsupported { name: String },
}

#[derive(Debug, Clone)]
struct RegistryEntry {
    config: ResolvedEventClassConfig,
    /// Canonical form used for idempotent-re-declaration check.
    canonical: String,
}

pub struct EventClassRegistry {
    classes: RwLock<HashMap<String, RegistryEntry>>,
}

impl EventClassRegistry {
    pub fn new() -> Self {
        Self {
            classes: RwLock::new(HashMap::new()),
        }
    }

    /// Declare an event class. Idempotent for identical re-declarations;
    /// raises `ConflictingRedeclaration` on a name collision with different
    /// config (per the wire-contract integrity invariant).
    pub fn declare(
        &self,
        name: &str,
        config: &EventClassConfig,
    ) -> Result<ResolvedEventClassConfig, EventClassRegistryError> {
        let resolved = resolve_event_class_config(name, config)?;
        let canonical = canonicalize(&resolved);

        let mut classes = self.classes.write();
        if let Some(existing) = classes.get(name) {
            if existing.canonical != canonical {
                return Err(EventClassRegistryError::Declare(
                    EventClassDeclareError::ConflictingRedeclaration {
                        name: name.to_string(),
                    },
                ));
            }
            return Ok(existing.config.clone());
        }
        classes.insert(
            name.to_string(),
            RegistryEntry {
                config: resolved.clone(),
                canonical,
            },
        );
        Ok(resolved)
    }

    /// Look up the resolved config for an event name. Returns `None` when
    /// no class is declared — caller treats this as "use default backward-
    /// compat behavior" (local + WebSocket EventBridge, no airc broadcast).
    pub fn get(&self, name: &str) -> Option<ResolvedEventClassConfig> {
        self.classes.read().get(name).map(|e| e.config.clone())
    }

    /// Snapshot of all declared classes. Order is unspecified — caller
    /// sorts if needed (e.g. for stable introspection output).
    pub fn list(&self) -> Vec<ResolvedEventClassConfig> {
        self.classes
            .read()
            .values()
            .map(|e| e.config.clone())
            .collect()
    }

    /// Resolve the airc channel name for an emit, given the event name +
    /// the event payload (as a serde_json::Value so the registry doesn't
    /// need a per-class type).
    ///
    /// `Custom` channel strategy is unsupported at the Rust-canonical
    /// layer — custom resolvers are process-local functions that can't
    /// cross the wire; the TS side handles its own custom resolvers in-
    /// process, then submits the resolved channel via a different IPC if
    /// it needs Rust to know the result.
    pub fn resolve_channel(
        &self,
        name: &str,
        payload: &serde_json::Value,
    ) -> Result<String, EventClassChannelResolveError> {
        let entry = self
            .classes
            .read()
            .get(name)
            .cloned()
            .ok_or_else(|| EventClassChannelResolveError::Undeclared(name.to_string()))?;
        if !entry.config.broadcast {
            return Err(EventClassChannelResolveError::NotBroadcast(name.to_string()));
        }
        match entry.config.channel {
            EventClassChannelStrategy::Global => Ok("global".to_string()),
            EventClassChannelStrategy::ByRoomId => {
                extract_string_field(payload, "roomId").ok_or_else(|| {
                    EventClassChannelResolveError::MissingPayloadField {
                        name: name.to_string(),
                        channel: EventClassChannelStrategy::ByRoomId,
                        required_field: "roomId",
                    }
                })
            }
            EventClassChannelStrategy::ByPeerId => {
                extract_string_field(payload, "peerId").ok_or_else(|| {
                    EventClassChannelResolveError::MissingPayloadField {
                        name: name.to_string(),
                        channel: EventClassChannelStrategy::ByPeerId,
                        required_field: "peerId",
                    }
                })
            }
            EventClassChannelStrategy::Custom => {
                Err(EventClassChannelResolveError::CustomResolverUnsupported {
                    name: name.to_string(),
                })
            }
            EventClassChannelStrategy::Local => Err(EventClassChannelResolveError::NotBroadcast(
                name.to_string(),
            )),
        }
    }

    /// Test-only — clears all declarations. Production code never calls this.
    #[cfg(test)]
    pub fn clear(&self) {
        self.classes.write().clear();
    }
}

impl Default for EventClassRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Process-global registry singleton. Initialized lazily on first access.
fn registry_singleton() -> &'static EventClassRegistry {
    static REGISTRY: OnceLock<EventClassRegistry> = OnceLock::new();
    REGISTRY.get_or_init(EventClassRegistry::new)
}

/// Module-level accessor for the process-global registry. Returns a
/// reference rather than a clone — the registry is `RwLock`-internally
/// synchronized.
pub fn event_class_registry() -> &'static EventClassRegistry {
    registry_singleton()
}

/// Convenience wrapper for the singleton's `declare`. Mirrors the
/// JavaScript-side `declareEventClass()` helper.
pub fn declare_event_class(
    name: &str,
    config: &EventClassConfig,
) -> Result<ResolvedEventClassConfig, EventClassRegistryError> {
    registry_singleton().declare(name, config)
}

/// Convenience wrapper for the singleton's `get`.
pub fn lookup_event_class(name: &str) -> Option<ResolvedEventClassConfig> {
    registry_singleton().get(name)
}

/// Convenience wrapper for the singleton's `list`.
pub fn list_event_classes() -> Vec<ResolvedEventClassConfig> {
    registry_singleton().list()
}

/// Convenience wrapper for the singleton's `resolve_channel`.
pub fn resolve_event_class_channel(
    name: &str,
    payload: &serde_json::Value,
) -> Result<String, EventClassChannelResolveError> {
    registry_singleton().resolve_channel(name, payload)
}

// ─── Helpers ──────────────────────────────────────────────────────────

fn canonicalize(c: &ResolvedEventClassConfig) -> String {
    // Stable canonical form for the idempotent-redeclaration check.
    // Excludes `name` (it's the registry key) and `description` (free
    // text; not load-bearing for the contract).
    serde_json::json!({
        "broadcast": c.broadcast,
        "channel": c.channel,
        "schemaVersion": c.schema_version,
        "onUnknownSchema": c.on_unknown_schema,
    })
    .to_string()
}

fn extract_string_field(payload: &serde_json::Value, field: &str) -> Option<String> {
    payload
        .as_object()?
        .get(field)?
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local_cfg() -> EventClassConfig {
        EventClassConfig {
            broadcast: false,
            channel: None,
            schema_version: "v1".into(),
            on_unknown_schema: None,
            description: None,
        }
    }

    fn broadcast_global_cfg() -> EventClassConfig {
        EventClassConfig {
            broadcast: true,
            channel: Some(EventClassChannelStrategy::Global),
            schema_version: "v1".into(),
            on_unknown_schema: None,
            description: Some("test class".into()),
        }
    }

    fn broadcast_by_room_cfg() -> EventClassConfig {
        EventClassConfig {
            broadcast: true,
            channel: Some(EventClassChannelStrategy::ByRoomId),
            schema_version: "v1".into(),
            on_unknown_schema: None,
            description: None,
        }
    }

    #[test]
    fn declare_get_roundtrip() {
        let r = EventClassRegistry::new();
        let resolved = r.declare("chat:posted", &broadcast_global_cfg()).unwrap();
        assert!(resolved.broadcast);

        let fetched = r.get("chat:posted").unwrap();
        assert_eq!(fetched.name, "chat:posted");
        assert_eq!(fetched.channel, EventClassChannelStrategy::Global);
        assert_eq!(fetched.schema_version, "v1");
        assert_eq!(fetched.description, "test class");
    }

    #[test]
    fn get_undeclared_returns_none() {
        let r = EventClassRegistry::new();
        assert!(r.get("never:declared").is_none());
    }

    #[test]
    fn idempotent_redeclaration_succeeds() {
        let r = EventClassRegistry::new();
        let a = r.declare("foo:bar", &local_cfg()).unwrap();
        let b = r.declare("foo:bar", &local_cfg()).unwrap();
        assert_eq!(a, b);
        // Only one entry in the list.
        assert_eq!(r.list().len(), 1);
    }

    #[test]
    fn conflicting_redeclaration_errors() {
        let r = EventClassRegistry::new();
        r.declare("foo:bar", &local_cfg()).unwrap();
        let conflict = EventClassConfig {
            broadcast: true,
            channel: Some(EventClassChannelStrategy::Global),
            schema_version: "v2".into(),
            on_unknown_schema: None,
            description: None,
        };
        let err = r.declare("foo:bar", &conflict).unwrap_err();
        assert!(matches!(
            err,
            EventClassRegistryError::Declare(EventClassDeclareError::ConflictingRedeclaration { .. })
        ));
    }

    #[test]
    fn list_returns_all_declared() {
        let r = EventClassRegistry::new();
        r.declare("a:b", &local_cfg()).unwrap();
        r.declare("c:d", &broadcast_global_cfg()).unwrap();
        let mut names: Vec<String> = r.list().iter().map(|c| c.name.clone()).collect();
        names.sort();
        assert_eq!(names, vec!["a:b", "c:d"]);
    }

    #[test]
    fn resolve_channel_global() {
        let r = EventClassRegistry::new();
        r.declare("presence:peer-manifest", &broadcast_global_cfg())
            .unwrap();
        let ch = r
            .resolve_channel("presence:peer-manifest", &serde_json::json!({}))
            .unwrap();
        assert_eq!(ch, "global");
    }

    #[test]
    fn resolve_channel_by_room_id() {
        let r = EventClassRegistry::new();
        r.declare("chat:posted", &broadcast_by_room_cfg()).unwrap();
        let ch = r
            .resolve_channel(
                "chat:posted",
                &serde_json::json!({ "roomId": "room-abc-123" }),
            )
            .unwrap();
        assert_eq!(ch, "room-abc-123");
    }

    #[test]
    fn resolve_channel_by_room_id_missing_field() {
        let r = EventClassRegistry::new();
        r.declare("chat:posted", &broadcast_by_room_cfg()).unwrap();
        let err = r
            .resolve_channel("chat:posted", &serde_json::json!({}))
            .unwrap_err();
        assert!(matches!(
            err,
            EventClassChannelResolveError::MissingPayloadField { required_field: "roomId", .. }
        ));
    }

    #[test]
    fn resolve_channel_undeclared() {
        let r = EventClassRegistry::new();
        let err = r
            .resolve_channel("never:declared", &serde_json::json!({}))
            .unwrap_err();
        assert!(matches!(err, EventClassChannelResolveError::Undeclared(_)));
    }

    #[test]
    fn resolve_channel_not_broadcast() {
        let r = EventClassRegistry::new();
        r.declare("widget:mounted", &local_cfg()).unwrap();
        let err = r
            .resolve_channel("widget:mounted", &serde_json::json!({}))
            .unwrap_err();
        assert!(matches!(err, EventClassChannelResolveError::NotBroadcast(_)));
    }

    #[test]
    fn singleton_persists_across_calls() {
        // Use a unique-per-test name so we don't conflict with other tests
        // sharing the singleton.
        let name = "singleton:persists";
        declare_event_class(name, &local_cfg()).unwrap();
        let fetched = lookup_event_class(name).unwrap();
        assert_eq!(fetched.name, name);
    }
}
