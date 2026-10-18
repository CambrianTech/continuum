//! Vision sidecar admission arithmetic (#106): the main-lane width a sidecar's
//! headroom is planned against, the planned headroom after the main lane's whole
//! working set, and the admissible bytes — the tighter of the live free read (minus the
//! OS floor) and that planned headroom. Pure, so the measured incidents are tests; the
//! admission itself runs inside the reconcile's vision arm.

use super::*;

/// What remains of physical RAM for a SIDECAR after the main lane's planned working
/// set — the same terms the host-cache lease law uses ([`moe_host_cache_lease_inputs`]),
/// with the FULL weights (on unified memory the device-resident share is host RAM):
/// `physical − weights − KV(served window) × lanes − compute reserve − OS floor`.
/// Saturating: an over-full box yields ZERO headroom, never a wrap. Pure over the
/// target and the served window, so a relaunch window cannot lie to it.
/// The main-lane width a sidecar's headroom is planned against: the served width or the
/// residents' requirement, whichever is larger. PURE.
pub fn sidecar_window_for_headroom(served_window: u32, persona_floor: Option<u32>) -> u32 {
    served_window.max(persona_floor.unwrap_or(0)) // unwrap_or: no requirement measured = the served width stands
}

pub(super) fn sidecar_planned_headroom_bytes(
    target: &ServingTarget,
    served_window: u32,
    physical_bytes: u64,
) -> u64 {
    let Some(fp) = footprint_for(&target.model) else {
        return 0; // no honest footprint = no honest headroom; the live read alone never admits
    };
    let lanes = target.lanes.max(1);
    let window = if served_window > 0 { served_window } else { target.context_window };
    let working_set = fp
        .weights_bytes
        .saturating_add(fp.kv_at(window).saturating_mul(lanes as u64))
        .saturating_add(fp.prefill_compute_reserve(window, lanes))
        .saturating_add((target.host_prompt_cache_mib as u64) * 1024 * 1024)
        .saturating_add(host_os_floor_bytes(physical_bytes));
    physical_bytes.saturating_sub(working_set)
}

/// ONE FLOOR EVERYWHERE: the bytes a sidecar may take is the tighter of the LIVE free
/// read minus the OS floor and the PLANNED headroom (which already keeps that floor).
/// The live read compared RAW admitted a 24.1 GB sidecar into 26.7 GB "free" on the M5
/// (2026-09-18 21:56:03Z, the 1800bd90b deploy): `available_bytes` counts purgeable and
/// inactive pages as free, and 2.6 GB of slack on a 64 GB box is the core's own RSS —
/// swap 6.7 GB, compressor 20 GB, the serving node paging. The planned term had kept
/// the floor (physical/8) all along; the live term did not. Pure, so the measured
/// numbers are a test.
pub(super) fn sidecar_admissible_bytes(live_free: u64, planned_headroom: u64, physical_bytes: u64) -> u64 {
    live_free
        .saturating_sub(host_os_floor_bytes(physical_bytes))
        .min(planned_headroom)
}
