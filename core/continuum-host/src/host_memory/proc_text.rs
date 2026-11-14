//! Linux `/proc` text to a reading. Pure, so it is tested on every host; `linux.rs` reads the
//! files and the page size.

use super::{HostMemoryReading, OsMemoryPressure};

/// A reading from the text of `/proc/meminfo`, `/proc/vmstat` and
/// `/proc/sys/vm/min_free_kbytes`, with `/proc/vmstat`'s swap counters in pages of `page_size`,
/// and the kernel's pressure verdict from `/proc/pressure/memory` when the kernel has PSI.
/// `None` when a field every Linux kernel since 3.14 prints is missing (`MemFree`,
/// `MemAvailable`, `pswpin`, `pswpout`, the reserve): no reading, never a guessed one. `Zswap`
/// is printed only by kernels that account zswap; without it nothing is compressed in RAM.
pub fn from_proc_text(
    meminfo: &str,
    vmstat: &str,
    min_free_kbytes: &str,
    psi_memory: Option<&str>,
    page_size: u64,
) -> Option<HostMemoryReading> {
    let kib = |key: &str| field(meminfo, key).map(|kb| kb.saturating_mul(1024));
    let free = kib("MemFree:")?;
    let available = kib("MemAvailable:")?;
    let pages = |key: &str| field(vmstat, key).map(|n| n.saturating_mul(page_size));
    Some(HostMemoryReading {
        free_bytes: free,
        reclaimable_bytes: available.saturating_sub(free),
        compressed_bytes: kib("Zswap:").unwrap_or(0), // unwrap_or: a kernel without zswap accounting compresses nothing in RAM (zram is swap)
        swapped_in_bytes: pages("pswpin")?,
        swapped_out_bytes: pages("pswpout")?,
        free_target_bytes: min_free_kbytes.trim().parse::<u64>().ok()?.saturating_mul(1024),
        pressure: psi_memory.and_then(pressure_from_psi),
    })
}

/// The kernel's verdict from the text of `/proc/pressure/memory`: any task stalled on memory in
/// the last 10 s (`some avg10`) is Warn, every non-idle task stalled at once (`full avg10`) is
/// Critical, no stall is Normal. The kernel's own stall measurement, compared with zero, never a
/// fraction of ours. `None` when either line is missing or malformed.
pub fn pressure_from_psi(text: &str) -> Option<OsMemoryPressure> {
    let some = psi_avg10(text, "some")?;
    let full = psi_avg10(text, "full")?;
    Some(if full > 0.0 {
        OsMemoryPressure::Critical
    } else if some > 0.0 {
        OsMemoryPressure::Warn
    } else {
        OsMemoryPressure::Normal
    })
}

/// The `avg10=` value on the PSI line that starts with `kind`.
fn psi_avg10(text: &str, kind: &str) -> Option<f64> {
    text.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        if parts.next() != Some(kind) {
            return None;
        }
        parts.find_map(|part| part.strip_prefix("avg10="))?.parse().ok()
    })
}

/// The number after `key` on the line it starts.
fn field(text: &str, key: &str) -> Option<u64> {
    text.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        if parts.next() != Some(key) {
            return None;
        }
        parts.next()?.parse().ok()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the swap counters read in pages instead of bytes, `MemAvailable` read as
    // free (it already includes the cache), and a missing counter read as zero (calm).
    #[test]
    fn proc_text_is_read_in_bytes_and_a_missing_counter_is_no_reading() {
        let meminfo = "MemTotal: 1000 kB\nMemFree: 100 kB\nMemAvailable: 400 kB\nZswap: 50 kB\n";
        let vmstat = "nr_free_pages 1\npswpin 10\npswpout 30\npgfault 9\n";
        let reading = from_proc_text(meminfo, vmstat, "8\n", None, 4096).expect("test: every field present");
        assert_eq!(reading.free_bytes, 100 * 1024);
        assert_eq!(reading.reclaimable_bytes, 300 * 1024);
        assert_eq!(reading.compressed_bytes, 50 * 1024);
        assert_eq!((reading.swapped_in_bytes, reading.swapped_out_bytes), (10 * 4096, 30 * 4096));
        assert_eq!(reading.free_target_bytes, 8 * 1024);
        assert_eq!(reading.pressure, None, "a kernel without PSI gives no verdict");
        assert_eq!(from_proc_text(meminfo, "pswpin 10\n", "8", None, 4096), None, "a missing counter is no reading");
        let no_zswap = "MemFree: 100 kB\nMemAvailable: 400 kB\n";
        assert_eq!(from_proc_text(no_zswap, vmstat, "8", None, 4096).map(|r| r.compressed_bytes), Some(0));
    }

    // what this catches: the PSI verdict misread: a stall read as calm, `full` (everyone stalled)
    // read as merely Warn, or a malformed file read as Normal.
    #[test]
    fn psi_stalls_are_the_kernels_verdict_and_a_malformed_file_is_none() {
        let psi = |some: &str, full: &str| format!("some avg10={some} avg60=0.00 avg300=0.00 total=1\nfull avg10={full} avg60=0.00 avg300=0.00 total=0\n");
        assert_eq!(pressure_from_psi(&psi("0.00", "0.00")), Some(OsMemoryPressure::Normal));
        assert_eq!(pressure_from_psi(&psi("3.21", "0.00")), Some(OsMemoryPressure::Warn));
        assert_eq!(pressure_from_psi(&psi("40.00", "12.50")), Some(OsMemoryPressure::Critical));
        assert_eq!(pressure_from_psi("some avg10=1.00 total=3\n"), None, "no full line: no verdict");
        assert_eq!(pressure_from_psi("some avg10=x\nfull avg10=0.00\n"), None);
    }
}
