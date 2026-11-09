//! Linux: `/proc/<pid>/stat` fields 14 (`utime`) and 15 (`stime`), in clock ticks of
//! `sysconf(_SC_CLK_TCK)`. Field 2 (`comm`) is parenthesised and may hold spaces, so the fields
//! are counted from after its last `)`, where field 3 (`state`) is the first.

use std::time::Duration;

/// `utime`'s position among the fields after `comm`: field 14, counted from field 3.
const UTIME_AFTER_COMM: usize = 14 - 3;

pub(super) fn cpu_time(pid: u32) -> Option<Duration> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let after_comm = stat.get(stat.rfind(')')? + 1..)?;
    let mut fields = after_comm.split_whitespace();
    let utime: u64 = fields.nth(UTIME_AFTER_COMM)?.parse().ok()?;
    let stime: u64 = fields.next()?.parse().ok()?;
    // SAFETY: sysconf reads a configuration value; it has no memory effects.
    let hz = u64::try_from(unsafe { libc::sysconf(libc::_SC_CLK_TCK) }).ok().filter(|hz| *hz > 0)?;
    let nanos = u128::from(utime.saturating_add(stime)) * Duration::from_secs(1).as_nanos() / u128::from(hz);
    u64::try_from(nanos).ok().map(Duration::from_nanos)
}
