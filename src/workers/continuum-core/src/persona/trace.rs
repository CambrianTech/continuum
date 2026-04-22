//! `CognitionTrace` — per-turn time-series of every seam a persona's
//! `respond()` call passes through.
//!
//! # Why this exists
//!
//! "We need our mechanic's meters, EE's oscilloscope, programmer's test
//! bench, F1 racer's re-simulation, on every persona, isolatable at any
//! level so we can replay or even use this to break and train."
//! — Joel, 2026-04-22.
//!
//! `CognitionTrace` is the spine of that. Each seam in the cognition
//! pipeline (analyze, prompt assembly, inference, post-process)
//! appends a `TraceSeam` with its name, start time, duration, and
//! seam-specific metadata. The completed trace serializes into the
//! turn record alongside the request/response, so a captured trace +
//! the captured request reconstructs WHAT happened AND HOW LONG each
//! step took, no live system needed.
//!
//! # Design
//!
//! - **Value object, not a service**. Created per call, dropped per
//!   call. No global state, no async machinery, no locks.
//! - **Thread-affined**. Owned by the cognition turn that created it.
//!   `respond()` is a `tokio::task` per persona; no two tasks share a
//!   trace. Borrow it `&mut` through helper functions; no `Arc<Mutex>`
//!   needed, no contention.
//! - **Open-vocabulary metadata**. Each seam carries a
//!   `serde_json::Value` of seam-specific fields (analyze records
//!   `from_cache: bool`, inference records `model: String`, etc.).
//!   Adding a new field doesn't touch the trace type.
//! - **Source-time strings for seam names**. `&'static str` instead of
//!   enum so new seams (recipe-specific later) don't require enum
//!   churn. Cost: typo-by-string, mitigated by the seam-name constants
//!   below.
//!
//! # Seam name conventions
//!
//! Use the constants in this module (`SEAM_*`) when emitting from the
//! known cognition path. Recipe-specific seams (added in Phase B+)
//! supply their own string at the call site.

use serde::{Deserialize, Serialize};
use std::time::SystemTime;

/// Standard seam names used by `respond()` and the in-process
/// cognition pipeline. Recipes / hosts adding their own seams supply
/// their own string at the call site — no enum churn required.
pub const SEAM_ANALYZE: &str = "analyze";
pub const SEAM_PROMPT_ASSEMBLY: &str = "prompt_assembly";
pub const SEAM_INFERENCE: &str = "inference";
pub const SEAM_POST_PROCESS: &str = "post_process";

/// One entry in the per-turn trace. Captures the seam's identity, when
/// it ran, how long it took, and an open-vocabulary `metadata` blob
/// for seam-specific signals (e.g. `analyze` records `from_cache`,
/// `inference` records `model_used`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceSeam {
    /// Seam identifier — see `SEAM_*` constants.
    pub name: String,
    /// Wall-clock start of the seam, ms since UNIX_EPOCH.
    pub started_at_ms: u64,
    /// Time spent in the seam, ms.
    pub duration_ms: u64,
    /// Seam-specific signals (cache hits, model id, token counts, etc.).
    /// Empty `{}` is fine — metadata is optional, the seam record
    /// itself is what matters for timing.
    pub metadata: serde_json::Value,
}

/// Per-turn trace. Created at the start of `respond()`, populated as
/// each seam runs, sealed at the end and handed to the recorder.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CognitionTrace {
    /// Wall-clock start of the turn, ms since UNIX_EPOCH. Acts as the
    /// trace's own start anchor — individual seams' `started_at_ms`
    /// MAY be after this if the host did setup work before invoking
    /// the cognition path.
    pub turn_started_at_ms: u64,
    /// Seams in chronological emission order.
    pub seams: Vec<TraceSeam>,
}

impl CognitionTrace {
    /// Start a fresh trace anchored at the current wall-clock time.
    pub fn new() -> Self {
        Self {
            turn_started_at_ms: now_ms(),
            seams: Vec::new(),
        }
    }

    /// Record a seam given an absolute start time + duration. Use
    /// when you've measured the duration yourself (e.g. with
    /// `Instant::now() ... elapsed()`).
    pub fn record(
        &mut self,
        name: &str,
        started_at_ms: u64,
        duration_ms: u64,
        metadata: serde_json::Value,
    ) {
        self.seams.push(TraceSeam {
            name: name.to_string(),
            started_at_ms,
            duration_ms,
            metadata,
        });
    }

    /// Total time across the trace = now() − turn start. Useful at
    /// the end of a turn for the outermost timing entry.
    pub fn total_duration_ms(&self) -> u64 {
        now_ms().saturating_sub(self.turn_started_at_ms)
    }
}

impl Default for CognitionTrace {
    fn default() -> Self {
        Self::new()
    }
}

/// Wall-clock ms since UNIX_EPOCH. Single source of truth for the
/// trace timestamps so seams compare apples-to-apples.
pub(crate) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What this catches: a fresh trace must have zero seams and a
    /// reasonable timestamp anchor (within seconds of "now"). Trivial
    /// but the regression baseline for "trace just got constructed".
    #[test]
    fn new_trace_starts_empty_with_recent_anchor() {
        let trace = CognitionTrace::new();
        assert!(trace.seams.is_empty());
        let now = now_ms();
        assert!(
            trace.turn_started_at_ms <= now && now - trace.turn_started_at_ms < 5_000,
            "anchor should be within 5s of now"
        );
    }

    /// What this catches: seams append in emission order. A trace
    /// reader downstream relies on this for timing reconstruction —
    /// reordering would break causality assertions in replay.
    #[test]
    fn seams_preserve_emission_order() {
        let mut trace = CognitionTrace::new();
        trace.record(SEAM_ANALYZE, 1000, 50, serde_json::json!({"from_cache": false}));
        trace.record(SEAM_INFERENCE, 1100, 1500, serde_json::json!({"model": "qwen"}));
        trace.record(SEAM_POST_PROCESS, 2700, 2, serde_json::json!({}));
        assert_eq!(trace.seams.len(), 3);
        assert_eq!(trace.seams[0].name, SEAM_ANALYZE);
        assert_eq!(trace.seams[1].name, SEAM_INFERENCE);
        assert_eq!(trace.seams[2].name, SEAM_POST_PROCESS);
    }

    /// What this catches: metadata round-trips through JSON cleanly,
    /// preserving keys + nested values. The recorder serializes the
    /// whole trace to disk; loss of metadata would silently strip
    /// signal from captured turns.
    #[test]
    fn metadata_round_trips_through_serde() {
        let mut trace = CognitionTrace::new();
        trace.record(
            SEAM_ANALYZE,
            1000,
            50,
            serde_json::json!({
                "from_cache": true,
                "intent": {"category": "question", "confidence": 0.87}
            }),
        );
        let json = serde_json::to_string(&trace).expect("serializes");
        let back: CognitionTrace = serde_json::from_str(&json).expect("round-trips");
        assert_eq!(back.seams[0].metadata["from_cache"], serde_json::json!(true));
        assert_eq!(back.seams[0].metadata["intent"]["category"], serde_json::json!("question"));
    }

    /// What this catches: `total_duration_ms()` returns elapsed since
    /// turn start. If the field name or computation drifts, dashboards
    /// downstream report wrong durations.
    #[test]
    fn total_duration_increases_after_anchor() {
        let trace = CognitionTrace::new();
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert!(
            trace.total_duration_ms() >= 15,
            "total should be >=15ms after a 20ms sleep"
        );
    }
}
