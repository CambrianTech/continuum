//! ABSENCE WATCH — the node says when it was suspended, instead of leaving a hole in the
//! ledger for a reader to infer the next day.
//!
//! One task, one `tokio::time::interval`, one probe class. Every tick compares the wall
//! clock against the previous tick; a gap far longer than the period means the process
//! was not running (system sleep, SIGSTOP, a VM pause, a laptop lid). The 2026-09-07
//! power outage produced a 15-minute hole with ZERO rows — even the pure-timer catch-up
//! tick — and the only way to learn the machine had slept was `pmset -g log` afterwards.
//! Walking that tick back showed the M5 had been sleeping in cycles for three hours.
//!
//! Emits `boot.absent {from_ms, to_ms, gap_s, cause}` once per gap. `cause` is what the
//! process can know on its own: `suspended` (the clock jumped; the OS did not run us).
//! Deciding WHY (battery idle sleep, thermal, lid) is the operator's `pmset -g log`, and
//! the row carries enough to line the two up.
//!
//! RTOS shape per CONCURRENCY-STYLE-GUIDE: own task, `interval`, no locks, no blocking,
//! nothing on the hot path; the only side effect is a probe row.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// How often the watch ticks. Short enough that a tick gap names the absence to the
/// nearest half-minute; long enough to cost nothing.
pub const PERIOD: Duration = Duration::from_secs(30);

/// A gap is an absence when the wall clock advanced by more than this many periods
/// between two consecutive ticks. Three periods (90 s) sits above every scheduler
/// hiccup and GC-class stall seen on the M5 and below the shortest dark-wake cycle.
pub const ABSENT_AFTER_PERIODS: u32 = 3;

/// The decision, pure: did the wall clock jump enough to call it an absence?
pub fn absence_gap_s(prev_ms: u64, now_ms: u64) -> Option<u64> {
    let gap_ms = now_ms.saturating_sub(prev_ms);
    let threshold_ms = PERIOD.as_millis() as u64 * ABSENT_AFTER_PERIODS as u64;
    (gap_ms > threshold_ms).then_some(gap_ms / 1000)
}

fn now_ms() -> Option<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_millis() as u64)
}

/// Spawn the watch. Called once from main after the runtime is up.
pub fn spawn() {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(PERIOD);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut prev = now_ms();
        loop {
            ticker.tick().await;
            let Some(now) = now_ms() else {
                // A clock before the epoch is its own absence of measurement; say so once.
                crate::probe!(
                    class = "boot.absent",
                    from_ms = prev.unwrap_or(0), // unwrap_or: 0 = no previous reading, a legible absence in the row
                    to_ms = 0u64,
                    gap_s = 0u64,
                    cause = "clock_unreadable",
                    "the wall clock could not be read — absence cannot be measured this tick"
                );
                continue;
            };
            if let Some(gap_s) = prev.and_then(|p| absence_gap_s(p, now)) {
                crate::probe!(
                    class = "boot.absent",
                    from_ms = prev.unwrap_or(0), // unwrap_or: unreachable here (gap needs prev) — 0 stays legible
                    to_ms = now,
                    gap_s = gap_s,
                    cause = "suspended",
                    "the process did not run for this long — the machine slept or was paused; every lane, lease and turn in flight aged by this much"
                );
                tracing::warn!(
                    gap_s,
                    "⏸ ABSENT for {gap_s} s — the node was suspended (sleep/pause); check `pmset -g log` for the cause"
                );
            }
            prev = Some(now);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the threshold drifting so that a real sleep (the M5's shortest
    // dark-wake cycle was ~4 min) reads as a tick, or a scheduler hiccup reads as sleep.
    #[test]
    fn a_four_minute_hole_is_an_absence_and_a_late_tick_is_not() {
        let t0 = 1_788_800_000_000u64;
        assert_eq!(absence_gap_s(t0, t0 + 240_000), Some(240));
        assert_eq!(absence_gap_s(t0, t0 + 91_000), Some(91), "just over three periods");
        assert_eq!(absence_gap_s(t0, t0 + 89_000), None, "under three periods is a late tick");
        assert_eq!(absence_gap_s(t0, t0 + 30_000), None);
        assert_eq!(absence_gap_s(t0 + 10, t0), None, "a clock stepping backwards is not an absence");
    }
}
