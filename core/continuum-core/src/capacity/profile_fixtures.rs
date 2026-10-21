//! System-profile fixtures with every input fixed — VRAM, free VRAM, RAM, perf cores,
//! drives and the operator headroom — so a budget derived from them is the same on any
//! machine and under any operator's `config.env` (card c4d84be3). One fixture shared by
//! the capacity and provisioning tests; gated like every fixture (`test-fixtures`).

use super::system_profile::{DriveInfo, SystemProfile};
use super::DeviceCapacity;
use crate::governor::types::HardwareClass;

/// The operator headroom test fixtures plan under (the shipped default, 0.80).
pub const FIXTURE_HEADROOM: f64 = 0.80;

/// A discrete-CUDA workstation with every input fixed: `vram_gb` of VRAM with
/// `gpu_free_gb` free, 128 GiB RAM (100 free), 24 perf cores, `drives`, and
/// [`FIXTURE_HEADROOM`] — so a budget derived from it is the same on any machine and
/// under any operator's config. The one fixture the capacity/provisioning tests share.
pub fn discrete_fixture(vram_gb: u64, gpu_free_gb: u64, drives: Vec<DriveInfo>) -> SystemProfile {
    use crate::governor::types::{PowerSource, TargetSilicon, ThermalClass};
    const GB: u64 = 1024 * 1024 * 1024;
    SystemProfile::from_parts(
        HardwareClass {
            silicon: TargetSilicon::NvidiaCuda,
            silicon_model: "test".into(),
            vram_mb: vram_gb * 1024,
            system_ram_mb: 128 * 1024,
            power_source: PowerSource::Plugged,
            thermal_class: ThermalClass::Workstation,
            battery_pct: None,
            thermal_headroom_pct: None,
        },
        DeviceCapacity {
            gpu_total_bytes: vram_gb * GB,
            gpu_free_bytes_live: gpu_free_gb * GB,
            system_ram_free_bytes: 100 * GB,
        },
        drives,
        24,
        FIXTURE_HEADROOM,
    )
}
