//! Mach page counts to a reading. Pure, so it is tested on every host; `macos.rs` fills it from
//! `host_statistics64(HOST_VM_INFO64)` and the `vm.vm_page_free_target` and
//! `kern.memorystatus_vm_pressure_level` sysctls.
//!
//! The accounting is Activity Monitor's: Mach's `free_count` includes speculative pages (which
//! `vm_stat` subtracts for "Pages free"), and speculative read-ahead is file-backed, so it is
//! counted once, in "Cached Files" (`external_page_count + purgeable_count`).

use super::{HostMemoryReading, OsMemoryPressure};

/// The `vm_statistics64` fields a reading needs, as page counts, plus the page size, the
/// kernel's free target in pages, and its memory-pressure verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MachVmCounts {
    pub page_size: u64,
    pub free_count: u64,
    pub speculative_count: u64,
    pub external_page_count: u64,
    pub purgeable_count: u64,
    pub compressor_page_count: u64,
    pub swapins: u64,
    pub swapouts: u64,
    pub free_target_pages: u64,
    /// The kernel's verdict from `kern.memorystatus_vm_pressure_level`, mapped once where it is read
    /// ([`OsMemoryPressure::from_memorystatus_level`]); `None` = the sysctl could not be read, or
    /// gave a level that is no verdict.
    pub pressure: Option<OsMemoryPressure>,
}

impl OsMemoryPressure {
    /// The kernel's memorystatus level as a verdict. Any other value is no verdict.
    pub fn from_memorystatus_level(level: u32) -> Option<Self> {
        match level {
            1 => Some(Self::Normal),
            2 => Some(Self::Warn),
            4 => Some(Self::Critical),
            _ => None,
        }
    }
}

impl MachVmCounts {
    pub fn reading(&self) -> HostMemoryReading {
        let bytes = |pages: u64| pages.saturating_mul(self.page_size);
        HostMemoryReading {
            free_bytes: bytes(self.free_count.saturating_sub(self.speculative_count)),
            reclaimable_bytes: bytes(self.external_page_count.saturating_add(self.purgeable_count)),
            compressed_bytes: bytes(self.compressor_page_count),
            swapped_in_bytes: bytes(self.swapins),
            swapped_out_bytes: bytes(self.swapouts),
            free_target_bytes: bytes(self.free_target_pages),
            pressure: self.pressure,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: speculative pages counted twice (once in free, once in cached files),
    // which is what free + speculative + inactive did; and a compressor read as its uncompressed
    // size. Fixture: `vm_stat` and `sysctl vm.vm_page_free_target` on the M5, 2026-11-13.
    #[test]
    fn an_m5_vm_stat_reads_as_activity_monitor_accounts_it() {
        let page = 16_384;
        let reading = MachVmCounts {
            page_size: page,
            free_count: 256_602,
            speculative_count: 193_717,
            external_page_count: 373_221,
            purgeable_count: 656,
            compressor_page_count: 398_429,
            swapins: 289_360_556,
            swapouts: 444_562_085,
            free_target_pages: 4_000,
            pressure: OsMemoryPressure::from_memorystatus_level(4),
        }
        .reading();
        assert_eq!(reading.free_bytes, (256_602 - 193_717) * page);
        assert_eq!(reading.reclaimable_bytes, (373_221 + 656) * page);
        assert_eq!(reading.compressed_bytes, 398_429 * page, "occupied, not stored: ~6.5 GB");
        assert_eq!(reading.free_target_bytes, 4_000 * page, "64 MB, the pageout daemon's target");
        assert_eq!(reading.swapped_out_bytes, 444_562_085 * page);
        assert_eq!(reading.pressure, Some(OsMemoryPressure::Critical), "the kernel's own verdict, level 4");
    }

    // what this catches: the kernel's pressure level misread (2 read as critical, an unknown value
    // or an unread sysctl read as calm). Only 1, 2 and 4 are verdicts; anything else is none.
    #[test]
    fn the_memorystatus_level_maps_to_the_kernels_verdict_and_nothing_else() {
        let verdict = OsMemoryPressure::from_memorystatus_level;
        assert_eq!((verdict(1), verdict(2), verdict(4)), (Some(OsMemoryPressure::Normal), Some(OsMemoryPressure::Warn), Some(OsMemoryPressure::Critical)));
        assert_eq!((verdict(0), verdict(3), verdict(8)), (None, None, None), "no verdict is never calm");
        let unread = MachVmCounts { page_size: 16_384, free_count: 0, speculative_count: 0, external_page_count: 0, purgeable_count: 0, compressor_page_count: 0, swapins: 0, swapouts: 0, free_target_pages: 0, pressure: None };
        assert_eq!(unread.reading().pressure, None, "an unread sysctl is no verdict");
    }
}
