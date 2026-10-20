//! A reading sustained over time: the samples of the last `window`, and their extremes.
//!
//! One owner for a pattern that had been written three times: the serving plan's memory
//! budget (a minimum over one relaunch's duration), the lane footprint ledger's recent
//! maximum (`lane_footprint::recent_max`), and the leased-in peak ring (`serving_daemon::
//! sustained_leased_in`). Each had its own deque, its own trim and its own extreme. A
//! decision input that must outlive its own noise is this type; a new one is a new caller,
//! never a fourth deque.

use std::collections::VecDeque;
use std::time::Duration;

/// The extremes of the samples inside the window. Never empty: `observe` pushes before it
/// reads, so the newest sample is always inside.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Extremes {
    pub min: u64,
    pub max: u64,
}

#[derive(Debug, Clone)]
pub struct Sustained {
    window_ms: u64,
    max_samples: usize,
    samples: VecDeque<(u64, u64)>,
}

impl Sustained {
    /// Samples of the last `window`, however many arrive.
    pub const fn new(window: Duration) -> Self {
        Self::bounded(window, usize::MAX)
    }

    /// Samples of the last `window`, at most `max_samples` of them (oldest dropped first).
    pub const fn bounded(window: Duration, max_samples: usize) -> Self {
        Self { window_ms: window.as_millis() as u64, max_samples, samples: VecDeque::new() }
    }

    /// Record `value` at `now_ms`, drop what fell out of the window, and return the
    /// extremes of what remains. A fall shows at once (it is the newest sample); a rise
    /// shows only once every lower sample has aged out of the window.
    pub fn observe(&mut self, now_ms: u64, value: u64) -> Extremes {
        let horizon = now_ms.saturating_sub(self.window_ms);
        self.samples.retain(|(at, _)| *at >= horizon);
        self.samples.push_back((now_ms, value));
        while self.samples.len() > self.max_samples {
            self.samples.pop_front();
        }
        let (mut min, mut max) = (value, value);
        for &(_, v) in &self.samples {
            min = min.min(v);
            max = max.max(v);
        }
        Extremes { min, max }
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a decision following a reading that will be gone next tick. On the
    // M5 (2026-10-18 15:27:47-15:28:09Z, `serving.plan.branch`) the plan's budget read
    // 22.9, 29.8, 29.1 then 41.9 GB within 22 s while the measured engine ledger sat at
    // 41.68 GB; three agreeing 5 s ticks relaunched the engine three times in 20 min. A rise
    // is earned only after it outlives the window; a fall is honoured at once.
    #[test]
    fn a_rise_waits_for_the_window_and_a_fall_shows_at_once() {
        const GB: u64 = 1 << 30;
        let mut s = Sustained::new(Duration::from_secs(90));
        assert_eq!(s.observe(0, 42 * GB).min, 42 * GB, "the first reading stands alone");
        assert_eq!(s.observe(5_000, 23 * GB).min, 23 * GB, "a fall is immediate");
        assert_eq!(s.observe(10_000, 42 * GB).min, 23 * GB, "a rise waits: the fall is 5 s old");
        assert_eq!(s.observe(60_000, 42 * GB).min, 23 * GB, "still inside the window");
        let e = s.observe(95_001, 42 * GB);
        assert_eq!((e.min, e.max), (42 * GB, 42 * GB), "the fall aged out: the rise is earned");
        assert!(s.len() <= 4, "aged samples are dropped: {}", s.len());
    }

    // what this catches: the bounded form keeping more than it was told (the footprint
    // ledger's per-process ring is bounded by count as well as time).
    #[test]
    fn a_bounded_window_keeps_only_the_newest_samples() {
        let mut s = Sustained::bounded(Duration::from_secs(3600), 3);
        for (i, v) in [5u64, 9, 1, 7].into_iter().enumerate() {
            s.observe(i as u64 * 1000, v);
        }
        assert_eq!(s.len(), 3);
        assert_eq!(s.observe(4000, 4), Extremes { min: 1, max: 7 }, "the 9 fell off the front, the 5 before it");
    }
}
