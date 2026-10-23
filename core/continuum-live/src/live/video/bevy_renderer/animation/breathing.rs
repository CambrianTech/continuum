//! Breathing animation — subtle spine/chest oscillation.

use bevy::prelude::*;

use super::super::scene::animation::{AnimationConfig, PORTRAIT_PROFILE};
use super::components::*;
use super::ExternalPose;

/// Animate breathing on any entity with BreathingAnimation + Skeleton.
/// Skips entities owned by an external animator (`Without<ExternalPose>`) so a
/// VLA-driven avatar isn't double-written by `apply_external_pose`.
pub(in crate::live::video::bevy_renderer) fn animate_breathing(
    time: Res<Time>,
    query: Query<(&BreathingAnimation, &Skeleton, Option<&AnimationConfig>), Without<ExternalPose>>,
    mut transforms: Query<&mut Transform>,
) {
    for (breathing, skeleton, anim_cfg) in &query {
        let spine = match &skeleton.spine {
            Some(s) => s,
            None => continue,
        };

        let (profile, freq_var) = match anim_cfg {
            Some(cfg) => (&cfg.profile, cfg.freq_variation),
            None => (&PORTRAIT_PROFILE, 1.0),
        };

        if let Ok(mut transform) = transforms.get_mut(spine.entity) {
            let t = time.elapsed_secs() + breathing.phase_offset;
            let breath = (t * profile.breathing_frequency * std::f32::consts::TAU).sin()
                * profile.breathing_scale_amplitude;
            transform.scale.y = 1.0 + breath;
            let sway =
                (t * profile.spine_sway_frequency * freq_var).sin() * profile.spine_sway_amplitude;
            let delta = Quat::from_rotation_z(sway);
            transform.rotation = spine.rest_rotation * delta;
        }
    }
}
