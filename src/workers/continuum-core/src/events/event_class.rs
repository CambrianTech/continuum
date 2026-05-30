//! EventClassConfig + validation. Pure types; no I/O, no registry mutation.
//!
//! Roadmap item L1-1 (see docs/grid/GRID-MIGRATION-ROADMAP.md).
//! Spec: GRID-BUS-ARCHITECTURE §2.2 (continuum#1439).
//!
//! ts-rs generates the TS bindings at `shared/generated/events/`.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

/// Channel-strategy for an event class — how the event-name maps to an airc
/// channel when `broadcast: true`. The transport consults this at emit time.
///
/// - `Local` — no broadcast (paired with `broadcast: false`).
/// - `Global` — mesh-wide single channel (e.g. `#presence`).
/// - `ByRoomId` — event payload must carry `roomId`; routed to that
///   room's airc channel.
/// - `ByPeerId` — event payload must carry `peerId`; routed to a
///   peer-targeted channel (DM-like).
/// - `Custom` — caller-supplied channel resolver runs at emit time.
///   (The resolver itself can't cross the wire — it's a per-process
///   function ref — so on the TS side the resolver is registered
///   separately from the Rust-canonical config.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../shared/generated/events/EventClassChannelStrategy.ts"
)]
pub enum EventClassChannelStrategy {
    Local,
    Global,
    ByRoomId,
    ByPeerId,
    Custom,
}

/// Behavior when a subscriber receives an event with a `schemaVersion`
/// it doesn't recognize. Default `Fail` matches the standing project rule
/// of never silently swallowing evidence.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../shared/generated/events/EventClassUnknownSchemaPolicy.ts"
)]
pub enum EventClassUnknownSchemaPolicy {
    Warn,
    #[default]
    Fail,
}

/// Caller-supplied event-class declaration. All optional fields fill with
/// conservative defaults (no broadcast, no airc cost).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../shared/generated/events/EventClassConfig.ts"
)]
pub struct EventClassConfig {
    /// Distribute this event class through the airc transport in addition
    /// to the local + WebSocket transports?
    ///
    /// `false` (default) — local + WebSocket only. Zero airc cost.
    /// `true`  — also durable on the airc log; reaches cross-machine
    ///           subscribers via the AircEventTransport (L1-2).
    #[serde(default)]
    pub broadcast: bool,

    /// How the event-name + payload map to an airc channel when broadcast
    /// is `true`. Defaults to `Local` when `broadcast: false`, otherwise
    /// required (validation throws on missing-when-broadcast).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub channel: Option<EventClassChannelStrategy>,

    /// Wire-format schema version. Subscribers fail loud on unknown
    /// versions per `on_unknown_schema`. Bump when the payload shape
    /// changes incompatibly.
    pub schema_version: String,

    /// Action when a subscriber receives an event whose declared
    /// `schemaVersion` doesn't match its build. Default `Fail`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub on_unknown_schema: Option<EventClassUnknownSchemaPolicy>,

    /// Optional human-readable description for `grid/show-event-classes`
    /// and similar introspection. Not load-bearing at runtime.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub description: Option<String>,
}

/// Canonical, post-validation form of an event-class declaration.
/// What the registry stores + what the TS side caches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../shared/generated/events/ResolvedEventClassConfig.ts"
)]
pub struct ResolvedEventClassConfig {
    pub name: String,
    pub broadcast: bool,
    pub channel: EventClassChannelStrategy,
    pub schema_version: String,
    pub on_unknown_schema: EventClassUnknownSchemaPolicy,
    pub description: String,
}

/// Validation errors raised when resolving an `EventClassConfig`. Each
/// variant carries the event-class name so a multi-class declaration
/// sweep can report which one failed.
#[derive(Debug, Error)]
pub enum EventClassDeclareError {
    #[error("EventClass name is required (non-empty string)")]
    EmptyName,

    #[error("EventClass '{name}': schemaVersion is required (non-empty)")]
    EmptySchemaVersion { name: String },

    #[error(
        "EventClass '{name}': broadcast: true requires an explicit non-local channel \
         (Global | ByRoomId | ByPeerId | Custom)"
    )]
    BroadcastWithoutChannel { name: String },

    #[error(
        "EventClass '{name}': channel: {channel:?} implies broadcast intent — \
         set broadcast: true OR drop the channel field"
    )]
    ChannelWithoutBroadcast {
        name: String,
        channel: EventClassChannelStrategy,
    },

    #[error(
        "EventClass '{name}' already declared with a conflicting config. \
         Event-class declarations are wire contracts; conflicting declarations \
         would silently shift transport behavior between callers. \
         If the config needs to change, bump schemaVersion + update subscribers."
    )]
    ConflictingRedeclaration { name: String },
}

/// Resolve user-supplied config into the canonical internal form (fills
/// defaults, validates internal consistency).
pub fn resolve_event_class_config(
    name: &str,
    config: &EventClassConfig,
) -> Result<ResolvedEventClassConfig, EventClassDeclareError> {
    if name.trim().is_empty() {
        return Err(EventClassDeclareError::EmptyName);
    }
    if config.schema_version.trim().is_empty() {
        return Err(EventClassDeclareError::EmptySchemaVersion {
            name: name.to_string(),
        });
    }

    let broadcast = config.broadcast;
    let channel = config.channel.unwrap_or(if broadcast {
        // Will fail validation below — broadcast requires explicit channel.
        EventClassChannelStrategy::Local
    } else {
        EventClassChannelStrategy::Local
    });

    if broadcast && channel == EventClassChannelStrategy::Local {
        return Err(EventClassDeclareError::BroadcastWithoutChannel {
            name: name.to_string(),
        });
    }
    if !broadcast && channel != EventClassChannelStrategy::Local {
        return Err(EventClassDeclareError::ChannelWithoutBroadcast {
            name: name.to_string(),
            channel,
        });
    }

    Ok(ResolvedEventClassConfig {
        name: name.to_string(),
        broadcast,
        channel,
        schema_version: config.schema_version.clone(),
        on_unknown_schema: config.on_unknown_schema.unwrap_or_default(),
        description: config.description.clone().unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg_minimal_local() -> EventClassConfig {
        EventClassConfig {
            broadcast: false,
            channel: None,
            schema_version: "v1".into(),
            on_unknown_schema: None,
            description: None,
        }
    }

    fn cfg_broadcast_global() -> EventClassConfig {
        EventClassConfig {
            broadcast: true,
            channel: Some(EventClassChannelStrategy::Global),
            schema_version: "v1".into(),
            on_unknown_schema: None,
            description: None,
        }
    }

    #[test]
    fn resolves_local_default() {
        let r = resolve_event_class_config("widget:mounted", &cfg_minimal_local()).unwrap();
        assert_eq!(r.name, "widget:mounted");
        assert!(!r.broadcast);
        assert_eq!(r.channel, EventClassChannelStrategy::Local);
        assert_eq!(r.schema_version, "v1");
        assert_eq!(r.on_unknown_schema, EventClassUnknownSchemaPolicy::Fail);
    }

    #[test]
    fn resolves_broadcast_global() {
        let r =
            resolve_event_class_config("presence:peer-manifest", &cfg_broadcast_global()).unwrap();
        assert!(r.broadcast);
        assert_eq!(r.channel, EventClassChannelStrategy::Global);
    }

    #[test]
    fn rejects_empty_name() {
        let err = resolve_event_class_config("", &cfg_minimal_local()).unwrap_err();
        assert!(matches!(err, EventClassDeclareError::EmptyName));
    }

    #[test]
    fn rejects_empty_schema_version() {
        let bad = EventClassConfig {
            schema_version: "".into(),
            ..cfg_minimal_local()
        };
        let err = resolve_event_class_config("foo:bar", &bad).unwrap_err();
        assert!(matches!(
            err,
            EventClassDeclareError::EmptySchemaVersion { .. }
        ));
    }

    #[test]
    fn rejects_broadcast_without_channel() {
        let bad = EventClassConfig {
            broadcast: true,
            channel: None,
            ..cfg_minimal_local()
        };
        let err = resolve_event_class_config("chat:posted", &bad).unwrap_err();
        assert!(matches!(
            err,
            EventClassDeclareError::BroadcastWithoutChannel { .. }
        ));
    }

    #[test]
    fn rejects_broadcast_with_local_channel() {
        let bad = EventClassConfig {
            broadcast: true,
            channel: Some(EventClassChannelStrategy::Local),
            ..cfg_minimal_local()
        };
        let err = resolve_event_class_config("chat:posted", &bad).unwrap_err();
        assert!(matches!(
            err,
            EventClassDeclareError::BroadcastWithoutChannel { .. }
        ));
    }

    #[test]
    fn rejects_channel_without_broadcast() {
        let bad = EventClassConfig {
            broadcast: false,
            channel: Some(EventClassChannelStrategy::Global),
            ..cfg_minimal_local()
        };
        let err = resolve_event_class_config("chat:posted", &bad).unwrap_err();
        assert!(matches!(
            err,
            EventClassDeclareError::ChannelWithoutBroadcast { .. }
        ));
    }

    #[test]
    fn defaults_on_unknown_schema_to_fail() {
        let r = resolve_event_class_config("foo:bar", &cfg_minimal_local()).unwrap();
        assert_eq!(r.on_unknown_schema, EventClassUnknownSchemaPolicy::Fail);
    }

    #[test]
    fn honors_explicit_on_unknown_schema_warn() {
        let cfg = EventClassConfig {
            on_unknown_schema: Some(EventClassUnknownSchemaPolicy::Warn),
            ..cfg_minimal_local()
        };
        let r = resolve_event_class_config("foo:bar", &cfg).unwrap();
        assert_eq!(r.on_unknown_schema, EventClassUnknownSchemaPolicy::Warn);
    }
}
