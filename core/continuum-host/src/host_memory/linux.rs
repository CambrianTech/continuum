//! Linux: `/proc/meminfo`, `/proc/vmstat`, `/proc/sys/vm/min_free_kbytes` and the PSI file
//! `/proc/pressure/memory` (absent on a kernel built without PSI), converted by `from_proc_text`.

use super::{from_proc_text, HostMemoryReading};

pub(super) fn read() -> Option<HostMemoryReading> {
    let meminfo = std::fs::read_to_string("/proc/meminfo").ok()?;
    let vmstat = std::fs::read_to_string("/proc/vmstat").ok()?;
    let reserve = std::fs::read_to_string("/proc/sys/vm/min_free_kbytes").ok()?;
    let psi = std::fs::read_to_string("/proc/pressure/memory").ok();
    // SAFETY: sysconf reads a configuration value; it has no memory effects.
    let page_size = u64::try_from(unsafe { libc::sysconf(libc::_SC_PAGESIZE) }).ok().filter(|p| *p > 0)?;
    from_proc_text(&meminfo, &vmstat, &reserve, psi.as_deref(), page_size)
}
