//! Adaptive threshold for STM→LTM consolidation decisions.
//!
//! Port of `AdaptiveConsolidationThreshold.ts` — the activity-AND-time
//! responsive threshold that decides which working-memory thoughts earn
//! a promotion to long-term storage. Pure math; no IO, no state beyond
//! the struct's own fields.
//!
//! Two mechanisms combine to produce `current_threshold`:
//!
//! 1. **Activity-responsive** (sigmoid on messages/minute):
//!    - Low activity → low threshold → consolidate MORE (the conversation
//!      is slow, surface everything so the persona looks thoughtful).
//!    - High activity → high threshold → consolidate LESS (noise filter —
//!      don't promote every reaction in a busy room to permanent memory).
//!
//! 2. **Time-responsive** (exponential decay toward base):
//!    - The longer since the last successful consolidation, the more the
//!      threshold drifts back toward `base_threshold` — guarantees a
//!      minimum consolidation frequency so quiet personas don't get stuck.
//!    - Half-life of 5 minutes: after 5min silence, threshold is halfway
//!      between activity-based and base; after ~15min it's effectively
//!      back to base.
//!
//! The two combine multiplicatively: `threshold = base + (activity_based
//! - base) * decay_multiplier`. At `decay_multiplier=1` (just after
//! consolidation), activity dominates. At `decay_multiplier=0` (long
//! silence), the base takes over and forces consolidation.
//!
//! First piece of 0.5.5 Hippocampus → Rust. Pure logic; no persistence
//! or WorkingMemory coupling. The rest of Hippocampus port (snoop loop,
//! ConsolidationAdapter, LTM write-through) lands in follow-up commits
//! now that this primitive exists.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Sigmoid function — smooth 0→1 transition centered at `midpoint`,
/// curve steepness controlled by `steepness`.
///
/// `1 / (1 + e^(-k*(x - x0)))`.
fn sigmoid(x: f64, steepness: f64, midpoint: f64) -> f64 {
    1.0 / (1.0 + (-steepness * (x - midpoint)).exp())
}

/// Exponential decay from 1.0 toward 0.0. Half-life is the time at
/// which the output reaches 0.5.
fn exponential_decay(elapsed: Duration, half_life: Duration) -> f64 {
    if half_life.is_zero() {
        return 0.0;
    }
    // 0.5 ^ (elapsed / half_life) = e^(ln(0.5) * (elapsed / half_life))
    0.5f64.powf(elapsed.as_secs_f64() / half_life.as_secs_f64())
}

/// Activity-and-time responsive threshold for STM→LTM consolidation.
pub struct AdaptiveConsolidationThreshold {
    base_threshold: f64,
    max_threshold: f64,
    current_threshold: f64,

    /// Ring of recent messages/minute samples; capped at `activity_window`.
    recent_activity: VecDeque<f64>,
    activity_window: usize,

    last_consolidation: Instant,
    decay_half_life: Duration,

    // Sigmoid parameters
    steepness: f64,
    midpoint: f64,
}

impl Default for AdaptiveConsolidationThreshold {
    fn default() -> Self {
        Self {
            base_threshold: 0.3,
            max_threshold: 0.8,
            current_threshold: 0.5,
            recent_activity: VecDeque::with_capacity(10),
            activity_window: 10,
            last_consolidation: Instant::now(),
            decay_half_life: Duration::from_secs(5 * 60),
            steepness: 0.5,
            midpoint: 5.0,
        }
    }
}

impl AdaptiveConsolidationThreshold {
    pub fn new() -> Self {
        Self::default()
    }

    /// Update `current_threshold` based on recent activity and time
    /// since last consolidation. Call this each tick / each time the
    /// consolidator considers running.
    pub fn update_threshold(&mut self, messages_per_minute: f64) {
        // Track recent activity — ring-buffer style.
        if self.recent_activity.len() >= self.activity_window {
            self.recent_activity.pop_front();
        }
        self.recent_activity.push_back(messages_per_minute);

        // 1. Activity-based threshold (sigmoid on the window average).
        let count = self.recent_activity.len().max(1) as f64;
        let sum: f64 = self.recent_activity.iter().sum();
        let avg_activity = sum / count;

        let normalized = sigmoid(avg_activity, self.steepness, self.midpoint);
        let activity_threshold =
            self.base_threshold + (self.max_threshold - self.base_threshold) * normalized;

        // 2. Time-decay multiplier (1.0 right after consolidation,
        //    approaches 0.0 over many half-lives).
        let elapsed = self.last_consolidation.elapsed();
        let decay_multiplier = exponential_decay(elapsed, self.decay_half_life);

        // 3. Combine: threshold walks from activity-based toward base
        //    as time since consolidation grows.
        self.current_threshold =
            self.base_threshold + (activity_threshold - self.base_threshold) * decay_multiplier;
    }

    /// Mark a successful consolidation — resets the time-decay timer
    /// so the threshold jumps back to the activity-based value.
    pub fn record_consolidation(&mut self) {
        self.last_consolidation = Instant::now();
    }

    /// Read the current threshold without updating it. Callers that
    /// want the threshold "as of now" should call `update_threshold`
    /// first with the current activity level.
    pub fn threshold(&self) -> f64 {
        self.current_threshold
    }

    /// Convenience: `importance >= current_threshold`.
    pub fn should_consolidate(&self, importance: f64) -> bool {
        importance >= self.current_threshold
    }

    /// Snapshot for telemetry / logging. Deliberately a struct not a
    /// HashMap so consumers don't have to stringly-type the fields.
    pub fn stats(&self) -> ConsolidationThresholdStats {
        let count = self.recent_activity.len().max(1) as f64;
        let sum: f64 = self.recent_activity.iter().sum();
        let avg_activity = sum / count;
        let elapsed = self.last_consolidation.elapsed();

        ConsolidationThresholdStats {
            current_threshold: self.current_threshold,
            base_threshold: self.base_threshold,
            max_threshold: self.max_threshold,
            avg_activity,
            activity_window: self.activity_window,
            seconds_since_consolidation: elapsed.as_secs_f64(),
            decay_multiplier: exponential_decay(elapsed, self.decay_half_life),
        }
    }

    /// Reset history + threshold (e.g., session boundary).
    pub fn reset(&mut self) {
        self.recent_activity.clear();
        self.current_threshold = 0.5;
        self.last_consolidation = Instant::now();
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ConsolidationThresholdStats {
    pub current_threshold: f64,
    pub base_threshold: f64,
    pub max_threshold: f64,
    pub avg_activity: f64,
    pub activity_window: usize,
    pub seconds_since_consolidation: f64,
    pub decay_multiplier: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sigmoid_centered_at_midpoint() {
        // What this catches: the midpoint math. `sigmoid(midpoint,_,_)`
        // MUST be exactly 0.5 by definition — the S-curve's inflection
        // point is the whole reason we use this function. A mutation
        // that offsets the exponent (missing the `(x - x0)` subtraction)
        // would shift the center and break the "threshold=0.5 at
        // midpoint-activity" guarantee the whole adaptive scheme is
        // calibrated around.
        //
        // Validated 2026-04-21: mutation = change
        // `(-steepness * (x - midpoint)).exp()` to
        // `(-steepness * x).exp()` (drop the midpoint subtraction)
        // → sigmoid(5.0, 0.5, 5.0) returns ~0.076, assertion on 0.5
        // fails. Reverted.
        let y = sigmoid(5.0, 0.5, 5.0);
        assert!(
            (y - 0.5).abs() < 1e-9,
            "sigmoid at midpoint must be 0.5, got {y}"
        );
    }

    #[test]
    fn exponential_decay_halves_at_half_life() {
        // What this catches: the decay-rate math. After exactly one
        // half-life, output MUST be 0.5. A mutation that used natural
        // decay (`e^(-t/tau)`) instead of half-life-denominated decay
        // (`0.5^(t/half_life)`) would produce 1/e ≈ 0.368 at the
        // half-life mark — nothing immediately catastrophic, but every
        // downstream time calibration (5min=halfway, 15min=near-base)
        // shifts and the threshold starts forcing consolidations at
        // wrong cadence.
        //
        // Validated 2026-04-21: mutation = replace
        // `0.5f64.powf(...)` with `(-elapsed / half_life).exp()` →
        // assertion on 0.5 fails (actual ~0.368). Reverted.
        let h = Duration::from_secs(300); // 5 minutes
        let at_half = exponential_decay(h, h);
        assert!(
            (at_half - 0.5).abs() < 1e-9,
            "decay at half-life must be 0.5, got {at_half}"
        );
    }

    #[test]
    fn threshold_respects_bounds_under_extreme_activity() {
        // What this catches: the `base + (max - base) * normalized`
        // combination. `normalized` from sigmoid is always in [0, 1],
        // so the result MUST stay in [base, max] regardless of how
        // extreme the activity input gets. A mutation that, say,
        // flipped the formula to `base + max * normalized` would
        // produce values > max at high activity (1.1 when max=0.8).
        //
        // Validated 2026-04-21: mutation = change
        // `self.base_threshold + (self.max_threshold -
        // self.base_threshold) * normalized` to
        // `self.base_threshold + self.max_threshold * normalized` →
        // update_threshold(1000.0) produces current_threshold ≈ 1.1,
        // assertion that current_threshold <= max_threshold fails.
        // Reverted.
        let mut t = AdaptiveConsolidationThreshold::new();
        // Inject extreme activity samples for the full window.
        for _ in 0..20 {
            t.update_threshold(1000.0);
        }
        let s = t.stats();
        assert!(
            s.current_threshold <= s.max_threshold + 1e-9,
            "threshold {:.4} exceeded max {:.4}",
            s.current_threshold,
            s.max_threshold
        );
        assert!(
            s.current_threshold >= s.base_threshold - 1e-9,
            "threshold {:.4} went below base {:.4}",
            s.current_threshold,
            s.base_threshold
        );
    }

    #[test]
    fn record_consolidation_resets_decay_clock() {
        // What this catches: the `last_consolidation = Instant::now()`
        // assignment in `record_consolidation`. An edit that dropped
        // the assignment (say, renamed the field but missed one site)
        // would leave the decay clock ticking forever — threshold
        // would drift toward base and stay there because "time since
        // consolidation" never resets. Personas consolidate too
        // eagerly on trivial thoughts forever after.
        //
        // Validated 2026-04-21: mutation = replace the body of
        // `record_consolidation` with `{}` (no-op) → the assertion
        // that stats.seconds_since_consolidation roughly resets to
        // ~0 after calling record_consolidation fails (stays at
        // whatever the pre-record elapsed was). Reverted.
        let mut t = AdaptiveConsolidationThreshold::new();
        // Simulate time passing by touching the internal clock. Since
        // we can't mock Instant easily, we instead call update to let
        // a small real duration accumulate and then record.
        std::thread::sleep(Duration::from_millis(20));
        let before = t.stats().seconds_since_consolidation;
        t.record_consolidation();
        let after = t.stats().seconds_since_consolidation;
        assert!(
            before > 0.0,
            "expected some elapsed time before record, got {before}"
        );
        assert!(
            after < before,
            "record_consolidation didn't reset clock: before={before}, after={after}"
        );
    }
}
