//! Render cadence — staggered camera activation for GPU load distribution.

use bevy::prelude::*;

use super::components::*;
use super::super::types::{RenderSchedule, ActiveSpeechClips};
use super::super::scene::SlotRegistry;

/// Staggered render cadence — controls which cameras render each frame.
pub(in crate::live::video::bevy_renderer) fn manage_render_cadence(
    mut schedule: ResMut<RenderSchedule>,
    registry: Res<SlotRegistry>,
    speech_clips: Res<ActiveSpeechClips>,
    mut cameras: Query<&mut Camera>,
) {
    schedule.frame_count = schedule.frame_count.wrapping_add(1);
    let frame = schedule.frame_count;
    let cadence = schedule.idle_cadence;

    for (slot, slot_data) in &registry.slots {
        if !slot_data.is_active() {
            continue;
        }
        let cam_entity = match slot_data.camera_entity {
            Some(e) => e,
            None => continue,
        };

        let is_speaking = speech_clips.clips.contains_key(slot);
        let should_render = is_speaking || (frame % cadence == (*slot as u32 % cadence));

        if let Ok(mut camera) = cameras.get_mut(cam_entity) {
            camera.is_active = should_render;
        }
    }
}
