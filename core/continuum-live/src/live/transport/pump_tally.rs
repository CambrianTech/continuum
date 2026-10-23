//! Per-second tally for a per-FRAME hot path — the probe for a 30 fps × N
//! pump is a SUMMARY, never a row per frame.
//!
//! Twelve avatar pumps at 30 fps wrote ~370 `media.pump.*` rows a second into
//! the size-rotated probe ledger; the whole file held ~34 s of history and
//! every cognition class read ZERO for the length of a live call — the
//! substrate looked dead while the run room filled with work receipts
//! (2026-09-05). Per-tick probes rotate truth away
//! ([[time-based-convergence-dies-under-restarts-and-per-tick-probes-rotate-truth-away]]);
//! a hot path reports once a second with counts and a maximum.

use std::time::{Duration, Instant};

/// Frames/bytes/max-latency accumulated since the last emit. Not thread-safe
/// by itself — the caller owns the lock (or the thread).
#[derive(Debug)]
pub struct PumpTally {
    frames: u64,
    bytes: u64,
    max_us: u64,
    since: Instant,
    period: Duration,
}

/// One second's worth of pump activity, handed to the probe at emit time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PumpSummary {
    pub frames: u64,
    pub bytes: u64,
    pub max_us: u64,
    pub span_ms: u64,
}

impl PumpTally {
    pub fn new(period: Duration) -> Self {
        Self { frames: 0, bytes: 0, max_us: 0, since: Instant::now(), period }
    }

    /// Record one frame. Returns the period's summary when it is time to emit
    /// (and resets), else `None` — the caller probes only on `Some`.
    pub fn record(&mut self, bytes: u64, latency_us: u64) -> Option<PumpSummary> {
        self.frames += 1;
        self.bytes += bytes;
        self.max_us = self.max_us.max(latency_us);
        let elapsed = self.since.elapsed();
        if elapsed < self.period {
            return None;
        }
        let out = PumpSummary {
            frames: self.frames,
            bytes: self.bytes,
            max_us: self.max_us,
            span_ms: elapsed.as_millis() as u64,
        };
        self.frames = 0;
        self.bytes = 0;
        self.max_us = 0;
        self.since = Instant::now();
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a per-frame hot path must emit at most once per
    // period — N frames inside the period produce no summary, the first frame
    // past it produces ONE carrying all N+1, and the tally restarts.
    #[test]
    fn a_period_of_frames_collapses_to_one_summary() {
        let mut t = PumpTally::new(Duration::from_millis(20));
        for _ in 0..9 {
            assert_eq!(t.record(100, 5), None);
        }
        std::thread::sleep(Duration::from_millis(25));
        let s = t.record(100, 40).expect("period elapsed → one summary");
        assert_eq!((s.frames, s.bytes, s.max_us), (10, 1000, 40));
        assert!(s.span_ms >= 20);
        assert_eq!(t.record(1, 1), None, "tally restarted after the emit");
    }
}
