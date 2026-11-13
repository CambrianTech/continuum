//! The process's own file-descriptor table, as a gauge with a named alert.
//!
//! 2026-09-12 00:57–01:13Z on 48fbbfd44: the core reached 19,866 open descriptors
//! (19,589 of them unix sockets to the airc daemon) sixteen minutes after boot.
//! Every spawn then failed with EBADF — the serving lane's `--list-devices`
//! probe, `web/fetch`'s browser, the lane relaunch itself — and every reader
//! saw a different symptom (a placement violation, a Chrome error, a silent
//! roster). Nothing named the table. This does: a count on every memory tick,
//! and a `process.open_fds.high` probe once it passes half the soft limit,
//! before the node goes dark instead of after.

/// Open descriptors right now, counted from `/dev/fd` (macOS and Linux both
/// expose the calling process's table there). `None` when it cannot be listed.
pub fn open_fds() -> Option<usize> {
    let n = std::fs::read_dir("/dev/fd").ok()?.count();
    // the directory handle used for the listing is itself one entry
    Some(n.saturating_sub(1))
}

/// The soft `RLIMIT_NOFILE` — the ceiling at which the next pipe or socket
/// fails. `None` when the limit is unlimited or cannot be read.
pub fn fd_soft_limit() -> Option<u64> {
    #[cfg(unix)]
    {
        let mut lim = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
        // SAFETY: getrlimit writes into the struct we own and reads nothing else.
        let rc = unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut lim) };
        if rc != 0 || lim.rlim_cur == libc::RLIM_INFINITY {
            return None;
        }
        Some(lim.rlim_cur as u64)
    }
    #[cfg(not(unix))]
    {
        None
    }
}

/// Raise the process's soft `RLIMIT_NOFILE` at startup so the core never inherits a
/// low ceiling from its launcher and wedges on ORDINARY operation.
///
/// 2026-09-17: the M5 core inherited a ~362 soft limit (launchd/shell default), hit
/// "Too many open files (os error 24)" on normal load — 148 airc unix sockets + 204
/// sqlite/file handles — its IPC listener died on the error and could not recover, and
/// `track-canary` could not even see the wedged core to redeploy it. The node was dark
/// for hours. The core opens many sockets and sqlite handles BY DESIGN (it is the grid's
/// hub), so it must set its OWN ceiling rather than depend on whatever launched it —
/// the same self-reliance the [[reliable-out-of-the-box-recover-itself-on-everyones-machine]]
/// law asks of every signal owner. This is the ceiling; [`FdGauge`]'s `open_fds.high`
/// probe is still the early warning that a genuine LEAK is climbing toward it.
///
/// Tries a high target and steps down until one takes, so it is correct on Linux (a
/// concrete hard cap) AND macOS (the kernel caps a per-process soft limit at
/// `kern.maxfilesperproc`, and a `setrlimit` above it is refused rather than clamped).
/// Never lowers an already-higher soft limit. Returns the soft limit in force afterward.
pub fn raise_fd_soft_limit() -> Option<u64> {
    #[cfg(unix)]
    {
        let mut lim = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
        // SAFETY: getrlimit writes into the struct we own and reads nothing else.
        if unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut lim) } != 0 {
            return None;
        }
        let cap = if lim.rlim_max == libc::RLIM_INFINITY {
            u64::MAX
        } else {
            lim.rlim_max as u64
        };
        for target in [1_048_576u64, 262_144, 65_536, 16_384, 10_240] {
            let want = target.min(cap);
            if want <= lim.rlim_cur as u64 {
                // Already at or above this target (never lower a good limit).
                break;
            }
            let mut next = lim;
            next.rlim_cur = want as libc::rlim_t;
            // SAFETY: setrlimit reads the struct we own; raising the SOFT limit up to the
            // existing HARD limit needs no privilege.
            if unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &next) } == 0 {
                lim = next;
                break;
            }
        }
        // Report what actually took (a kernel cap can be below what we asked).
        if unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut lim) } != 0 {
            return None;
        }
        (lim.rlim_cur != libc::RLIM_INFINITY).then_some(lim.rlim_cur as u64)
    }
    #[cfg(not(unix))]
    {
        // Windows has no RLIMIT_NOFILE; its handle ceiling is orders of magnitude higher.
        None
    }
}

/// Above this share of the soft limit the gauge raises `process.open_fds.high`.
pub const HIGH_WATER_PCT: u64 = 50;

/// One reading, as the tick emits it. Pure over its inputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FdReading {
    pub open: usize,
    pub soft_limit: Option<u64>,
}

impl FdReading {
    pub fn take() -> Option<Self> {
        Some(Self { open: open_fds()?, soft_limit: fd_soft_limit() })
    }

    /// Percent of the soft limit in use; `None` without a limit.
    pub fn pct(&self) -> Option<u64> {
        let lim = self.soft_limit?;
        if lim == 0 {
            return None;
        }
        Some(self.open as u64 * 100 / lim)
    }

    pub fn is_high(&self) -> bool {
        self.pct().is_some_and(|p| p >= HIGH_WATER_PCT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (2026-09-17 fd wedge): `raise_fd_soft_limit` NEVER lowers the
    // soft limit and reports the limit in force. The M5 core inherited a ~362 soft limit
    // and wedged on "Too many open files (os error 24)" during ordinary operation — the
    // core must set its OWN ceiling at startup rather than depend on the launcher.
    // Deterministic: whatever the test process's limit is, after the raise it is >= before.
    #[cfg(unix)]
    #[test]
    fn raise_fd_soft_limit_never_lowers_the_limit() {
        let before = fd_soft_limit();
        let after = raise_fd_soft_limit();
        if let (Some(b), Some(a)) = (before, after) {
            assert!(a >= b, "the raise must never LOWER the fd soft limit: {a} < {b}");
        }
    }

    // what this catches: the gauge reading nothing on the platforms it exists
    // for (a `/dev/fd` that lists nothing, a limit read that always fails) and the
    // high-water arithmetic drifting — the alert must fire at exactly the share
    // the constant names.
    #[test]
    fn the_gauge_reads_this_process_and_names_the_high_water_mark() {
        let r = FdReading::take().expect("this process can count its own descriptors");
        assert!(r.open >= 3, "stdin/stdout/stderr at least: {r:?}");
        let half = FdReading { open: 50, soft_limit: Some(100) };
        assert_eq!(half.pct(), Some(50));
        assert!(half.is_high());
        let low = FdReading { open: 49, soft_limit: Some(100) };
        assert!(!low.is_high());
        assert_eq!(FdReading { open: 5, soft_limit: None }.pct(), None);
    }
}
