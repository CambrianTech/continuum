//! What the host OS says about its memory beyond "used": how much is genuinely free, how much
//! the kernel can drop without paging anyone (file-backed cache), how much it holds COMPRESSED,
//! how many bytes it has swapped in and out since boot, and its own free target (the level below
//! which its pageout daemon starts working). A used/total ratio reads none of this: on the M5 on
//! 2026-11-13, 60 MB were free, ~34 GB sat in the compressor and swap was 8.6 of 10 GB while
//! "used" stayed under every gate, so nothing was shed and prefill ran at half speed.
//!
//! What differs by OS is only where the counters come from: macOS reads Mach
//! `host_statistics64(HOST_VM_INFO64)` and the `vm.vm_page_free_target` sysctl, Linux reads
//! `/proc/meminfo`, `/proc/vmstat` and `/proc/sys/vm/min_free_kbytes`. The two text and count
//! formats are converted by pure functions (`mach`, `proc_text`) that compile everywhere, so each
//! is tested on every host; the `os` adapter holds only the calls. Elsewhere there is no reading,
//! which callers hold as unknown, never as calm.

mod mach;
mod proc_text;

pub use mach::MachVmCounts;
pub use proc_text::from_proc_text;

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
#[cfg_attr(not(any(target_os = "macos", target_os = "linux")), path = "other.rs")]
mod os;

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
    // state (4 GiB free, 2 GiB cache, 1 GiB compressed, 64 MiB target, 3/5 GiB swapped in/out)
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
        let linux = from_proc_text(&meminfo, &vmstat, &format!("{}\n", kib(64 * MIB)), linux_page);
        assert_eq!(Some(mach), linux);
        assert_eq!(mach.available_bytes(), 6 * GIB);
        assert_eq!(mach.shortfall_bytes(), 0, "6 GiB available is above a 64 MiB target");
    }
}
