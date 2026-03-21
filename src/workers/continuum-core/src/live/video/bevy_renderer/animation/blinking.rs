//! Blinking animation — random eye blinks via morph targets.

use bevy::mesh::morph::MorphWeights;
use bevy::prelude::*;

use super::components::*;
use super::prng::SlotRng;

/// Animate eye blinks on any entity with BlinkAnimation + MorphTargets.
pub(in crate::live::video::bevy_renderer) fn animate_blinking(
    time: Res<Time>,
    mut query: Query<(&mut BlinkAnimation, &MorphTargets, &MorphMeshLink, &SlotId)>,
    mut morph_weights: Query<&mut MorphWeights>,
) {
    let elapsed = time.elapsed_secs();

    for (mut blink, targets, mesh_link, slot_id) in &mut query {
        if !targets.has_blink() {
            continue;
        }

        if blink.blink_frames_remaining == 0 && elapsed >= blink.next_blink_time {
            blink.blink_frames_remaining = 3;
            let mut rng = SlotRng::new(elapsed, slot_id.0);
            blink.next_blink_time = elapsed + 2.0 + rng.range(0.0, 4.0);
        }

        if let Ok(mut weights) = morph_weights.get_mut(mesh_link.0) {
            let w = weights.weights_mut();
            let blink_weight = if blink.blink_frames_remaining > 0 {
                blink.blink_frames_remaining -= 1;
                1.0
            } else {
                0.0
            };

            set_morph(w, targets.blink, blink_weight);
            set_morph(w, targets.blink_left, blink_weight);
            set_morph(w, targets.blink_right, blink_weight);
        }
    }
}
