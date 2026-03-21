//! Speaking animation — mouth morph targets + subtle head nod during speech.

use bevy::mesh::morph::MorphWeights;
use bevy::prelude::*;

use super::components::*;
use super::super::scene::animation::{AnimationConfig, PORTRAIT_PROFILE};
use crate::clog_info;

/// Animate mouth + head nod on speaking entities.
pub(in crate::live::video::bevy_renderer) fn animate_speaking(
    time: Res<Time>,
    query: Query<(
        Entity,
        &MorphTargets,
        &MorphMeshLink,
        &Skeleton,
        Option<&SpeechClip>,
        Option<&MouthWeight>,
        Option<&AnimationConfig>,
        Has<Speaking>,
    )>,
    mut morph_weights: Query<&mut MorphWeights>,
    mut transforms: Query<&mut Transform>,
    mut commands: Commands,
) {
    let now = time.elapsed_secs();

    for (entity, targets, mesh_link, skeleton, clip, mouth_weight, anim_cfg, is_speaking) in &query {
        // Compute mouth weight from best available source
        let mouth_val = if let Some(clip) = clip {
            // Check if clip has expired
            let elapsed_ms = ((now - clip.start_time) * 1000.0) as u64;
            if elapsed_ms > clip.duration_ms + 200 {
                // Remove expired clip and Speaking marker
                commands.entity(entity).remove::<SpeechClip>();
                commands.entity(entity).remove::<Speaking>();
                0.0
            } else if clip.interval_ms == 0 || clip.mouth_weights.is_empty() {
                0.0
            } else {
                let elapsed = now - clip.start_time;
                let t = elapsed * 1000.0 / clip.interval_ms as f32;
                let idx = t as usize;
                if idx >= clip.mouth_weights.len() {
                    0.0
                } else if idx + 1 < clip.mouth_weights.len() {
                    let frac = t - idx as f32;
                    let a = clip.mouth_weights[idx];
                    let b = clip.mouth_weights[idx + 1];
                    (a + (b - a) * frac).clamp(0.0, 1.0)
                } else {
                    clip.mouth_weights[idx].clamp(0.0, 1.0)
                }
            }
        } else if let Some(mw) = mouth_weight {
            mw.0.clamp(0.0, 1.0)
        } else if is_speaking {
            // Fallback sine wave when Speaking marker present but no clip data
            ((now * 3.0 * std::f32::consts::TAU).sin() * 0.4 + 0.5).clamp(0.1, 0.9)
        } else {
            0.0
        };

        // Apply mouth morph
        if let Ok(mut weights) = morph_weights.get_mut(mesh_link.0) {
            set_morph(weights.weights_mut(), targets.mouth_open, mouth_val);
        }

        // Head nod during speech
        let should_nod = clip.is_some() || is_speaking;
        let profile = anim_cfg
            .map(|c| &c.profile)
            .unwrap_or(&PORTRAIT_PROFILE);

        if let Some(ref head) = skeleton.head {
            if let Ok(mut transform) = transforms.get_mut(head.entity) {
                if should_nod {
                    let t = now;
                    let nod = (t * 1.5 * std::f32::consts::TAU).sin() * profile.speaking_nod_amplitude;
                    let tilt = (t * 0.9).sin() * profile.speaking_tilt_amplitude;
                    let delta = Quat::from_euler(EulerRot::XYZ, nod, 0.0, tilt);
                    transform.rotation = head.rest_rotation * delta;
                } else {
                    transform.rotation = transform.rotation.slerp(head.rest_rotation, 0.3);
                }
            }
        }
    }
}
