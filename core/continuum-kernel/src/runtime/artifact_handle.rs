//! Artifact handle, selector, and cadence — pure data layer for PIECE-2
//! of the CBAR substrate (artifact subscription, cadence, dependency
//! declarations that `ServiceModule` will adopt in PR-2).
//!
//! Carries no runtime wiring. PR-2 adds these as Optional fields on
//! `ModuleConfig` + a default `on_artifact_available` method on the
//! trait. PR-3 wires the runtime to deliver artifact events on the
//! configured cadence. This file ships the typed wire shape so PR-2
//! has stable types to depend on + downstream consumers can start
//! reasoning about subscriptions independently.
//!
//! ## What an artifact is
//!
//! An **artifact** is any named output a `ServiceModule` produces that
//! other modules can subscribe to. Concrete examples from the codebase:
//!
//! - `cognition/rate_proposals.result` — produced when rate_proposals
//!   IPC handler emits its scoring output. PR-2's persona module can
//!   subscribe and react.
//! - `paging/broker.snapshot` — produced each tick by PressureBroker.
//!   Modules reading global pressure subscribe rather than poll.
//! - `inference_capability/registry.update` — produced when
//!   GridCapabilityAnnouncer.ingest_peer mutates the registry. Lane D's
//!   `CognitionTurnFrame` can subscribe to know when remote inference
//!   capacity changed.
//!
//! ## Why no hardcoded enum
//!
//! Per CLAUDE.md anti-pattern rules + Joel's "we do not hardcode"
//! directive (vhsm-d1f4 audit pass 6): `ArtifactKind` is a `String`
//! newtype, not a `pub enum`. Modules register their own artifact
//! kinds at boot; the runtime doesn't carry a closed list. Adding a
//! new module's artifact stream MUST NOT require a schema change.
//!
//! Same shape used by `inference_capability::InferenceKind` (codex's
//! PR-1 of GRID-INFERENCE-ROUTING) — the convention is established and
//! this file follows it.
//!
//! ## Failure-mode discipline
//!
//! - **No silent defaults**: every field carries explicit data; no
//!   `Cadence::default()` that picks an arbitrary tick interval. The
//!   broker / supervisor decides cadence per the dynamic-hardware-detect
//!   rule.
//! - **No fixed concurrency**: there's no `max_subscribers` field. A
//!   subscription is a record, not a slot. Broker meters delivery
//!   downstream.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::Duration;
use ts_rs::TS;

/// Stable identifier for an artifact stream. Producer-side modules
/// declare a key when they publish; consumer-side modules name a key
/// when they subscribe.
///
/// Format convention (not enforced): `<module>/<surface>.<event>`. E.g.
/// `paging/broker.snapshot`, `cognition/rate_proposals.result`,
/// `inference_capability/registry.peer_announced`. The runtime does
/// not parse the structure — it's a string match. Convention is for
/// humans reading subscription lists, not the dispatcher.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(transparent)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/runtime/ArtifactKey.ts"
)]
pub struct ArtifactKey(pub String);

impl ArtifactKey {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for ArtifactKey {
    fn from(s: &str) -> Self {
        ArtifactKey(s.to_string())
    }
}

impl From<String> for ArtifactKey {
    fn from(s: String) -> Self {
        ArtifactKey(s)
    }
}

impl fmt::Display for ArtifactKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// What a subscriber wants to be notified about.
///
/// `Exact` — match one specific `ArtifactKey` (the common case).
/// `Prefix` — match every key starting with a string (e.g. a persona
///   module wanting every `cognition/*` artifact).
///
/// Glob/regex deliberately omitted: the matcher is the hot path the
/// runtime walks every publish, and string-prefix is cheap + covers
/// the cases we have. If a future module needs glob, it can compose
/// `Prefix` + filter in its own handler — keeps the matcher fast for
/// the 99% case.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", tag = "kind", content = "value")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/runtime/ArtifactSelector.ts"
)]
pub enum ArtifactSelector {
    Exact(ArtifactKey),
    Prefix(String),
}

impl ArtifactSelector {
    /// True iff this selector would deliver an artifact published
    /// under `key`. Cheap — string equality or `starts_with`.
    pub fn matches(&self, key: &ArtifactKey) -> bool {
        match self {
            ArtifactSelector::Exact(want) => key == want,
            ArtifactSelector::Prefix(prefix) => key.as_str().starts_with(prefix),
        }
    }
}

/// How the runtime should drive a module's work surface. PR-2 adds
/// this as an Optional field on `ModuleConfig`; modules that don't
/// declare a cadence keep their current behavior (purely reactive to
/// commands and events).
///
/// `Periodic(Duration)` — broker-paced tick at the given interval. The
///   runtime calls `tick()` at this cadence. Duration is the requested
///   floor — broker can stretch under pressure (no hardcoded ceiling
///   anywhere; broker decides per pressure state).
///
/// `EventDriven` — woken only when one of the module's
///   `event_subscriptions` fires. No periodic call. Lowest overhead
///   for modules that genuinely have nothing to do until something
///   external happens.
///
/// `OnArtifact` — woken when an artifact this module subscribes to is
///   published. Composes with subscriptions: subscriber list lives in
///   `ModuleConfig.artifact_subscriptions` (PR-2); cadence says "wake
///   me on those subscriptions, otherwise rest."
///
/// `Mixed` — periodic tick AND artifact wakes. For modules that
///   need a heartbeat (e.g. cache TTL eviction) plus reactive bursts.
///
/// Deliberately no `OnDemand` / `Manual` variant. Every supervised
/// task has a cadence policy the supervisor knows; a module that
/// truly never wakes shouldn't exist as a registered module.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", tag = "kind")]
#[ts(export, export_to = "../../../protocol/typescript/runtime/Cadence.ts")]
pub enum Cadence {
    Periodic {
        /// Requested floor on tick interval. ms over the wire so the
        /// TS side doesn't have to handle bigint Duration shape.
        #[serde(rename = "intervalMs")]
        #[ts(rename = "intervalMs", type = "number")]
        interval_ms: u64,
    },
    EventDriven,
    OnArtifact,
    Mixed {
        #[serde(rename = "intervalMs")]
        #[ts(rename = "intervalMs", type = "number")]
        interval_ms: u64,
    },
}

impl Cadence {
    /// Get the periodic tick interval if this cadence has one. Returns
    /// `None` for `EventDriven` / `OnArtifact` (no periodic wake).
    /// The runtime's `start_tick_loops` uses this to decide whether
    /// to spawn a tokio interval task for the module.
    pub fn tick_interval(&self) -> Option<Duration> {
        match self {
            Cadence::Periodic { interval_ms } | Cadence::Mixed { interval_ms } => {
                Some(Duration::from_millis(*interval_ms))
            }
            Cadence::EventDriven | Cadence::OnArtifact => None,
        }
    }

    /// True iff this cadence reacts to artifact publications. Runtime's
    /// artifact-dispatch path skips modules whose cadence returns false.
    pub fn wants_artifact_wakes(&self) -> bool {
        matches!(self, Cadence::OnArtifact | Cadence::Mixed { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── ArtifactKey ──────────────────────────────────────────────────

    /// What this catches: equality + hash + display are all string-based
    /// so a key can roundtrip through HashMap + log formatting without
    /// surprise.
    #[test]
    fn artifact_key_string_semantics() {
        let a = ArtifactKey::from("cognition/rate_proposals.result");
        let b = ArtifactKey::from("cognition/rate_proposals.result".to_string());
        let c = ArtifactKey::from("paging/broker.snapshot");
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(a.to_string(), "cognition/rate_proposals.result");
        assert_eq!(a.as_str(), "cognition/rate_proposals.result");
    }

    /// What this catches: serde transparent serializes as a bare string,
    /// not as `{"0": "..."}`. The wire format the TS side reads.
    #[test]
    fn artifact_key_serializes_as_string() {
        let k = ArtifactKey::from("paging/broker.snapshot");
        let json = serde_json::to_string(&k).unwrap();
        assert_eq!(json, "\"paging/broker.snapshot\"");
        let round: ArtifactKey = serde_json::from_str(&json).unwrap();
        assert_eq!(round, k);
    }

    // ─── ArtifactSelector ─────────────────────────────────────────────

    /// What this catches: Exact only matches identical keys, doesn't
    /// accidentally prefix-match. The matcher is the runtime's hot
    /// path; getting Exact wrong wakes every subscriber on every
    /// publish.
    #[test]
    fn selector_exact_matches_only_identical_key() {
        let sel = ArtifactSelector::Exact(ArtifactKey::from("paging/broker.snapshot"));
        assert!(sel.matches(&ArtifactKey::from("paging/broker.snapshot")));
        assert!(!sel.matches(&ArtifactKey::from("paging/broker.snapshot.delta")));
        assert!(!sel.matches(&ArtifactKey::from("paging/broker")));
        assert!(!sel.matches(&ArtifactKey::from("cognition/broker.snapshot")));
    }

    /// What this catches: Prefix matches by string-prefix, including
    /// the empty prefix (every key matches "") and including the
    /// degenerate case where the prefix equals the key.
    #[test]
    fn selector_prefix_matches_by_string_prefix() {
        let sel = ArtifactSelector::Prefix("cognition/".to_string());
        assert!(sel.matches(&ArtifactKey::from("cognition/rate_proposals.result")));
        assert!(sel.matches(&ArtifactKey::from("cognition/generate_recipe.result")));
        assert!(sel.matches(&ArtifactKey::from("cognition/")));
        assert!(!sel.matches(&ArtifactKey::from("paging/broker.snapshot")));
        assert!(!sel.matches(&ArtifactKey::from("Cognition/foo"))); // case-sensitive
    }

    /// What this catches: selector serde uses internally-tagged
    /// `{kind, value}` shape so TS consumers can pattern-match on
    /// .kind. Pinning the wire shape against accidental rename.
    #[test]
    fn selector_serializes_with_kind_tag() {
        let exact = ArtifactSelector::Exact(ArtifactKey::from("paging/broker.snapshot"));
        let json = serde_json::to_value(&exact).unwrap();
        assert_eq!(json["kind"], "exact");
        assert_eq!(json["value"], "paging/broker.snapshot");

        let prefix = ArtifactSelector::Prefix("cognition/".to_string());
        let json = serde_json::to_value(&prefix).unwrap();
        assert_eq!(json["kind"], "prefix");
        assert_eq!(json["value"], "cognition/");
    }

    // ─── Cadence ──────────────────────────────────────────────────────

    /// What this catches: tick_interval projects Duration only for
    /// variants that have one. EventDriven / OnArtifact have no
    /// periodic wake; spawning an interval task for them is the bug.
    #[test]
    fn cadence_tick_interval_projection() {
        assert_eq!(
            Cadence::Periodic { interval_ms: 5000 }.tick_interval(),
            Some(Duration::from_millis(5000))
        );
        assert_eq!(
            Cadence::Mixed { interval_ms: 1000 }.tick_interval(),
            Some(Duration::from_millis(1000))
        );
        assert_eq!(Cadence::EventDriven.tick_interval(), None);
        assert_eq!(Cadence::OnArtifact.tick_interval(), None);
    }

    /// What this catches: wants_artifact_wakes is true only for the
    /// variants that opt into artifact delivery. The runtime's
    /// artifact dispatch walks `wants_artifact_wakes` modules; getting
    /// this wrong either delivers nothing (silent drop) or wakes
    /// every module on every publish (spam).
    #[test]
    fn cadence_artifact_wake_semantics() {
        assert!(Cadence::OnArtifact.wants_artifact_wakes());
        assert!(Cadence::Mixed { interval_ms: 100 }.wants_artifact_wakes());
        assert!(!Cadence::EventDriven.wants_artifact_wakes());
        assert!(!Cadence::Periodic { interval_ms: 5000 }.wants_artifact_wakes());
    }

    /// What this catches: Cadence serde uses internally-tagged
    /// `{kind, ...}` shape; the unit variants serialize as just
    /// `{"kind": "..."}` (no value), the struct variants include
    /// their fields inline. TS consumers pattern-match on .kind.
    #[test]
    fn cadence_serializes_with_kind_tag() {
        let periodic = Cadence::Periodic { interval_ms: 5000 };
        let json = serde_json::to_value(&periodic).unwrap();
        assert_eq!(json["kind"], "periodic");
        assert_eq!(json["intervalMs"], 5000);

        let event_driven = Cadence::EventDriven;
        let json = serde_json::to_value(&event_driven).unwrap();
        assert_eq!(json["kind"], "eventDriven");
        assert!(json.get("intervalMs").is_none());

        let on_artifact = Cadence::OnArtifact;
        let json = serde_json::to_value(&on_artifact).unwrap();
        assert_eq!(json["kind"], "onArtifact");

        let mixed = Cadence::Mixed { interval_ms: 1000 };
        let json = serde_json::to_value(&mixed).unwrap();
        assert_eq!(json["kind"], "mixed");
        assert_eq!(json["intervalMs"], 1000);
    }

    /// What this catches: roundtrip — every variant survives
    /// serialization. Catches the variant we forget when extending
    /// the enum.
    #[test]
    fn cadence_roundtrip_every_variant() {
        for original in [
            Cadence::Periodic { interval_ms: 250 },
            Cadence::EventDriven,
            Cadence::OnArtifact,
            Cadence::Mixed { interval_ms: 7500 },
        ] {
            let json = serde_json::to_string(&original).unwrap();
            let back: Cadence = serde_json::from_str(&json).unwrap();
            assert_eq!(back, original, "roundtrip lost {original:?} via {json}");
        }
    }
}
