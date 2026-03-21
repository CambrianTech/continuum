//! Camera animation — lock camera to head rest position per-slot.

use bevy::prelude::*;

use super::components::*;
use super::super::skeleton::{camera_z_for_head, REFERENCE_HEAD_Y};

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
        let eye_y = head_y + 0.06;
        let cam_z = camera_z_for_head(head_y);

        transform.translation.x = 0.0;
        transform.translation.y = eye_y + 0.02;
        transform.translation.z = cam_z;
        let look_target = Vec3::new(0.0, eye_y, 0.0);
        *transform = transform.looking_at(look_target, Vec3::Y);
    }
}
