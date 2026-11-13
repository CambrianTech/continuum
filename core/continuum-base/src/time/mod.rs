//! Wall-clock time, as values that own their arithmetic.
//!
//! [`EpochMs`] is the one place the core reads the wall clock. Before it, 68 private
//! `now_ms` / `now_millis` / `epoch_ms` functions in this library each did the same
//! read. Other modules called persona's `trace::now_ms` by path 55 times. Each copy
//! chose its own meaning for a clock set before 1970: 0, a panic, `u64::MAX`, an
//! `Option` or a `Result`.
//!
//! That choice is now made here, as three named reads:
//! - [`EpochMs::now`] treats a clock before 1970 as the epoch itself. Most copies did
//!   this, and it is right for a timestamp written into a row or compared against a
//!   deadline taken from the same clock.
//! - [`EpochMs::now_or_latest`] treats it as later than every deadline, for a caller
//!   that must fail open.
//! - [`EpochMs::read`] returns [`ClockBeforeEpoch`], for a caller that must report a
//!   broken clock as an error.
//!
//! [`age::Age`] is how long ago something happened, rendered in coarse buckets so that a
//! prompt does not change every minute.

pub mod age;

use std::time::{SystemTime, UNIX_EPOCH};

/// Milliseconds since the Unix epoch on the wall clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EpochMs(u64);

/// The wall clock reads before 1970-01-01, so it was set wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClockBeforeEpoch;

impl std::fmt::Display for ClockBeforeEpoch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the system clock reads before the Unix epoch")
    }
}

impl std::error::Error for ClockBeforeEpoch {}

impl EpochMs {
    /// Now. A clock before 1970 reads as the epoch (see the module note).
    ///
    /// Callers rely on that 0. When this type replaced the private clocks, 43 of the reads
    /// carried a written reason for reading 0, each specific to its caller: the safe side
    /// ("every lease reads live", "the cooldown always allows", "a retry, never a stuck
    /// seat") or only a label on a record. Each reason sits at its caller. Before
    /// changing this policy, re-audit those callers.
    pub fn now() -> Self {
        Self::read().unwrap_or(EpochMs(0)) // unwrap_or: the one stated pre-1970 policy, see the module note
    }

    /// Now. A clock before 1970 reads as the latest time there is, so every deadline
    /// has passed. That is right for a caller where a broken clock must fail open:
    /// - the airc lane breaker: `now < until` is false, so the lane reads warm and
    ///   requests still go out; 0 would read every tripped lane cold forever (review of
    ///   #3814);
    /// - SWE refusal expiry: every refusal reads expired, which errs toward grading;
    ///   0 would make every refusal stand forever.
    pub fn now_or_latest() -> Self {
        Self::read().unwrap_or(EpochMs(u64::MAX)) // unwrap_or: fails open, see above
    }

    /// Now, or [`ClockBeforeEpoch`] when the clock is set before 1970.
    pub fn read() -> Result<Self, ClockBeforeEpoch> {
        Self::of(SystemTime::now())
    }

    /// A `SystemTime` from the same wall clock (a file's mtime, say), or
    /// [`ClockBeforeEpoch`] when it is before 1970.
    pub fn of(time: SystemTime) -> Result<Self, ClockBeforeEpoch> {
        let since = time.duration_since(UNIX_EPOCH).map_err(|_| ClockBeforeEpoch)?;
        // u64 milliseconds last until the year 584,556,019.
        Ok(EpochMs(since.as_millis() as u64))
    }

    pub const fn from_millis(millis: u64) -> Self {
        EpochMs(millis)
    }

    pub const fn millis(self) -> u64 {
        self.0
    }

    /// Whole seconds since the epoch, rounded down, the same as `Duration::as_secs`.
    pub const fn secs(self) -> u64 {
        self.0 / 1000
    }

    /// RFC 3339 in UTC with milliseconds, e.g. `2026-10-12T14:03:07.412Z`.
    pub fn rfc3339(self) -> String {
        chrono::DateTime::from_timestamp_millis(self.0 as i64)
            .unwrap_or(chrono::DateTime::<chrono::Utc>::MAX_UTC) // unwrap_or: chrono's range ends in the year 262,143; only now_or_latest's MAX is past it
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
    }

    /// The time from `earlier` to this moment, or zero when `earlier` is later.
    pub fn since(self, earlier: EpochMs) -> age::Age {
        age::Age::from_millis(self.0.saturating_sub(earlier.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: two reads of the one clock going backwards, or `since` wrapping
    // when the earlier stamp is in fact later (a stored row from a clock that was ahead).
    #[test]
    fn the_clock_reads_forward_and_an_age_never_wraps() {
        let first = EpochMs::now();
        let second = EpochMs::read().expect("the test host's clock is after 1970");
        assert!(second >= first);
        assert!(first.millis() > 1_700_000_000_000, "a real wall clock, not the pre-1970 epoch read");
        assert_eq!(first.since(second).millis(), 0, "a later 'earlier' is zero, not a wrap");
        assert_eq!(EpochMs::from_millis(5_000).since(EpochMs::from_millis(2_000)).millis(), 3_000);
    }

    // what this catches: the hand-rolled calendar math this replaced (two copies of a
    // days-to-date loop, and one formatter that wrote bare seconds plus "Z") drifting
    // from a real RFC 3339 date.
    #[test]
    fn rfc3339_is_a_real_utc_date_with_milliseconds() {
        assert_eq!(EpochMs::from_millis(0).rfc3339(), "1970-01-01T00:00:00.000Z");
        assert_eq!(EpochMs::from_millis(951_782_400_123).rfc3339(), "2000-02-29T00:00:00.123Z", "a leap day");
        assert_eq!(EpochMs::from_millis(1_791_840_496_906).rfc3339(), "2026-10-12T21:28:16.906Z");
    }
}
