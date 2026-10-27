//! The host prompt cache's disk tier (`--cache-disk`, engine fork #78). Without it an entry the
//! RAM tier evicted was dropped and that mind re-prefilled her whole conversation; with it the
//! entry is written under the KV page root and read back at SSD speed. On the M5 (2026-10-27)
//! the RAM tier held 2.4-6.6 GiB entries in 11,961 MiB, macOS compressed 13-15 GB of the engine
//! and swapped 11.9 of 13.3 GB, and 40+ entries were dropped ("removing oldest entry").
use super::*;

/// Whether the serving engine lists `--cache-disk`, read off its `--help` by the same
/// once-per-process probe as `--train-dir`. Unprobed or unanswered is no: an engine older
/// than the flag refuses to start with it, and the tier is never worth a lane.
pub(super) static ENGINE_CACHE_DISK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn engine_accepts_cache_disk() -> bool {
    ENGINE_CACHE_DISK.load(std::sync::atomic::Ordering::Relaxed) // unprobed or unanswered is false: the one answer that cannot keep a lane from starting
}

/// The engine lists the flag itself as an option, not only `--cache-disk-mib` or a mention.
pub(super) fn engine_help_lists_cache_disk(help: &str) -> bool {
    help.lines().any(|line| {
        line.trim_start()
            .strip_prefix("--cache-disk")
            .is_some_and(|rest| rest.is_empty() || rest.starts_with([' ', ',', '\t']))
    })
}

/// One dir per model under the KV page root, so the `kv-pages` disk reporter counts it. The
/// engine owns what is inside: it removes another instance's states when it starts, its own
/// when it exits, and evicts its oldest at its limit.
pub fn prompt_cache_disk_dir(model_id: &str) -> PathBuf {
    let page_dir = kv_page_dir(model_id);
    let model = page_dir.file_name().map(std::ffi::OsStr::to_owned).unwrap_or_default(); // unwrap_or_default: kv_page_dir always ends in the sanitized model name
    kv_pages_root().join("prompt-cache").join(model)
}

/// The tier's limit: what the volume holding `dir` can take before it reaches the threshold
/// at which every page write is already refused (`paging::pool`, 95%), so the tier never
/// claims room a page save would be refused for. `None` when the disk monitor has no sample
/// of the volume yet: the lane then starts without the tier, and says so.
pub fn prompt_cache_disk_mib(dir: &Path) -> Option<u32> {
    let volume = crate::system_resources::DiskPressureMonitor::volume_for(dir)?;
    let ceiling = volume.total_bytes.saturating_mul(crate::paging::pool::DISK_CAPACITY_REFUSAL_BASIS_POINTS) / 10_000;
    let used = volume.total_bytes.saturating_sub(volume.available_bytes);
    let mib = ceiling.saturating_sub(used) / (1024 * 1024);
    (mib > 0).then(|| u32::try_from(mib).unwrap_or(u32::MAX)) // unwrap_or: a volume past 4 PiB of headroom is granted the most the flag can say
}
