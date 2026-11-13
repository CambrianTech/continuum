//! What a process has used, read from the kernel by pid: its ACCUMULATED CPU time (user +
//! system), a counter that only moves forward while the process works. A usage percentage is not
//! this: sysinfo's per-process percentage on macOS needs the system-wide CPU refreshed alongside
//! it and read 0 for a spinning process (window 45 on the M5, 2026-11-04). What differs by OS is
//! only the call: macOS `proc_pid_rusage` (in Mach time units), Linux `/proc/<pid>/stat` (in
//! clock ticks); elsewhere no reading, which is unknown, never zero.

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
#[cfg_attr(not(any(target_os = "macos", target_os = "linux")), path = "other.rs")]
mod os;

/// The CPU time `pid` has accumulated since it started. `None` = no reading (no such pid, or an
/// OS with no adapter): unknown, never zero. The pid alone names no process; a caller fences the
/// reading by start time (`continuum_kernel`'s process table owner).
pub fn cpu_time(pid: u32) -> Option<std::time::Duration> {
    os::cpu_time(pid)
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (window 45 on the M5, 2026-11-04: a spinning process read 0% CPU, which
    // would have read every busy engine as stopped): the counter advances while this process
    // spins, wherever an adapter reads it.
    #[test]
    fn a_spinning_process_accumulates_cpu_time() {
        let Some(before) = cpu_time(std::process::id()) else {
            return; // no adapter on this OS: unknown is the contract, nothing to compare
        };
        let spin_until = std::time::Instant::now() + std::time::Duration::from_millis(400);
        let mut spun = 0u64;
        while std::time::Instant::now() < spin_until {
            spun = std::hint::black_box(spun.wrapping_add(1));
        }
        let after = cpu_time(std::process::id()).expect("test: read once, readable again");
        assert!(after > before, "the counter advanced: {before:?} -> {after:?}");
    }
}
