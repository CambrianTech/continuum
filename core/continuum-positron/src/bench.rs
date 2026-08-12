//! Typed benchmark-board payload — `BenchViewState`, the substrate-shaped
//! view of the node's LIVE benchmark runs (#329: a benchmark IS a live room;
//! the run rows ARE the panel). Joel, 2026-08-12: the board doubles as the
//! efficiency instrument — "it will also help us really see what's going on".
//!
//! Same define-once discipline as `serving.rs`: the core emitter folds the ONE
//! run-ledger projection (`benchmark/runs`' own scan — never a parallel file
//! scrape) into rows, so the web rail widget, a TUI board, and a teacher
//! persona's grounding all render the SAME facts, and reconnect resyncs the
//! board instead of starting blank.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// One benchmark run — one board row. Mirrors the fields `BenchRunCard`
/// (the command projection) carries; every field here is REAL ledger data,
/// absent when the ledger hasn't written it yet (honest absence — a queued
/// run renders as queued, never dressed as work).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/positron/BenchRunRow.ts")]
pub struct BenchRunRow {
    pub run_id: String,
    /// Instance under test ("sympy__sympy-24066"); absent on non-SWE runs.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    pub instance: Option<String>,
    /// Solver persona id; absent while attempt 1 hasn't journaled.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    pub solver: Option<String>,
    /// `resolved` | `failed` | `active` | `quiet` — the projection's phases.
    pub phase: String,
    /// True exactly when `phase == "quiet"` (stall-window silence).
    pub stalled: bool,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional, type = "number")]
    pub attempt: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional, type = "number")]
    pub max_attempts: Option<u32>,
    /// Seconds since the newest artifact write — the pulse.
    #[ts(type = "number")]
    pub age_secs: u64,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional, type = "number")]
    pub acts: Option<u32>,
    /// Graded diff bytes when a grade exists; the result's own live diff
    /// length before that (the "patch is forming" leading indicator).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional, type = "number")]
    pub patch_bytes: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    pub resolved: Option<bool>,
    /// "passed/total" strings, render-ready ("1/1", "38/40").
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    pub fail_to_pass: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    pub pass_to_pass: Option<String>,
    /// Failed test NAMES (capped upstream) — a verdict that can teach.
    pub failed_tests: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    pub infra_error: Option<String>,
}

/// The benchmark board — what the ACADEMY right-rail widget draws.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/positron/BenchViewState.ts")]
pub struct BenchViewState {
    /// Rows, most recently active first, bounded at the emitter. EMPTY =
    /// no runs on this node — the awaiting frame, never a fabricated row.
    pub runs: Vec<BenchRunRow>,
    /// Emitter cadence in ms so renderers label freshness from data.
    #[ts(type = "number")]
    pub sample_interval_ms: u64,
}

impl BenchViewState {
    /// The on-wire `kind` this view is published under (open
    /// self-registration, not a central enum).
    pub const KIND: &'static str = "bench";
}

impl positron_core::ViewState for BenchViewState {
    fn kind(&self) -> &'static str {
        Self::KIND
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the "bench" kind string never drifts from the trait,
    // and the empty view is honest (no rows) — a widget rendering it shows the
    // awaiting frame, never a fabricated run.
    #[test]
    fn kind_is_stable_and_empty_view_is_honest() {
        use positron_core::ViewState;
        let view = BenchViewState { runs: vec![], sample_interval_ms: 5000 };
        assert_eq!(view.kind(), "bench");
        assert_eq!(BenchViewState::KIND, "bench");
        assert!(view.runs.is_empty());
        // Optional fields elide from the wire — the TS optional contract.
        let row = BenchRunRow {
            run_id: "r1".into(),
            instance: None,
            solver: None,
            phase: "active".into(),
            stalled: false,
            attempt: None,
            max_attempts: None,
            age_secs: 3,
            acts: None,
            patch_bytes: None,
            resolved: None,
            fail_to_pass: None,
            pass_to_pass: None,
            failed_tests: vec![],
            infra_error: None,
        };
        let wire = serde_json::to_value(&row).expect("serialize");
        assert!(wire.get("instance").is_none(), "absent facts elide, never null-fabricate");
    }
}
