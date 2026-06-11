//! Command processing — handles AvatarCommand messages from the main application.
//!
//! Commands insert/remove Components on entities. Animation systems find
//! entities via Query — no shared mutable state between command processing
//! and animation.

use bevy::camera::visibility::RenderLayers;
use bevy::camera::RenderTarget;
use bevy::prelude::*;
use bevy::scene::SceneInstanceReady;

use super::animation::{
    BreathingAnimation, CameraHeadLock, CognitiveGesture, Emotion, EmotionAnimation,
    GestureAnimation, GesturePhase, IdleMotion, ModelPath, MouthWeight, SlotId, Speaking,
    SpeechClip, EMOTION_DECAY_SECS,
};
use super::api::gpu_manager;
use super::scene::{
    build_scene, room_color_from_identity, scene_model_path, select_scene_for_identity,
    AnimationConfig, AvatarObject, LightRig, RoomConfig, SceneConfig, SceneObject,
};
use super::setup::spawn_readback_entity_opt;
use super::skeleton;
use super::types::*;
use super::{AVATAR_HEIGHT, AVATAR_WIDTH, HD_HEIGHT, HD_WIDTH};
use crate::gpu::make_entry;
use crate::gpu::memory_manager::{GpuPriority, GpuSubsystem};
use crate::{clog_info, clog_warn};

#[allow(clippy::too_many_arguments)]
pub(super) fn process_commands(
    command_channel: Res<CommandChannel>,
    time: Res<Time>,
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut registry: ResMut<SlotRegistry>,
    mut cameras: Query<&mut Camera>,
    mut pending: ResMut<PendingLoads>,
    mut speech_clips: ResMut<ActiveSpeechClips>,
    mut health: ResMut<SlotHealthStatus>,
    mut slot_dims: ResMut<SlotDimensions>,
    mut hd_pool: ResMut<HdRenderTargetPool>,
    mut gpu_guards: ResMut<GpuGuards>,
) {
    while let Ok(cmd) = command_channel.0.try_recv() {
        match cmd {
            AvatarCommand::Load {
                slot,
                model_path,
                display_name,
                identity,
            } => {
                health.identities.insert(slot, identity.clone());
                health.model_paths.insert(slot, model_path.clone());

                if let Some(slot_data) = registry.slots.get_mut(&slot) {
                    let layer = RenderLayers::layer((slot + 1) as usize);

                    slot_data.teardown(&mut commands);
                    gpu_guards.model_guards.remove(&slot);

                    let bg_color = room_color_from_identity(&identity);
                    let config = SceneConfig {
                        slot_id: slot,
                        render_target: slot_data.render_target.clone(),
                        background_color: bg_color,
                        layer: layer.clone(),
                        light_rig: LightRig::Portrait,
                        camera_transform: None,
                    };
                    let (scene_root, camera_entity) = build_scene(&mut commands, &config);
                    // Camera gets SlotId + CameraHeadLock
                    commands
                        .entity(camera_entity)
                        .insert((SlotId(slot), CameraHeadLock { head_y: None }));

                    let scene_entry = select_scene_for_identity(&identity);
                    let asset_path = scene_model_path(scene_entry.filename)
                        .to_string_lossy()
                        .to_string();
                    commands.entity(scene_root).insert(RoomConfig {
                        asset_path,
                        layer: layer.clone(),
                        scene_id: scene_entry.id.to_string(),
                    });

                    slot_data.scene_root = Some(scene_root);
                    slot_data.camera_entity = Some(camera_entity);

                    // Bevy's glTF loader requires .glb/.gltf extension.
                    // VRM files are glTF-compatible — create a .glb symlink if needed.
                    let load_path = if model_path.ends_with(".vrm") {
                        let glb_path = model_path.replacen(".vrm", ".glb", 1);
                        if !std::path::Path::new(&glb_path).exists()
                            && std::path::Path::new(&model_path).exists()
                        {
                            // Symlink target must be relative to the link's directory (just the filename)
                            let vrm_filename = std::path::Path::new(&model_path)
                                .file_name()
                                .unwrap_or_default();
                            #[cfg(unix)]
                            {
                                let _ = std::os::unix::fs::symlink(vrm_filename, &glb_path);
                            }
                            #[cfg(not(unix))]
                            {
                                let _ = std::fs::copy(&model_path, &glb_path);
                            }
                        }
                        glb_path
                    } else {
                        model_path.clone()
                    };

                    let mut avatar = AvatarObject::new(
                        model_path.clone(),
                        display_name.clone(),
                        identity.clone(),
                    );

                    let asset_path = format!("{}#Scene0", load_path);
                    let scene_handle: Handle<Scene> = asset_server.load(&asset_path);
                    let gltf_handle: Handle<bevy::gltf::Gltf> = asset_server.load(&load_path);
                    clog_info!(
                        "🎨 Slot {}: loading '{}' from {}",
                        slot,
                        display_name,
                        load_path
                    );
                    pending.scene_handles.push(PendingLoadEntry {
                        slot,
                        handle: scene_handle.clone(),
                        path: asset_path,
                        logged_final: false,
                    });
                    pending.gltf_handles.push(PendingLoadEntry {
                        slot,
                        handle: gltf_handle.clone(),
                        path: load_path.clone(),
                        logged_final: false,
                    });
                    avatar.state.gltf_handle = Some(gltf_handle);

                    // Spawn avatar entity with animation Components
                    let avatar_entity = commands
                        .spawn((
                            SceneRoot(scene_handle),
                            Transform::default(),
                            layer.clone(),
                            SlotId(slot),
                            AnimationConfig::portrait(slot),
                            ModelPath(load_path.clone()),
                            // Animation components — entity IS the animated object
                            BreathingAnimation::new(slot),
                            IdleMotion::new(slot),
                        ))
                        .id();
                    commands.entity(scene_root).add_child(avatar_entity);

                    let layer_for_observer = layer;
                    let slot_for_observer = slot;
                    let model_path_for_observer = load_path.clone();
                    let identity_for_observer = identity.clone();
                    commands.entity(avatar_entity).observe(
                        move |event: On<SceneInstanceReady>,
                              children_query: Query<&Children>,
                              names: Query<&Name>,
                              mut transforms: Query<&mut Transform>,
                              mut cmds: Commands,
                              mut slot_registry: ResMut<SlotRegistry>,
                              mut gpu_guards: ResMut<GpuGuards>,
                              mut snapshots: ResMut<SnapshotTracker>| {
                            let root = event.entity;
                            let child_count = skeleton::count_descendants(root, &children_query);
                            skeleton::propagate_render_layers(
                                root,
                                &layer_for_observer,
                                &children_query,
                                &mut cmds,
                            );
                            skeleton::dump_bone_names(root, &children_query, &names);
                            skeleton::fix_tpose_arms(
                                root,
                                &children_query,
                                &names,
                                &mut transforms,
                            );

                            // Discover bones and insert Skeleton Component directly on the entity
                            let bones = skeleton::discover_bones(
                                root,
                                slot_for_observer,
                                &model_path_for_observer,
                                &children_query,
                                &names,
                                &transforms,
                            );
                            cmds.entity(root).insert(bones);

                            if let Some(slot_data) = slot_registry.slots.get_mut(&slot_for_observer)
                            {
                                if let Some(avatar) = slot_data.avatar_mut(&identity_for_observer) {
                                    avatar.state.model_loaded = true;
                                }
                            }

                            snapshots.mark_loaded(slot_for_observer);

                            let model_bytes = std::fs::metadata(&model_path_for_observer)
                                .map(|m| m.len())
                                .unwrap_or(0);
                            if model_bytes > 0 {
                                if let Some(mgr) = gpu_manager() {
                                    match mgr.allocate(
                                        GpuSubsystem::Rendering,
                                        model_bytes,
                                        GpuPriority::Interactive,
                                    ) {
                                        Ok(guard) => {
                                            mgr.eviction_registry.register(make_entry(
                                                &format!("render:model:slot{}", slot_for_observer),
                                                &format!(
                                                    "Avatar Model (slot {})",
                                                    slot_for_observer
                                                ),
                                                GpuPriority::Interactive,
                                                model_bytes,
                                            ));
                                            gpu_guards
                                                .model_guards
                                                .insert(slot_for_observer, guard);
                                        }
                                        Err(e) => {
                                            clog_warn!(
                                                "🎨 GPU: model allocation for slot {} failed ({})",
                                                slot_for_observer,
                                                e
                                            );
                                        }
                                    }
                                }
                            }

                            clog_info!(
                                "🎨 SceneInstanceReady: slot {}, entity {:?}, {} descendants",
                                slot_for_observer,
                                root,
                                child_count
                            );
                        },
                    );

                    avatar.entity = Some(avatar_entity);
                    slot_data.add_object(identity, SceneObject::Avatar(avatar));

                    if let Ok(mut camera) = cameras.get_mut(camera_entity) {
                        camera.is_active = true;
                    }

                    clog_info!(
                        "🎨 Slot {}: loaded '{}' from {}",
                        slot,
                        display_name,
                        load_path
                    );
                }
            }
            AvatarCommand::Unload { slot } => {
                if let Some(slot_data) = registry.slots.get_mut(&slot) {
                    if let Some(cam) = slot_data.camera_entity {
                        if let Ok(mut camera) = cameras.get_mut(cam) {
                            camera.is_active = false;
                        }
                    }
                    if let Some(hd_target) = hd_pool.assigned.remove(&slot) {
                        hd_pool.available.push(hd_target);
                        slot_data.render_target = slot_data.default_render_target.clone();
                        slot_dims.dims.insert(slot, (AVATAR_WIDTH, AVATAR_HEIGHT));
                    }
                    slot_data.teardown(&mut commands);
                    gpu_guards.model_guards.remove(&slot);
                    clog_info!("🎨 Slot {}: unloaded", slot);
                }
            }
            AvatarCommand::SetSpeaking { slot, speaking } => {
                if let Some(slot_data) = registry.slots.get(&slot) {
                    if let Some(avatar_entity) = slot_data.primary_avatar().and_then(|a| a.entity) {
                        if speaking {
                            commands.entity(avatar_entity).insert(Speaking);
                        } else {
                            commands.entity(avatar_entity).remove::<Speaking>();
                            commands.entity(avatar_entity).remove::<SpeechClip>();
                        }
                    }
                }
            }
            AvatarCommand::SetMouthWeight { slot, weight } => {
                if let Some(slot_data) = registry.slots.get(&slot) {
                    if let Some(avatar_entity) = slot_data.primary_avatar().and_then(|a| a.entity) {
                        commands.entity(avatar_entity).insert(MouthWeight(weight));
                    }
                }
            }
            AvatarCommand::SetMouthWeightSequence {
                slot,
                weights,
                interval_ms,
            } => {
                if let Some(slot_data) = registry.slots.get(&slot) {
                    if let Some(avatar_entity) = slot_data.primary_avatar().and_then(|a| a.entity) {
                        let duration_ms = weights.len() as u64 * interval_ms as u64;
                        commands.entity(avatar_entity).insert(SpeechClip {
                            mouth_weights: weights,
                            interval_ms,
                            duration_ms,
                            start_time: time.elapsed_secs(),
                        });
                        commands.entity(avatar_entity).insert(Speaking);
                        speech_clips.clips_started += 1;
                    }
                }
            }
            AvatarCommand::PlaySpeech { slot, clip } => {
                if let Some(slot_data) = registry.slots.get(&slot) {
                    if let Some(avatar_entity) = slot_data.primary_avatar().and_then(|a| a.entity) {
                        commands.entity(avatar_entity).insert(SpeechClip {
                            mouth_weights: clip.mouth_weights,
                            interval_ms: clip.interval_ms,
                            duration_ms: clip.duration_ms,
                            start_time: time.elapsed_secs(),
                        });
                        commands.entity(avatar_entity).insert(Speaking);
                        speech_clips.clips_started += 1;
                    }
                }
            }
            AvatarCommand::StopSpeech { slot } => {
                if let Some(slot_data) = registry.slots.get(&slot) {
                    if let Some(avatar_entity) = slot_data.primary_avatar().and_then(|a| a.entity) {
                        commands.entity(avatar_entity).remove::<SpeechClip>();
                        commands.entity(avatar_entity).remove::<Speaking>();
                        speech_clips.clips_interrupted += 1;
                    }
                }
            }
            AvatarCommand::SetEmotion {
                slot,
                emotion,
                weight,
                transition_ms,
            } => {
                if let Some(slot_data) = registry.slots.get(&slot) {
                    if let Some(avatar_entity) = slot_data.primary_avatar().and_then(|a| a.entity) {
                        let rate = if transition_ms > 0 {
                            1.0 / (transition_ms as f32 / 1000.0)
                        } else {
                            100.0
                        };
                        // Map public Emotion to component Emotion
                        let target = match emotion {
                            super::Emotion::Neutral => Emotion::Neutral,
                            super::Emotion::Happy => Emotion::Happy,
                            super::Emotion::Sad => Emotion::Sad,
                            super::Emotion::Angry => Emotion::Angry,
                            super::Emotion::Surprised => Emotion::Surprised,
                            super::Emotion::Relaxed => Emotion::Relaxed,
                        };
                        // Insert or update EmotionAnimation component
                        commands.entity(avatar_entity).insert(EmotionAnimation {
                            current: Emotion::Neutral,
                            current_weight: 0.0,
                            target,
                            target_weight: weight.clamp(0.0, 1.0),
                            transition_rate: rate,
                            decay_timer: EMOTION_DECAY_SECS,
                        });
                    }
                }
            }
            AvatarCommand::SetGesture {
                slot,
                gesture,
                duration_ms,
            } => {
                if let Some(slot_data) = registry.slots.get(&slot) {
                    if let Some(avatar_entity) = slot_data.primary_avatar().and_then(|a| a.entity) {
                        let g = match gesture {
                            super::Gesture::None => {
                                commands.entity(avatar_entity).remove::<GestureAnimation>();
                                continue;
                            }
                            super::Gesture::Wave => super::animation::Gesture::Wave,
                            super::Gesture::Think => super::animation::Gesture::Think,
                            super::Gesture::Nod => super::animation::Gesture::Nod,
                            super::Gesture::Shrug => super::animation::Gesture::Shrug,
                            super::Gesture::Point => super::animation::Gesture::Point,
                            super::Gesture::OpenHands => super::animation::Gesture::OpenHands,
                        };
                        commands.entity(avatar_entity).insert(GestureAnimation {
                            gesture: g,
                            phase: GesturePhase::Attack,
                            duration_secs: duration_ms as f32 / 1000.0,
                            elapsed: 0.0,
                            weight: 0.0,
                        });
                    }
                }
            }
            AvatarCommand::SetCognitiveState { slot, state } => {
                use crate::live::session::cognitive_animation::CognitiveState;
                if let Some(slot_data) = registry.slots.get(&slot) {
                    if let Some(avatar_entity) = slot_data.primary_avatar().and_then(|a| a.entity) {
                        match state {
                            CognitiveState::Idle => {
                                commands.entity(avatar_entity).remove::<CognitiveGesture>();
                            }
                            _ => {
                                commands.entity(avatar_entity).insert(CognitiveGesture {
                                    state,
                                    config: crate::live::session::cognitive_animation::CognitiveAnimationConfig::default(),
                                    time_since_reroll: 999.0,
                                });
                            }
                        }
                    }
                }
            }
            AvatarCommand::Resize {
                slot,
                width,
                height,
            } => {
                if let Some(slot_data) = registry.slots.get_mut(&slot) {
                    let is_hd_request = width >= HD_WIDTH && height >= HD_HEIGHT;
                    let currently_hd = hd_pool.assigned.contains_key(&slot);

                    let new_rt_handle = if is_hd_request && !currently_hd {
                        if let Some(hd_target) = hd_pool.available.pop() {
                            hd_pool.assigned.insert(slot, hd_target.clone());
                            clog_info!("🎨 Slot {}: promoted to HD", slot);
                            hd_target
                        } else {
                            clog_warn!("🎨 Slot {}: HD pool exhausted", slot);
                            continue;
                        }
                    } else if !is_hd_request && currently_hd {
                        if let Some(hd_target) = hd_pool.assigned.remove(&slot) {
                            hd_pool.available.push(hd_target);
                        }
                        slot_data.default_render_target.clone()
                    } else if is_hd_request && currently_hd {
                        continue;
                    } else {
                        slot_data.default_render_target.clone()
                    };

                    if let Some(cam) = slot_data.camera_entity {
                        commands
                            .entity(cam)
                            .insert(RenderTarget::Image(new_rt_handle.clone().into()));
                    }

                    commands.entity(slot_data.readback_entity).despawn();
                    let has_bridge = crate::live::avatar::publishers::gpu_bridge::has_bridge(slot);
                    let new_readback = spawn_readback_entity_opt(
                        &mut commands,
                        new_rt_handle.clone(),
                        slot,
                        !has_bridge,
                    );

                    slot_data.readback_entity = new_readback;
                    slot_data.render_target = new_rt_handle;

                    let (ew, eh) = if is_hd_request {
                        (HD_WIDTH, HD_HEIGHT)
                    } else {
                        (AVATAR_WIDTH, AVATAR_HEIGHT)
                    };
                    slot_dims.dims.insert(slot, (ew, eh));
                    clog_info!("🎨 Slot {}: resized to {}x{}", slot, ew, eh);
                }
            }
            AvatarCommand::UnloadIdle => {
                let idle_slots: Vec<u8> = registry
                    .slots
                    .iter()
                    .filter(|(_, s)| s.is_active() && !s.is_speaking())
                    .map(|(k, _)| *k)
                    .collect();
                for slot in &idle_slots {
                    if let Some(slot_data) = registry.slots.get_mut(slot) {
                        if let Some(cam) = slot_data.camera_entity {
                            if let Ok(mut camera) = cameras.get_mut(cam) {
                                camera.is_active = false;
                            }
                        }
                        slot_data.teardown(&mut commands);
                        gpu_guards.model_guards.remove(slot);
                    }
                }
                if !idle_slots.is_empty() {
                    clog_info!("🎨 UnloadIdle: freed {} idle model slots", idle_slots.len());
                }
            }
            AvatarCommand::Shutdown => {
                clog_info!("🎨 Bevy renderer shutting down");
                let all_slots: Vec<u8> = registry.slots.keys().copied().collect();
                for slot in &all_slots {
                    if let Some(slot_data) = registry.slots.get_mut(slot) {
                        slot_data.teardown(&mut commands);
                        gpu_guards.model_guards.remove(slot);
                    }
                }
                commands.write_message(AppExit::from_code(0));
                return;
            }
        }
    }
}
