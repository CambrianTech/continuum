//! STM→LTM consolidation state container.
//!
//! Second 0.5.5 Hippocampus piece (follows
//! `consolidation_threshold.rs`). Bundles the adaptive threshold with
//! per-session metrics and a tick-based dispatch rule so callers can
//! ask "is it time to consolidate this tick?" without reimplementing
//! the cadence gate.
//!
//! What this does NOT own (future commits):
//! - The actual snoop over WorkingMemory — needs a Rust WorkingMemory
//!   primitive which doesn't exist yet; landing with that piece.
//! - The synthesis/raw ConsolidationAdapter — the LLM call that turns
//!   N thoughts into M memories. Orthogonal adapter trait; lands in
//!   its own commit once WorkingMemory is in place.
//! - The LTM write-through to persistent storage — requires
//!   `MemoryCorpus.append_memory` to actually persist, which is a
//!   separate cross-cutting commit on its own.
//!
//! Consolidator is the state-container layer so the future snoop loop
//! doesn't have to re-derive the threshold or keep its own metrics —
//! just call `should_consolidate_this_tick` + `record_success` at
//! the right moments.

use crate::memory::consolidation_threshold::{
    AdaptiveConsolidationThreshold, ConsolidationThresholdStats,
};

/// Running telemetry for a persona's consolidation loop. Counters are
/// cumulative over the Consolidator's lifetime (typically a persona
/// session).
#[derive(Debug, Clone, Copy, Default)]
pub struct ConsolidationMetrics {
    /// Number of tick() calls since construction. Useful for the "every
    /// N ticks, consolidate" cadence check in snoop loops.
    pub tick_count: u64,
    /// Total thoughts promoted to LTM across all consolidation passes.
    pub consolidation_count: u64,
    /// Thoughts that aged out of STM without being consolidated —
    /// either below threshold at the time of each consolidation pass,
    /// or the STM buffer filled and the oldest got dropped.
    pub stm_evictions: u64,
}

/// Cadence: run the consolidation pass every Nth tick. TS used 10.
/// Keeps the pass from running on every single incoming message —
/// batching N turns into one LLM-synthesis call is the point of
/// consolidation.
const TICKS_PER_CONSOLIDATION_PASS: u64 = 10;

/// The state container for per-persona STM→LTM consolidation.
///
/// Thread-affinity: one Consolidator per persona (the snoop loop owns
/// it). Not `Send` / `Sync`-gated here because the adaptive threshold
/// inside is neither by default — calling code serializes access per
/// persona, which is already how the autonomous loop operates.
pub struct Consolidator {
    threshold: AdaptiveConsolidationThreshold,
    metrics: ConsolidationMetrics,
}

impl Default for Consolidator {
    fn default() -> Self {
        Self::new()
    }
}

impl Consolidator {
    pub fn new() -> Self {
        Self {
            threshold: AdaptiveConsolidationThreshold::new(),
            metrics: ConsolidationMetrics::default(),
        }
    }

    /// Advance one tick. Updates the adaptive threshold with the
    /// current activity level, increments tick_count, and returns
    /// `true` when this tick is one where the caller should run its
    /// consolidation pass (every `TICKS_PER_CONSOLIDATION_PASS`).
    ///
    /// Caller pattern:
    /// ```ignore
    /// if consolidator.tick(messages_per_min) {
    ///     let promoted = do_the_snoop_and_write(consolidator.threshold());
    ///     consolidator.record_success(promoted);
    /// }
    /// ```
    pub fn tick(&mut self, messages_per_minute: f64) -> bool {
        self.metrics.tick_count = self.metrics.tick_count.saturating_add(1);
        self.threshold.update_threshold(messages_per_minute);
        self.metrics.tick_count % TICKS_PER_CONSOLIDATION_PASS == 0
    }

    /// Current importance threshold — callers use this to filter
    /// WorkingMemory thoughts in the consolidation pass.
    pub fn threshold(&self) -> f64 {
        self.threshold.threshold()
    }

    /// `true` when the given importance clears the current threshold.
    pub fn should_consolidate(&self, importance: f64) -> bool {
        self.threshold.should_consolidate(importance)
    }

    /// Record a successful consolidation pass — `promoted` thoughts
    /// went to LTM. Resets the threshold's time-decay clock so the
    /// next passes use fresh activity-based numbers, and bumps the
    /// cumulative counter.
    pub fn record_success(&mut self, promoted: u64) {
        self.metrics.consolidation_count =
            self.metrics.consolidation_count.saturating_add(promoted);
        self.threshold.record_consolidation();
    }

    /// Record STM thoughts that aged out without promotion. Pure
    /// telemetry — doesn't touch threshold state.
    pub fn record_evictions(&mut self, evicted: u64) {
        self.metrics.stm_evictions = self.metrics.stm_evictions.saturating_add(evicted);
    }

    /// Read-only metrics snapshot.
    pub fn metrics(&self) -> ConsolidationMetrics {
        self.metrics
    }

    /// Full stats incl. the threshold's internal state.
    pub fn stats(&self) -> ConsolidatorStats {
        ConsolidatorStats {
            metrics: self.metrics,
            threshold: self.threshold.stats(),
        }
    }

    /// Reset threshold + metrics (session boundary).
    pub fn reset(&mut self) {
        self.threshold.reset();
        self.metrics = ConsolidationMetrics::default();
    }
}

/// Combined telemetry: per-session counters + threshold's internal
/// state. One struct so telemetry emitters don't have to decide which
/// of the two to read.
#[derive(Debug, Clone, Copy)]
pub struct ConsolidatorStats {
    pub metrics: ConsolidationMetrics,
    pub threshold: ConsolidationThresholdStats,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_returns_true_on_configured_cadence() {
        // What this catches: the `tick_count % TICKS_PER_PASS == 0`
        // gate. Consolidation is batched specifically so N tick-level
        // events turn into ONE synthesis call — if the gate mutation
        // fires on every tick, snoop runs 10× as often, multiplying
        // LLM cost and grinding synthesis calls into noise. The test
        // ticks up to 2 * TICKS_PER_CONSOLIDATION_PASS and checks
        // exactly 2 "true" returns arrive, exactly at tick N and 2N.
        //
        // Validated 2026-04-21: mutation = replace
        // `self.metrics.tick_count % TICKS_PER_CONSOLIDATION_PASS == 0`
        // with `true` → assertion that ticks with true_count==2 fails
        // (becomes 20). Reverted.
        let mut c = Consolidator::new();
        let n = 2 * TICKS_PER_CONSOLIDATION_PASS as usize;
        let true_ticks: Vec<usize> = (1..=n)
            .filter_map(|i| if c.tick(1.0) { Some(i) } else { None })
            .collect();
        assert_eq!(
            true_ticks,
            vec![
                TICKS_PER_CONSOLIDATION_PASS as usize,
                2 * TICKS_PER_CONSOLIDATION_PASS as usize
            ],
            "tick() should return true exactly at every {TICKS_PER_CONSOLIDATION_PASS}th call; \
             got trues at {true_ticks:?}"
        );
    }

    #[test]
    fn record_success_accumulates_promoted_count() {
        // What this catches: `saturating_add` + proper accumulation in
        // `record_success`. A mutation that used assignment instead
        // (`self.metrics.consolidation_count = promoted`) would lose
        // all prior counts on every pass — per-session telemetry
        // would show only the most recent pass's number, and load-
        // tracking / policy decisions keyed on cumulative counts
        // would silently break.
        //
        // Validated 2026-04-21: mutation = replace
        // `self.metrics.consolidation_count.saturating_add(promoted)`
        // with `promoted` (assignment not add) → total assertion (18)
        // fails (shows 8, the last value). Reverted.
        let mut c = Consolidator::new();
        c.record_success(3);
        c.record_success(7);
        c.record_success(8);
        assert_eq!(
            c.metrics().consolidation_count,
            18,
            "expected 3+7+8=18 cumulative, got {}",
            c.metrics().consolidation_count
        );
    }

    #[test]
    fn record_success_resets_threshold_decay_clock() {
        // What this catches: `record_success` delegates to
        // `threshold.record_consolidation` so the time-decay clock
        // resets on success. Without this, the threshold-side clock
        // keeps ticking and decays to base forever — the bug
        // `consolidation_threshold::record_consolidation_resets_decay_clock`
        // already catches at the threshold layer, but this test pins
        // the DELEGATION so a Consolidator refactor (e.g. moving the
        // threshold under an Arc, inlining `record_success`) can't
        // silently stop forwarding the call.
        //
        // Validated 2026-04-21: mutation = remove the
        // `self.threshold.record_consolidation()` line from
        // `record_success` → after sleep + record_success, threshold
        // stats still show before-level elapsed; assertion fails.
        // Reverted.
        let mut c = Consolidator::new();
        std::thread::sleep(std::time::Duration::from_millis(20));
        let before = c.stats().threshold.seconds_since_consolidation;
        c.record_success(1);
        let after = c.stats().threshold.seconds_since_consolidation;
        assert!(before > 0.0, "expected elapsed>0 before record, got {before}");
        assert!(
            after < before,
            "record_success didn't forward to threshold: before={before}, after={after}"
        );
    }

    #[test]
    fn reset_zeros_metrics_and_restores_threshold() {
        // What this catches: `reset` forwards to both underlying
        // components. A partial reset that only cleared metrics but
        // left the threshold's `recent_activity` ring full would keep
        // biasing new sessions by the previous session's activity —
        // exactly what "session boundary" reset is meant to prevent.
        //
        // Validated 2026-04-21: mutation = remove the
        // `self.threshold.reset()` call from `reset` → after feeding
        // high activity and then resetting, threshold.avg_activity
        // stays non-zero; the assertion that avg_activity drops back
        // to 0 fails. Reverted.
        let mut c = Consolidator::new();
        for _ in 0..5 {
            c.tick(100.0);
        }
        c.record_success(3);
        c.record_evictions(2);
        let before_reset = c.stats();
        assert!(before_reset.metrics.tick_count > 0);
        assert!(before_reset.threshold.avg_activity > 50.0);

        c.reset();

        let after_reset = c.stats();
        assert_eq!(after_reset.metrics.tick_count, 0);
        assert_eq!(after_reset.metrics.consolidation_count, 0);
        assert_eq!(after_reset.metrics.stm_evictions, 0);
        // Threshold reset drains activity window → avg reverts to 0.
        assert!(
            after_reset.threshold.avg_activity < 1e-9,
            "threshold activity window didn't reset: {}",
            after_reset.threshold.avg_activity
        );
    }
}
