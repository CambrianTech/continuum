//! Per-command RSS tracking — surfaces which IPC commands leak memory.
//!
//! Split out of `ipc/mod.rs` (was 1288 LOC single-file dir, parallel-dir
//! smell flagged in claude-tab-1's audit broadcast 2026-05-18 19:40Z).
//! Pure observability — no behavioral wire impact. mod.rs callers use
//! the `pub(crate)` API to record + dump.

use std::collections::HashMap;
use std::sync::Mutex;

/// Get current process RSS in MB using macOS task_info API.
/// Returns actual resident memory (not peak like getrusage ru_maxrss).
#[cfg(target_os = "macos")]
pub(crate) fn current_rss_mb() -> u64 {
    #[repr(C)]
    struct MachTaskBasicInfo {
        virtual_size: u64,
        resident_size: u64,
        resident_size_max: u64,
        user_time_seconds: u32,
        user_time_microseconds: u32,
        system_time_seconds: u32,
        system_time_microseconds: u32,
        policy: i32,
        suspend_count: i32,
    }

    extern "C" {
        fn mach_task_self() -> u32;
        fn task_info(
            target_task: u32,
            flavor: u32,
            task_info: *mut MachTaskBasicInfo,
            task_info_count: *mut u32,
        ) -> i32;
    }

    const MACH_TASK_BASIC_INFO: u32 = 20;

    unsafe {
        let mut info: MachTaskBasicInfo = std::mem::zeroed();
        let mut count =
            (std::mem::size_of::<MachTaskBasicInfo>() / std::mem::size_of::<u32>()) as u32;
        let kr = task_info(
            mach_task_self(),
            MACH_TASK_BASIC_INFO,
            &mut info,
            &mut count,
        );
        if kr == 0 {
            info.resident_size / (1024 * 1024)
        } else {
            0
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn current_rss_mb() -> u64 {
    0 // No-op on non-macOS
}

/// Periodic RSS reporter — logs every 10s so we can see growth trends.
/// Also tracks per-command cumulative deltas to identify the leaker.
static COMMAND_MEMORY_DELTAS: once_cell::sync::Lazy<Mutex<HashMap<String, i64>>> =
    once_cell::sync::Lazy::new(|| Mutex::new(HashMap::new()));

pub(crate) fn log_command_rss_delta(command: &str, before_mb: u64, after_mb: u64) {
    let delta = after_mb as i64 - before_mb as i64;
    if delta > 0 {
        // Accumulate per-command
        if let Ok(mut map) = COMMAND_MEMORY_DELTAS.lock() {
            *map.entry(command.to_string()).or_insert(0) += delta;
        }
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
        let top: Vec<String> = entries
            .iter()
            .take(10)
            .map(|(cmd, delta)| format!("{}:+{}MB", cmd, delta))
            .collect();
        eprintln!("[MEMLEAK] RSS={}MB | Top leakers: {}", rss, top.join(", "));
    }
}
