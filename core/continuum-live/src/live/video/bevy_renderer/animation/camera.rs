//! Camera animation — lock camera to head rest position per-slot.

use bevy::prelude::*;

use super::super::skeleton::{camera_z_for_head, REFERENCE_HEAD_Y};
use super::components::*;

/// Lock each camera to its own avatar's head-Y.
/// Matches camera to avatar via SlotId — each slot's camera frames that slot's avatar.
pub(in crate::live::video::bevy_renderer) fn animate_idle(
    mut camera_query: Query<(&mut CameraHeadLock, &mut Transform, &SlotId)>,
    skeleton_query: Query<(&Skeleton, &SlotId)>,
    global_transforms: Query<&GlobalTransform>,
) {
    for (mut lock, mut transform, cam_slot) in &mut camera_query {
        // One-time capture: find the skeleton that matches this camera's slot
        if lock.head_y.is_none() {
            for (skeleton, skel_slot) in &skeleton_query {
                if skel_slot.0 != cam_slot.0 {
                    continue;
                }
                if let Some(ref head) = skeleton.head {
                    if let Ok(global) = global_transforms.get(head.entity) {
                        lock.head_y = Some(global.translation().y);
                        break;
                    }
                }
            }
        }

        let head_y = lock.head_y.unwrap_or(REFERENCE_HEAD_Y);
        let cam_z = camera_z_for_head(head_y);

        // Derive look target from actual bone positions discovered at instantiation:
        // spine entity → neck entity → proportional fallback (mid-spine ≈ 60% of head height).
        let mut look_target_y: f32 = head_y * 0.6; // default: mid-torso proportion
        for (skeleton, skel_slot) in &skeleton_query {
            if skel_slot.0 != cam_slot.0 {
                continue;
            }
            // Primary: spine bone position (actual measurement of torso height)
            if let Some(ref spine) = skeleton.spine {
                if let Ok(global_spine) = global_transforms.get(spine.entity) {
                    look_target_y = global_spine.translation().y;
                    break;
                }
            }
            // Secondary: neck bone position as fallback
            if let Some(ref neck) = skeleton.neck {
                if let Ok(global_neck) = global_transforms.get(neck.entity) {
                    look_target_y = global_neck.translation().y;
                    break;
                }
            }
        }

        // Camera Y: positioned midway between head and look target for natural downward angle.
        // This ensures the camera frames both head and torso without manual offsets.
        let cam_y = (head_y + look_target_y) / 2.0;

        transform.translation.x = 0.0;
        transform.translation.y = cam_y;
        transform.translation.z = cam_z;
        let look_target = Vec3::new(0.0, look_target_y, 0.0);
        *transform = transform.looking_at(look_target, Vec3::Y);
    }
}
