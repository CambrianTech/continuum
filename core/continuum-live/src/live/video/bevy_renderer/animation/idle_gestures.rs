//! Idle gesture animation — subtle upper-body micro-movements.

use bevy::prelude::*;

use super::super::scene::animation::{AnimationConfig, PORTRAIT_PROFILE};
use super::components::*;
use super::ExternalPose;

/// Idle micro-movements on entities with IdleMotion + Skeleton.
/// Skips entities owned by an external animator (`Without<ExternalPose>`).
pub(in crate::live::video::bevy_renderer) fn animate_idle_gestures(
    time: Res<Time>,
    mut query: Query<
        (
            &mut IdleMotion,
            &Skeleton,
            Option<&AnimationConfig>,
            Has<Speaking>,
            Has<GestureAnimation>,
        ),
        Without<ExternalPose>,
    >,
    speaking_entities: Query<Entity, With<Speaking>>,
    mut transforms: Query<&mut Transform>,
) {
    let dt = time.delta_secs();
    let any_speaking = !speaking_entities.is_empty();

    for (mut idle, skeleton, anim_cfg, is_speaking, has_gesture) in &mut query {
        // Active gesture takes over — skip idle
        if has_gesture {
            continue;
        }

        let t = time.elapsed_secs() + idle.phase;

        // Head turn toward speakers
        if is_speaking {
            idle.head_turn_target = 0.0;
        } else if any_speaking {
            // Simplified: turn slightly toward "something speaking"
            idle.head_turn_target = 0.1;
        } else {
            idle.head_turn_target = 0.0;
        }

        let lerp_factor = 1.0 - (-dt * 3.0_f32).exp();
        idle.head_turn_current += (idle.head_turn_target - idle.head_turn_current) * lerp_factor;

        if is_speaking {
            continue;
        }

        let (profile, freq_var) = match anim_cfg {
            Some(cfg) => (&cfg.profile, cfg.freq_variation),
            None => (&PORTRAIT_PROFILE, 1.0),
        };

        if let Some(ref neck) = skeleton.neck {
            if let Ok(mut transform) = transforms.get_mut(neck.entity) {
                let tilt_x = (t * 0.15 * freq_var).sin() * profile.neck_tilt_x_amplitude
                    + (t * 0.23 * freq_var).cos() * (profile.neck_tilt_x_amplitude * 0.67)
                    + (t * 0.37 * freq_var).sin() * (profile.neck_tilt_x_amplitude * 0.33);
                let tilt_z = (t * 0.12 * freq_var).cos() * profile.neck_tilt_z_amplitude
                    + (t * 0.31 * freq_var).sin() * (profile.neck_tilt_z_amplitude * 0.67);
                let idle_turn = (t * 0.08 * freq_var).sin() * profile.neck_turn_amplitude;
                let turn_y = idle_turn + idle.head_turn_current;

                let delta = Quat::from_euler(EulerRot::XYZ, tilt_x, turn_y, tilt_z);
                transform.rotation = neck.rest_rotation * delta;
            }
        }

        if let Some(ref left_shoulder) = skeleton.left_shoulder {
            if let Ok(mut transform) = transforms.get_mut(left_shoulder.entity) {
                let shift = (t * 0.4).sin() * profile.shoulder_shift_amplitude
                    + (t * 0.17).cos() * (profile.shoulder_shift_amplitude * 0.5);
                transform.translation.y = left_shoulder.rest_translation.y + shift;
            }
        }
        if let Some(ref right_shoulder) = skeleton.right_shoulder {
            if let Ok(mut transform) = transforms.get_mut(right_shoulder.entity) {
                let shift = (t * 0.4 + std::f32::consts::PI).sin()
                    * profile.shoulder_shift_amplitude
                    + (t * 0.17 + 1.0).cos() * (profile.shoulder_shift_amplitude * 0.5);
                transform.translation.y = right_shoulder.rest_translation.y + shift;
            }
        }
    }
}
