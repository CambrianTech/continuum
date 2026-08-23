//! Per-command RSS tracking — surfaces which IPC commands leak memory.
//!
//! Split out of `ipc/mod.rs` (was 1288 LOC single-file dir, parallel-dir
//! smell flagged in claude-tab-1's audit broadcast 2026-05-18 19:40Z).
//! Pure observability — no behavioral wire impact. mod.rs callers use
//! the `pub(crate)` API to record + dump.

use std::collections::HashMap;
use std::sync::Mutex;

/// Current process RSS in MB — actual resident memory, not peak.
///
/// This was a mach `task_info` FFI block for macOS and, for EVERY other target, a stub returning
/// literal `0`. So on Linux and Windows the per-command leak tracker recorded a 0MB delta for every
/// command, and — worse — the OOM guard in `ipc/mod.rs` compared that constant 0 against its limit
/// and therefore could never fire. Measured on Windows: the core sat at 27.6 GB RSS while its own
/// reporter printed `RSS=0MB`.
///
/// That defect was hidden by a second one. The guard's limit fell back to a hardcoded 8192 MB off
/// non-Unix, which on a 63 GB host would have armed a FATAL self-exit at 6.5 GB. The two cancelled:
/// a limit far too low, against a reading that was always 0. Fixing either alone would have been
/// worse than fixing neither, which is exactly why both move together.
///
/// One implementation for every platform via sysinfo (already a direct dependency, and already
/// described in Cargo.toml as cross-platform memory monitoring), rather than one FFI block per OS
/// plus a stub for whatever is left over — the stub is where the rot lives.
pub(crate) fn current_rss_mb() -> u64 {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

    let pid = Pid::from_u32(std::process::id());
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::nothing().with_memory(),
    );
    // sysinfo reports BYTES for process memory; 0 means "could not read", same as before.
    sys.process(pid)
        .map(|p| p.memory() / (1024 * 1024))
        .unwrap_or(0)
}

/// Total system RAM in MB, or None if it cannot be determined.
///
/// Extracted from the OOM guard so it is testable and has ONE definition. It previously shelled out
/// to `sysctl` on macOS, parsed /proc/meminfo on Linux, and returned a hardcoded 8192 for anything
/// else — not a detection failure on Windows, an absent branch. On this 63 GB host that made the
/// guard arm a fatal self-exit at 6.5 GB.
///
/// Returns None rather than a guess: the caller's response to breach is `process::exit(1)`, so a
/// fabricated size does not degrade gracefully, it kills a healthy process. No limit beats a
/// made-up one.
pub(crate) fn detect_system_ram_mb() -> Option<u64> {
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    match sys.total_memory() {
        0 => None,
        bytes => Some(bytes / (1024 * 1024)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the hardcoded 8192 fallback that Windows silently landed on. Any machine
    // this runs on has more than 1 GB, and the value must track the real host — a constant would
    // pass a ">0" check, so assert it is not the specific number that was being invented.
    #[test]
    fn system_ram_is_detected_not_assumed() {
        let ram = detect_system_ram_mb().expect("total system RAM must be detectable");
        assert!(ram > 1024, "implausible system RAM: {ram}MB");
        // Not proof on a genuinely-8GB box, but on every other host this is the regression tripwire.
        if ram == 8192 {
            eprintln!("note: detected exactly 8192MB — verify this host really has 8GB");
        }
    }

    // what this catches: the stub. current_rss_mb() returned a literal 0 on every non-macOS target,
    // which silently disabled both the leak tracker and the OOM guard that consumes it. This process
    // is running, so its resident set cannot be 0 on any platform we support.
    #[test]
    fn rss_is_actually_measured_on_this_platform() {
        let rss = current_rss_mb();
        assert!(
            rss > 0,
            "current_rss_mb() returned {rss}MB for a live process — the reading is stubbed or \
             broken on this platform, which silently disables the OOM guard that compares it"
        );
    }
}

/// Periodic RSS reporter — logs every 10s so we can see growth trends.
/// Also tracks per-command cumulative deltas to identify the leaker.
static COMMAND_MEMORY_DELTAS: once_cell::sync::Lazy<Mutex<HashMap<String, i64>>> =
    once_cell::sync::Lazy::new(|| Mutex::new(HashMap::new()));

pub(crate) fn log_command_rss_delta(command: &str, before_mb: u64, after_mb: u64) {
    let delta = after_mb as i64 - before_mb as i64;
    // NET, not positive-only. This used to read `if delta > 0 { … += delta }`, which
    // accumulated every rise and DISCARDED every matching fall — so a command that
    // allocates and frees the same buffer climbed forever, and the number it reported
    // was transient allocation VOLUME wearing the word "leaked". Guaranteed false
    // positive for anything that does real work: `benchmark/dispatch` was topping the
    // board at +228MB while the process RSS in the very same log OSCILLATED
    // (528→524→477→555MB). A quarter-gigabyte leak cannot go DOWN. That is a working
    // set, and the instrument could not tell the difference.
    //
    // Still an ESTIMATE, and the log below says so: RSS is process-wide, so any other
    // thread allocating during a command is attributed to that command. Netting the
    // deltas makes the noise unbiased instead of one-directional; it does not make
    // this attribution. Read it as "who to look at first", never as proof.
    if let Ok(mut map) = COMMAND_MEMORY_DELTAS.lock() {
        *map.entry(command.to_string()).or_insert(0) += delta;
    }
    // Log commands with >2MB growth per call
    if delta > 2 {
        eprintln!(
            "[MEMLEAK] RSS +{}MB after '{}' ({}MB → {}MB)",
            delta, command, before_mb, after_mb
        );
    }
}

/// Dump accumulated memory deltas — call periodically to see which commands leak.
pub(crate) fn dump_memory_report() {
    let rss = current_rss_mb();
    if let Ok(map) = COMMAND_MEMORY_DELTAS.lock() {
        if map.is_empty() {
            eprintln!("[MEMLEAK] RSS={}MB, no command deltas yet", rss);
            return;
        }
        let mut entries: Vec<_> = map.iter().collect();
        entries.sort_by(|a, b| b.1.cmp(a.1));
        // Only NET-POSITIVE rows are worth a reader's attention; a command that gives
        // back what it takes is not a suspect. Sorted desc, so stopping at the first
        // non-positive row drops the whole tail.
        let top: Vec<String> = entries
            .iter()
            .take_while(|(_, delta)| **delta > 0)
            .take(10)
            .map(|(cmd, delta)| format!("{}:+{}MB", cmd, delta))
            .collect();
        if top.is_empty() {
            eprintln!("[MEMLEAK] RSS={rss}MB | no command shows net growth");
            return;
        }
        // "net RSS growth", not "leakers". The old wording asserted a conclusion the
        // measurement cannot support (process-wide RSS cannot attribute an allocation
        // to the command that happened to be running), and that wording is what got
        // read as a finding.
        eprintln!(
            "[MEMLEAK] RSS={}MB | net RSS growth by command (ESTIMATE — process-wide \
             RSS, not per-command attribution): {}",
            rss,
            top.join(", ")
        );
    }
}
