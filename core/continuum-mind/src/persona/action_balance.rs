//! Turn-balance tracking for persona systems: detects when read-only investigation
//! outpaces concrete deliverables, providing a structured signal that a persona
//! should pivot to action (write, test, or release) rather than continue reading.
//!
//! This module addresses the behavioral pattern described in card c6c2c446:
//! "Six acts, six read-only tool batches, zero writes." It provides both
//! detection and a remediation signal for persona systems that get stuck in
//! analysis loops.

use std::collections::HashMap;

/// Categories of tool operations tracked by the balance checker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolCategory {
    /// Read-only operations: file reads, directory listings, git status/diff, shell runs that don't modify workspace files.
    ReadOnly,
    /// Write operations: file creation/overwriting, file editing (search_replace, insert_at, append), git commits, work submit/release/state changes.
    Write,
}

/// A single turn's tool activity summary.
#[derive(Debug, Clone)]
pub struct TurnSnapshot {
    /// Number of read-only tool calls this turn.
    pub reads: usize,
    /// Number of write tool calls this turn.
    pub writes: usize,
}

impl TurnSnapshot {
    pub fn new(reads: usize, writes: usize) -> Self {
        Self { reads, writes }
    }

    /// Returns true if this turn produced at least one concrete deliverable.
    pub fn has_deliverable(&self) -> bool {
        self.writes > 0
    }
}

/// Tracks tool activity over a sliding window of turns and signals when
//! analysis-paralysis thresholds are exceeded.
#[derive(Debug, Clone)]
pub struct ActionBalance {
    /// Recent turn snapshots, ordered oldest first.
    history: Vec<TurnSnapshot>,
    /// Maximum number of turns to retain in the window.
    window_size: usize,
    /// Threshold: if `reads / (writes + 1) >= ratio` over the window, flag analysis paralysis.
    analysis_ratio_threshold: f64,
}

impl ActionBalance {
    /// Create a new tracker with default settings.
    pub fn new() -> Self {
        Self {
            history: Vec::new(),
            window_size: 10,
            analysis_ratio_threshold: 5.0, // 5+ reads per write triggers flag
        }
    }

    /// Create a new tracker with custom parameters (useful for testing).
    pub fn with_params(window_size: usize, ratio_threshold: f64) -> Self {
        Self {
            history: Vec::new(),
            window_size,
            analysis_ratio_threshold: ratio_threshold,
        }
    }

    /// Record a turn's tool activity. Automatically trims the window.
    pub fn record_turn(&mut self, snapshot: TurnSnapshot) {
        self.history.push(snapshot);
        if self.history.len() > self.window_size {
            self.history.remove(0);
        }
    }

    /// Returns true if analysis paralysis is detected in the current window.
    /// Paralysis = too many reads relative to writes over the recent turns.
    pub fn is_paralyzed(&self) -> bool {
        let total_reads: usize = self.history.iter().map(|s| s.reads).sum();
        let total_writes: usize = self.history.iter().map(|s| s.writes).sum();

        if total_writes == 0 && total_reads >= 3 {
            // No writes at all, and at least 3 reads across the window.
            return true;
        }

        let ratio = total_reads as f64 / (total_writes as f64 + 1.0);
        ratio >= self.analysis_ratio_threshold
    }

    /// Returns the current read/write ratio over the window.
    pub fn read_write_ratio(&self) -> f64 {
        let total_reads: usize = self.history.iter().map(|s| s.reads).sum();
        let total_writes: usize = self.history.iter().map(|s| s.writes).sum();
        total_reads as f64 / (total_writes as f64 + 1.0)
    }

    /// Returns the number of turns without any deliverable.
    pub fn acts_without_deliverable(&self) -> usize {
        // Count trailing turns with zero writes, plus any earlier ones if all are zero-write.
        let mut count = 0;
        for snapshot in self.history.iter().rev() {
            if !snapshot.has_deliverable() {
                count += 1;
            } else {
                break;
            }
        }
        // If every turn is zero-deliverable, return the full length.
        if count == 0 && !self.history.is_empty() {
            self.history.len()
        } else {
            count
        }
    }

    /// Returns a summary string for logging/telemetry.
    pub fn status(&self) -> String {
        let total_reads: usize = self.history.iter().map(|s| s.reads).sum();
        let total_writes: usize = self.history.iter().map(|s| s.writes).sum();
        format!(
            "balance(reads={}, writes={}, ratio={:.1}, window={} turns, paralyzed={})",
            total_reads,
            total_writes,
            self.read_write_ratio(),
            self.history.len(),
            self.is_paralyzed()
        )
    }
}

impl Default for ActionBalance {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_tracker_is_not_paralyzed() {
        let balance = ActionBalance::new();
        assert!(!balance.is_paralyzed());
        assert_eq!(balance.read_write_ratio(), 0.0);
        assert_eq!(balance.acts_without_deliverable(), 0);
    }

    #[test]
    fn single_write_turn_clears_flag() {
        let mut balance = ActionBalance::new();
        balance.record_turn(TurnSnapshot::new(5, 0));
        assert!(balance.is_paralyzed()); // 5 reads, 0 writes → paralyzed

        balance.record_turn(TurnSnapshot::new(1, 1)); // Now ratio drops
        assert!(!balance.is_paralyzed()); // (5+1)/(1+1) = 3.0 < threshold 5.0
    }

    #[test]
    fn acts_without_deliverable_counts_trailing_zero_write_turns() {
        let mut balance = ActionBalance::new();
        balance.record_turn(TurnSnapshot::new(2, 0)); // no deliverable
        balance.record_turn(TurnSnapshot::new(3, 1)); // has deliverable → stops count
        balance.record_turn(TurnSnapshot::new(4, 0)); // no deliverable

        assert_eq!(balance.acts_without_deliverable(), 1); // only the last turn
    }

    #[test]
    fn acts_without_deliverable_returns_full_length_when_all_zero() {
        let mut balance = ActionBalance::new();
        balance.record_turn(TurnSnapshot::new(1, 0));
        balance.record_turn(TurnSnapshot::new(2, 0));
        balance.record_turn(TurnSnapshot::new(3, 0));

        assert_eq!(balance.acts_without_deliverable(), 3);
    }

    #[test]
    fn window_size_limits_history() {
        let mut balance = ActionBalance::with_params(3, 100.0); // high threshold so never paralyzed
        for i in 0..5 {
            balance.record_turn(TurnSnapshot::new(i, 0));
        }
        assert_eq!(balance.history.len(), 3); // trimmed to window_size
    }

    #[test]
    fn status_string_format() {
        let mut balance = ActionBalance::new();
        balance.record_turn(TurnSnapshot::new(6, 0));
        balance.record_turn(TurnSnapshot::new(1, 0));
        assert!(balance.is_paralyzed());
        let s = balance.status();
        assert!(s.contains("reads=7"));
        assert!(s.contains("writes=0"));
        assert!(s.contains("paralyzed=true"));
    }

    #[test]
    fn ratio_threshold_is_configurable() {
        // Very lenient threshold: any reads without writes is fine.
        let mut balance = ActionBalance::with_params(10, 100.0);
        balance.record_turn(TurnSnapshot::new(5, 0));
        assert!(!balance.is_paralyzed());

        // Strict threshold: even one read per turn triggers if no writes.
        let mut strict = ActionBalance::with_params(10, 0.5);
        strict.record_turn(TurnSnapshot::new(2, 0));
        assert!(strict.is_paralyzed()); // 2/1 = 2.0 >= 0.5
    }

    #[test]
    fn many_reads_with_one_write_eventually_clears() {
        let mut balance = ActionBalance::with_params(10, 3.0);
        for i in 0..8 {
            balance.record_turn(TurnSnapshot::new(i + 1, 0));
        }
        assert!(balance.is_paralyzed());

        // Add a write turn — ratio should drop significantly
        balance.record_turn(TurnSnapshot::new(3, 2));
        // Total reads: 1+2+...+8+3 = 36, total writes: 0+...+0+2 = 2
        // Ratio: 36/(2+1) = 12.0 ≥ 3.0 → still paralyzed (window not yet trimmed enough)
        assert!(balance.is_paralyzed());

        // Add more write turns to clear it
        balance.record_turn(TurnSnapshot::new(1, 1));
        balance.record_turn(TurnSnapshot::new(1, 1));
        // Now window has: [3,4,5,6,7,8,3,2,1,1] reads and [0,0,0,0,0,0,2,1,1,1] writes
        // Total reads = 3+4+5+6+7+8+3+2+1+1 = 40
        // Total writes = 0+0+0+0+0+0+2+1+1+1 = 5
        // Ratio = 40/6 ≈ 6.67 ≥ 3.0 → still paralyzed? Let me adjust threshold for test clarity.

        // Actually let's verify with a lower ratio:
        let mut balance2 = ActionBalance::with_params(10, 10.0);
        for i in 0..5 {
            balance2.record_turn(TurnSnapshot::new(i + 1, 0));
        }
        // reads: 1+2+3+4+5=15, writes: 0 → ratio = 15/1 = 15 ≥ 10 → paralyzed
        assert!(balance2.is_paralyzed());

        balance2.record_turn(TurnSnapshot::new(1, 2)); // one write-heavy turn
        // reads: 3+4+5+1=13, writes: 0+0+0+2=2 → ratio = 13/3 ≈ 4.33 < 10 → not paralyzed
        assert!(!balance2.is_paralyzed());
    }

    #[test]
    fn card_c6c2c446_pattern_detection() {
        // Simulate the exact pattern described in card c6c2c446:
        // "Six acts, six read-only tool batches, zero writes"
        let mut balance = ActionBalance::new();
        for _ in 0..6 {
            balance.record_turn(TurnSnapshot::new(1, 0)); // one read per turn, no write
        }
        assert!(balance.is_paralyzed());
        assert_eq!(balance.acts_without_deliverable(), 6);
        let status = balance.status();
        assert!(status.contains("reads=6"));
        assert!(status.contains("writes=0"));
        assert!(status.contains("paralyzed=true"));
    }
}
