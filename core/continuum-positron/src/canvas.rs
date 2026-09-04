//! Typed canvas payload — `CanvasViewState`, the ninth ViewState: the run
//! room's live-artifact region (DESIGN-BENCH-VISUAL-CRAFT.md §5 — the
//! persona's RENDERED page, re-observed on her writes; the walk-in sees the
//! design evolve). Joel, 2026-08-23: "you'll want to actually see live
//! persona doing benchmark work in the desktop app too, all the visibility."
//!
//! Same define-once discipline as `bench.rs`: the core folds the persona's
//! own `perception/observe` / `perception/hot-edit` RESULTS — the exact
//! observation her mind acted on — into this snapshot, so the web canvas
//! face, a TUI, and a teammate's grounding all render the SAME frame.
//! Field names are the wire contract `chat-view/canvasProjections.ts`
//! already speaks (snake_case, every field optional): an empty snapshot IS
//! the pre-first-observation room, never a fabricated frame.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// One graded oracle check as it rides the wire (§3 tiers: `v1` structure
/// UiCheck, `v2` measured-craft StyleCheck).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/positron/CanvasCheckRow.ts"
)]
pub struct CanvasCheckRow {
    pub name: String,
    /// `"v1"` | `"v2"` — the oracle tier.
    pub tier: String,
    pub passed: bool,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    pub detail: Option<String>,
}

/// The live-feed snapshot for a canvas region (`kind: "canvas"`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/positron/CanvasViewState.ts"
)]
pub struct CanvasViewState {
    /// The artifact's display name ("index.html — pricing card").
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    pub artifact_title: Option<String>,
    /// The page as inline self-contained HTML (the persona's actual writing).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    pub artifact_html: Option<String>,
    /// The artifact's URL when it is served rather than inlined.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    pub artifact_url: Option<String>,
    /// Last-observed screenshot as a data URL — pixels-only fallback.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    pub screenshot_data_url: Option<String>,
    /// The observing citizen's display name.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    pub persona: Option<String>,
    /// Epoch ms of the last observation.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional, type = "number")]
    pub observed_at_ms: Option<u64>,
    /// The observation's viewport, when the observe carried one.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    pub viewport: Option<CanvasViewport>,
    /// Observation count for this artifact (ticks as she iterates).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional, type = "number")]
    pub revision: Option<u32>,
    /// Graded oracle checks (V1·V2 gates), when the room's oracle has run.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    pub checks: Option<Vec<CanvasCheckRow>>,
    /// The V3 judge-panel objective 0..=1, when a panel has scored.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional, type = "number")]
    pub judge: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/positron/CanvasViewport.ts"
)]
pub struct CanvasViewport {
    #[ts(type = "number")]
    pub width: u32,
    #[ts(type = "number")]
    pub height: u32,
}

impl CanvasViewState {
    /// The on-wire `kind` (open self-registration, not a central enum).
    pub const KIND: &'static str = "canvas";
}

impl positron_core::ViewState for CanvasViewState {
    fn kind(&self) -> &'static str {
        Self::KIND
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the "canvas" kind string drifting from the trait, and
    // the wire keys drifting from what chat-view's canvasProjections speaks —
    // snake_case, optionals absent (not null) when unset. A key rename here is
    // a blank canvas face with no compile error anywhere.
    #[test]
    fn kind_is_stable_and_wire_keys_are_the_projection_contract() {
        use positron_core::ViewState as _;
        let view = CanvasViewState {
            artifact_title: Some("index.html — pricing card".into()),
            observed_at_ms: Some(5),
            revision: Some(3),
            ..Default::default()
        };
        assert_eq!(view.kind(), "canvas");
        let wire = serde_json::to_value(&view).expect("serializes");
        assert_eq!(wire["artifact_title"], "index.html — pricing card");
        assert_eq!(wire["observed_at_ms"], 5);
        assert_eq!(wire["revision"], 3);
        assert!(wire.get("artifact_html").is_none(), "absent stays absent, never null");
        let empty = serde_json::to_value(CanvasViewState::default()).expect("serializes");
        assert_eq!(empty, serde_json::json!({}), "empty snapshot is the honest awaiting frame");
    }
}
