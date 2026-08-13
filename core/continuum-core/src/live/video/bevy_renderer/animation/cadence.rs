//! Render cadence — staggered camera activation for GPU load distribution.

use bevy::prelude::*;

use super::super::scene::SlotRegistry;
use super::super::types::{ActiveSpeechClips, RenderSchedule, SharedMemoryStats};
use super::components::*;

/// The render governor's policy: given how many slots are ACTIVE and how many are
/// SPEAKING, decide how often the IDLE slots should render (`idle_cadence`).
///
/// [[multimodal-live-mode-is-a-latency-obsession]] made concrete: a moving mouth at
/// low fps is the ONE thing an eye catches, so speakers always render every frame
/// (cadence 1) and consume the frame budget first. Idle faces (breathing, listening)
/// are near-static — dropping them to 1/N fps is unnoticed, so they share whatever
/// budget the speakers leave. This is the render's CBAR-DVFS knob that keeps 14
/// personas viable on one machine: 1 speaker in a small call renders full-fps; a
/// crowded call throttles the idle faces, never the talker.
///
/// The `FULL_FPS_BUDGET` (how many slots a machine can render every frame) is a sane
/// single-machine default here ("priorities are what they are, non-grid first"); a
/// follow-up feeds it from the CBAR governor / measured GPU headroom.
pub fn adaptive_idle_cadence(active_slots: usize, speaking_slots: usize) -> u32 {
    const FULL_FPS_BUDGET: usize = 4;
    const MAX_CADENCE: u32 = 8;

    let idle = active_slots.saturating_sub(speaking_slots);
    let left = FULL_FPS_BUDGET.saturating_sub(speaking_slots).max(1);
    if idle <= left {
        1 // light enough — every face renders smooth
    } else {
        // ceil(idle / left), clamped: the more idle faces pile onto the remaining
        // budget, the slower each one renders.
        let raw = (idle + left - 1) / left;
        (raw as u32).clamp(1, MAX_CADENCE)
    }
}

/// The render-governor SYSTEM: each tick, count the active + speaking slots and store
/// the policy's `idle_cadence` into `desired_idle_cadence`. `sync_idle_cadence` then
/// applies it to the `RenderSchedule`, and `manage_render_cadence` gates the cameras.
/// Without this, `desired_idle_cadence` stays at its init value (1) forever and idle
/// faces never throttle — the adaptive machinery was built but never driven.
pub(in crate::live::video::bevy_renderer) fn govern_idle_cadence(
    registry: Res<SlotRegistry>,
    speech_clips: Res<ActiveSpeechClips>,
    stats: Res<SharedMemoryStats>,
) {
    let mut active = 0usize;
    let mut speaking = 0usize;
    for (slot, slot_data) in &registry.slots {
        if !slot_data.is_active() {
            continue;
        }
        active += 1;
        if speech_clips.clips.contains_key(slot) {
            speaking += 1;
        }
    }
    let cadence = adaptive_idle_cadence(active, speaking);
    stats
        .0
        .desired_idle_cadence
        .store(cadence, std::sync::atomic::Ordering::Relaxed);
}

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

#[cfg(test)]
mod tests {
    use super::adaptive_idle_cadence;

    // what this catches: the "managed for misfits" invariant — a light call renders
    // every face smooth, a crowded call throttles the IDLE faces (never the speaker),
    // so 14 personas stay viable on one machine. Speakers (a moving mouth) never drop.
    #[test]
    fn cadence_light_smooth_heavy_throttles_idle_never_speaker() {
        // Under budget → everything smooth (cadence 1).
        assert_eq!(
            adaptive_idle_cadence(2, 1),
            1,
            "1 speaker, small call → full fps"
        );
        assert_eq!(
            adaptive_idle_cadence(4, 0),
            1,
            "4 idle within budget → smooth"
        );
        // Crowded call → idle faces throttle.
        assert!(
            adaptive_idle_cadence(14, 2) > 1,
            "14 active / 2 speaking must throttle the idle faces"
        );
        // More idle pressure → cadence never decreases (monotonic).
        assert!(adaptive_idle_cadence(14, 1) >= adaptive_idle_cadence(8, 1));
        // All speakers → no idle to throttle → cadence 1 (speakers never self-throttle).
        assert_eq!(adaptive_idle_cadence(4, 4), 1);
        // Clamped so a runaway count can't make idle render effectively never.
        assert!(adaptive_idle_cadence(100, 0) <= 8);
    }
}
