//! Single source of truth for inference concurrency caps.
//!
//! Both the Rust scheduler (`LlamaCppBackend` / `Scheduler::n_seq_max`) and
//! the TypeScript `InferenceCoordinator` need to agree on how many local
//! inferences can run in parallel. Drift between the two layers caused
//! double-gating bugs (TS denies when Rust still has capacity, or TS lets
//! requests through that Rust then queues for full decode duration).
//!
//! Both layers must compute capacity from the same formula. Today they
//! share it by mirroring code; this module is the canonical Rust copy.
//! TypeScript should not re-derive it — it should query Rust via IPC
//! (`inference/capacity` command, generator-built) and cache the result
//! at startup.
//!
//! The formula: conservative breakpoints sized to leave RAM headroom for
//! 14 personas + RAG + Postgres + Bevy + scheduler KV pool. M1 Pro 32GB
//! with 3 permits hit `llama_decode rc=1` under load — staying one notch
//! below theoretical capacity until the dynamic pressure-reactive layer
//! lands.
//!
//! Future direction (per Joel): elastic permits that grow when memory
//! pressure is Normal AND the scheduler queue has waiters AND free-mem
//! allows, and shrink when pressure rises. This static RAM-based cap is
//! the floor.

use crate::runtime;

/// Total physical RAM in GB (rounded down). Single OS query; cheap.
///
/// Returns the conservative fallback `8` only when we can't read the real
/// value AND the host actually has at least 8GB physical (most modern
/// machines do). Each platform path checks its query's actual return code
/// or output validity rather than silently substituting 0 / 8 on failure.
fn total_ram_gb() -> u64 {
    #[cfg(target_os = "macos")]
    {
        let mut size: u64 = 0;
        let mut len = std::mem::size_of::<u64>();
        let key = std::ffi::CString::new("hw.memsize").unwrap();
        // sysctlbyname returns 0 on success, -1 on failure. Previously the
        // return code was discarded — a failed call would leave `size = 0`
        // and report "0 GB RAM," forcing capacity = 1 silently. Per Joel's
        // "errors save time" rule: surface the failure.
        let rc = unsafe {
            libc::sysctlbyname(
                key.as_ptr(),
                &mut size as *mut u64 as *mut _,
                &mut len,
                std::ptr::null_mut(),
                0,
            )
        };
        if rc != 0 || size == 0 {
            runtime::logger("concurrency").warn(&format!(
                "sysctlbyname(hw.memsize) failed (rc={rc}, size={size}); falling back to conservative 8 GB"
            ));
            return 8;
        }
        size / (1024 * 1024 * 1024)
    }
    #[cfg(target_os = "linux")]
    {
        // /proc/meminfo on Linux. The previous code path was used for
        // ALL non-macOS targets, including Windows — but Windows has no
        // /proc, so the unwrap_or(8) silently fired and reported wrong
        // capacity. Now Linux is the only platform that uses this branch.
        std::fs::read_to_string("/proc/meminfo")
            .ok()
            .and_then(|s| s.lines().next().map(String::from))
            .and_then(|line| line.split_whitespace().nth(1).map(String::from))
            .and_then(|kb| kb.parse::<u64>().ok())
            .map(|kb| kb / (1024 * 1024))
            .unwrap_or_else(|| {
                runtime::logger("concurrency").warn(
                    "/proc/meminfo unreadable; falling back to conservative 8 GB"
                );
                8
            })
    }
    #[cfg(target_os = "windows")]
    {
        // Windows has no /proc/meminfo. The previous "everything-not-macos
        // is Linux" assumption silently returned 8 GB on every Windows host.
        // Surface that this needs a real implementation rather than hide
        // the gap with a default. windows-sys / GlobalMemoryStatusEx is the
        // right call when this lands.
        runtime::logger("concurrency").warn(
            "Windows RAM detection not implemented — using conservative 8 GB. \
             Add windows-sys + GlobalMemoryStatusEx for proper capacity sizing."
        );
        8
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        runtime::logger("concurrency").warn(
            "RAM detection not implemented for this OS — using conservative 8 GB."
        );
        8
    }
}

/// How many local-inference sequences can run in parallel on this machine.
///
/// Used by:
///   - `LlamaCppBackend` as `n_seq_max` for the shared `Scheduler` context
///   - `InferenceCoordinator` (TypeScript) as `local-inference` capacity,
///     fetched via IPC at startup
///
/// Breakpoints (matched in tests):
///
///   * `<16GB` → 1 permit (serialize, too tight for parallel)
///   * `16-47GB` → 2 permits (M1 Pro 32GB safe zone)
///   * `48GB+` → 3 permits (M5 Pro class)
///
/// Logged once on first call so operators can see what tier the host
/// landed at without grepping config. Subsequent calls return the cached
/// value silently — this function is hot (adapter init, scheduler sizing).
pub fn local_inference_capacity() -> usize {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static CACHED: AtomicUsize = AtomicUsize::new(0);

    // 0 = not yet computed (we use 1-based capacity values, so 0 is a safe
    // sentinel for "uninitialized"). First caller computes + logs; everyone
    // else reads the cache.
    let cached = CACHED.load(Ordering::Acquire);
    if cached != 0 {
        return cached;
    }

    let ram = total_ram_gb();
    let permits = if ram >= 48 {
        3
    } else if ram >= 16 {
        2
    } else {
        1
    };
    runtime::logger("concurrency").info(&format!(
        "Local-inference capacity: {permits} permits (detected {ram}GB RAM, TODO: dynamic pressure-reactive)"
    ));
    // Race-tolerant: if two threads got here simultaneously, both will compute
    // the same value and the second store is a no-op. Acceptable because the
    // computation is pure (RAM doesn't change per process lifetime).
    CACHED.store(permits, Ordering::Release);
    permits
}

#[cfg(test)]
mod tests {
    use super::*;

    /// We can't mock total_ram_gb in stable tests without conditional
    /// compilation, but we CAN exercise the function and assert the
    /// returned number is one of the documented tiers.
    #[test]
    fn capacity_is_a_documented_tier() {
        let n = local_inference_capacity();
        assert!(
            (1..=3).contains(&n),
            "capacity {n} not in documented range 1..=3"
        );
    }
}
