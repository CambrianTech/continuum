//! What the host OS says about its memory beyond "used": how much is genuinely free, how much
//! the kernel can drop without paging anyone (file-backed cache), how much it holds COMPRESSED,
//! how many bytes it has swapped in and out since boot, its own free target (the level below
//! which its pageout daemon starts working), and its own verdict on whether it is under memory
//! pressure (`OsMemoryPressure`). A used/total ratio reads none of this: on the M5 on
//! 2026-11-13, 60 MB were free, ~34 GB sat in the compressor and swap was 8.6 of 10 GB while
//! "used" stayed under every gate, so nothing was shed and prefill ran at half speed.
//!
//! What differs by OS is only where the counters come from: macOS reads Mach
//! `host_statistics64(HOST_VM_INFO64)` and the `vm.vm_page_free_target` and
//! `kern.memorystatus_vm_pressure_level` sysctls, Linux reads `/proc/meminfo`, `/proc/vmstat`,
//! `/proc/sys/vm/min_free_kbytes` and the PSI file `/proc/pressure/memory`. The two text and count
//! formats are converted by pure functions (`mach`, `proc_text`) that compile everywhere, so each
//! is tested on every host; the `os` adapter holds only the calls. Elsewhere there is no reading,
//! which callers hold as unknown, never as calm.

mod mach;
mod proc_text;

pub use mach::MachVmCounts;
pub use proc_text::{from_proc_text, pressure_from_psi};

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
#[cfg_attr(not(any(target_os = "macos", target_os = "linux")), path = "other.rs")]
mod os;

/// The OS's own verdict on its memory, from its own measurement: never derived from our counters.
///
/// Why it exists: on the M5 on 2026-11-13 (23:40Z) the kernel held its free pages at its small
/// free target BY compressing, so the compressor (42 GB) and swap (14.5 of 15 GB) stopped growing
/// and free memory read above the target: every counter rule read the starved host as clean. The
/// kernel itself knew: its pressure level is what it acts on (macOS
/// `kern.memorystatus_vm_pressure_level`; Linux PSI, `/proc/pressure/memory`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OsMemoryPressure {
    /// The OS reads itself out of memory pressure.
    Normal,
    /// The OS is working to find memory (macOS level 2; Linux: some task stalled on memory in
    /// the last 10 s).
    Warn,
    /// The OS is starved (macOS level 4; Linux: every non-idle task stalled on memory at once in
    /// the last 10 s).
    Critical,
}

/// One reading of the host's memory, every field in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostMemoryReading {
    /// Pages nobody holds (macOS: free minus speculative, as `vm_stat` prints "Pages free";
    /// Linux: `MemFree`).
    pub free_bytes: u64,
    /// Pages the kernel can drop without paging anyone out: file-backed cache and purgeable
    /// memory (macOS: Activity Monitor's "Cached Files", external + purgeable pages; Linux: the
    /// kernel's own estimate, `MemAvailable - MemFree`).
    pub reclaimable_bytes: u64,
    /// Physical memory the OS compressor occupies (macOS: `compressor_page_count`, `vm_stat`'s
    /// "Pages occupied by compressor"; Linux: zswap's `Zswap`, 0 where the kernel compresses
    /// nothing, and zram appears as swap).
    pub compressed_bytes: u64,
    /// Cumulative bytes swapped in since boot.
    pub swapped_in_bytes: u64,
    /// Cumulative bytes swapped out since boot.
    pub swapped_out_bytes: u64,
    /// The kernel's own free target: below it, it reclaims (macOS `vm.vm_page_free_target`
    /// pages; Linux `min_free_kbytes`, its reserve).
    pub free_target_bytes: u64,
    /// The OS's own pressure verdict. `None` = this OS gives none (a Linux kernel built without
    /// PSI, an unknown macOS level): no verdict, never calm.
    pub pressure: Option<OsMemoryPressure>,
}

impl HostMemoryReading {
    /// Memory available to us without the OS displacing anyone: free plus reclaimable.
    pub fn available_bytes(&self) -> u64 {
        self.free_bytes.saturating_add(self.reclaimable_bytes)
    }

    /// Bytes the OS still needs freed to get back to its own free target; 0 at or above it.
    pub fn shortfall_bytes(&self) -> u64 {
        self.free_target_bytes.saturating_sub(self.available_bytes())
    }
}

/// The host's memory now. `None` = no reading (a failed call, or an OS with no adapter): unknown,
/// never calm.
pub fn read() -> Option<HostMemoryReading> {
    os::read()
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the two adapters disagreeing about what a field means. The same host
    // state (4 GiB free, 2 GiB cache, 1 GiB compressed, 64 MiB target, 3/5 GiB swapped in/out,
    // the OS reading itself normal)
    // expressed as Mach page counts and as /proc text must convert to the SAME reading, so one
    // deficit rule above serves both.
    #[test]
    fn mach_counts_and_proc_text_convert_to_the_same_reading() {
        const GIB: u64 = 1 << 30;
        const MIB: u64 = 1 << 20;
        let page = 16_384;
        let pages = |bytes: u64| bytes / page;
        let mach = MachVmCounts {
            page_size: page,
            free_count: pages(4 * GIB) + pages(256 * MIB),
            speculative_count: pages(256 * MIB),
            external_page_count: pages(2 * GIB) - pages(MIB),
            purgeable_count: pages(MIB),
            compressor_page_count: pages(GIB),
            swapins: pages(3 * GIB),
            swapouts: pages(5 * GIB),
            free_target_pages: pages(64 * MIB),
            pressure: Some(OsMemoryPressure::Normal),
        }
        .reading();
        let kib = |bytes: u64| bytes / 1024;
        let meminfo = format!(
            "MemTotal: {} kB\nMemFree: {} kB\nMemAvailable: {} kB\nZswap: {} kB\n",
            kib(64 * GIB),
            kib(4 * GIB),
            kib(6 * GIB),
            kib(GIB)
        );
        let linux_page = 4096;
        let vmstat = format!("pswpin {}\npswpout {}\n", 3 * GIB / linux_page, 5 * GIB / linux_page);
        let psi = "some avg10=0.00 avg60=0.00 avg300=0.00 total=0\nfull avg10=0.00 avg60=0.00 avg300=0.00 total=0\n";
        let linux = from_proc_text(&meminfo, &vmstat, &format!("{}\n", kib(64 * MIB)), Some(psi), linux_page);
        assert_eq!(Some(mach), linux);
        assert_eq!(mach.available_bytes(), 6 * GIB);
        assert_eq!(mach.shortfall_bytes(), 0, "6 GiB available is above a 64 MiB target");
        assert_eq!(mach.pressure, Some(OsMemoryPressure::Normal));
    }
}
