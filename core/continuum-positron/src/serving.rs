//! Typed serving glass-box payload — `ServingViewState`, the substrate-shaped
//! view of the node's LIVE inference serving: what model is up, and when the
//! MoE expert pager is streaming, the per-token control loop itself (hit rate,
//! tok/s, fetch bandwidth, bandit arms, pager events) — the beat-WASTE
//! campaign made visible (#141 first slice, greenlit 2026-08-01).
//!
//! Same define-once discipline as `system_metrics.rs`: the core emitter folds
//! the ONE serving snapshot + the ONE capture feed into bounded rings, so web
//! sparkline / terminal bars / a persona's grounding line all render the SAME
//! series, and reconnect resyncs the window instead of starting blank.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::system_metrics::MetricSeriesView;

/// The serving header line — folded from the daemon's `ServingSnapshot`
/// (the process's own truth, never a recomputed plan value).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/positron/ServingHeaderView.ts")]
pub struct ServingHeaderView {
    /// Model id currently served. `None` = nothing live (honest empty) —
    /// omitted from the wire so the TS optional (`model?: string`) is true.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    pub model: Option<String>,
    /// True once /health answered for the active model.
    pub ready: bool,
    /// `--parallel` lane count (0 on the empty snapshot).
    #[ts(type = "number")]
    pub lanes: u32,
    /// REAL per-slot context window from the server's own /props.
    #[ts(type = "number")]
    pub context_window: u32,
    /// WHY nothing serves, when degraded — verbatim, never swallowed.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    pub degraded_reason: Option<String>,
}

/// One bandit arm's belief — the learned-decay dial the pager serves with.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/positron/ServingArmView.ts")]
pub struct ServingArmView {
    /// The arm's decay constant as its display label ("0.99", "0.30"…).
    pub label: String,
    /// EMA reward — the bandit's belief state for this arm.
    pub reward: f32,
    /// True for the arm currently serving predictions.
    pub chosen: bool,
}

/// One pager event card — the discrete moments of the control loop, rendered
/// as activity cards (room = the activity's full event stream doctrine).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/positron/ServingEventCard.ts")]
pub struct ServingEventCard {
    /// Decode-token index the event fired at.
    #[ts(type = "number")]
    pub at_token: u64,
    /// Event class: "serve-start" | "decay-switch" | "residency-shift".
    pub kind: String,
    /// Human-readable one-liner, formatted at the source.
    pub detail: String,
}

/// The serving glass box — what the SERVING panel draws.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/positron/ServingViewState.ts")]
pub struct ServingViewState {
    /// The serving header. `None` before the daemon has ever published —
    /// honest unknown, never a fabricated "ready".
    #[ts(optional)]
    pub header: Option<ServingHeaderView>,
    /// Pager time-series (hit %, tok/s, fetch MB/s), in display order.
    /// EMPTY when no capture feed is live — absence over fabrication.
    pub series: Vec<MetricSeriesView>,
    /// The bandit's per-arm belief state; empty until the Rust controller
    /// feed carries decision fields.
    pub arms: Vec<ServingArmView>,
    /// Recent pager event cards, oldest → newest, bounded.
    pub events: Vec<ServingEventCard>,
    /// Emitter cadence in ms so renderers label the window from data.
    #[ts(type = "number")]
    pub sample_interval_ms: u64,
}

impl ServingViewState {
    /// The on-wire `kind` this view is published under (open
    /// self-registration, not a central enum).
    pub const KIND: &'static str = "serving";
}

impl positron_core::ViewState for ServingViewState {
    fn kind(&self) -> &'static str {
        Self::KIND
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the "serving" kind string never drifts from the
    // trait, and the empty view is honest (no header, no series) — a widget
    // that renders it shows "nothing serving", never a fabricated gauge.
    #[test]
    fn kind_is_stable_and_empty_view_is_honest() {
        use positron_core::ViewState;
        let view = ServingViewState {
            header: None,
            series: Vec::new(),
            arms: Vec::new(),
            events: Vec::new(),
            sample_interval_ms: 2000,
        };
        assert_eq!(view.kind(), ServingViewState::KIND);
        assert_eq!(view.kind(), "serving");
        assert!(view.header.is_none() && view.series.is_empty());
    }
}
