//! Animation systems — morph targets, blinking, breathing, speaking, gestures, eye gaze.
//!
//! Bevy systems that apply animation to scene objects. All amplitudes and
//! frequencies come from `AnimationConfig` components (see `scene/animation.rs`),
//! NOT from inline constants. This keeps the systems generic — they work
//! for any scene (webcam portrait, Sims-like world, cutscene, etc.).

use bevy::mesh::morph::MorphWeights;
use bevy::prelude::*;

use super::scene::animation::{AnimationConfig, PORTRAIT_PROFILE};
use super::skeleton::{camera_z_for_head, REFERENCE_HEAD_Y};
use super::types::*;
use super::vrm;
use crate::clog_info;

/// Cache speaking slot IDs once per frame — consumed by 5 animation systems.
pub(super) fn cache_speaking_slots(
    speaking_query: Query<&AvatarSlotId, With<Speaking>>,
    mut speaking_slots: ResMut<SpeakingSlots>,
) {
    speaking_slots.0.clear();
    for id in &speaking_query {
        speaking_slots.0.insert(id.0);
    }
}

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
    for (slot, slot_data) in &registry.slots {
        if morph_targets.layouts.contains_key(slot) || !slot_data.is_active() {
            continue;
        }
        let avatar_entity = match slot_data.primary_avatar().and_then(|a| a.entity) {
            Some(e) => e,
            None => continue,
        };

        let morph_entity = match find_morph_entity(avatar_entity, &children_query, &morph_query) {
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

        let mut layout = MorphTargetLayout {
            mesh_entity: morph_entity,
            mouth_open_index: None,
            blink_index: None,
            blink_left_index: None,
            blink_right_index: None,
            happy_index: None,
            sad_index: None,
            angry_index: None,
            surprised_index: None,
            relaxed_index: None,
            look_up: None,
            look_down: None,
            look_left: None,
            look_right: None,
        };

        if !mesh_names.is_empty() {
            discover_from_mesh_names(&mesh_names, &mut layout);
        } else if let Some(model_path) = slot_data.primary_avatar().and_then(|a| a.state.model_path.as_deref()) {
            discover_from_vrm_extension(model_path, *slot, &mut layout);
        }

        let weight_count = morph_query
            .get(morph_entity)
            .ok()
            .map(|(_, w)| w.weights().len())
            .unwrap_or(0);

        let emotion_count = [
            layout.happy_index, layout.sad_index, layout.angry_index,
            layout.surprised_index, layout.relaxed_index,
        ].iter().filter(|i| i.is_some()).count();
        let gaze_count = [
            layout.look_up, layout.look_down, layout.look_left, layout.look_right,
        ].iter().filter(|i| i.is_some()).count();
        clog_info!(
            "🎨 Morph discovery slot {}: {} weights, {} names, mouth={:?}, blink={:?}, blink_l={:?}, blink_r={:?}, emotions={}/5, gaze={}/4",
            slot, weight_count, mesh_names.len(), layout.mouth_open_index, layout.blink_index,
            layout.blink_left_index, layout.blink_right_index, emotion_count, gaze_count,
        );

        morph_targets.layouts.insert(*slot, layout);
    }
}

/// Discover morph target indices from standard glTF mesh target names.
fn discover_from_mesh_names(mesh_names: &[String], layout: &mut MorphTargetLayout) {
    for (i, name) in mesh_names.iter().enumerate() {
        let lower = name.to_lowercase();

        // Helper: set index if not already discovered
        macro_rules! set_first {
            ($field:ident, $cond:expr) => {
                if layout.$field.is_none() && $cond {
                    layout.$field = Some(i);
                }
            };
        }

        set_first!(mouth_open_index,
            lower == "aa" || lower == "a"
            || lower.ends_with("_mth_a") || lower.ends_with("mth_a")
            || lower.ends_with("_v_aa") || lower == "v_aa"
            || lower.ends_with("mouth_open") || lower.ends_with("jawopen")
            || lower == "fcl_mth_a"
        );
        set_first!(blink_index,
            lower == "blink" || lower == "fcl_eye_close" || lower == "vrc.blink"
            || (lower.contains("eye_close")
                && !lower.contains("_l") && !lower.contains("_r")
                && !lower.contains("left") && !lower.contains("right"))
        );
        set_first!(blink_left_index,
            lower == "blinkleft" || lower == "blink_l" || lower == "fcl_eye_close_l"
            || lower.contains("eye_close_l") || lower.contains("eye_close_left")
        );
        set_first!(blink_right_index,
            lower == "blinkright" || lower == "blink_r" || lower == "fcl_eye_close_r"
            || lower.contains("eye_close_r") || lower.contains("eye_close_right")
        );
        set_first!(happy_index,
            lower == "happy" || lower == "joy"
            || lower.ends_with("_joy") || lower.ends_with("_happy")
            || lower == "fcl_all_joy" || lower == "fcl_eye_joy"
        );
        set_first!(sad_index,
            lower == "sad" || lower == "sorrow"
            || lower.ends_with("_sad") || lower.ends_with("_sorrow")
            || lower == "fcl_all_sorrow" || lower == "fcl_eye_sorrow"
        );
        set_first!(angry_index,
            lower == "angry" || lower.ends_with("_angry")
            || lower == "fcl_all_angry" || lower == "fcl_mth_angry"
        );
        set_first!(surprised_index,
            lower == "surprised" || lower == "fun"
            || lower.ends_with("_surprised") || lower.ends_with("_fun")
            || lower == "fcl_all_fun" || lower == "fcl_brw_surprised"
        );
        set_first!(relaxed_index,
            lower == "relaxed" || lower.ends_with("_relaxed") || lower == "fcl_all_relaxed"
        );
        set_first!(look_up,
            lower == "lookup" || lower == "look_up"
            || lower.ends_with("lookup") || lower == "fcl_eye_lookup"
        );
        set_first!(look_down,
            lower == "lookdown" || lower == "look_down"
            || lower.ends_with("lookdown") || lower == "fcl_eye_lookdown"
        );
        set_first!(look_left,
            lower == "lookleft" || lower == "look_left"
            || lower.ends_with("lookleft") || lower == "fcl_eye_lookleft"
        );
        set_first!(look_right,
            lower == "lookright" || lower == "look_right"
            || lower.ends_with("lookright") || lower == "fcl_eye_lookright"
        );
    }
}

/// Discover morph target indices from VRM extension blend shapes.
fn discover_from_vrm_extension(model_path: &str, slot: u8, layout: &mut MorphTargetLayout) {
    let vrm_shapes = match vrm::parse_vrm_blend_shapes(model_path) {
        Some(s) => s,
        None => return,
    };

    for shape in &vrm_shapes {
        let preset = shape.preset_name.to_lowercase();
        let first_index = shape.binds.first().map(|b| b.index);

        // Helper: set layout field from first bind index if preset matches
        macro_rules! map_preset {
            ($field:ident, $($name:literal)|+) => {
                if layout.$field.is_none() && matches!(preset.as_str(), $($name)|+) {
                    layout.$field = first_index;
                }
            };
        }

        map_preset!(mouth_open_index, "a" | "aa");
        map_preset!(happy_index, "joy" | "happy");
        map_preset!(sad_index, "sorrow" | "sad");
        map_preset!(angry_index, "angry");
        map_preset!(surprised_index, "fun" | "surprised");
        map_preset!(relaxed_index, "relaxed");
        map_preset!(blink_left_index, "blink_l" | "blinkleft");
        map_preset!(blink_right_index, "blink_r" | "blinkright");
        map_preset!(look_up, "lookup");
        map_preset!(look_down, "lookdown");
        map_preset!(look_left, "lookleft");
        map_preset!(look_right, "lookright");

        // "blink" preset with 2 binds → split into left/right
        if layout.blink_index.is_none() && preset == "blink" {
            layout.blink_index = first_index;
            if shape.binds.len() >= 2 {
                layout.blink_left_index = Some(shape.binds[0].index);
                layout.blink_right_index = Some(shape.binds[1].index);
            }
        }
    }
    clog_info!("🎨 VRM blend shapes slot {}: {} groups parsed", slot, vrm_shapes.len());
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

/// Set a morph weight by optional index, with bounds check.
#[inline(always)]
fn set_morph(w: &mut [f32], idx: Option<usize>, val: f32) {
    if let Some(i) = idx {
        if i < w.len() {
            w[i] = val;
        }
    }
}

// =============================================================================
// Camera / Idle
// =============================================================================

/// Idle animation — lock camera to head rest position (captured once).
///
/// On the first frame where the head bone's GlobalTransform is available,
/// we capture the head world Y and lock the camera there. Subsequent frames
/// skip the query entirely. This prevents breathing/sway animations from
/// bobbing the camera (and thus the room background).
pub(super) fn animate_idle(
    mut registry: ResMut<SlotRegistry>,
    bone_registry: Res<BoneRegistry>,
    global_transforms: Query<&GlobalTransform>,
    mut transforms: Query<&mut Transform, With<AvatarSlotId>>,
) {
    for (slot, slot_data) in &mut registry.slots {
        let cam_entity = match slot_data.camera_entity {
            Some(e) => e,
            None => continue,
        };

        // One-time capture: lock camera to head rest position.
        if slot_data.camera_head_y.is_none() {
            if let Some(slot_bones) = bone_registry.slots.get(slot) {
                if let Some(ref head) = slot_bones.head {
                    if let Ok(global) = global_transforms.get(head.entity) {
                        slot_data.camera_head_y = Some(global.translation().y);
                    }
                }
            }
        }

        if let Ok(mut transform) = transforms.get_mut(cam_entity) {
            let head_y = slot_data.camera_head_y.unwrap_or(REFERENCE_HEAD_Y);
            let eye_y = head_y + 0.06;
            let cam_z = camera_z_for_head(head_y);

            transform.translation.x = 0.0;
            transform.translation.y = eye_y + 0.02;
            transform.translation.z = cam_z;
            let look_target = Vec3::new(0.0, eye_y, 0.0);
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
    speaking_slots: Res<SpeakingSlots>,
    morph_targets: Res<SlotMorphTargets>,
    bone_registry: Res<BoneRegistry>,
    mut speech_clips: ResMut<ActiveSpeechClips>,
    legacy_mouth: Res<LegacyMouthWeights>,
    anim_configs: Query<(&AvatarSlotId, &AnimationConfig)>,
    mut morph_weights: Query<&mut MorphWeights>,
    mut transforms: Query<&mut Transform>,
    mut commands: Commands,
    registry: Res<SlotRegistry>,
) {
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
        if let Some(slot_data) = registry.slots.get(slot) {
            if let Some(avatar_entity) = slot_data.primary_avatar().and_then(|a| a.entity) {
                commands.entity(avatar_entity).remove::<Speaking>();
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
        let is_speaking = speaking_slots.0.contains(slot);

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

            set_morph(w, layout.mouth_open_index, mouth_weight);
        }

        // Head nod during speech — amplitudes from AnimationConfig.
        let should_nod = has_clip || is_speaking;
        let profile = anim_configs.iter()
            .find(|(id, _)| id.0 == *slot)
            .map(|(_, cfg)| &cfg.profile)
            .unwrap_or(&PORTRAIT_PROFILE);
        if let Some(slot_bones) = bone_registry.slots.get(slot) {
            if let Some(ref head) = slot_bones.head {
                if let Ok(mut transform) = transforms.get_mut(head.entity) {
                    if should_nod {
                        let t = now + *slot as f32 * 1.3;
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
    speaking_slots: Res<SpeakingSlots>,
    mut morph_weights: Query<&mut MorphWeights>,
) {
    let dt = time.delta_secs();

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

        let is_speaking = speaking_slots.0.contains(slot) || speech_clips.clips.contains_key(slot);
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
            set_morph(w, idx, effective_weight);
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

            set_morph(w, layout.blink_index, blink_weight);
            set_morph(w, layout.blink_left_index, blink_weight);
            set_morph(w, layout.blink_right_index, blink_weight);
        }
    }
}

// =============================================================================
// Idle Gestures (micro-movements)
// =============================================================================

/// Idle gesture system — subtle upper-body micro-movements.
#[allow(clippy::too_many_arguments)]
pub(super) fn animate_idle_gestures(
    time: Res<Time>,
    registry: Res<SlotRegistry>,
    bone_registry: Res<BoneRegistry>,
    speaking_slots: Res<SpeakingSlots>,
    active_gestures: Res<ActiveGestures>,
    anim_configs: Query<(&AvatarSlotId, &AnimationConfig)>,
    mut gesture_state: ResMut<IdleGestureState>,
    mut transforms: Query<&mut Transform>,
) {
    let dt = time.delta_secs();

    for (slot, slot_data) in &registry.slots {
        if !slot_data.is_active() {
            continue;
        }

        let is_speaking = speaking_slots.0.contains(slot);

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
        } else if !speaking_slots.0.is_empty() {
            let turn_bias: f32 = speaking_slots.0
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

        let (profile, freq_var) = anim_configs.iter()
            .find(|(id, _)| id.0 == *slot)
            .map(|(_, cfg)| (&cfg.profile, cfg.freq_variation))
            .unwrap_or((&PORTRAIT_PROFILE, 1.0));

        if let Some(ref neck) = slot_bones.neck {
            if let Ok(mut transform) = transforms.get_mut(neck.entity) {
                let tilt_x = (t * 0.15 * freq_var).sin() * profile.neck_tilt_x_amplitude
                    + (t * 0.23 * freq_var).cos() * (profile.neck_tilt_x_amplitude * 0.67)
                    + (t * 0.37 * freq_var).sin() * (profile.neck_tilt_x_amplitude * 0.33);
                let tilt_z = (t * 0.12 * freq_var).cos() * profile.neck_tilt_z_amplitude
                    + (t * 0.31 * freq_var).sin() * (profile.neck_tilt_z_amplitude * 0.67);
                let idle_turn = (t * 0.08 * freq_var).sin() * profile.neck_turn_amplitude;
                let turn_y = idle_turn + gesture.head_turn_current;

                let delta = Quat::from_euler(EulerRot::XYZ, tilt_x, turn_y, tilt_z);
                transform.rotation = neck.rest_rotation * delta;
            }
        }

        if let Some(ref left_shoulder) = slot_bones.left_shoulder {
            if let Ok(mut transform) = transforms.get_mut(left_shoulder.entity) {
                let shift = (t * 0.4).sin() * profile.shoulder_shift_amplitude
                    + (t * 0.17).cos() * (profile.shoulder_shift_amplitude * 0.5);
                transform.translation.y = left_shoulder.rest_translation.y + shift;
            }
        }
        if let Some(ref right_shoulder) = slot_bones.right_shoulder {
            if let Ok(mut transform) = transforms.get_mut(right_shoulder.entity) {
                let shift = (t * 0.4 + std::f32::consts::PI).sin() * profile.shoulder_shift_amplitude
                    + (t * 0.17 + 1.0).cos() * (profile.shoulder_shift_amplitude * 0.5);
                transform.translation.y = right_shoulder.rest_translation.y + shift;
            }
        }
    }
}

// =============================================================================
// Breathing
// =============================================================================

/// Subtle breathing animation — gentle spine/chest oscillation.
/// Reads amplitudes from AnimationConfig on the avatar entity.
pub(super) fn animate_breathing(
    time: Res<Time>,
    registry: Res<SlotRegistry>,
    bone_registry: Res<BoneRegistry>,
    anim_configs: Query<(&AvatarSlotId, &AnimationConfig)>,
    mut transforms: Query<&mut Transform>,
) {
    // Build slot → config lookup from entities with AnimationConfig.
    let mut slot_configs: [Option<&AnimationConfig>; 16] = [None; 16];
    for (id, cfg) in &anim_configs {
        if (id.0 as usize) < 16 {
            slot_configs[id.0 as usize] = Some(cfg);
        }
    }

    for (slot, slot_data) in &registry.slots {
        if !slot_data.is_active() {
            continue;
        }

        let spine = match bone_registry.slots.get(slot).and_then(|b| b.spine.as_ref()) {
            Some(s) => s,
            None => continue,
        };

        let (profile, freq_var) = match slot_configs.get(*slot as usize).and_then(|c| *c) {
            Some(cfg) => (&cfg.profile, cfg.freq_variation),
            None => (&PORTRAIT_PROFILE, 1.0),
        };

        if let Ok(mut transform) = transforms.get_mut(spine.entity) {
            let t = time.elapsed_secs() + *slot as f32 * 1.1;
            let breath = (t * profile.breathing_frequency * std::f32::consts::TAU).sin()
                * profile.breathing_scale_amplitude;
            transform.scale.y = 1.0 + breath;
            let sway = (t * profile.spine_sway_frequency * freq_var).sin()
                * profile.spine_sway_amplitude;
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
    speaking_slots: Res<SpeakingSlots>,
    mut morph_weights: Query<&mut MorphWeights>,
    mut transforms: Query<&mut Transform>,
) {
    let t = time.elapsed_secs();

    for (slot, slot_data) in &registry.slots {
        if !slot_data.is_active() {
            continue;
        }

        let is_speaking = speaking_slots.0.contains(slot);
        let phase = *slot as f32 * 2.73;

        let (gaze_x, gaze_y) = if is_speaking {
            let drift_x = (t * 0.3 + phase).sin() * 0.05;
            let drift_y = (t * 0.25 + phase).cos() * 0.03;
            (drift_x, drift_y)
        } else {
            let speaker_bias: f32 = speaking_slots.0
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
                        set_morph(w, layout.look_left, (-gaze_x).min(1.0));
                        set_morph(w, layout.look_right, 0.0);
                    } else {
                        set_morph(w, layout.look_right, gaze_x.min(1.0));
                        set_morph(w, layout.look_left, 0.0);
                    }

                    if gaze_y < 0.0 {
                        set_morph(w, layout.look_down, (-gaze_y).min(1.0));
                        set_morph(w, layout.look_up, 0.0);
                    } else {
                        set_morph(w, layout.look_up, gaze_y.min(1.0));
                        set_morph(w, layout.look_down, 0.0);
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
    speaking_slots: Res<SpeakingSlots>,
) {
    use crate::live::session::cognitive_animation::{select_weighted_gesture, CognitiveState};

    let dt = time.delta_secs();
    let elapsed = time.elapsed_secs();

    for (slot, cog) in cognitive_anim.slots.iter_mut() {
        cog.time_since_reroll += dt;

        if speaking_slots.0.contains(slot) {
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
    anim_configs: Query<(&AvatarSlotId, &AnimationConfig)>,
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

        let profile = anim_configs.iter()
            .find(|(id, _)| id.0 == *slot)
            .map(|(_, cfg)| &cfg.profile)
            .unwrap_or(&PORTRAIT_PROFILE);

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
                        let tilt = Quat::from_euler(
                            EulerRot::XYZ,
                            profile.gesture_think_head_tilt * w,
                            0.0,
                            profile.gesture_think_head_roll * w,
                        );
                        transform.rotation = head.rest_rotation * tilt;
                    }
                }
            }
            Gesture::Nod => {
                if let Some(ref head) = slot_bones.head {
                    if let Ok(mut transform) = transforms.get_mut(head.entity) {
                        let nod = (t * 1.5 * std::f32::consts::TAU).sin() * profile.gesture_nod_amplitude * w;
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
