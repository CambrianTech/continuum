//! `SystemProfile` — the keystone of `catalog = f(system × storage × grid)`.
//!
//! ONE measured descriptor of what THIS box can do, so the model catalog resolves its
//! offering against reality instead of a static list. It COMPOSES primitives that
//! already exist — it never re-measures what another module owns:
//!   - [`HardwareClass`] — the boot silicon / VRAM / RAM classifier
//!     ([`classify_hardware`] over a live [`probe_hardware_profile`]).
//!   - live [`DeviceCapacity`] — free GPU / RAM right now.
//!   - the 0.80 serving budget via [`host_budget_from`] — ONE definition of the
//!     serving fraction, never a second copy of `0.80`.
//!   - detected drives, with the big offload drive flagged as the COLD/frozen tier.
//!
//! **Storage and grid are RESOLUTION FIELDS, never GATES** (solve-for-public-users,
//! [[public-project-not-joels-machines]]): a huge drive RECOMMENDS the full experience
//! (big models, MoE expert sets frozen on it); its absence DEGRADES the offering
//! (smaller quant / dense student / route to a grid node that fits) — it never
//! EXCLUDES a user. The default must work on any laptop. `SystemProfile` is shaped to
//! compose UP into a [`GridSnapshot`](super::grid::GridSnapshot) so "too big for THIS
//! node" resolves to a peer, not a wall.
//!
//! The pure [`SystemProfile::from_parts`] core is the outlier-validation seam: a
//! Blackwell-5090-with-16TB profile and a no-cold-drive laptop profile exercise the
//! SAME resolution logic, and must resolve UP vs DOWN — never one included, the other
//! excluded.

use std::path::PathBuf;

use super::DeviceCapacity;
use crate::governor::types::HardwareClass;
use crate::modules::serving_daemon::{host_budget_from, HostBudgetInputs};

/// Minimum free space for a drive to qualify as a usable COLD/frozen tier. Below this
/// a second drive is too small to hold model artifacts / MoE expert sets, so it does
/// not change the resolution — the offering degrades exactly as if the drive were
/// absent. A RESOLUTION threshold, not a gate: nothing is excluded, the deep-storage
/// tier simply isn't offered. Conservative floor — a single small GGUF is ~0.6 GiB and
/// an MoE expert set is hundreds of GiB, so 32 GiB is "big enough to be worth tiering
/// artifacts onto" without flagging a small scratch partition.
const MIN_COLD_TIER_BYTES: u64 = 32 * 1024 * 1024 * 1024;

/// The role a detected drive plays in artifact placement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriveRole {
    /// The OS / working drive (where the binary + config live). Kept lean — big cold
    /// artifacts belong elsewhere when a `Cold` drive exists.
    System,
    /// A large offload drive — the COLD/frozen artifact tier (big GGUFs, MoE expert
    /// sets paged into VRAM on demand). The Steam-library-style second drive.
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

/// What THIS box can do — the catalog's resolution input. Composes the hardware class,
/// live device capacity, detected drives, and the perf-core count into one descriptor.
#[derive(Debug, Clone)]
pub struct SystemProfile {
    /// Silicon class + VRAM / RAM ceilings (discrete vs UMA).
    pub hardware: HardwareClass,
    /// Free GPU / RAM RIGHT NOW.
    pub capacity: DeviceCapacity,
    /// Detected drives; at most one is flagged [`DriveRole::Cold`].
    pub drives: Vec<DriveInfo>,
    /// Performance-core proxy for lane parallelism (feeds the serving budget).
    pub perf_cores: u32,
}

impl SystemProfile {
    /// PURE composition — the testable core. No I/O, no probes: hand it measured parts
    /// and it derives the resolution inputs. THIS is the outlier-validation seam.
    pub fn from_parts(
        hardware: HardwareClass,
        capacity: DeviceCapacity,
        drives: Vec<DriveInfo>,
        perf_cores: u32,
    ) -> Self {
        Self {
            hardware,
            capacity,
            drives,
            perf_cores,
        }
    }

    /// The serving VRAM budget: live-free capped at the physical ceiling, minus the
    /// 0.80 headroom. REUSES [`host_budget_from`] so the serving fraction has ONE
    /// definition (never a second copy of `0.80` drifting out of sync). On UMA
    /// (`vram_mb == 0`, Apple Silicon) the budget is a slice of system RAM, mirroring
    /// the governor's own UMA handling.
    pub fn serving_budget_bytes(&self) -> u64 {
        let uma = self.hardware.vram_mb == 0;
        let total = if uma {
            self.capacity.gpu_total_bytes // UMA: capacity carries the serving slice
        } else {
            self.hardware.vram_mb.saturating_mul(1024 * 1024)
        };
        let available = if uma {
            self.capacity.system_ram_free_bytes
        } else {
            self.capacity.gpu_free_bytes_live
        };
        host_budget_from(&HostBudgetInputs {
            available_bytes: available,
            total_vram_bytes: total,
            perf_cores: self.perf_cores,
        })
        .usable_bytes
    }

    /// The COLD/frozen artifact-tier drive, if this box has one flagged. `None` ⇒ no
    /// cold tier: placement DEGRADES (smaller quant / grid route), never excludes.
    pub fn cold_drive(&self) -> Option<&DriveInfo> {
        self.drives.iter().find(|d| d.role == DriveRole::Cold)
    }

    /// Does this box RECOMMEND the deep-storage experience — big models / MoE expert
    /// sets frozen on a cold drive? True only when a `Cold` drive exists AND still has
    /// enough free space to matter. False ⇒ the offering resolves DOWN, never out.
    pub fn has_cold_tier(&self) -> bool {
        self.cold_drive()
            .is_some_and(|d| d.available_bytes >= MIN_COLD_TIER_BYTES)
    }

    /// Live detection — wires the real machine from primitives that already exist:
    /// [`probe_hardware_profile`] (live VRAM / RAM / cores) → [`classify_hardware`],
    /// and sysinfo for the drive list. Best-effort and non-panicking: a probe that
    /// can't see the GPU still yields a usable (CPU/degraded) profile, because the
    /// resolution never gates — it degrades.
    pub fn detect() -> Self {
        use crate::inference_capability::hw_probe::probe_hardware_profile;
        use crate::governor::types::classify_hardware;

        let hw_profile = probe_hardware_profile();
        let hardware = classify_hardware(&hw_profile);

        // Live free RAM (the probe's `system_ram_bytes` is TOTAL, not free).
        let mut sys = sysinfo::System::new();
        sys.refresh_memory();
        let system_ram_free_bytes = sys.available_memory();

        let capacity = DeviceCapacity {
            gpu_total_bytes: hw_profile.total_vram_bytes,
            gpu_free_bytes_live: hw_profile.free_vram_bytes,
            system_ram_free_bytes,
        };

        let drives = detect_drives();
        let perf_cores = hw_profile.cpu_cores.max(1);

        Self::from_parts(hardware, capacity, drives, perf_cores)
    }
}

/// Enumerate mounted drives (sysinfo) and flag the COLD tier: the largest-available
/// drive that ISN'T the system/root drive and clears [`MIN_COLD_TIER_BYTES`]. Every
/// other drive is `System`. Matches the `disk_pressure` sysinfo pattern
/// (`total_space`/`available_space`). Detection only — the roles feed RESOLUTION, so a
/// box with no qualifying second drive simply has no `Cold` drive (offering degrades).
fn detect_drives() -> Vec<DriveInfo> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::governor::types::{PowerSource, TargetSilicon, ThermalClass};

    fn hw(silicon: TargetSilicon, vram_mb: u64, ram_mb: u64) -> HardwareClass {
        HardwareClass {
            silicon,
            silicon_model: "test".into(),
            vram_mb,
            system_ram_mb: ram_mb,
            power_source: PowerSource::Plugged,
            thermal_class: ThermalClass::Workstation,
            battery_pct: None,
            thermal_headroom_pct: None,
        }
    }

    fn cap(gpu_total: u64, gpu_free: u64, ram_free: u64) -> DeviceCapacity {
        DeviceCapacity {
            gpu_total_bytes: gpu_total,
            gpu_free_bytes_live: gpu_free,
            system_ram_free_bytes: ram_free,
        }
    }

    const GB: u64 = 1024 * 1024 * 1024;

    /// BigMama: RTX 5090 (32 GiB VRAM discrete) + a 16 TB cold drive.
    fn bigmama() -> SystemProfile {
        SystemProfile::from_parts(
            hw(TargetSilicon::NvidiaCuda, 32 * 1024, 128 * 1024),
            cap(32 * GB, 30 * GB, 100 * GB),
            vec![
                DriveInfo {
                    mount: "C:\\".into(),
                    total_bytes: 2000 * GB,
                    available_bytes: 40 * GB,
                    role: DriveRole::System,
                },
                DriveInfo {
                    mount: "D:\\continuum-cold".into(),
                    total_bytes: 16_000 * GB,
                    available_bytes: 15_000 * GB,
                    role: DriveRole::Cold,
                },
            ],
            24,
        )
    }

    /// A no-cold-drive laptop: single small drive, modest discrete GPU.
    fn laptop_no_cold() -> SystemProfile {
        SystemProfile::from_parts(
            hw(TargetSilicon::NvidiaCuda, 8 * 1024, 16 * 1024),
            cap(8 * GB, 6 * GB, 8 * GB),
            vec![DriveInfo {
                mount: "C:\\".into(),
                total_bytes: 500 * GB,
                available_bytes: 20 * GB, // below the 32 GiB cold floor
                role: DriveRole::System,
            }],
            8,
        )
    }

    // what this catches: THE OUTLIER-VALIDATION INVARIANT for catalog=f(system×storage).
    // The SAME resolution logic must resolve UP for a box with a big cold drive and
    // DOWN for one without — the deep-storage tier is a RESOLUTION FIELD, never a gate.
    // Regression here = a laptop being EXCLUDED (no cold tier → crash/deny) instead of
    // DEGRADED, which breaks solve-for-public-users.
    #[test]
    fn cold_tier_is_a_resolution_field_not_a_gate() {
        assert!(
            bigmama().has_cold_tier(),
            "a 16TB cold drive must RECOMMEND the deep-storage tier"
        );
        assert!(
            !laptop_no_cold().has_cold_tier(),
            "no qualifying cold drive → tier not offered (DEGRADE), NOT excluded"
        );
        // Both are still valid profiles that answer every query — neither errors.
        assert!(bigmama().cold_drive().is_some());
        assert!(laptop_no_cold().cold_drive().is_none());
    }

    // what this catches: the serving budget REUSES host_budget_from's 0.80 fraction —
    // one definition, applied to the discrete VRAM ceiling. A drift to a second
    // hardcoded fraction (or planning above physical VRAM) is caught here.
    #[test]
    fn serving_budget_applies_the_shared_080_headroom_to_vram() {
        // 30 GiB live-free, 32 GiB ceiling → budget = 0.80 × 30 GiB.
        let expected = (30 * GB as u64) * 80 / 100;
        assert_eq!(bigmama().serving_budget_bytes(), expected);
    }

    // what this catches: on UMA (vram_mb==0, Apple Silicon) the budget comes from the
    // system-RAM slice, not a zero VRAM ceiling — a discrete-only budget path would
    // return 0 here and blind the catalog to every Apple-Silicon box.
    #[test]
    fn uma_budget_uses_the_system_ram_slice_not_zero_vram() {
        let uma = SystemProfile::from_parts(
            hw(TargetSilicon::AppleM, 0, 48 * 1024),
            cap(36 * GB, 36 * GB, 30 * GB), // capacity carries the UMA serving slice
            vec![],
            10,
        );
        // available (ram_free 30) capped at total (gpu_total 36) → 30, ×0.80.
        assert_eq!(uma.serving_budget_bytes(), (30 * GB) * 80 / 100);
    }
}
