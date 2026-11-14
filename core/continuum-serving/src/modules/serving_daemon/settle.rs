//! Whether the host is still RETURNING memory, and what a launch does about it (card 8569b03d).
//!
//! The M5, 2026-11-13 19:36Z: the boot plan read 16 GB usable while the memory of the engine just
//! stopped was still coming back (the OS compressor shrinking), launched the engine at a 2,048-token
//! pool, and 15 s later the plan read 25 GB. The one launch keeps its pool for life, so a pool sized
//! from that reading left every mind dark until the engine was killed by hand.
//!
//! The reading is the host's own (`continuum_host::host_memory`, #5971): memory is being returned
//! while the compressor shrinks between two readings (a stopped process's compressed pages are freed,
//! and the OS decompresses the rest into the room it left). No fraction and no timer: the comparison
//! is with the last reading, and a compressor cannot shrink forever, so the wait is bounded by the
//! host itself. A host with no reading (an OS without an adapter) never waits: unknown is not
//! calm, but a launch that waits on a reading that can never come leaves every mind dark.

use continuum_host::host_memory::HostMemoryReading;

use super::ServingDaemonModule;

impl ServingDaemonModule {
    /// Fold the host's memory reading in, off the async runtime (a Mach / procfs call). Run before
    /// every reconcile, so a launch reads the last two readings, and its pool fits the last one
    /// (`host_fit`).
    pub(super) async fn observe_host_settle(&self) {
        // A panicked read is no reading: never calm, never a wait that cannot end.
        let reading = tokio::task::spawn_blocking(continuum_host::host_memory::read).await.ok().flatten();
        *self.host_reading.lock() = reading;
        let mut settle = self.host_settle.lock();
        *settle = settle.observe(reading.as_ref());
    }
}

/// What the host's last two readings say about memory coming back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum HostSettle {
    /// No reading folded yet: nothing to compare with.
    #[default]
    Unread,
    /// One reading, nothing to compare it with yet.
    First { compressed_bytes: u64 },
    /// The compressor shrank since the last reading: memory is still being returned.
    Returning { compressed_bytes: u64 },
    /// The compressor did not shrink since the last reading.
    Steady { compressed_bytes: u64 },
    /// The host gave no reading (no adapter on this OS, or a failed call).
    Unreadable,
}

impl HostSettle {
    /// Fold one reading in.
    pub(super) fn observe(self, reading: Option<&HostMemoryReading>) -> Self {
        let Some(reading) = reading else {
            return Self::Unreadable;
        };
        let now = reading.compressed_bytes;
        match self.compressed_bytes() {
            None => Self::First { compressed_bytes: now },
            Some(before) if now < before => Self::Returning { compressed_bytes: now },
            Some(_) => Self::Steady { compressed_bytes: now },
        }
    }

    fn compressed_bytes(self) -> Option<u64> {
        match self {
            Self::First { compressed_bytes } | Self::Returning { compressed_bytes } | Self::Steady { compressed_bytes } => Some(compressed_bytes),
            Self::Unread | Self::Unreadable => None,
        }
    }

    /// The live reading sizes a launch only once the host has stopped returning memory, or when it
    /// can never say (no reading).
    fn settled(self) -> bool {
        matches!(self, Self::Steady { .. } | Self::Unreadable)
    }
}

/// What a launch with no engine on its port does now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LaunchGate {
    /// Launch at the computed pool.
    Launch,
    /// The pool is below what the seated minds need and the host is still returning memory (or has
    /// not been read twice): the next reconcile sizes it from the settled reading.
    AwaitSettle { pool: u64, need: u64, settle: HostSettle },
}

/// Whether a launch of `pool` tokens may go now, beside seated minds needing `need` in all. A pool
/// that holds their need never waits; neither does one on a settled host, which is the most the
/// host holds.
pub(super) fn launch_gate(settle: HostSettle, pool: u64, need: u64) -> LaunchGate {
    match pool >= need || settle.settled() {
        true => LaunchGate::Launch,
        false => LaunchGate::AwaitSettle { pool, need, settle },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reading(compressed_bytes: u64) -> HostMemoryReading {
        HostMemoryReading {
            free_bytes: 0,
            reclaimable_bytes: 0,
            compressed_bytes,
            swapped_in_bytes: 0,
            swapped_out_bytes: 0,
            free_target_bytes: 0,
            pressure: None,
        }
    }

    // what this catches: a launch sized while memory is still coming back. The compressor shrinking
    // between readings holds a pool below the minds' need; the first reading (nothing to compare)
    // holds it too; a compressor that stopped shrinking, or a host with no reading, launches.
    #[test]
    fn a_launch_below_the_need_waits_only_while_the_host_returns_memory() {
        const GIB: u64 = 1 << 30;
        let first = HostSettle::Unread.observe(Some(&reading(34 * GIB)));
        assert!(matches!(launch_gate(first, 2_048, 400_000), LaunchGate::AwaitSettle { .. }), "one reading compares with nothing");
        let returning = first.observe(Some(&reading(20 * GIB)));
        assert!(matches!(launch_gate(returning, 2_048, 400_000), LaunchGate::AwaitSettle { .. }), "the compressor shrank");
        assert_eq!(launch_gate(returning, 400_000, 400_000), LaunchGate::Launch, "a pool that holds the need never waits");
        let steady = returning.observe(Some(&reading(20 * GIB)));
        assert_eq!(launch_gate(steady, 2_048, 400_000), LaunchGate::Launch, "a settled host launches what it holds");
        assert_eq!(steady.observe(Some(&reading(21 * GIB))), HostSettle::Steady { compressed_bytes: 21 * GIB }, "a growing compressor is not memory coming back");
        assert_eq!(launch_gate(returning.observe(None), 2_048, 400_000), LaunchGate::Launch, "no reading can never settle: never a dark node");
    }
}
