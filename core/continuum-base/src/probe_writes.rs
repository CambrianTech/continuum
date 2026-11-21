//! How many probe records a log sink failed to write, process-wide. A sink never fails its caller
//! over a write it could not make (a probe is telemetry), so it counts the failure here instead of
//! dropping it silently, and a reader that claims records reached the log (a stop's settle) states
//! how many writes failed in its window (Codex on #6208 at 162fbe5fa7: "written" confirmed only
//! that the tracing callbacks returned).

use std::sync::atomic::{AtomicU64, Ordering};

static FAILED: AtomicU64 = AtomicU64::new(0);

/// A probe record a sink could not write (serialize, lock, write or flush).
pub fn count_failure() {
    FAILED.fetch_add(1, Ordering::Relaxed);
}

/// The failures counted so far, read at one moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reading(u64);

impl Reading {
    pub fn now() -> Self {
        Self(FAILED.load(Ordering::Relaxed))
    }

    /// Failures counted between `earlier` and this reading.
    pub fn since(self, earlier: Self) -> u64 {
        self.0.saturating_sub(earlier.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a failed probe write that no later reading can see.
    #[test]
    fn a_counted_failure_shows_in_the_next_reading() {
        let before = Reading::now();
        count_failure();
        assert!(Reading::now().since(before) >= 1);
        assert_eq!(before.since(Reading::now()), 0, "an earlier reading never goes negative");
    }
}
