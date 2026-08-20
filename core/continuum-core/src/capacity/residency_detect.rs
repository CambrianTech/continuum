//! Residency tier DETECTION — the live "what storage exists RIGHT NOW" producer that feeds
//! the N-tier expert pager ([`super::expert_residency::plan_tiered_residency`]).
//!
//! Joel 2026-07-25: **"detect it at any time. It could be added or removed or be filled up
//! (with not our shit)."** The tier set is a LIVE SNAPSHOT, never a boot fact — a drive gets
//! plugged in, yanked, or a neighbor dumps 500GB of video onto the flash, and the very next
//! tick reflects it. This module turns the substrate's already-live readings (VRAM/RAM from
//! `system_resources::monitor`, per-volume from `system_resources::disk_pressure`'s live
//! `sysinfo::Disks` refresh) into the ordered [`ResidencyTier`] vector the pager consumes.
//!
//! ## Why "not our shit" needs zero ownership tracking
//!
//! We read **available** (free) bytes, never **total**. A volume filled with non-Continuum
//! data simply reports less room, so the pager promotes fewer experts there and they spill to
//! the next tier — no "whose bytes are these" accounting, no quota, no config. The elegance is
//! that the honest live number already encodes the answer.
//!
//! ## Pure, re-derived every tick
//!
//! [`assemble_residency_tiers`] is a pure projection: raw live readings → ordered tier vector.
//! Because it's re-run each tick, add/remove/fill-up all fall out for free — a yanked drive is
//! simply absent from next tick's input; a plugged-in one appears; a filled one shrinks. The
//! impure half — probing `sysinfo` off-thread on the monitor's own task, mapping
//! `sysinfo::DiskKind` → [`StorageKind`] at that boundary — is the runtime's job (it reuses
//! the EXISTING `DiskPressureMonitor` / `SystemResourceMonitor`, never a new parallel probe).
//! This is the brain they hand their readings to.

use super::expert_residency::{ResidencyMedium, ResidencyTier};

/// How a volume physically stores data — the only storage fact that matters for fault cost.
/// Kept `sysinfo`-free so `capacity/` stays dependency-light; the runtime maps
/// `sysinfo::DiskKind` → this at the probe boundary (SSD/NVMe → `SolidState`, HDD →
/// `Spinning`, anything else → `Unknown`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageKind {
    SolidState,
    Spinning,
    Unknown,
}

/// The fault-cost tier a storage kind maps to. NVMe/SSD flash faults in single-digit ms
/// (`Flash`); a spinning disk or a RAID of them pays a seek (`ColdDisk`); unknown is treated
/// as cold (conservative — never claim a promotion is faster than it is).
pub fn medium_for(kind: StorageKind) -> ResidencyMedium {
    match kind {
        StorageKind::SolidState => ResidencyMedium::Flash,
        StorageKind::Spinning | StorageKind::Unknown => ResidencyMedium::ColdDisk,
    }
}

/// One mounted volume's LIVE reading this tick: its physical kind + its ACTUAL free bytes
/// (available, not total — so non-Continuum data filling it just lowers this number).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VolumeFree {
    pub kind: StorageKind,
    pub free_bytes: u64,
}

/// Assemble the ordered residency tier vector from LIVE readings — the pure producer, meant to
/// be re-run every tick.
///
/// - `gpu_free_per_device`: one entry per GPU, so a multi-GPU box (Joel's 3×1080ti) yields one
///   `Vram` tier per device with no special-casing.
/// - `ram_free`: live free system RAM.
/// - `volumes`: each mounted volume's live kind + free bytes.
///
/// Ordered by fault cost (`Vram < Ram < Flash < ColdDisk`, the derived `Ord` on
/// [`ResidencyMedium`]) so the pager always sees hottest-first regardless of probe order.
/// Components with zero free are DROPPED — a full or yanked drive is simply not a promotion
/// target this tick (it reappears the moment room or the drive returns). This is what makes
/// "detect at any time" fall out for free: the vector is a projection of the current instant,
/// holding no memory of a drive that's gone or a tier that filled.
pub fn assemble_residency_tiers(
    gpu_free_per_device: &[u64],
    ram_free: u64,
    volumes: &[VolumeFree],
) -> Vec<ResidencyTier> {
    let mut tiers = Vec::new();

    for &g in gpu_free_per_device {
        if g > 0 {
            tiers.push(ResidencyTier {
                medium: ResidencyMedium::Vram,
                free_bytes: g,
            });
        }
    }
    if ram_free > 0 {
        tiers.push(ResidencyTier {
            medium: ResidencyMedium::Ram,
            free_bytes: ram_free,
        });
    }
    for v in volumes {
        if v.free_bytes > 0 {
            tiers.push(ResidencyTier {
                medium: medium_for(v.kind),
                free_bytes: v.free_bytes,
            });
        }
    }

    // Fault cost first; within a medium, the roomiest tier leads (fill the emptiest first).
    tiers.sort_by(|a, b| {
        a.medium
            .cmp(&b.medium)
            .then(b.free_bytes.cmp(&a.free_bytes))
    });
    tiers
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1024 * 1024 * 1024;

    // what this catches: the live readings assemble into the fault-cost-ordered vector the
    // pager relies on — multi-GPU VRAM tiers first, then RAM, then flash, then spinning/RAID —
    // with SSD→Flash and HDD→ColdDisk classification. If ordering or classification drifts, an
    // expert lands a tier too slow and locality degrades.
    #[test]
    fn assembles_and_orders_by_fault_cost() {
        let gpus = [8 * GB, 8 * GB]; // 2 GPUs
        let vols = [
            VolumeFree {
                kind: StorageKind::Spinning,
                free_bytes: 8000 * GB,
            }, // RAID, huge
            VolumeFree {
                kind: StorageKind::SolidState,
                free_bytes: 1500 * GB,
            }, // NVMe flash
        ];
        let tiers = assemble_residency_tiers(&gpus, 64 * GB, &vols);

        let mediums: Vec<ResidencyMedium> = tiers.iter().map(|t| t.medium).collect();
        assert_eq!(
            mediums,
            vec![
                ResidencyMedium::Vram,
                ResidencyMedium::Vram,
                ResidencyMedium::Ram,
                ResidencyMedium::Flash,
                ResidencyMedium::ColdDisk,
            ],
            "ordered hottest-fault-first regardless of probe order"
        );
    }

    // what this catches: "detect at any time" — a volume filled with non-Continuum data (≈0
    // free) or a yanked drive (absent from the input) simply is not a tier this tick, with no
    // ownership tracking. The projection holds no memory of what's gone.
    #[test]
    fn filled_or_removed_storage_drops_out() {
        let vols = [
            VolumeFree {
                kind: StorageKind::SolidState,
                free_bytes: 0,
            }, // full of not-our-shit
            VolumeFree {
                kind: StorageKind::Spinning,
                free_bytes: 500 * GB,
            },
        ];
        let tiers = assemble_residency_tiers(&[16 * GB], 32 * GB, &vols);
        // VRAM + RAM + the one spinning volume with room; the full SSD is dropped.
        assert_eq!(tiers.len(), 3);
        assert!(
            !tiers.iter().any(|t| t.medium == ResidencyMedium::Flash),
            "full flash volume is not a promotion target"
        );

        // A box with no discrete GPU and no extra drives → just the RAM tier. Still valid.
        let laptop = assemble_residency_tiers(&[], 16 * GB, &[]);
        assert_eq!(laptop.len(), 1);
        assert_eq!(laptop[0].medium, ResidencyMedium::Ram);

        // Everything gone → empty vector, no panic.
        assert!(assemble_residency_tiers(&[], 0, &[]).is_empty());
    }

    // what this catches: the kind→medium classification the fault-cost ordering rests on.
    #[test]
    fn medium_classification_maps_kinds() {
        assert_eq!(medium_for(StorageKind::SolidState), ResidencyMedium::Flash);
        assert_eq!(medium_for(StorageKind::Spinning), ResidencyMedium::ColdDisk);
        assert_eq!(medium_for(StorageKind::Unknown), ResidencyMedium::ColdDisk);
    }
}
