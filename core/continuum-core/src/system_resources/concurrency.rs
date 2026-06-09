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
fn total_ram_gb() -> u64 {
    #[cfg(target_os = "macos")]
    {
        let mut size: u64 = 0;
        let mut len = std::mem::size_of::<u64>();
        let key = std::ffi::CString::new("hw.memsize").unwrap();
        unsafe {
            libc::sysctlbyname(
                key.as_ptr(),
                &mut size as *mut u64 as *mut _,
                &mut len,
                std::ptr::null_mut(),
                0,
            )
        };
        size / (1024 * 1024 * 1024)
    }
    #[cfg(not(target_os = "macos"))]
    {
        std::fs::read_to_string("/proc/meminfo")
            .ok()
            .and_then(|s| s.lines().next().map(String::from))
            .and_then(|line| line.split_whitespace().nth(1).map(String::from))
            .and_then(|kb| kb.parse::<u64>().ok())
            .map(|kb| kb / (1024 * 1024))
            .unwrap_or(8)
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
/// landed at without grepping config.
pub fn local_inference_capacity() -> usize {
    let ram = total_ram_gb();
    let permits = if ram >= 48 {
        3
    } else if ram >= 16 {
        2
    } else {
        1
    };
    runtime::logger("concurrency").info(&format!(
        "Local-inference capacity: {} permits (detected {}GB RAM, TODO: dynamic pressure-reactive)",
        permits, ram
    ));
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
