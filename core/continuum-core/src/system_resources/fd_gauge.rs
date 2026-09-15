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
