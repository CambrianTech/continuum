//! When a physical reading was taken. The ledger orders physical samples by the moment the probe
//! that produced them STARTED, never by when the daemon happened to read a cached value: a monitor
//! that probes on its own tick hands back the same frame however many times it is asked (Codex on
//! #6124). A lease declared landed stops being owed only once a reading observed after the
//! declaration is accepted.

use std::sync::OnceLock;
use std::time::Instant;

/// A monotonic observation instant: nanoseconds since this process first asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Observed(u64);

static ORIGIN: OnceLock<Instant> = OnceLock::new();

impl Observed {
    /// Now, on the process's monotonic clock.
    pub fn now() -> Self {
        let elapsed = ORIGIN.get_or_init(Instant::now).elapsed().as_nanos();
        Self(u64::try_from(elapsed).unwrap_or(u64::MAX)) // unwrap_or: 584 years of uptime saturates instead of wrapping
    }

    /// Before every reading: what a source with no reading yet reports, so it never clears a landing.
    pub const NEVER: Self = Self(0);

    pub fn from_nanos(nanos: u64) -> Self {
        Self(nanos)
    }

    pub fn nanos(self) -> u64 {
        self.0
    }
}

/// A source's physical use and when the observation behind it began.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhysicalSample {
    pub used: u64,
    pub observed: Observed,
}
