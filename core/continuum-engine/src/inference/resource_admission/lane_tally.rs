//! The serving lanes' grants and releases as COUNTS, reported once per window, never a ledger row
//! per event (card 6742aad1). A livelock in turn admission granted and released 35,000 ambient
//! lanes a second on the M5 (2026-11-13, canary 0ed0d231b): a row each rotated the probe ledger and
//! the core log every ~0.3 s and buried every other probe on the node, so nothing else could be
//! diagnosed while it ran. A count costs two atomic adds at any rate; only a grant that WAITED
//! still gets its own row (`admission.lane.granted`), because a wait is what a reader looks for.

use std::sync::atomic::{AtomicU64, Ordering};

/// How often the counts are reported.
/// derived-or-floor: a reporting cadence for a human reader (one ledger row a second at most,
/// whatever the grant rate); no admission decision reads it, so no measurement could replace it.
const WINDOW_MS: u64 = 1_000;

pub(super) struct LaneTally {
    /// Since the window opened.
    granted: AtomicU64,
    waited: AtomicU64,
    released: AtomicU64,
    held_ms: AtomicU64,
    window_opened_ms: AtomicU64,
}

impl LaneTally {
    pub(super) const fn new() -> Self {
        Self {
            granted: AtomicU64::new(0),
            waited: AtomicU64::new(0),
            released: AtomicU64::new(0),
            held_ms: AtomicU64::new(0),
            window_opened_ms: AtomicU64::new(0),
        }
    }

    pub(super) fn granted(&self, waited: bool) {
        self.granted.fetch_add(1, Ordering::Relaxed);
        #[cfg(test)]
        GRANTED_ON_THIS_THREAD.with(|granted| granted.set(granted.get() + 1));
        if waited {
            self.waited.fetch_add(1, Ordering::Relaxed);
        }
        self.report_if_due(since_boot_ms());
    }

    pub(super) fn released(&self, held_ms: u64) {
        self.released.fetch_add(1, Ordering::Relaxed);
        self.held_ms.fetch_add(held_ms, Ordering::Relaxed);
        self.report_if_due(since_boot_ms());
    }

    /// The caller that closes a due window reports it; every other caller only counts.
    fn report_if_due(&self, now: u64) {
        let opened = self.window_opened_ms.load(Ordering::Acquire);
        let window_ms = now.saturating_sub(opened);
        if window_ms < WINDOW_MS
            || self.window_opened_ms.compare_exchange(opened, now, Ordering::AcqRel, Ordering::Acquire).is_err()
        {
            return;
        }
        // Reset here, never inside the probe: its fields are evaluated only when a sink listens.
        let (granted, waited) = (self.granted.swap(0, Ordering::AcqRel), self.waited.swap(0, Ordering::AcqRel));
        let (released, held_ms) = (self.released.swap(0, Ordering::AcqRel), self.held_ms.swap(0, Ordering::AcqRel));
        continuum_base::probe!(
            class = "admission.lane.tally",
            granted,
            waited,
            released,
            held_ms,
            window_ms,
            "serving-lane grants and releases in this window"
        );
    }
}

#[cfg(test)]
thread_local! {
    /// Grants made on this thread: a `#[tokio::test]` runs on its own current-thread runtime, so a
    /// scenario counts its own grants, never another test's on the process-wide gate.
    static GRANTED_ON_THIS_THREAD: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn granted_on_this_thread() -> u64 {
    GRANTED_ON_THIS_THREAD.with(std::cell::Cell::get)
}

fn since_boot_ms() -> u64 {
    static BOOT: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    BOOT.get_or_init(std::time::Instant::now).elapsed().as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (card 6742aad1): a hot path that writes a ledger row per grant. Ten thousand
    // grants and releases inside one window report nothing until the window closes, and then once.
    #[test]
    fn counts_accumulate_inside_a_window_and_report_once() {
        let tally = LaneTally::new();
        let opened = since_boot_ms() + WINDOW_MS; // a window that cannot close while the loop runs
        tally.window_opened_ms.store(opened, Ordering::Release);
        for _ in 0..10_000 {
            tally.granted(false);
            tally.released(0);
        }
        assert_eq!(tally.granted.load(Ordering::Relaxed), 10_000, "still counting: the window has not closed");
        tally.report_if_due(opened + WINDOW_MS);
        assert_eq!(tally.granted.load(Ordering::Relaxed), 0, "the window closed once: reported and reset");
        assert_eq!(tally.window_opened_ms.load(Ordering::Acquire), opened + WINDOW_MS, "the next window opens at the report");
        assert!(granted_on_this_thread() >= 10_000, "and this thread counted every grant");
    }
}
