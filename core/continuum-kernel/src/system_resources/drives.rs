//! The drives a machine has: detected, never configured. A ring-0 machine fact: the
//! disk-pressure monitor, the cargo-target budget, the artifact paths and the model
//! placement code all ask "which drive holds this path" and must get ONE answer, so
//! the type and its resolver live here and `capacity` (the engine, ring 1) reads them.

use std::path::{Path, PathBuf};

/// Minimum free space for a drive to qualify as a usable COLD/frozen tier. Below this
/// a second drive is too small to hold model artifacts / MoE expert sets, so it does
/// not change the resolution — the offering degrades exactly as if the drive were
/// absent. A RESOLUTION threshold, not a gate: nothing is excluded, the deep-storage
/// tier simply isn't offered. Conservative floor — a single small GGUF is ~0.6 GiB and
/// an MoE expert set is hundreds of GiB, so 32 GiB is "big enough to be worth tiering
/// artifacts onto" without flagging a small scratch partition.
pub const MIN_COLD_TIER_BYTES: u64 = 32 * 1024 * 1024 * 1024;

/// The role a detected drive plays in artifact placement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriveRole {
    /// The OS / working drive (where the binary + config live). Kept lean — big cold
    /// artifacts belong elsewhere when a `Cold` drive exists.
    System,
    /// A large offload drive — the COLD/FROZEN artifact tier: big source GGUFs,
    /// backups, models not currently served. The Steam-library-style second drive.
    ///
    /// **NEVER the per-token streaming tier** (#302 correction, measured 2026-08-02:
    /// streaming an expert bank off a 130 MB/s HDD = 156–544 s/token = unservable —
    /// the prior doc's "MoE expert sets paged into VRAM on demand" conflated two
    /// physically different tiers). The hot per-token-paged set (expert containers,
    /// device-fit overrides) lives on the NVMe serving tier, governed by
    /// `NvmeServingTierPool`; artifacts here are frozen until MIGRATED up.
    /// [[docs/architecture/STORAGE-SERVING-TIER-GOVERNOR.md]]
    Cold,
}

/// A drive the system can place artifacts on — DETECTED, not configured. This is the
/// "recognize the storage" primitive: the system sees its OWN drives, so a user never
/// hand-places a model file (the manual `curl`-to-`D:` that this exists to retire).
#[derive(Debug, Clone)]
pub struct DriveInfo {
    /// Mount root (e.g. `C:\` or `D:\continuum-cold`).
    pub mount: PathBuf,
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub role: DriveRole,
}

/// Enumerate mounted drives (sysinfo) and flag the COLD tier: the largest-available
/// drive that ISN'T the system/root drive and clears [`MIN_COLD_TIER_BYTES`]. Every
/// other drive is `System`. Matches the `disk_pressure` sysinfo pattern
/// (`total_space`/`available_space`). Detection only — the roles feed RESOLUTION, so a
/// box with no qualifying second drive simply has no `Cold` drive (offering degrades).
pub fn detect_drives() -> Vec<DriveInfo> {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    let mut infos: Vec<DriveInfo> = disks
        .iter()
        .map(|d| DriveInfo {
            mount: d.mount_point().to_path_buf(),
            total_bytes: d.total_space(),
            available_bytes: d.available_space(),
            role: DriveRole::System,
        })
        .collect();

    // The cold tier is the biggest-available drive that clears the floor AND is not
    // the smallest (the OS drive tends to be busiest/smallest-free). If exactly one
    // drive clears the floor and there are others, it's the cold tier; with a single
    // drive there is no cold tier (nowhere distinct to offload to).
    if infos.len() > 1 {
        if let Some((idx, _)) = infos
            .iter()
            .enumerate()
            .filter(|(_, d)| d.available_bytes >= MIN_COLD_TIER_BYTES)
            .max_by_key(|(_, d)| d.available_bytes)
        {
            infos[idx].role = DriveRole::Cold;
        }
    }
    infos
}

/// THE drive that holds `path` — the ONE resolver every disk-shaped decision uses,
/// so the platform's mount naming lives here and nowhere else. The mount that is the
/// LONGEST prefix of the path wins (a nested mount beats its parent); `C:\` on
/// Windows, `/` on Unix, `/Volumes/x` for a home on a second disk — resolved, never
/// named by a constant one platform does not have.
///
/// Until 2026-09-18 the disk-pressure monitor matched mount points against the
/// string `"/"`. On Windows nothing matches, so it read `(0, 0)` and reported
/// pressure 0.0 / `Normal` for as long as it existed — the 5090 filled its 1.9 TB
/// volume to 0 bytes free with the broker never asked to relieve a byte. The cargo
/// budget already resolved its drive by longest prefix (`ipc/mod.rs`); this is that
/// rule, hoisted, so the two can never disagree again.
pub fn drive_holding<'a>(drives: &'a [DriveInfo], path: &Path) -> Option<&'a DriveInfo> {
    let path = mount_comparison_path(path);
    drives
        .iter()
        .filter(|d| path.starts_with(mount_comparison_path(&d.mount)))
        .max_by_key(|d| d.mount.as_os_str().len())
}

// canonicalize returns verbatim prefixes on Windows, while sysinfo mount points
// use ordinary prefixes. This is comparison only; filesystem operations retain
// their native spelling and long-path support.
fn mount_comparison_path(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        use std::path::{Component, Prefix};
        let mut components = path.components();
        if let Some(Component::Prefix(prefix)) = components.next() {
            let mut normalized = match prefix.kind() {
                Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => PathBuf::from(format!("{}:", char::from(letter.to_ascii_uppercase()))),
                Prefix::UNC(server, share) | Prefix::VerbatimUNC(server, share) => {
                    let mut root = PathBuf::from(r"\\");
                    root.push(server);
                    root.push(share);
                    root
                },
                _ => return path.to_path_buf(),
            };
            normalized.extend(components);
            return normalized;
        }
    }
    path.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (2026-09-18, the 5090 at 0 bytes free reading Normal): the
    // drive holding a path is found by LONGEST mount prefix in the platform's own
    // spelling — never by a constant one platform lacks — and a path on no listed
    // drive is None, an absence, not a zero-sized reading.
    #[test]
    fn the_drive_holding_a_path_is_the_longest_mount_prefix_never_a_named_root() {
        let drive = |mount: &str, total: u64| DriveInfo {
            mount: PathBuf::from(mount),
            total_bytes: total,
            available_bytes: total / 2,
            role: DriveRole::System,
        };
        // Each platform in ITS OWN spelling — `Path` components are platform-defined,
        // so `C:\Users\x` is ONE component on Unix and this case can only mean
        // anything where backslash separates. (CI on Linux caught the first version
        // asserting Windows spelling everywhere: the same class as the bug.)
        #[cfg(windows)]
        {
            let win = [drive("C:\\", 2_000), drive("D:\\", 16_000)];
            let home = Path::new("C:\\Users\\someone\\.continuum");
            assert_eq!(drive_holding(&win, home).map(|d| d.total_bytes), Some(2_000));
            assert_eq!(drive_holding(&win, Path::new(r"\\?\c:\Users\someone\cache")).map(|d| d.total_bytes), Some(2_000), "canonical roots retain their owning volume");
            // The paging owner supplies a real canonical directory while the
            // monitor's mount sample uses ordinary Windows spelling.
            let page_dir = tempfile::tempdir().unwrap();
            let physical = page_dir.path().canonicalize().unwrap();
            let ordinary = mount_comparison_path(&physical);
            let mount: PathBuf = ordinary.components().take(2).collect();
            let observed = [DriveInfo { mount, total_bytes: 8_000,
                available_bytes: 4_000, role: DriveRole::System }];
            assert_eq!(drive_holding(&observed, &physical).map(|d| d.total_bytes), Some(8_000),
                "actual canonical page directory resolves through the monitor's volume selector");
            let unc = [drive(r"\\server\share\", 9_000)];
            assert_eq!(drive_holding(&unc, Path::new(r"\\?\UNC\server\share\pages")).map(|d| d.total_bytes), Some(9_000));
            assert!(drive_holding(&unc, Path::new(r"\\?\UNC\server\share-other\pages")).is_none());
            assert!(
                drive_holding(&win, Path::new("Z:\\nowhere")).is_none(),
                "no drive = None, never (0, 0)"
            );
        }
        // Unix spelling: a nested mount beats its parent by prefix length; a path on
        // no listed drive is None. Valid on every platform (forward slash separates
        // everywhere), so it runs everywhere.
        let unix = [drive("/", 500), drive("/Volumes/big", 4_000)];
        assert_eq!(
            drive_holding(&unix, Path::new("/Volumes/big/continuum")).map(|d| d.total_bytes),
            Some(4_000)
        );
        assert_eq!(
            drive_holding(&unix, Path::new("/home/x/.continuum")).map(|d| d.total_bytes),
            Some(500)
        );
        let none = [drive("/Volumes/big", 4_000)];
        assert!(
            drive_holding(&none, Path::new("/home/x/.continuum")).is_none(),
            "no drive = None, never (0, 0)"
        );
    }
}
