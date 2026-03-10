//! Animation systems — morph targets, blinking, breathing, speaking, gestures, eye gaze.
//!
//! All Bevy systems that animate avatar bones and morph targets.
//! Gated by `has_active_slots` — zero work when no avatars are loaded.

use bevy::mesh::morph::MorphWeights;
use bevy::prelude::*;
use std::collections::HashSet;

use super::skeleton::{camera_z_for_head, REFERENCE_CAMERA_Z};
use super::types::*;
use super::vrm;
use crate::clog_info;


// =============================================================================
// Morph Target Discovery
// =============================================================================

/// Discover morph target indices from loaded mesh assets.
/// Runs every frame but only acts on slots that haven't been discovered yet.
pub(super) fn discover_morph_targets(
    registry: Res<SlotRegistry>,
    meshes: Res<Assets<Mesh>>,
    morph_query: Query<(Entity, &MorphWeights)>,
    children_query: Query<&Children>,
    mut morph_targets: ResMut<SlotMorphTargets>,
) {
    for (slot, state) in &registry.slots {
        if morph_targets.layouts.contains_key(slot) || !state.active {
            continue;
        }
        let scene_entity = match state.scene_entity {
            Some(e) => e,
            None => continue,
        };

        let morph_entity = match find_morph_entity(scene_entity, &children_query, &morph_query) {
            Some(e) => e,
            None => continue,
        };

        // Get morph target names from the Mesh asset
        let mesh_names: Vec<String> = morph_query
            .get(morph_entity)
            .ok()
            .and_then(|(_, weights)| weights.first_mesh())
            .and_then(|mesh_handle| meshes.get(mesh_handle))
            .and_then(|mesh| mesh.morph_target_names())
            .map(|names| names.to_vec())
            .unwrap_or_default();

        let mut mouth_open_index = None;
        let mut blink_index = None;
        let mut blink_left_index = None;
        let mut blink_right_index = None;
        let mut happy_index = None;
        let mut sad_index = None;
        let mut angry_index = None;
        let mut surprised_index = None;
        let mut relaxed_index = None;
        let mut look_up = None;
        let mut look_down = None;
        let mut look_left = None;
        let mut look_right = None;

        if !mesh_names.is_empty() {
            discover_from_mesh_names(
                &mesh_names,
                &mut mouth_open_index,
                &mut blink_index,
                &mut blink_left_index,
                &mut blink_right_index,
                &mut happy_index,
                &mut sad_index,
                &mut angry_index,
                &mut surprised_index,
                &mut relaxed_index,
                &mut look_up,
                &mut look_down,
                &mut look_left,
                &mut look_right,
            );
        } else if let Some(model_path) = &state.model_path {
            discover_from_vrm_extension(
                model_path,
                *slot,
                &mut mouth_open_index,
                &mut blink_index,
                &mut blink_left_index,
                &mut blink_right_index,
                &mut happy_index,
                &mut sad_index,
                &mut angry_index,
                &mut surprised_index,
                &mut relaxed_index,
                &mut look_up,
                &mut look_down,
                &mut look_left,
                &mut look_right,
            );
        }

        let weight_count = morph_query
            .get(morph_entity)
            .ok()
            .map(|(_, w)| w.weights().len())
            .unwrap_or(0);

        let emotion_count = [
            happy_index,
            sad_index,
            angry_index,
            surprised_index,
            relaxed_index,
        ]
        .iter()
        .filter(|i| i.is_some())
        .count();
        let gaze_count = [look_up, look_down, look_left, look_right]
            .iter()
            .filter(|i| i.is_some())
            .count();
        clog_info!(
            "🎨 Morph discovery slot {}: {} weights, {} names, mouth={:?}, blink={:?}, blink_l={:?}, blink_r={:?}, emotions={}/5, gaze={}/4",
            slot, weight_count, mesh_names.len(), mouth_open_index, blink_index, blink_left_index, blink_right_index,
            emotion_count, gaze_count,
        );

        morph_targets.layouts.insert(
            *slot,
            MorphTargetLayout {
                mesh_entity: morph_entity,
                mouth_open_index,
                blink_index,
                blink_left_index,
                blink_right_index,
                happy_index,
                sad_index,
                angry_index,
                surprised_index,
                relaxed_index,
                look_up,
                look_down,
                look_left,
                look_right,
            },
        );
    }
}

/// Discover morph target indices from standard glTF mesh target names.
#[allow(clippy::too_many_arguments)]
fn discover_from_mesh_names(
    mesh_names: &[String],
    mouth_open_index: &mut Option<usize>,
    blink_index: &mut Option<usize>,
    blink_left_index: &mut Option<usize>,
    blink_right_index: &mut Option<usize>,
    happy_index: &mut Option<usize>,
    sad_index: &mut Option<usize>,
    angry_index: &mut Option<usize>,
    surprised_index: &mut Option<usize>,
    relaxed_index: &mut Option<usize>,
    look_up: &mut Option<usize>,
    look_down: &mut Option<usize>,
    look_left: &mut Option<usize>,
    look_right: &mut Option<usize>,
) {
    for (i, name) in mesh_names.iter().enumerate() {
        let lower = name.to_lowercase();
        if mouth_open_index.is_none()
            && (lower == "aa"
                || lower == "a"
                || lower.ends_with("_mth_a")
                || lower.ends_with("mth_a")
                || lower.ends_with("_v_aa")
                || lower == "v_aa"
                || lower.ends_with("mouth_open")
                || lower.ends_with("jawopen")
                || lower == "fcl_mth_a")
        {
            *mouth_open_index = Some(i);
        }
        if blink_index.is_none()
            && (lower == "blink"
                || lower == "fcl_eye_close"
                || (lower.contains("eye_close")
                    && !lower.contains("_l")
                    && !lower.contains("_r")
                    && !lower.contains("left")
                    && !lower.contains("right"))
                || lower == "vrc.blink")
        {
            *blink_index = Some(i);
        }
        if blink_left_index.is_none()
            && (lower == "blinkleft"
                || lower == "blink_l"
                || lower == "fcl_eye_close_l"
                || lower.contains("eye_close_l")
                || lower.contains("eye_close_left"))
        {
            *blink_left_index = Some(i);
        }
        if blink_right_index.is_none()
            && (lower == "blinkright"
                || lower == "blink_r"
                || lower == "fcl_eye_close_r"
                || lower.contains("eye_close_r")
                || lower.contains("eye_close_right"))
        {
            *blink_right_index = Some(i);
        }
        if happy_index.is_none()
            && (lower == "happy"
                || lower == "joy"
                || lower.ends_with("_joy")
                || lower.ends_with("_happy")
                || lower == "fcl_all_joy"
                || lower == "fcl_eye_joy")
        {
            *happy_index = Some(i);
        }
        if sad_index.is_none()
            && (lower == "sad"
                || lower == "sorrow"
                || lower.ends_with("_sad")
                || lower.ends_with("_sorrow")
                || lower == "fcl_all_sorrow"
                || lower == "fcl_eye_sorrow")
        {
            *sad_index = Some(i);
        }
        if angry_index.is_none()
            && (lower == "angry"
                || lower.ends_with("_angry")
                || lower == "fcl_all_angry"
                || lower == "fcl_mth_angry")
        {
            *angry_index = Some(i);
        }
        if surprised_index.is_none()
            && (lower == "surprised"
                || lower == "fun"
                || lower.ends_with("_surprised")
                || lower.ends_with("_fun")
                || lower == "fcl_all_fun"
                || lower == "fcl_brw_surprised")
        {
            *surprised_index = Some(i);
        }
        if relaxed_index.is_none()
            && (lower == "relaxed"
                || lower.ends_with("_relaxed")
                || lower == "fcl_all_relaxed")
        {
            *relaxed_index = Some(i);
        }
        if look_up.is_none()
            && (lower == "lookup"
                || lower == "look_up"
                || lower.ends_with("lookup")
                || lower == "fcl_eye_lookup")
        {
            *look_up = Some(i);
        }
        if look_down.is_none()
            && (lower == "lookdown"
                || lower == "look_down"
                || lower.ends_with("lookdown")
                || lower == "fcl_eye_lookdown")
        {
            *look_down = Some(i);
        }
        if look_left.is_none()
            && (lower == "lookleft"
                || lower == "look_left"
                || lower.ends_with("lookleft")
                || lower == "fcl_eye_lookleft")
        {
            *look_left = Some(i);
        }
        if look_right.is_none()
            && (lower == "lookright"
                || lower == "look_right"
                || lower.ends_with("lookright")
                || lower == "fcl_eye_lookright")
        {
            *look_right = Some(i);
        }
    }
}

/// Discover morph target indices from VRM extension blend shapes.
#[allow(clippy::too_many_arguments)]
fn discover_from_vrm_extension(
    model_path: &str,
    slot: u8,
    mouth_open_index: &mut Option<usize>,
    blink_index: &mut Option<usize>,
    blink_left_index: &mut Option<usize>,
    blink_right_index: &mut Option<usize>,
    happy_index: &mut Option<usize>,
    sad_index: &mut Option<usize>,
    angry_index: &mut Option<usize>,
    surprised_index: &mut Option<usize>,
    relaxed_index: &mut Option<usize>,
    look_up: &mut Option<usize>,
    look_down: &mut Option<usize>,
    look_left: &mut Option<usize>,
    look_right: &mut Option<usize>,
) {
    if let Some(vrm_shapes) = vrm::parse_vrm_blend_shapes(model_path) {
        for shape in &vrm_shapes {
            let preset = shape.preset_name.to_lowercase();
            if mouth_open_index.is_none() && (preset == "a" || preset == "aa") {
                if let Some(bind) = shape.binds.first() {
                    *mouth_open_index = Some(bind.index);
                }
            }
            if blink_index.is_none() && preset == "blink" {
                if let Some(bind) = shape.binds.first() {
                    *blink_index = Some(bind.index);
                }
                if shape.binds.len() >= 2 {
                    *blink_left_index = Some(shape.binds[0].index);
                    *blink_right_index = Some(shape.binds[1].index);
                }
            }
            if blink_left_index.is_none() && (preset == "blink_l" || preset == "blinkleft") {
                if let Some(bind) = shape.binds.first() {
                    *blink_left_index = Some(bind.index);
                }
            }
            if blink_right_index.is_none() && (preset == "blink_r" || preset == "blinkright") {
                if let Some(bind) = shape.binds.first() {
                    *blink_right_index = Some(bind.index);
                }
            }
            if happy_index.is_none() && (preset == "joy" || preset == "happy") {
                if let Some(bind) = shape.binds.first() {
                    *happy_index = Some(bind.index);
                }
            }
            if sad_index.is_none() && (preset == "sorrow" || preset == "sad") {
                if let Some(bind) = shape.binds.first() {
                    *sad_index = Some(bind.index);
                }
            }
            if angry_index.is_none() && preset == "angry" {
                if let Some(bind) = shape.binds.first() {
                    *angry_index = Some(bind.index);
                }
            }
            if surprised_index.is_none() && (preset == "fun" || preset == "surprised") {
                if let Some(bind) = shape.binds.first() {
                    *surprised_index = Some(bind.index);
                }
            }
            if relaxed_index.is_none() && preset == "relaxed" {
                if let Some(bind) = shape.binds.first() {
                    *relaxed_index = Some(bind.index);
                }
            }
            if look_up.is_none() && (preset == "lookup" || preset == "lookUp") {
                if let Some(bind) = shape.binds.first() {
                    *look_up = Some(bind.index);
                }
            }
            if look_down.is_none() && (preset == "lookdown" || preset == "lookDown") {
                if let Some(bind) = shape.binds.first() {
                    *look_down = Some(bind.index);
                }
            }
            if look_left.is_none() && (preset == "lookleft" || preset == "lookLeft") {
                if let Some(bind) = shape.binds.first() {
                    *look_left = Some(bind.index);
                }
            }
            if look_right.is_none() && (preset == "lookright" || preset == "lookRight") {
                if let Some(bind) = shape.binds.first() {
                    *look_right = Some(bind.index);
                }
            }
        }
        clog_info!(
            "🎨 VRM blend shapes slot {}: {} groups parsed",
            slot,
            vrm_shapes.len()
        );
    }
}

/// Find the first entity with MorphWeights in a scene hierarchy.
fn find_morph_entity(
    root: Entity,
    children: &Query<&Children>,
    morph_query: &Query<(Entity, &MorphWeights)>,
) -> Option<Entity> {
    if morph_query.get(root).is_ok() {
        return Some(root);
    }
    if let Ok(child_list) = children.get(root) {
        for child in child_list.iter() {
            if let Some(found) = find_morph_entity(child, children, morph_query) {
                return Some(found);
            }
        }
    }
    None
}

// =============================================================================
// Camera / Idle
// =============================================================================

/// Idle animation — gentle camera sway + head-targeted framing.
pub(super) fn animate_idle(
    time: Res<Time>,
    registry: Res<SlotRegistry>,
    bone_registry: Res<BoneRegistry>,
    global_transforms: Query<&GlobalTransform>,
    mut transforms: Query<&mut Transform, With<AvatarSlotId>>,
) {
    for (slot, state) in &registry.slots {
        if !state.active {
            continue;
        }
        if let Ok(mut transform) = transforms.get_mut(state.camera_entity) {
            let t = time.elapsed_secs() + *slot as f32 * 0.7;
            let sway_x = (t * 0.3).sin() * 0.02;
            let sway_y = (t * 0.2).cos() * 0.01;

            let (base_y, look_y, cam_z) = if let Some(slot_bones) = bone_registry.slots.get(slot) {
                if let Some(ref head) = slot_bones.head {
                    if let Ok(global) = global_transforms.get(head.entity) {
                        let head_world_y = global.translation().y;
                        let eye_y = head_world_y + 0.06;
                        let z = camera_z_for_head(head_world_y);
                        (eye_y + 0.02, eye_y, z)
                    } else {
                        (1.50, 1.47, REFERENCE_CAMERA_Z)
                    }
                } else {
                    (1.50, 1.47, REFERENCE_CAMERA_Z)
                }
            } else {
                (1.50, 1.47, REFERENCE_CAMERA_Z)
            };

            transform.translation.x = sway_x;
            transform.translation.y = base_y + sway_y;
            transform.translation.z = cam_z;
            let look_target = Vec3::new(0.0, look_y, 0.0);
            *transform = transform.looking_at(look_target, Vec3::Y);
        }
    }
}

// =============================================================================
// Speaking
// =============================================================================

/// Animate mouth morph targets + subtle head nod during speech.
#[allow(clippy::too_many_arguments)]
pub(super) fn animate_speaking(
    time: Res<Time>,
    speaking_query: Query<&AvatarSlotId, With<Speaking>>,
    morph_targets: Res<SlotMorphTargets>,
    bone_registry: Res<BoneRegistry>,
    mut speech_clips: ResMut<ActiveSpeechClips>,
    legacy_mouth: Res<LegacyMouthWeights>,
    mut morph_weights: Query<&mut MorphWeights>,
    mut transforms: Query<&mut Transform>,
    mut commands: Commands,
    registry: Res<SlotRegistry>,
) {
    let speaking_slots: HashSet<u8> = speaking_query.iter().map(|id| id.0).collect();
    let now = time.elapsed_secs();

    // Auto-stop expired clips
    let mut expired: Vec<u8> = Vec::new();
    for (&slot, clip) in &speech_clips.clips {
        let elapsed_ms = ((now - clip.start_time) * 1000.0) as u64;
        if elapsed_ms > clip.duration_ms + 200 {
            expired.push(slot);
        }
    }
    for slot in &expired {
        speech_clips.clips.remove(slot);
        speech_clips.clips_auto_stopped += 1;
        if let Some(state) = registry.slots.get(slot) {
            if let Some(scene_entity) = state.scene_entity {
                commands.entity(scene_entity).remove::<Speaking>();
            }
        }
    }

    // Periodic stats flush
    {
        use std::sync::atomic::{AtomicU32, Ordering};
        static FRAME_COUNTER: AtomicU32 = AtomicU32::new(0);
        let frame = FRAME_COUNTER.fetch_add(1, Ordering::Relaxed);
        if frame.is_multiple_of(300) {
            let started = speech_clips.clips_started;
            let stopped = speech_clips.clips_auto_stopped;
            let interrupted = speech_clips.clips_interrupted;
            if started > 0 || stopped > 0 || interrupted > 0 {
                clog_info!(
                    "🎨 Speech stats: {} started, {} auto-stopped, {} interrupted, {} active",
                    started,
                    stopped,
                    interrupted,
                    speech_clips.clips.len()
                );
                speech_clips.clips_started = 0;
                speech_clips.clips_auto_stopped = 0;
                speech_clips.clips_interrupted = 0;
            }
        }
    }

    for (slot, layout) in &morph_targets.layouts {
        let has_clip = speech_clips.clips.contains_key(slot);
        let is_speaking = speaking_slots.contains(slot);

        if let Ok(mut weights) = morph_weights.get_mut(layout.mesh_entity) {
            let w = weights.weights_mut();

            let mouth_weight = if let Some(clip) = speech_clips.clips.get(slot) {
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
            } else if let Some(&amplitude) = legacy_mouth.weights.get(slot) {
                amplitude.clamp(0.0, 1.0)
            } else if is_speaking {
                let t = now;
                ((t * 3.0 * std::f32::consts::TAU).sin() * 0.4 + 0.5).clamp(0.1, 0.9)
            } else {
                0.0
            };

            if let Some(idx) = layout.mouth_open_index {
                if idx < w.len() {
                    w[idx] = mouth_weight;
                }
            }
        }

        // Head nod during speech
        let should_nod = has_clip || is_speaking;
        if let Some(slot_bones) = bone_registry.slots.get(slot) {
            if let Some(ref head) = slot_bones.head {
                if let Ok(mut transform) = transforms.get_mut(head.entity) {
                    if should_nod {
                        let t = now + *slot as f32 * 1.3;
                        let nod = (t * 1.5 * std::f32::consts::TAU).sin() * 0.035;
                        let tilt = (t * 0.9).sin() * 0.02;
                        let delta = Quat::from_euler(EulerRot::XYZ, nod, 0.0, tilt);
                        transform.rotation = head.rest_rotation * delta;
                    } else {
                        transform.rotation = transform.rotation.slerp(head.rest_rotation, 0.3);
                    }
                }
            }
        }
    }
}

// =============================================================================
// Expression
// =============================================================================

/// Animate emotional expressions via discovered VRM blend shapes.
pub(super) fn animate_expression(
    time: Res<Time>,
    morph_targets: Res<SlotMorphTargets>,
    mut emotion_state: ResMut<EmotionState>,
    speech_clips: Res<ActiveSpeechClips>,
    speaking_query: Query<&AvatarSlotId, With<Speaking>>,
    mut morph_weights: Query<&mut MorphWeights>,
) {
    let dt = time.delta_secs();
    let speaking_slots: HashSet<u8> = speaking_query.iter().map(|id| id.0).collect();

    for (slot, layout) in &morph_targets.layouts {
        let state = match emotion_state.slots.get_mut(slot) {
            Some(s) => s,
            None => continue,
        };

        if state.decay_timer > 0.0 {
            state.decay_timer -= dt;
            if state.decay_timer <= 0.0 {
                state.target = Emotion::Neutral;
                state.target_weight = 0.0;
                state.transition_rate = 1.0;
            }
        }

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

        let is_speaking = speaking_slots.contains(slot) || speech_clips.clips.contains_key(slot);
        let effective_weight = if is_speaking {
            state.current_weight * SPEECH_ATTENUATION
        } else {
            state.current_weight
        };

        if state.current == Emotion::Neutral || effective_weight < 0.001 {
            continue;
        }

        if let Ok(mut weights) = morph_weights.get_mut(layout.mesh_entity) {
            let w = weights.weights_mut();
            let idx = match state.current {
                Emotion::Happy => layout.happy_index,
                Emotion::Sad => layout.sad_index,
                Emotion::Angry => layout.angry_index,
                Emotion::Surprised => layout.surprised_index,
                Emotion::Relaxed => layout.relaxed_index,
                Emotion::Neutral => None,
            };
            if let Some(i) = idx {
                if i < w.len() {
                    w[i] = effective_weight;
                }
            }
        }
    }
}

// =============================================================================
// Blinking
// =============================================================================

/// Animate random eye blinks across all active avatar slots.
pub(super) fn animate_blinking(
    time: Res<Time>,
    morph_targets: Res<SlotMorphTargets>,
    mut blink_state: ResMut<BlinkState>,
    mut morph_weights: Query<&mut MorphWeights>,
) {
    let elapsed = time.elapsed_secs();

    for (slot, layout) in &morph_targets.layouts {
        let state = blink_state.slots.entry(*slot).or_insert_with(|| {
            SlotBlinkState {
                next_blink_time: elapsed + 1.0 + (*slot as f32 * 0.73) % 4.0,
                blink_frames_remaining: 0,
            }
        });

        let has_blink = layout.blink_index.is_some()
            || (layout.blink_left_index.is_some() && layout.blink_right_index.is_some());

        if !has_blink {
            continue;
        }

        if state.blink_frames_remaining == 0 && elapsed >= state.next_blink_time {
            state.blink_frames_remaining = 3;
            let pseudo_rand = ((elapsed * 1000.0 + *slot as f32 * 137.0) % 4000.0) / 1000.0;
            state.next_blink_time = elapsed + 2.0 + pseudo_rand;
        }

        if let Ok(mut weights) = morph_weights.get_mut(layout.mesh_entity) {
            let w = weights.weights_mut();
            let blink_weight = if state.blink_frames_remaining > 0 {
                state.blink_frames_remaining -= 1;
                1.0
            } else {
                0.0
            };

            if let Some(idx) = layout.blink_index {
                if idx < w.len() {
                    w[idx] = blink_weight;
                }
            }
            if let Some(idx) = layout.blink_left_index {
                if idx < w.len() {
                    w[idx] = blink_weight;
                }
            }
            if let Some(idx) = layout.blink_right_index {
                if idx < w.len() {
                    w[idx] = blink_weight;
                }
            }
        }
    }
}

// =============================================================================
// Idle Gestures (micro-movements)
// =============================================================================

/// Idle gesture system — subtle upper-body micro-movements.
pub(super) fn animate_idle_gestures(
    time: Res<Time>,
    registry: Res<SlotRegistry>,
    bone_registry: Res<BoneRegistry>,
    speaking_query: Query<&AvatarSlotId, With<Speaking>>,
    active_gestures: Res<ActiveGestures>,
    mut gesture_state: ResMut<IdleGestureState>,
    mut transforms: Query<&mut Transform>,
) {
    let dt = time.delta_secs();
    let speaking_slots: HashSet<u8> = speaking_query.iter().map(|id| id.0).collect();

    for (slot, state) in &registry.slots {
        if !state.active {
            continue;
        }

        let is_speaking = speaking_slots.contains(slot);

        if active_gestures.slots.contains_key(slot) {
            continue;
        }

        let slot_bones = match bone_registry.slots.get(slot) {
            Some(b) => b,
            None => continue,
        };

        let gesture = gesture_state
            .slots
            .entry(*slot)
            .or_insert_with(|| SlotGestureState {
                phase: *slot as f32 * 2.37,
                head_turn_current: 0.0,
                head_turn_target: 0.0,
            });

        let t = time.elapsed_secs() + gesture.phase;

        if is_speaking {
            gesture.head_turn_target = 0.0;
        } else if !speaking_slots.is_empty() {
            let turn_bias: f32 = speaking_slots
                .iter()
                .map(|&s| {
                    let diff = s as f32 - *slot as f32;
                    diff.signum() * 0.15
                })
                .sum::<f32>()
                .clamp(-0.25, 0.25);
            gesture.head_turn_target = turn_bias;
        } else {
            gesture.head_turn_target = 0.0;
        }

        let lerp_factor = 1.0 - (-dt * 3.0_f32).exp();
        gesture.head_turn_current +=
            (gesture.head_turn_target - gesture.head_turn_current) * lerp_factor;

        if is_speaking {
            continue;
        }

        if let Some(ref neck) = slot_bones.neck {
            if let Ok(mut transform) = transforms.get_mut(neck.entity) {
                let tilt_x = (t * 0.15).sin() * 0.03
                    + (t * 0.23).cos() * 0.02
                    + (t * 0.37).sin() * 0.01;
                let tilt_z = (t * 0.12).cos() * 0.025 + (t * 0.31).sin() * 0.015;
                let idle_turn = (t * 0.08).sin() * 0.02;
                let turn_y = idle_turn + gesture.head_turn_current;

                let delta = Quat::from_euler(EulerRot::XYZ, tilt_x, turn_y, tilt_z);
                transform.rotation = neck.rest_rotation * delta;
            }
        }

        if let Some(ref left_shoulder) = slot_bones.left_shoulder {
            if let Ok(mut transform) = transforms.get_mut(left_shoulder.entity) {
                let shift = (t * 0.4).sin() * 0.002 + (t * 0.17).cos() * 0.001;
                transform.translation.y = left_shoulder.rest_translation.y + shift;
            }
        }
        if let Some(ref right_shoulder) = slot_bones.right_shoulder {
            if let Ok(mut transform) = transforms.get_mut(right_shoulder.entity) {
                let shift =
                    (t * 0.4 + std::f32::consts::PI).sin() * 0.002 + (t * 0.17 + 1.0).cos() * 0.001;
                transform.translation.y = right_shoulder.rest_translation.y + shift;
            }
        }
    }
}

// =============================================================================
// Breathing
// =============================================================================

/// Subtle breathing animation — gentle spine/chest oscillation.
pub(super) fn animate_breathing(
    time: Res<Time>,
    registry: Res<SlotRegistry>,
    bone_registry: Res<BoneRegistry>,
    mut transforms: Query<&mut Transform>,
) {
    for (slot, state) in &registry.slots {
        if !state.active {
            continue;
        }

        let spine = match bone_registry.slots.get(slot).and_then(|b| b.spine.as_ref()) {
            Some(s) => s,
            None => continue,
        };

        if let Ok(mut transform) = transforms.get_mut(spine.entity) {
            let t = time.elapsed_secs() + *slot as f32 * 1.1;
            let breath = (t * 0.8 * std::f32::consts::TAU).sin() * 0.005;
            transform.scale.y = 1.0 + breath;
            let sway = (t * 0.12).sin() * 0.012;
            let delta = Quat::from_rotation_z(sway);
            transform.rotation = spine.rest_rotation * delta;
        }
    }
}

// =============================================================================
// Eye Gaze
// =============================================================================

/// Animate eye gaze via look blend shapes or bone rotation.
pub(super) fn animate_eye_gaze(
    time: Res<Time>,
    registry: Res<SlotRegistry>,
    morph_targets: Res<SlotMorphTargets>,
    bone_registry: Res<BoneRegistry>,
    speaking_query: Query<&AvatarSlotId, With<Speaking>>,
    mut morph_weights: Query<&mut MorphWeights>,
    mut transforms: Query<&mut Transform>,
) {
    let speaking_slots: HashSet<u8> = speaking_query.iter().map(|id| id.0).collect();
    let t = time.elapsed_secs();

    for (slot, state) in &registry.slots {
        if !state.active {
            continue;
        }

        let is_speaking = speaking_slots.contains(slot);
        let phase = *slot as f32 * 2.73;

        let (gaze_x, gaze_y) = if is_speaking {
            let drift_x = (t * 0.3 + phase).sin() * 0.05;
            let drift_y = (t * 0.25 + phase).cos() * 0.03;
            (drift_x, drift_y)
        } else {
            let speaker_bias: f32 = speaking_slots
                .iter()
                .map(|&s| {
                    let diff = s as f32 - *slot as f32;
                    diff.signum() * 0.15
                })
                .sum::<f32>()
                .clamp(-0.3, 0.3);

            let drift_x = (t * 0.13 + phase).sin() * 0.12
                + (t * 0.07 + phase * 0.7).cos() * 0.08
                + speaker_bias;
            let drift_y = (t * 0.11 + phase).cos() * 0.08 + (t * 0.19 + phase * 1.3).sin() * 0.05;
            (drift_x.clamp(-0.4, 0.4), drift_y.clamp(-0.3, 0.3))
        };

        // Path 1: Bone-based eye gaze
        let mut used_bone_gaze = false;
        if let Some(slot_bones) = bone_registry.slots.get(slot) {
            if slot_bones.left_eye.is_some() && slot_bones.right_eye.is_some() {
                let config = slot_bones.look_at_config.unwrap_or_default();

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

                if let Some(ref left_eye) = slot_bones.left_eye {
                    if let Ok(mut transform) = transforms.get_mut(left_eye.entity) {
                        transform.rotation = left_eye.rest_rotation * gaze_delta;
                    }
                }
                if let Some(ref right_eye) = slot_bones.right_eye {
                    if let Ok(mut transform) = transforms.get_mut(right_eye.entity) {
                        transform.rotation = right_eye.rest_rotation * gaze_delta;
                    }
                }
                used_bone_gaze = true;
            }
        }

        // Path 2: Blend shape gaze
        if !used_bone_gaze {
            if let Some(layout) = morph_targets.layouts.get(slot) {
                let has_gaze = layout.look_up.is_some()
                    || layout.look_down.is_some()
                    || layout.look_left.is_some()
                    || layout.look_right.is_some();
                if !has_gaze {
                    continue;
                }

                if let Ok(mut weights) = morph_weights.get_mut(layout.mesh_entity) {
                    let w = weights.weights_mut();

                    if gaze_x < 0.0 {
                        if let Some(idx) = layout.look_left {
                            if idx < w.len() {
                                w[idx] = (-gaze_x).min(1.0);
                            }
                        }
                        if let Some(idx) = layout.look_right {
                            if idx < w.len() {
                                w[idx] = 0.0;
                            }
                        }
                    } else {
                        if let Some(idx) = layout.look_right {
                            if idx < w.len() {
                                w[idx] = gaze_x.min(1.0);
                            }
                        }
                        if let Some(idx) = layout.look_left {
                            if idx < w.len() {
                                w[idx] = 0.0;
                            }
                        }
                    }

                    if gaze_y < 0.0 {
                        if let Some(idx) = layout.look_down {
                            if idx < w.len() {
                                w[idx] = (-gaze_y).min(1.0);
                            }
                        }
                        if let Some(idx) = layout.look_up {
                            if idx < w.len() {
                                w[idx] = 0.0;
                            }
                        }
                    } else {
                        if let Some(idx) = layout.look_up {
                            if idx < w.len() {
                                w[idx] = gaze_y.min(1.0);
                            }
                        }
                        if let Some(idx) = layout.look_down {
                            if idx < w.len() {
                                w[idx] = 0.0;
                            }
                        }
                    }
                }
            }
        }
    }
}

// =============================================================================
// Body Gestures
// =============================================================================

/// Smoothstep easing function.
fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Cognitive gesture driver — selects and triggers gestures based on cognitive state.
pub(super) fn drive_cognitive_gestures(
    time: Res<Time>,
    mut cognitive_anim: ResMut<CognitiveAnimState>,
    mut active_gestures: ResMut<ActiveGestures>,
    speaking_query: Query<&AvatarSlotId, With<Speaking>>,
) {
    use crate::live::session::cognitive_animation::{select_weighted_gesture, CognitiveState};

    let dt = time.delta_secs();
    let elapsed = time.elapsed_secs();
    let speaking_slots: HashSet<u8> = speaking_query.iter().map(|id| id.0).collect();

    for (slot, cog) in cognitive_anim.slots.iter_mut() {
        cog.time_since_reroll += dt;

        if speaking_slots.contains(slot) {
            continue;
        }

        if active_gestures.slots.contains_key(slot) {
            continue;
        }

        if cog.time_since_reroll < cog.config.reroll_interval_secs {
            continue;
        }

        cog.time_since_reroll = 0.0;

        let table = match cog.state {
            CognitiveState::Evaluating => &cog.config.evaluating,
            CognitiveState::Generating => &cog.config.generating,
            CognitiveState::Idle => continue,
        };

        if let Some((gesture, duration_ms)) = select_weighted_gesture(table, elapsed, *slot) {
            if gesture != Gesture::None {
                active_gestures.slots.insert(
                    *slot,
                    SlotGestureAnimState {
                        gesture,
                        phase: GesturePhase::Attack,
                        duration_secs: duration_ms as f32 / 1000.0,
                        elapsed: 0.0,
                        weight: 0.0,
                    },
                );
            }
        }
    }
}

/// Body gesture animation system — drives arm/shoulder bones through gesture poses.
pub(super) fn animate_body_gestures(
    time: Res<Time>,
    bone_registry: Res<BoneRegistry>,
    mut active_gestures: ResMut<ActiveGestures>,
    mut transforms: Query<&mut Transform>,
) {
    let dt = time.delta_secs();
    let now = time.elapsed_secs();

    let mut finished: Vec<u8> = Vec::new();

    for (slot, anim) in active_gestures.slots.iter_mut() {
        anim.elapsed += dt;

        let attack_end = GESTURE_EASE_SECS;
        let sustain_end = anim.duration_secs - GESTURE_EASE_SECS;
        let total_end = anim.duration_secs;

        if anim.elapsed >= total_end {
            finished.push(*slot);
            continue;
        }

        anim.weight = if anim.elapsed < attack_end {
            anim.phase = GesturePhase::Attack;
            smoothstep(anim.elapsed / GESTURE_EASE_SECS)
        } else if anim.elapsed < sustain_end {
            anim.phase = GesturePhase::Sustain;
            1.0
        } else {
            anim.phase = GesturePhase::Release;
            let release_progress = (anim.elapsed - sustain_end) / GESTURE_EASE_SECS;
            1.0 - smoothstep(release_progress)
        };

        let slot_bones = match bone_registry.slots.get(slot) {
            Some(b) => b,
            None => continue,
        };

        let w = anim.weight;
        let t = now + *slot as f32 * 1.7;

        match anim.gesture {
            Gesture::Wave => {
                if let Some(ref rua) = slot_bones.right_upper_arm {
                    if let Ok(mut transform) = transforms.get_mut(rua.entity) {
                        let up_angle = -1.2 * w;
                        let delta = Quat::from_rotation_z(up_angle);
                        transform.rotation = rua.rest_rotation * delta;
                    }
                }
                if let Some(ref rla) = slot_bones.right_lower_arm {
                    if let Ok(mut transform) = transforms.get_mut(rla.entity) {
                        let wave = if anim.phase == GesturePhase::Sustain {
                            (t * 2.0 * std::f32::consts::TAU).sin() * 0.35
                        } else {
                            0.0
                        };
                        let bend = (-0.5 + wave) * w;
                        let delta = Quat::from_rotation_z(bend);
                        transform.rotation = rla.rest_rotation * delta;
                    }
                }
            }
            Gesture::Think => {
                if let Some(ref rua) = slot_bones.right_upper_arm {
                    if let Ok(mut transform) = transforms.get_mut(rua.entity) {
                        let forward = Quat::from_rotation_x(-0.8 * w);
                        let inward = Quat::from_rotation_z(-0.3 * w);
                        let delta = forward * inward;
                        transform.rotation = rua.rest_rotation * delta;
                    }
                }
                if let Some(ref rla) = slot_bones.right_lower_arm {
                    if let Ok(mut transform) = transforms.get_mut(rla.entity) {
                        let bend = Quat::from_rotation_z(2.0 * w);
                        transform.rotation = rla.rest_rotation * bend;
                    }
                }
                if let Some(ref head) = slot_bones.head {
                    if let Ok(mut transform) = transforms.get_mut(head.entity) {
                        let tilt = Quat::from_euler(EulerRot::XYZ, 0.05 * w, 0.0, 0.08 * w);
                        transform.rotation = head.rest_rotation * tilt;
                    }
                }
            }
            Gesture::Nod => {
                if let Some(ref head) = slot_bones.head {
                    if let Ok(mut transform) = transforms.get_mut(head.entity) {
                        let nod = (t * 1.5 * std::f32::consts::TAU).sin() * 0.12 * w;
                        let delta = Quat::from_rotation_x(nod);
                        transform.rotation = head.rest_rotation * delta;
                    }
                }
            }
            Gesture::Shrug => {
                if let Some(ref ls) = slot_bones.left_shoulder {
                    if let Ok(mut transform) = transforms.get_mut(ls.entity) {
                        transform.translation.y = ls.rest_translation.y + 0.01 * w;
                    }
                }
                if let Some(ref rs) = slot_bones.right_shoulder {
                    if let Ok(mut transform) = transforms.get_mut(rs.entity) {
                        transform.translation.y = rs.rest_translation.y + 0.01 * w;
                    }
                }
                if let Some(ref lua) = slot_bones.left_upper_arm {
                    if let Ok(mut transform) = transforms.get_mut(lua.entity) {
                        let out = Quat::from_rotation_z(-0.35 * w);
                        transform.rotation = lua.rest_rotation * out;
                    }
                }
                if let Some(ref rua) = slot_bones.right_upper_arm {
                    if let Ok(mut transform) = transforms.get_mut(rua.entity) {
                        let out = Quat::from_rotation_z(0.35 * w);
                        transform.rotation = rua.rest_rotation * out;
                    }
                }
            }
            Gesture::Point => {
                if let Some(ref rua) = slot_bones.right_upper_arm {
                    if let Ok(mut transform) = transforms.get_mut(rua.entity) {
                        let forward = Quat::from_rotation_x(-1.05 * w);
                        transform.rotation = rua.rest_rotation * forward;
                    }
                }
                if let Some(ref rla) = slot_bones.right_lower_arm {
                    if let Ok(mut transform) = transforms.get_mut(rla.entity) {
                        let straighten = Quat::from_rotation_z(0.26 * w);
                        transform.rotation = rla.rest_rotation * straighten;
                    }
                }
            }
            Gesture::OpenHands => {
                if let Some(ref lua) = slot_bones.left_upper_arm {
                    if let Ok(mut transform) = transforms.get_mut(lua.entity) {
                        let out = Quat::from_rotation_z(-0.4 * w);
                        let forward = Quat::from_rotation_x(-0.3 * w);
                        transform.rotation = lua.rest_rotation * forward * out;
                    }
                }
                if let Some(ref rua) = slot_bones.right_upper_arm {
                    if let Ok(mut transform) = transforms.get_mut(rua.entity) {
                        let out = Quat::from_rotation_z(0.4 * w);
                        let forward = Quat::from_rotation_x(-0.3 * w);
                        transform.rotation = rua.rest_rotation * forward * out;
                    }
                }
                if anim.phase == GesturePhase::Sustain {
                    if let Some(ref lla) = slot_bones.left_lower_arm {
                        if let Ok(mut transform) = transforms.get_mut(lla.entity) {
                            let osc = (t * 0.5).sin() * 0.05 * w;
                            let delta = Quat::from_rotation_x(osc);
                            transform.rotation = lla.rest_rotation * delta;
                        }
                    }
                    if let Some(ref rla) = slot_bones.right_lower_arm {
                        if let Ok(mut transform) = transforms.get_mut(rla.entity) {
                            let osc = (t * 0.5 + 0.5).sin() * 0.05 * w;
                            let delta = Quat::from_rotation_x(osc);
                            transform.rotation = rla.rest_rotation * delta;
                        }
                    }
                }
            }
            Gesture::None => {}
        }
    }

    for slot in finished {
        active_gestures.slots.remove(&slot);
    }
}

// =============================================================================
// Render Cadence
// =============================================================================

/// Staggered render cadence — controls which cameras render each frame.
pub(super) fn manage_render_cadence(
    mut schedule: ResMut<RenderSchedule>,
    registry: Res<SlotRegistry>,
    speech_clips: Res<ActiveSpeechClips>,
    mut cameras: Query<&mut Camera>,
) {
    schedule.frame_count = schedule.frame_count.wrapping_add(1);
    let frame = schedule.frame_count;
    let cadence = schedule.idle_cadence;

    for (slot, state) in &registry.slots {
        if !state.active || !state.model_loaded {
            continue;
        }

        let is_speaking = speech_clips.clips.contains_key(slot);
        let should_render = is_speaking || (frame % cadence == (*slot as u32 % cadence));

        if let Ok(mut camera) = cameras.get_mut(state.camera_entity) {
            camera.is_active = should_render;
        }
    }
}
