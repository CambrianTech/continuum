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

/// The ANONYMOUS memory `pid` holds now: dirty plus compressed or swapped, mapped files excluded
/// (macOS `ri_phys_footprint`, Linux `RssAnon + VmSwap`). The current reading only; a caller that
/// wants a recent maximum keeps its own window. `None` = no reading (no such pid, or an OS with
/// no adapter): unknown, never zero.
pub fn anon_footprint(pid: u32) -> Option<u64> {
    os::anon_footprint(pid)
}

/// The argv `pid` was launched with (macOS `KERN_PROCARGS2`, Linux `/proc/<pid>/cmdline`).
/// `None` = no reading (no such pid, or an OS with no adapter).
pub fn args(pid: u32) -> Option<Vec<String>> {
    os::args(pid)
}

/// PURE: the argv inside a `KERN_PROCARGS2` buffer: an i32 argc, the exec path, NUL padding,
/// then argc NUL-terminated args. Parsing is the same on every host; only macOS produces it.
pub fn parse_procargs2(buf: &[u8]) -> Option<Vec<String>> {
    let argc = usize::try_from(i32::from_ne_bytes(buf.get(..4)?.try_into().ok()?)).ok()?;
    let mut rest = buf.get(4..)?;
    // Skip the exec path and the NUL padding after it.
    let path_end = rest.iter().position(|b| *b == 0)?;
    rest = rest.get(path_end..)?;
    let first = rest.iter().position(|b| *b != 0)?;
    rest = rest.get(first..)?;
    let args: Vec<String> = rest
        .split(|b| *b == 0)
        .take(argc)
        .map(|a| String::from_utf8_lossy(a).into_owned())
        .collect();
    (args.len() == argc).then_some(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (window 45 on the M5, 2026-11-04: a spinning process read 0% CPU, which
    // would have read every busy engine as stopped): the counter advances while this process
    // spins, wherever an adapter reads it.
    #[test]
    fn a_procargs2_buffer_yields_its_argv() {
        // what this catches: the exec path or its NUL padding read as an argument.
        let mut buf = 2i32.to_ne_bytes().to_vec();
        buf.extend_from_slice(b"/bin/llama-server\0\0\0-m\0/w/a.gguf\0ENV=1\0");
        assert_eq!(parse_procargs2(&buf), Some(vec!["-m".to_string(), "/w/a.gguf".to_string()]));
        assert_eq!(parse_procargs2(&[1, 0]), None, "too short for argc");
    }

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
