//! Readback management — continuous readback for non-GPU-bridge slots, snapshot readback for bridge slots.

use bevy::prelude::*;
use bevy::render::gpu_readback::Readback;

use super::types::*;

/// Ensure non-bridge slots always have Readback attached for continuous frame delivery.
/// Bridge slots only get Readback for snapshot capture (one-shot).
#[allow(clippy::type_complexity)]
pub(super) fn ensure_continuous_readback(
    query: Query<(Entity, &AvatarSlotId), (With<ReadbackMarker>, Without<Readback>)>,
    query_with_readback: Query<(Entity, &AvatarSlotId), (With<ReadbackMarker>, With<Readback>)>,
    registry: Res<SlotRegistry>,
    snapshots: Res<SnapshotTracker>,
    cameras: Query<&Camera>,
    mut commands: Commands,
) {
    // Remove Readback from bridge slots — if Readback was inserted before the bridge
    // was registered, it must be removed to prevent dual-writing.
    // Checks BOTH Metal IOSurface bridge (macOS) and wgpu compute bridge (cross-platform).
    // EXCEPTION: keep Readback alive for one frame if this slot has a pending snapshot.
    for (entity, slot_id) in &query_with_readback {
        let has_any_bridge = crate::live::avatar::publishers::gpu_bridge::has_bridge(slot_id.0)
            || crate::live::video::wgpu_gpu_convert::has_bridge(slot_id.0);
        if has_any_bridge {
            if snapshots.pending_readback_slot != Some(slot_id.0) {
                commands.entity(entity).remove::<Readback>();
            }
        }
    }

    // Re-insert Readback for non-bridge slots that need continuous readback,
    // OR for bridge slots that need a one-shot snapshot readback.
    for (entity, slot_id) in &query {
        if let Some(slot_data) = registry.slots.get(&slot_id.0) {
            if !slot_data.is_active() {
                continue;
            }

            let is_bridge = crate::live::avatar::publishers::gpu_bridge::has_bridge(slot_id.0)
                || crate::live::video::wgpu_gpu_convert::has_bridge(slot_id.0);
            let is_snapshot_target = snapshots.pending_readback_slot == Some(slot_id.0);

            if is_bridge && !is_snapshot_target {
                continue;
            }

            if let Some(cam_entity) = slot_data.camera_entity {
                if let Ok(camera) = cameras.get(cam_entity) {
                    if !camera.is_active {
                        continue;
                    }
                }
            }
            commands
                .entity(entity)
                .insert(Readback::texture(slot_data.render_target.clone()));
        }
    }
}

/// Periodically checks if any GPU bridge slot needs a snapshot.
/// Requests a temporary one-frame Readback for exactly one slot at a time.
pub(super) fn request_snapshot_readback(
    mut snapshots: ResMut<SnapshotTracker>,
    health: Res<SlotHealthStatus>,
    registry: Res<SlotRegistry>,
) {
    // Clean up loaded_at for slots that are no longer active.
    let stale: Vec<u8> = snapshots
        .loaded_at
        .keys()
        .filter(|slot| {
            registry
                .slots
                .get(slot)
                .map(|s| !s.is_active())
                .unwrap_or(true)
        })
        .copied()
        .collect();
    for slot in stale {
        snapshots.mark_unloaded(slot);
    }

    if snapshots.pending_readback_slot.is_some() {
        return;
    }

    for (&slot, _slot_data) in &registry.slots {
        let has_any_bridge = crate::live::avatar::publishers::gpu_bridge::has_bridge(slot)
            || crate::live::video::wgpu_gpu_convert::has_bridge(slot);
        if !has_any_bridge {
            continue;
        }
        if let Some(identity) = health.identities.get(&slot) {
            if snapshots.needs_capture(slot, identity) {
                snapshots.pending_readback_slot = Some(slot);
                return;
            }
        }
    }
}
