//! Expression animation — emotional expressions via blend shapes.

use bevy::mesh::morph::MorphWeights;
use bevy::prelude::*;

use super::components::*;

/// Animate emotional expressions on entities with EmotionAnimation + MorphTargets.
pub(in crate::live::video::bevy_renderer) fn animate_expression(
    time: Res<Time>,
    mut query: Query<(
        &mut EmotionAnimation,
        &MorphTargets,
        &MorphMeshLink,
        Has<Speaking>,
        Has<SpeechClip>,
    )>,
    mut morph_weights: Query<&mut MorphWeights>,
) {
    let dt = time.delta_secs();

    for (mut state, targets, mesh_link, is_speaking, has_clip) in &mut query {
        // Decay timer
        if state.decay_timer > 0.0 {
            state.decay_timer -= dt;
            if state.decay_timer <= 0.0 {
                state.target = Emotion::Neutral;
                state.target_weight = 0.0;
                state.transition_rate = 1.0;
            }
        }

        // Transition current → target
        if state.target != state.current && state.current_weight > 0.01 {
            state.current_weight = (state.current_weight - state.transition_rate * dt).max(0.0);
            if state.current_weight <= 0.01 {
                state.current_weight = 0.0;
                state.current = state.target;
            }
        } else {
            state.current = state.target;
            if state.current_weight < state.target_weight {
                state.current_weight =
                    (state.current_weight + state.transition_rate * dt).min(state.target_weight);
            } else if state.current_weight > state.target_weight {
                state.current_weight =
                    (state.current_weight - state.transition_rate * dt).max(state.target_weight);
            }
        }

        let effective_weight = if is_speaking || has_clip {
            state.current_weight * SPEECH_ATTENUATION
        } else {
            state.current_weight
        };

        if state.current == Emotion::Neutral || effective_weight < 0.001 {
            continue;
        }

        if let Ok(mut weights) = morph_weights.get_mut(mesh_link.0) {
            let w = weights.weights_mut();
            let idx = match state.current {
                Emotion::Happy => targets.happy,
                Emotion::Sad => targets.sad,
                Emotion::Angry => targets.angry,
                Emotion::Surprised => targets.surprised,
                Emotion::Relaxed => targets.relaxed,
                Emotion::Neutral => None,
            };
            set_morph(w, idx, effective_weight);
        }
    }
}
