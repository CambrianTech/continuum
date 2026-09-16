//! HOST LOAD — is the box busy with something other than serving?
//!
//! One reader for "the host is contended right now": the 1-minute load average against
//! the logical core count. A measurement of the serving lane taken while `rustc -j18`
//! or a cold boot saturates the cores says nothing about the lane (2026-09-16: the
//! decode knee read 2 in flight at 0.18 of catalog during a `continuum reboot` build
//! and clamped seventeen minds onto two lanes). Consumers ask [`HostLoad::read`] once
//! per sample — a `getloadavg` syscall, never a monitor.

/// Load above this share of the cores is contention from outside the serving lane
/// (the core + one llama-server idle at ~3–5 on a quiet box; a build runs at ≥ cores).
pub const CONTENDED_SHARE: f64 = 0.5;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HostLoad {
    /// 1-minute load average.
    pub load1: f64,
    pub cores: u32,
}

impl HostLoad {
    pub fn read() -> Self {
        let load1 = sysinfo::System::load_average().one;
        let cores = std::thread::available_parallelism()
            .map(|n| n.get() as u32)
            .unwrap_or(1); // unwrap_or: the count is unknowable on this platform — one core keeps the share honest (contended), never a silent zero
        Self { load1, cores }
    }

    /// True when the run queue exceeds half the cores: a build, a boot, a test run —
    /// anything that makes a per-stream decode rate a measurement of the neighbour.
    pub fn is_contended(&self) -> bool {
        self.load1 > self.cores as f64 * CONTENDED_SHARE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the contention line is a share of the cores, not a fixed number —
    // 8 on an 18-core M5 is quiet, 8 on an 8-core Air is a build.
    #[test]
    fn contention_is_a_share_of_the_cores() {
        assert!(!HostLoad { load1: 8.0, cores: 18 }.is_contended());
        assert!(HostLoad { load1: 17.0, cores: 18 }.is_contended());
        assert!(HostLoad { load1: 8.0, cores: 8 }.is_contended());
        assert!(!HostLoad { load1: 3.5, cores: 8 }.is_contended());
    }
}
