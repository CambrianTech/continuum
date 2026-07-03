//! Eye gaze animation — blend shape and bone-based eye tracking.

use bevy::mesh::morph::MorphWeights;
use bevy::prelude::*;

use super::components::*;
use super::ExternalPose;

/// Animate eye gaze on entities with EyeGaze + Skeleton or MorphTargets.
/// Skips entities owned by an external animator (`Without<ExternalPose>`).
pub(in crate::live::video::bevy_renderer) fn animate_eye_gaze(
    time: Res<Time>,
    query: Query<
        (
            &EyeGaze,
            &Skeleton,
            Option<&MorphTargets>,
            Option<&MorphMeshLink>,
            Has<Speaking>,
        ),
        Without<ExternalPose>,
    >,
    mut morph_weights: Query<&mut MorphWeights>,
    mut transforms: Query<&mut Transform>,
) {
    let t = time.elapsed_secs();

    for (gaze, skeleton, morph_targets, mesh_link, is_speaking) in &query {
        let phase = gaze.phase;

        let (gaze_x, gaze_y) = if is_speaking {
            let drift_x = (t * 0.3 + phase).sin() * 0.05;
            let drift_y = (t * 0.25 + phase).cos() * 0.03;
            (drift_x, drift_y)
        } else {
            let drift_x = (t * 0.13 + phase).sin() * 0.12 + (t * 0.07 + phase * 0.7).cos() * 0.08;
            let drift_y = (t * 0.11 + phase).cos() * 0.08 + (t * 0.19 + phase * 1.3).sin() * 0.05;
            (drift_x.clamp(-0.4, 0.4), drift_y.clamp(-0.3, 0.3))
        };

        // Path 1: Bone-based eye gaze
        let mut used_bone_gaze = false;
        if skeleton.left_eye.is_some() && skeleton.right_eye.is_some() {
            let config = skeleton.look_at_config.unwrap_or_default();
            let h_deg = (config.horizontal_inner_deg + config.horizontal_outer_deg) * 0.5;
            let v_up_deg = config.vertical_up_deg;
            let v_down_deg = config.vertical_down_deg;

            let yaw_rad = gaze_x * h_deg.to_radians();
            let pitch_rad = if gaze_y >= 0.0 {
                -gaze_y * v_up_deg.to_radians()
            } else {
                -gaze_y * v_down_deg.to_radians()
            };

            let gaze_delta = Quat::from_euler(EulerRot::XYZ, pitch_rad, yaw_rad, 0.0);

            if let Some(ref left_eye) = skeleton.left_eye {
                if let Ok(mut transform) = transforms.get_mut(left_eye.entity) {
                    transform.rotation = left_eye.rest_rotation * gaze_delta;
                }
            }
            if let Some(ref right_eye) = skeleton.right_eye {
                if let Ok(mut transform) = transforms.get_mut(right_eye.entity) {
                    transform.rotation = right_eye.rest_rotation * gaze_delta;
                }
            }
            used_bone_gaze = true;
        }

        // Path 2: Blend shape gaze
        if !used_bone_gaze {
            if let (Some(targets), Some(mesh_link)) = (morph_targets, mesh_link) {
                if !targets.has_gaze() {
                    continue;
                }

                if let Ok(mut weights) = morph_weights.get_mut(mesh_link.0) {
                    let w = weights.weights_mut();

                    if gaze_x < 0.0 {
                        set_morph(w, targets.look_left, (-gaze_x).min(1.0));
                        set_morph(w, targets.look_right, 0.0);
                    } else {
                        set_morph(w, targets.look_right, gaze_x.min(1.0));
                        set_morph(w, targets.look_left, 0.0);
                    }

                    if gaze_y < 0.0 {
                        set_morph(w, targets.look_down, (-gaze_y).min(1.0));
                        set_morph(w, targets.look_up, 0.0);
                    } else {
                        set_morph(w, targets.look_up, gaze_y.min(1.0));
                        set_morph(w, targets.look_down, 0.0);
                    }
                }
            }
        }
    }
}
