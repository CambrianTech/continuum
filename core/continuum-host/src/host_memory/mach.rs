//! Mach page counts to a reading. Pure, so it is tested on every host; `macos.rs` fills it from
//! `host_statistics64(HOST_VM_INFO64)` and the `vm.vm_page_free_target` sysctl.
//!
//! The accounting is Activity Monitor's: Mach's `free_count` includes speculative pages (which
//! `vm_stat` subtracts for "Pages free"), and speculative read-ahead is file-backed, so it is
//! counted once, in "Cached Files" (`external_page_count + purgeable_count`).

use super::HostMemoryReading;

/// The `vm_statistics64` fields a reading needs, as page counts, plus the page size and the
/// kernel's free target in pages.
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
        }
        .reading();
        assert_eq!(reading.free_bytes, (256_602 - 193_717) * page);
        assert_eq!(reading.reclaimable_bytes, (373_221 + 656) * page);
        assert_eq!(reading.compressed_bytes, 398_429 * page, "occupied, not stored: ~6.5 GB");
        assert_eq!(reading.free_target_bytes, 4_000 * page, "64 MB, the pageout daemon's target");
        assert_eq!(reading.swapped_out_bytes, 444_562_085 * page);
    }
}
