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

use super::DeviceCapacity;
use crate::system_resources::drives::{detect_drives, DriveInfo, DriveRole, MIN_COLD_TIER_BYTES};
use crate::governor::types::HardwareClass;
use crate::modules::serving_daemon::{host_budget_from, HostBudgetInputs};

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
    /// The operator's serving headroom (`usable = live × this`): config, not hardware,
    /// so it is an INPUT like every measured part — [`Self::detect`] reads
    /// `config_env::vram_headroom()`, a fixture states its own. Read inside
    /// `serving_budget_bytes` it made every budget test a test of whichever
    /// `~/.continuum/config.env` the runner carries (card c4d84be3).
    pub budget_fraction: f64,
}

impl SystemProfile {
    /// PURE composition — the testable core. No I/O, no probes: hand it measured parts
    /// and it derives the resolution inputs. THIS is the outlier-validation seam.
    pub fn from_parts(
        hardware: HardwareClass,
        capacity: DeviceCapacity,
        drives: Vec<DriveInfo>,
        perf_cores: u32,
        budget_fraction: f64,
    ) -> Self {
        Self {
            hardware,
            capacity,
            drives,
            perf_cores,
            budget_fraction,
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
            // Same operator headroom policy as every live serving-budget caller
            // (serving_daemon): usable = live × the config fraction, handed in.
            budget_fraction: self.budget_fraction,
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
        use crate::governor::types::classify_hardware;
        use crate::inference_capability::hw_probe::probe_hardware_profile;

        let hw_profile = probe_hardware_profile();
        let hardware = classify_hardware(&hw_profile);

        // Live free RAM (the probe's `system_ram_bytes` is TOTAL, not free).
        let mut sys = sysinfo::System::new();
        sys.refresh_memory();
        // NOT `available_memory()` — 0 on macOS, so `DeviceCapacity::system_ram_free_bytes`
        // was a permanent zero on every Mac for anyone budgeting against it.
        let system_ram_free_bytes =
            crate::system_resources::memory_pressure::available_from(&sys);

        let capacity = DeviceCapacity {
            gpu_total_bytes: hw_profile.total_vram_bytes,
            gpu_free_bytes_live: hw_profile.free_vram_bytes,
            system_ram_free_bytes,
        };

        let drives = detect_drives();
        let perf_cores = hw_profile.cpu_cores.max(1);

        Self::from_parts(hardware, capacity, drives, perf_cores, continuum_base::config_env::vram_headroom())
    }
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
        crate::capacity::profile_fixtures::discrete_fixture(
            32,
            30,
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
            crate::capacity::profile_fixtures::FIXTURE_HEADROOM,
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
            crate::capacity::profile_fixtures::FIXTURE_HEADROOM,
        );
        // available (ram_free 30) capped at total (gpu_total 36) → 30, ×0.80.
        assert_eq!(uma.serving_budget_bytes(), (30 * GB) * 80 / 100);
    }
}
