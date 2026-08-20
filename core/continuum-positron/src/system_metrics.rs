//! Typed system-metrics payload — `SystemMetricsViewState`, the substrate-shaped
//! view of the NODE's live resource use (CPU / memory / GPU when present) that
//! fills `StateEnvelope.payload` for `kind="system-metrics"`.
//!
//! ## The Gauge brick (POSITRON-WIDGET-SOPHISTICATION.md brick 2)
//!
//! The old sidebar's SYS graph (CPU 58% · MEM 25.3/32G · GPU 6.5/25G with a
//! sparkline) reborn as a define-once view: the core emitter samples the ONE
//! shared `SystemResourceMonitor` (never a second probe —
//! CONCURRENCY-STYLE-GUIDE) and carries a short ring of normalized samples, so
//! every surface renders the SAME series — a web sparkline, a terminal bar
//! strip, a persona's `CPU 58% · MEM 25/32G` grounding line. Core-carried
//! history (not client-accumulated) is what makes reconnect/resync and the RAG
//! view consistent with the pixels ([[eval-measures-the-true-full-being]] spirit:
//! one source, many renderers).
//!
//! ## Why structs, not `serde_json::Value`
//!
//! Same rationale as `chat.rs`/`nav.rs` ([[strong-typing-across-boundaries]]):
//! these ARE the schema; ts-rs mirrors them; the widget reads typed objects.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// One named resource series — a rolling window of normalized samples plus the
/// display-ready current reading. `points` are 0–100 percentages (newest last)
/// so every renderer draws without unit math; `current` is the human/persona
/// string ("58%", "25.3/32G") the legend and the RAG line share.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/positron/MetricSeriesView.ts")]
pub struct MetricSeriesView {
    /// Short uppercase-able label ("cpu", "mem", "gpu").
    pub label: String,
    /// Rolling normalized samples, 0–100, oldest → newest. Bounded by the
    /// emitter's window; a fresh boot honestly starts short.
    pub points: Vec<f32>,
    /// Display-ready current reading ("58%", "25.3/32G") — formatted at the
    /// source so web legend and persona grounding can never disagree.
    pub current: String,
}

/// The node's live resource view — what the SYS gauge draws.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/positron/SystemMetricsViewState.ts")]
pub struct SystemMetricsViewState {
    /// The series, in display order (cpu, mem, gpu…). A resource the node
    /// doesn't have (no GPU) is ABSENT, never a fabricated flat line.
    pub series: Vec<MetricSeriesView>,
    /// Sample cadence in milliseconds — lets a renderer label the window span
    /// ("last 3 min") from the data instead of hardcoding it.
    #[ts(type = "number")]
    pub sample_interval_ms: u64,
    /// The producing node's host name — the identity line of the "nodes
    /// online" strip. `None` = the OS reported none (honest unknown, never a
    /// fabricated name). `#[serde(default)]` so a view serialized before this
    /// field folds as absent, never dropped.
    #[serde(default)]
    #[ts(optional)]
    pub node: Option<String>,
}

impl SystemMetricsViewState {
    /// The on-wire `kind` string this view is published under. Owned by the
    /// view (open self-registration), NOT a central enum.
    pub const KIND: &'static str = "system-metrics";
}

impl positron_core::ViewState for SystemMetricsViewState {
    fn kind(&self) -> &'static str {
        Self::KIND
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the "system-metrics" kind string never drifts from the
    // trait's view of it, and absence-over-fabrication holds (no series ≠ error).
    #[test]
    fn kind_is_stable_and_empty_series_is_honest() {
        use positron_core::ViewState;
        let view =
            SystemMetricsViewState { series: Vec::new(), sample_interval_ms: 2000, node: None };
        assert_eq!(view.kind(), SystemMetricsViewState::KIND);
        assert_eq!(view.kind(), "system-metrics");
        assert!(view.series.is_empty());
    }
}
