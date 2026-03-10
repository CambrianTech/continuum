//! Bevy Headless Avatar Renderer
//!
//! A single Bevy app instance renders all AI persona avatars simultaneously.
//! Each avatar gets its own RenderLayer + Camera + render target texture.
//! GPU readback delivers RGBA frames via crossbeam channels to LiveKit video loops.
//!
//! Architecture:
//!   BevyAvatarSystem (singleton)
//!     └── Bevy App (dedicated OS thread)
//!           ├── Avatar slot 0: Camera → RenderTarget → Readback → channel
//!           ├── Avatar slot 1: Camera → RenderTarget → Readback → channel
//!           ├── ...
//!           └── Avatar slot 15: Camera → RenderTarget → Readback → channel
//!
//! Performance: 16 avatars × 640×360 @ ~7fps effective readback (15fps Bevy tick).
//! On Apple Silicon (shared memory + GPU bridge), readback is zero-copy IOSurface.
//!
//! ## Module Structure
//!
//! - `types` — All components, resources, enums, structs
//! - `animation` — Morph targets, blinking, breathing, speaking, gestures, eye gaze
//! - `vrm` — VRM extension parsing (blend shapes, humanoid bones, lookAt)
//! - `skeleton` — Bone discovery, T-pose fix, scene tree helpers

mod animation;
mod skeleton;
pub(crate) mod types;
mod vrm;

use crate::gpu::make_entry;
use crate::gpu::memory_manager::{GpuMemoryManager, GpuPriority, GpuSubsystem};
use crate::{clog_info, clog_warn};
use bevy::app::ScheduleRunnerPlugin;
use bevy::asset::LoadState;
use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::{RenderLayers, SetViewVisibility};
use bevy::camera::RenderTarget;
use bevy::prelude::*;
use bevy::render::gpu_readback::{Readback, ReadbackComplete};
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use bevy::scene::SceneInstanceReady;
use crossbeam_channel::{Receiver, Sender};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use crate::live::avatar::RgbaFrame;

// Re-export public types
pub use types::{
    AvatarCommand, BevyMemoryStats, Emotion, Gesture, SpeechAnimationClip,
};
// Re-export for metal_gpu_convert (crate-internal)
pub(crate) use types::{FrameNotifiers, SlotDimensions, SlotRegistry};

use types::*;

// GpuConvertPlugin DISABLED: Metal compute shader runs on a separate command queue
// from Bevy/wgpu, with no GPU synchronization guarantee that the render pass has
// completed before the compute shader reads the texture. This causes alternating
// correct/stale frames (the strobe). The ReadbackComplete → try_write_bridge path
// is correct and should be used until proper GPU sync is implemented.

/// Maximum number of concurrent avatar render slots.
pub const MAX_AVATAR_SLOTS: u8 = 16;

/// Default render resolution per avatar.
pub const AVATAR_WIDTH: u32 = 640;
pub const AVATAR_HEIGHT: u32 = 360;

/// Target framerate for avatar rendering.
const AVATAR_FPS: f64 = 15.0;

// ============================================================================
// Public API — BevyAvatarSystem singleton
// ============================================================================

/// GPU memory manager for render VRAM tracking.
static RENDERER_GPU_MANAGER: OnceLock<Arc<GpuMemoryManager>> = OnceLock::new();

/// Provide the GPU memory manager to the renderer subsystem.
pub fn set_gpu_manager(mgr: Arc<GpuMemoryManager>) {
    let _ = RENDERER_GPU_MANAGER.set(mgr);
}

fn gpu_manager() -> Option<&'static Arc<GpuMemoryManager>> {
    RENDERER_GPU_MANAGER.get()
}

/// Global singleton for the Bevy avatar rendering system.
static BEVY_SYSTEM: OnceLock<BevyAvatarSystem> = OnceLock::new();

/// Get or initialize the global BevyAvatarSystem.
pub fn get_or_init() -> &'static BevyAvatarSystem {
    BEVY_SYSTEM.get_or_init(|| {
        clog_info!("🎨 Starting Bevy headless avatar renderer ({MAX_AVATAR_SLOTS} slots, {AVATAR_WIDTH}x{AVATAR_HEIGHT} @{AVATAR_FPS}fps)");
        BevyAvatarSystem::start()
    })
}

/// Get the BevyAvatarSystem if it has already been initialized.
pub fn try_get() -> Option<&'static BevyAvatarSystem> {
    BEVY_SYSTEM.get()
}

/// The singleton Bevy avatar rendering system.
pub struct BevyAvatarSystem {
    command_tx: Sender<AvatarCommand>,
    frame_receivers: Vec<Receiver<RgbaFrame>>,
    frame_notifiers: Vec<Arc<tokio::sync::Notify>>,
    ready: Arc<std::sync::atomic::AtomicBool>,
    identity_to_slot: std::sync::Mutex<HashMap<String, u8>>,
    pub memory_stats: Arc<BevyMemoryStats>,
}

impl BevyAvatarSystem {
    fn start() -> Self {
        let (command_tx, command_rx) = crossbeam_channel::bounded(512);
        let ready = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let ready_clone = ready.clone();

        let mut frame_senders = Vec::with_capacity(MAX_AVATAR_SLOTS as usize);
        let mut frame_receivers = Vec::with_capacity(MAX_AVATAR_SLOTS as usize);
        let mut frame_notifiers = Vec::with_capacity(MAX_AVATAR_SLOTS as usize);
        for _ in 0..MAX_AVATAR_SLOTS {
            let (tx, rx) = crossbeam_channel::bounded(4);
            frame_senders.push(tx);
            frame_receivers.push(rx);
            frame_notifiers.push(Arc::new(tokio::sync::Notify::new()));
        }

        let notifiers_for_bevy: Vec<Arc<tokio::sync::Notify>> = frame_notifiers.to_vec();

        let memory_stats = Arc::new(BevyMemoryStats::new());
        let stats_for_bevy = memory_stats.clone();

        std::thread::Builder::new()
            .name("bevy-avatar-renderer".into())
            .spawn(move || {
                run_bevy_app(command_rx, frame_senders, notifiers_for_bevy, ready_clone, stats_for_bevy);
            })
            .expect("Failed to spawn Bevy avatar renderer thread");

        for _ in 0..50 {
            if ready.load(std::sync::atomic::Ordering::Acquire) {
                clog_info!("🎨 Bevy renderer confirmed ready");
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }

        if !ready.load(std::sync::atomic::Ordering::Acquire) {
            clog_warn!(
                "🎨 Bevy renderer did not report ready within 5s — may have failed to init GPU"
            );
        }

        Self {
            command_tx,
            frame_receivers,
            frame_notifiers,
            ready,
            identity_to_slot: std::sync::Mutex::new(HashMap::new()),
            memory_stats,
        }
    }

    pub fn is_ready(&self) -> bool {
        self.ready.load(std::sync::atomic::Ordering::Acquire)
    }

    pub fn frame_receiver(&self, slot: u8) -> Option<&Receiver<RgbaFrame>> {
        self.frame_receivers.get(slot as usize)
    }

    pub fn frame_notifier(&self, slot: u8) -> Option<Arc<tokio::sync::Notify>> {
        self.frame_notifiers.get(slot as usize).cloned()
    }

    pub fn load_model(&self, slot: u8, model_path: &str, display_name: &str, identity: &str) {
        if slot >= MAX_AVATAR_SLOTS {
            clog_warn!("Avatar slot {slot} exceeds max {MAX_AVATAR_SLOTS}");
            return;
        }
        let _ = self.command_tx.send(AvatarCommand::Load {
            slot,
            model_path: model_path.to_string(),
            display_name: display_name.to_string(),
            identity: identity.to_string(),
        });
    }

    pub fn unload_model(&self, slot: u8) {
        let _ = self.command_tx.send(AvatarCommand::Unload { slot });
    }

    pub fn set_speaking(&self, slot: u8, speaking: bool) {
        let _ = self
            .command_tx
            .send(AvatarCommand::SetSpeaking { slot, speaking });
    }

    pub fn register_identity(&self, identity: &str, slot: u8) {
        self.identity_to_slot
            .lock()
            .unwrap()
            .insert(identity.to_string(), slot);
    }

    pub fn unregister_identity(&self, identity: &str) {
        self.identity_to_slot.lock().unwrap().remove(identity);
    }

    pub fn identity_to_slot_map(&self) -> HashMap<String, u8> {
        self.identity_to_slot.lock().unwrap().clone()
    }

    /// Resolve identity to slot and execute a command. Returns false if identity not registered.
    fn send_by_identity(&self, identity: &str, make_cmd: impl FnOnce(u8) -> AvatarCommand) -> bool {
        if let Some(&slot) = self.identity_to_slot.lock().unwrap().get(identity) {
            let _ = self.command_tx.send(make_cmd(slot));
            true
        } else {
            false
        }
    }

    pub fn set_speaking_by_identity(&self, identity: &str, speaking: bool) -> bool {
        let found = self.send_by_identity(identity, |slot| {
            AvatarCommand::SetSpeaking { slot, speaking }
        });
        if !found && speaking {
            clog_warn!(
                "🎨 set_speaking: identity '{}' not registered (no slot)",
                &identity[..8.min(identity.len())]
            );
        }
        found
    }

    pub fn set_mouth_weight(&self, slot: u8, weight: f32) {
        let _ = self
            .command_tx
            .send(AvatarCommand::SetMouthWeight { slot, weight });
    }

    pub fn set_mouth_weight_by_identity(&self, identity: &str, weight: f32) -> bool {
        self.send_by_identity(identity, |slot| {
            AvatarCommand::SetMouthWeight { slot, weight }
        })
    }

    pub fn set_mouth_weight_sequence_by_identity(
        &self,
        identity: &str,
        weights: Vec<f32>,
        interval_ms: u32,
    ) -> bool {
        self.send_by_identity(identity, |slot| {
            AvatarCommand::SetMouthWeightSequence { slot, weights, interval_ms }
        })
    }

    pub fn play_speech_by_identity(&self, identity: &str, clip: SpeechAnimationClip) -> bool {
        self.send_by_identity(identity, |slot| {
            AvatarCommand::PlaySpeech { slot, clip }
        })
    }

    pub fn stop_speech_by_identity(&self, identity: &str) -> bool {
        self.send_by_identity(identity, |slot| AvatarCommand::StopSpeech { slot })
    }

    pub fn resize_slot(&self, slot: u8, width: u32, height: u32) {
        let _ = self.command_tx.send(AvatarCommand::Resize { slot, width, height });
    }

    pub fn set_emotion_by_identity(
        &self,
        identity: &str,
        emotion: Emotion,
        weight: f32,
        transition_ms: u32,
    ) -> bool {
        self.send_by_identity(identity, |slot| {
            AvatarCommand::SetEmotion { slot, emotion, weight, transition_ms }
        })
    }

    pub fn set_gesture_by_identity(
        &self,
        identity: &str,
        gesture: Gesture,
        duration_ms: u32,
    ) -> bool {
        self.send_by_identity(identity, |slot| {
            AvatarCommand::SetGesture { slot, gesture, duration_ms }
        })
    }

    pub fn set_cognitive_state_by_identity(
        &self,
        identity: &str,
        state: crate::live::session::cognitive_animation::CognitiveState,
    ) -> bool {
        self.send_by_identity(identity, |slot| {
            AvatarCommand::SetCognitiveState { slot, state }
        })
    }

    pub fn resize_by_identity(&self, identity: &str, width: u32, height: u32) -> bool {
        self.send_by_identity(identity, |slot| {
            AvatarCommand::Resize { slot, width, height }
        })
    }
}

// ============================================================================
// Bevy App (runs on dedicated OS thread)
// ============================================================================

fn run_bevy_app(
    command_rx: Receiver<AvatarCommand>,
    frame_senders: Vec<Sender<RgbaFrame>>,
    frame_notifiers: Vec<Arc<tokio::sync::Notify>>,
    ready_flag: Arc<std::sync::atomic::AtomicBool>,
    memory_stats: Arc<BevyMemoryStats>,
) {
    let asset_base = std::env::current_dir()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    App::new()
        .insert_resource(CommandChannel(command_rx))
        .insert_resource(FrameChannels(frame_senders))
        .insert_resource(FrameNotifiers(frame_notifiers))
        .insert_resource(ReadyFlag(ready_flag))
        .insert_resource(SlotRegistry {
            slots: HashMap::new(),
        })
        .insert_resource(PendingLoads::default())
        .insert_resource(SlotMorphTargets::default())
        .insert_resource(BlinkState::default())
        .insert_resource(BoneRegistry::default())
        .insert_resource(IdleGestureState::default())
        .insert_resource(ActiveSpeechClips::default())
        .insert_resource(LegacyMouthWeights::default())
        .insert_resource(EmotionState::default())
        .insert_resource(ActiveGestures::default())
        .insert_resource(CognitiveAnimState::default())
        .insert_resource(SlotDimensions::default())
        .insert_resource(SlotHealthStatus::default())
        .insert_resource(RenderSchedule::default())
        .insert_resource(GpuGuards::default())
        .insert_resource(SharedMemoryStats(memory_stats))
        .insert_resource(SpeakingSlots::default())
        .add_plugins(
            DefaultPlugins
                .set(bevy::window::WindowPlugin {
                    primary_window: None,
                    exit_condition: bevy::window::ExitCondition::DontExit,
                    ..default()
                })
                .set(ImagePlugin::default_nearest())
                .set(bevy::asset::AssetPlugin {
                    file_path: asset_base,
                    ..default()
                }),
        )
        .add_plugins(ScheduleRunnerPlugin::run_loop(Duration::from_secs_f64(
            1.0 / AVATAR_FPS,
        )))
        .register_type::<bevy::transform::components::TransformTreeChanged>()
        .add_systems(Startup, (setup_render_slots, signal_ready).chain())
        .add_systems(
            Update,
            (
                process_commands,
                monitor_load_states,
                touch_ambient_light,
                update_memory_stats,
            ),
        )
        .add_systems(
            Update,
            animation::cache_speaking_slots.run_if(has_active_slots),
        )
        .add_systems(
            Update,
            (
                animation::manage_render_cadence,
                ensure_continuous_readback,
                animation::discover_morph_targets,
                animation::animate_idle,
                animation::animate_speaking,
                animation::animate_expression,
                animation::animate_blinking,
                animation::animate_breathing,
                animation::animate_idle_gestures,
                animation::animate_eye_gaze,
                animation::drive_cognitive_gestures,
                animation::animate_body_gestures,
            )
                .run_if(has_active_slots)
                .after(animation::cache_speaking_slots),
        )
        .add_systems(
            PostUpdate,
            force_light_visibility
                .after(bevy::camera::visibility::VisibilitySystems::CheckVisibility),
        )
        .run();
}

// ============================================================================
// Readback
// ============================================================================

fn spawn_readback_entity(commands: &mut Commands, rt_handle: Handle<Image>, slot_id: u8) -> Entity {
    spawn_readback_entity_opt(commands, rt_handle, slot_id, true)
}

fn spawn_readback_entity_opt(
    commands: &mut Commands,
    rt_handle: Handle<Image>,
    slot_id: u8,
    start_active: bool,
) -> Entity {
    let mut entity_cmds = if start_active {
        commands.spawn((
            Readback::texture(rt_handle),
            AvatarSlotId(slot_id),
            ReadbackMarker,
        ))
    } else {
        commands.spawn((AvatarSlotId(slot_id), ReadbackMarker))
    };
    entity_cmds
        .observe(
            move |event: On<ReadbackComplete>,
                  channels: Res<FrameChannels>,
                  notifiers: Res<FrameNotifiers>,
                  health: Res<SlotHealthStatus>,
                  slot_dims: Res<SlotDimensions>| {
                let pixel_bytes: &[u8] = &event.data;

                let (slot_w, slot_h) = slot_dims.dims
                    .get(&slot_id)
                    .copied()
                    .unwrap_or((AVATAR_WIDTH, AVATAR_HEIGHT));

                static FIRST_READBACK: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(0);
                #[allow(clippy::declare_interior_mutable_const)]
                const ATOMIC_ZERO: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
                #[allow(clippy::borrow_interior_mutable_const)]
                static FRAME_COUNTER: [std::sync::atomic::AtomicU32; 16] = [ATOMIC_ZERO; 16];
                let mask = 1u16 << slot_id;
                let prev = FIRST_READBACK.fetch_or(mask, std::sync::atomic::Ordering::Relaxed);
                if prev & mask == 0 {
                    clog_info!("🎨 Slot {}: first ReadbackComplete ({} bytes, {}x{})", slot_id, pixel_bytes.len(), slot_w, slot_h);
                }

                let frame_n = FRAME_COUNTER[slot_id as usize].fetch_add(1, std::sync::atomic::Ordering::Relaxed);

                if frame_n == 150 || frame_n == 300 {
                    let test_frame = crate::live::avatar::RgbaFrame {
                        width: slot_w,
                        height: slot_h,
                        data: pixel_bytes.to_vec(),
                    };
                    let analysis = crate::live::avatar::frame_analysis::analyze(&test_frame);
                    let verdict = analysis.verdict();
                    match verdict {
                        crate::live::avatar::HealthVerdict::Healthy => {
                            clog_info!("🎨 Slot {} frame {}: HEALTHY — coverage={:.0}%, colors={}, roughness={:.1}, symmetry={:.2}",
                                slot_id, frame_n, analysis.coverage * 100.0, analysis.color_diversity,
                                analysis.edge_roughness, analysis.symmetry);
                        }
                        _ => {
                            clog_warn!("🎨 Slot {} frame {}: {:?} — coverage={:.0}%, colors={}, roughness={:.1}, white={:.0}%, symmetry={:.2}",
                                slot_id, frame_n, verdict, analysis.coverage * 100.0, analysis.color_diversity,
                                analysis.edge_roughness, analysis.white_ratio * 100.0, analysis.symmetry);
                            if let Some(model_path) = health.model_paths.get(&slot_id) {
                                clog_warn!("🎨 Slot {}: model '{}' rendered unhealthy", slot_id, model_path);
                            }
                        }
                    }
                }
                #[cfg(target_os = "macos")]
                {
                    if crate::live::avatar::publishers::gpu_bridge::try_write_bridge(slot_id, pixel_bytes) {
                        if let Some(notify) = notifiers.0.get(slot_id as usize) {
                            notify.notify_one();
                        }
                        return;
                    }
                }

                if let Some(tx) = channels.0.get(slot_id as usize) {
                    match tx.try_send(RgbaFrame {
                        width: slot_w,
                        height: slot_h,
                        data: pixel_bytes.to_vec(),
                    }) {
                        Ok(()) => {
                            if let Some(notify) = notifiers.0.get(slot_id as usize) {
                                notify.notify_one();
                            }
                        }
                        Err(crossbeam_channel::TrySendError::Full(_)) => {
                            #[allow(clippy::declare_interior_mutable_const)]
                            const DROP_ZERO: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
                            #[allow(clippy::borrow_interior_mutable_const)]
                            static DROP_COUNTS: [std::sync::atomic::AtomicU32; 16] = [DROP_ZERO; 16];
                            let count = DROP_COUNTS[slot_id as usize].fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            if count.is_multiple_of(150) {
                                clog_warn!("🎨 Slot {}: {} frames dropped (channel full)", slot_id, count + 1);
                            }
                        }
                        Err(crossbeam_channel::TrySendError::Disconnected(_)) => {}
                    }
                }
            },
        )
        .id()
}

#[allow(clippy::type_complexity)]
fn ensure_continuous_readback(
    query: Query<(Entity, &AvatarSlotId), (With<ReadbackMarker>, Without<Readback>)>,
    registry: Res<SlotRegistry>,
    cameras: Query<&Camera>,
    mut commands: Commands,
) {
    for (entity, slot_id) in &query {
        if let Some(state) = registry.slots.get(&slot_id.0) {
            if !state.active || !state.model_loaded {
                continue;
            }
            if let Ok(camera) = cameras.get(state.camera_entity) {
                if !camera.is_active {
                    continue;
                }
            }
            commands
                .entity(entity)
                .insert(Readback::texture(state._render_target.clone()));
        }
    }
}

// ============================================================================
// Setup Systems
// ============================================================================

fn setup_render_slots(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut registry: ResMut<SlotRegistry>,
    _frame_channels: Res<FrameChannels>,
) {
    commands.insert_resource(GlobalAmbientLight {
        color: Color::WHITE,
        brightness: 300.0,
        affects_lightmapped_meshes: false,
    });

    {
        let all_layers: Vec<usize> = (1..=(MAX_AVATAR_SLOTS as usize)).collect();
        commands.spawn((
            DirectionalLight {
                illuminance: 12000.0,
                shadows_enabled: false,
                ..default()
            },
            Transform::from_rotation(Quat::from_euler(
                EulerRot::XYZ,
                -0.4,
                std::f32::consts::PI,
                0.0,
            )),
            RenderLayers::from_layers(&all_layers),
            AvatarSceneLight,
        ));
    }

    for slot in 0..MAX_AVATAR_SLOTS {
        let layer = RenderLayers::layer((slot + 1) as usize);

        let size = Extent3d {
            width: AVATAR_WIDTH,
            height: AVATAR_HEIGHT,
            depth_or_array_layers: 1,
        };
        let mut rt_image = Image::new_fill(
            size,
            TextureDimension::D2,
            &[26, 26, 46, 255],
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        );
        rt_image.texture_descriptor.usage = TextureUsages::RENDER_ATTACHMENT
            | TextureUsages::COPY_SRC
            | TextureUsages::COPY_DST
            | TextureUsages::TEXTURE_BINDING;
        let rt_handle = images.add(rt_image);

        let camera_entity = commands
            .spawn((
                Camera3d::default(),
                Camera {
                    order: slot as isize,
                    clear_color: ClearColorConfig::Custom(Color::srgb(0.1, 0.1, 0.18)),
                    is_active: false,
                    ..default()
                },
                RenderTarget::Image(rt_handle.clone().into()),
                bevy::core_pipeline::tonemapping::Tonemapping::None,
                Msaa::Off,
                Transform::from_xyz(0.0, skeleton::REFERENCE_HEAD_Y, skeleton::REFERENCE_CAMERA_Z)
                    .looking_at(Vec3::new(0.0, skeleton::REFERENCE_HEAD_Y - 0.02, 0.0), Vec3::Y),
                layer.clone(),
                AvatarSlotId(slot),
            ))
            .id();

        let slot_id = slot;
        let readback_entity =
            spawn_readback_entity_opt(&mut commands, rt_handle.clone(), slot_id, false);

        registry.slots.insert(
            slot,
            SlotState {
                camera_entity,
                _readback_entity: readback_entity,
                scene_entity: None,
                _render_target: rt_handle.clone(),
                default_render_target: rt_handle,
                active: false,
                model_loaded: false,
                gltf_handle: None,
                model_path: None,
            },
        );
    }

    // Pre-allocate HD render target pool
    let mut hd_targets = Vec::with_capacity(MAX_HD_SLOTS);
    for _ in 0..MAX_HD_SLOTS {
        let hd_size = Extent3d {
            width: HD_WIDTH,
            height: HD_HEIGHT,
            depth_or_array_layers: 1,
        };
        let mut hd_image = Image::new_fill(
            hd_size,
            TextureDimension::D2,
            &[26, 26, 46, 255],
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        );
        hd_image.texture_descriptor.usage = TextureUsages::RENDER_ATTACHMENT
            | TextureUsages::COPY_SRC
            | TextureUsages::COPY_DST
            | TextureUsages::TEXTURE_BINDING;
        hd_targets.push(images.add(hd_image));
    }
    commands.insert_resource(HdRenderTargetPool {
        available: hd_targets,
        assigned: HashMap::new(),
    });

    // Track total render target VRAM allocation
    let lowres_bytes = MAX_AVATAR_SLOTS as u64 * AVATAR_WIDTH as u64 * AVATAR_HEIGHT as u64 * 4;
    let hd_bytes = MAX_HD_SLOTS as u64 * HD_WIDTH as u64 * HD_HEIGHT as u64 * 4;
    let total_rt_bytes = lowres_bytes + hd_bytes;
    if let Some(mgr) = gpu_manager() {
        match mgr.allocate(
            GpuSubsystem::Rendering,
            total_rt_bytes,
            GpuPriority::Realtime,
        ) {
            Ok(guard) => {
                mgr.eviction_registry.register(make_entry(
                    "render:targets",
                    "Render Targets (pre-allocated)",
                    GpuPriority::Realtime,
                    total_rt_bytes,
                ));
                commands.insert_resource(GpuGuards {
                    _render_targets: Some(guard),
                    model_guards: HashMap::new(),
                });
                clog_info!(
                    "🎨 GPU: allocated {:.1}MB for render targets",
                    total_rt_bytes as f64 / 1_048_576.0
                );
            }
            Err(e) => {
                clog_warn!(
                    "🎨 GPU: render target allocation failed ({}), proceeding untracked",
                    e
                );
            }
        }
    }

    clog_info!(
        "🎨 Bevy renderer ready: {} slots x {}x{} @{}fps ({} HD targets pooled at {}x{})",
        MAX_AVATAR_SLOTS,
        AVATAR_WIDTH,
        AVATAR_HEIGHT,
        AVATAR_FPS,
        MAX_HD_SLOTS,
        HD_WIDTH,
        HD_HEIGHT
    );
}

fn signal_ready(flag: Res<ReadyFlag>) {
    flag.0.store(true, std::sync::atomic::Ordering::Release);
}

fn has_active_slots(registry: Res<SlotRegistry>) -> bool {
    registry
        .slots
        .values()
        .any(|s| s.active && s.model_loaded)
}

fn touch_ambient_light(mut ambient: ResMut<GlobalAmbientLight>) {
    ambient.set_changed();
}

fn update_memory_stats(
    registry: Res<SlotRegistry>,
    pending: Res<PendingLoads>,
    speaking_query: Query<&AvatarSlotId, With<Speaking>>,
    stats: Res<SharedMemoryStats>,
    slot_dims: Res<SlotDimensions>,
) {
    let mut active = 0u8;
    let mut loaded = 0u8;
    let mut rt_bytes = 0u64;

    for (slot_id, state) in &registry.slots {
        if state.active {
            active += 1;
        }
        if state.model_loaded {
            loaded += 1;
        }
        let (w, h) = slot_dims
            .dims
            .get(slot_id)
            .copied()
            .unwrap_or((AVATAR_WIDTH, AVATAR_HEIGHT));
        rt_bytes += (w as u64) * (h as u64) * 4;
    }

    let speaking = speaking_query.iter().count() as u8;
    let pending_count = pending.gltf_handles.len() + pending.scene_handles.len();

    stats.0.active_slots.store(active, std::sync::atomic::Ordering::Relaxed);
    stats.0.loaded_models.store(loaded, std::sync::atomic::Ordering::Relaxed);
    stats.0.speaking_slots.store(speaking, std::sync::atomic::Ordering::Relaxed);
    stats.0.render_target_bytes.store(rt_bytes, std::sync::atomic::Ordering::Relaxed);
    stats.0.pending_loads.store(pending_count as u32, std::sync::atomic::Ordering::Relaxed);
}

fn force_light_visibility(
    mut lights: Query<&mut bevy::camera::visibility::ViewVisibility, With<AvatarSceneLight>>,
) {
    for mut vis in &mut lights {
        vis.set_visible();
    }
}

fn monitor_load_states(
    asset_server: Res<AssetServer>,
    mut pending: ResMut<PendingLoads>,
    gltf_assets: Res<Assets<bevy::gltf::Gltf>>,
) {
    static CWD_LOGGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if !CWD_LOGGED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        if let Ok(cwd) = std::env::current_dir() {
            clog_info!("🎨 Asset server cwd: {:?}", cwd);
        }
        let test_path = "models/avatars/vroid-female-base.glb";
        clog_info!(
            "🎨 File check '{}': exists={}",
            test_path,
            std::path::Path::new(test_path).exists()
        );
    }

    for entry in pending.gltf_handles.iter_mut() {
        if entry.logged_final {
            continue;
        }
        match asset_server.load_state(entry.handle.id()) {
            LoadState::Loaded => {
                if let Some(gltf) = gltf_assets.get(entry.handle.id()) {
                    let named: Vec<&Box<str>> = gltf.named_scenes.keys().collect();
                    clog_info!(
                        "🎨 Gltf LOADED slot {}: {} — {} scenes, named: {:?}",
                        entry.slot,
                        entry.path,
                        gltf.scenes.len(),
                        named,
                    );
                } else {
                    clog_info!(
                        "🎨 Gltf LOADED slot {}: {} (not yet in Assets<Gltf>)",
                        entry.slot,
                        entry.path
                    );
                }
                entry.logged_final = true;
            }
            LoadState::Failed(ref err) => {
                clog_warn!(
                    "🎨 Gltf FAILED slot {}: {} — error: {:?}",
                    entry.slot,
                    entry.path,
                    err
                );
                entry.logged_final = true;
            }
            _ => {}
        }
    }

    for entry in pending.scene_handles.iter_mut() {
        if entry.logged_final {
            continue;
        }
        match asset_server.load_state(entry.handle.id()) {
            LoadState::Loaded => {
                clog_info!("🎨 Scene LOADED slot {}: {}", entry.slot, entry.path);
                entry.logged_final = true;
            }
            LoadState::Failed(ref err) => {
                clog_warn!(
                    "🎨 Scene FAILED slot {}: {} — error: {:?}",
                    entry.slot,
                    entry.path,
                    err
                );
                entry.logged_final = true;
            }
            _ => {}
        }
    }

    pending.gltf_handles.retain(|e| !e.logged_final);
    pending.scene_handles.retain(|e| !e.logged_final);
}

// ============================================================================
// Command Processing
// ============================================================================

#[allow(clippy::too_many_arguments)]
fn process_commands(
    command_channel: Res<CommandChannel>,
    time: Res<Time>,
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut registry: ResMut<SlotRegistry>,
    mut cameras: Query<&mut Camera>,
    mut pending: ResMut<PendingLoads>,
    mut speech_clips: ResMut<ActiveSpeechClips>,
    mut legacy_mouth: ResMut<LegacyMouthWeights>,
    mut emotion_state: ResMut<EmotionState>,
    mut active_gestures: ResMut<ActiveGestures>,
    mut cognitive_anim: ResMut<CognitiveAnimState>,
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

                if let Some(state) = registry.slots.get_mut(&slot) {
                    let layer = RenderLayers::layer((slot + 1) as usize);

                    if let Some(old_entity) = state.scene_entity.take() {
                        commands.entity(old_entity).despawn();
                    }
                    gpu_guards.model_guards.remove(&slot);

                    state.gltf_handle = None;
                    state.model_path = Some(model_path.clone());

                    let load_path = if model_path.ends_with(".vrm") {
                        model_path.replacen(".vrm", ".glb", 1)
                    } else {
                        model_path.clone()
                    };

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
                    state.gltf_handle = Some(gltf_handle);
                    let scene_entity = commands
                        .spawn((
                            SceneRoot(scene_handle),
                            Transform::default(),
                            layer.clone(),
                            AvatarSlotId(slot),
                        ))
                        .id();

                    let layer_for_observer = layer;
                    let slot_for_observer = slot;
                    let model_path_for_observer = load_path.clone();
                    commands.entity(scene_entity).observe(
                        move |
                            event: On<SceneInstanceReady>,
                            children_query: Query<&Children>,
                            names: Query<&Name>,
                            mut transforms: Query<&mut Transform>,
                            mut cmds: Commands,
                            mut bone_registry: ResMut<BoneRegistry>,
                            mut slot_registry: ResMut<SlotRegistry>,
                            mut gpu_guards: ResMut<GpuGuards>,
                        | {
                            let root = event.entity;
                            let child_count = skeleton::count_descendants(root, &children_query);
                            skeleton::propagate_render_layers(root, &layer_for_observer, &children_query, &mut cmds);
                            skeleton::dump_bone_names(root, &children_query, &names);
                            skeleton::fix_tpose_arms(root, &children_query, &names, &mut transforms);
                            skeleton::discover_upper_body_bones(root, slot_for_observer, &model_path_for_observer, &children_query, &names, &transforms, &mut bone_registry);

                            if let Some(state) = slot_registry.slots.get_mut(&slot_for_observer) {
                                state.model_loaded = true;
                            }

                            let model_bytes = std::fs::metadata(&model_path_for_observer)
                                .map(|m| m.len())
                                .unwrap_or(0);
                            if model_bytes > 0 {
                                if let Some(mgr) = gpu_manager() {
                                    match mgr.allocate(GpuSubsystem::Rendering, model_bytes, GpuPriority::Interactive) {
                                        Ok(guard) => {
                                            mgr.eviction_registry.register(make_entry(
                                                &format!("render:model:slot{}", slot_for_observer),
                                                &format!("Avatar Model (slot {})", slot_for_observer),
                                                GpuPriority::Interactive,
                                                model_bytes,
                                            ));
                                            gpu_guards.model_guards.insert(slot_for_observer, guard);
                                        }
                                        Err(e) => {
                                            clog_warn!("🎨 GPU: model allocation for slot {} failed ({})", slot_for_observer, e);
                                        }
                                    }
                                }
                            }

                            clog_info!("🎨 SceneInstanceReady: slot {}, entity {:?}, {} descendants — render layers propagated, readback enabled", slot_for_observer, root, child_count);
                        }
                    );

                    state.scene_entity = Some(scene_entity);
                    state.active = true;

                    if let Ok(mut camera) = cameras.get_mut(state.camera_entity) {
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
                if let Some(state) = registry.slots.get_mut(&slot) {
                    if let Some(entity) = state.scene_entity.take() {
                        commands.entity(entity).despawn();
                    }
                    state.active = false;
                    state.model_loaded = false;
                    state.gltf_handle = None;
                    state.model_path = None;
                    if let Ok(mut camera) = cameras.get_mut(state.camera_entity) {
                        camera.is_active = false;
                    }
                    if let Some(hd_target) = hd_pool.assigned.remove(&slot) {
                        hd_pool.available.push(hd_target);
                        commands
                            .entity(state.camera_entity)
                            .insert(RenderTarget::Image(
                                state.default_render_target.clone().into(),
                            ));
                        state._render_target = state.default_render_target.clone();
                        slot_dims.dims.insert(slot, (AVATAR_WIDTH, AVATAR_HEIGHT));
                    }
                    gpu_guards.model_guards.remove(&slot);
                    emotion_state.slots.remove(&slot);
                    active_gestures.slots.remove(&slot);
                    cognitive_anim.slots.remove(&slot);
                    clog_info!("🎨 Slot {}: unloaded", slot);
                }
            }
            AvatarCommand::SetSpeaking { slot, speaking } => {
                if let Some(state) = registry.slots.get(&slot) {
                    if let Some(scene_entity) = state.scene_entity {
                        if speaking {
                            commands.entity(scene_entity).insert(Speaking);
                        } else {
                            commands.entity(scene_entity).remove::<Speaking>();
                        }
                    }
                }
            }
            AvatarCommand::SetMouthWeight { slot, weight } => {
                legacy_mouth.weights.insert(slot, weight);
            }
            AvatarCommand::SetMouthWeightSequence {
                slot,
                weights,
                interval_ms,
            } => {
                let duration_ms = weights.len() as u64 * interval_ms as u64;
                speech_clips.clips.insert(
                    slot,
                    ActiveClip {
                        mouth_weights: weights,
                        interval_ms,
                        duration_ms,
                        start_time: time.elapsed_secs(),
                    },
                );
                if let Some(state) = registry.slots.get(&slot) {
                    if let Some(scene_entity) = state.scene_entity {
                        commands.entity(scene_entity).insert(Speaking);
                    }
                }
                speech_clips.clips_started += 1;
            }
            AvatarCommand::PlaySpeech { slot, clip } => {
                speech_clips.clips.insert(
                    slot,
                    ActiveClip {
                        mouth_weights: clip.mouth_weights,
                        interval_ms: clip.interval_ms,
                        duration_ms: clip.duration_ms,
                        start_time: time.elapsed_secs(),
                    },
                );
                if let Some(state) = registry.slots.get(&slot) {
                    if let Some(scene_entity) = state.scene_entity {
                        commands.entity(scene_entity).insert(Speaking);
                    }
                }
                speech_clips.clips_started += 1;
            }
            AvatarCommand::StopSpeech { slot } => {
                if speech_clips.clips.remove(&slot).is_some() {
                    speech_clips.clips_interrupted += 1;
                }
                if let Some(state) = registry.slots.get(&slot) {
                    if let Some(scene_entity) = state.scene_entity {
                        commands.entity(scene_entity).remove::<Speaking>();
                    }
                }
            }
            AvatarCommand::SetEmotion {
                slot,
                emotion,
                weight,
                transition_ms,
            } => {
                let rate = if transition_ms > 0 {
                    1.0 / (transition_ms as f32 / 1000.0)
                } else {
                    100.0
                };
                let state = emotion_state
                    .slots
                    .entry(slot)
                    .or_default();
                state.target = emotion;
                state.target_weight = weight.clamp(0.0, 1.0);
                state.transition_rate = rate;
                state.decay_timer = EMOTION_DECAY_SECS;
            }
            AvatarCommand::SetGesture {
                slot,
                gesture,
                duration_ms,
            } => {
                if gesture == Gesture::None {
                    active_gestures.slots.remove(&slot);
                } else {
                    active_gestures.slots.insert(
                        slot,
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
            AvatarCommand::SetCognitiveState { slot, state } => {
                use crate::live::session::cognitive_animation::CognitiveState;
                match state {
                    CognitiveState::Idle => {
                        cognitive_anim.slots.remove(&slot);
                    }
                    _ => {
                        cognitive_anim.slots.insert(slot, SlotCognitiveState {
                            state,
                            config: crate::live::session::cognitive_animation::CognitiveAnimationConfig::default(),
                            time_since_reroll: 999.0,
                        });
                    }
                }
            }
            AvatarCommand::Resize {
                slot,
                width,
                height,
            } => {
                if let Some(state) = registry.slots.get_mut(&slot) {
                    let is_hd_request = width >= HD_WIDTH && height >= HD_HEIGHT;
                    let currently_hd = hd_pool.assigned.contains_key(&slot);

                    let new_rt_handle = if is_hd_request && !currently_hd {
                        if let Some(hd_target) = hd_pool.available.pop() {
                            hd_pool.assigned.insert(slot, hd_target.clone());
                            clog_info!(
                                "🎨 Slot {}: promoted to HD ({}x{}, {} HD targets remaining)",
                                slot, HD_WIDTH, HD_HEIGHT, hd_pool.available.len()
                            );
                            hd_target
                        } else {
                            clog_warn!("🎨 Slot {}: HD pool exhausted ({} already assigned), staying at current res",
                                slot, hd_pool.assigned.len());
                            continue;
                        }
                    } else if !is_hd_request && currently_hd {
                        if let Some(hd_target) = hd_pool.assigned.remove(&slot) {
                            hd_pool.available.push(hd_target);
                            clog_info!(
                                "🎨 Slot {}: demoted to low-res ({}x{}, {} HD targets available)",
                                slot, AVATAR_WIDTH, AVATAR_HEIGHT, hd_pool.available.len()
                            );
                        }
                        state.default_render_target.clone()
                    } else if is_hd_request && currently_hd {
                        continue;
                    } else {
                        state.default_render_target.clone()
                    };

                    commands
                        .entity(state.camera_entity)
                        .insert(RenderTarget::Image(new_rt_handle.clone().into()));

                    commands.entity(state._readback_entity).despawn();
                    let new_readback =
                        spawn_readback_entity(&mut commands, new_rt_handle.clone(), slot);

                    state._readback_entity = new_readback;
                    state._render_target = new_rt_handle;

                    let (effective_w, effective_h) = if is_hd_request {
                        (HD_WIDTH, HD_HEIGHT)
                    } else {
                        (AVATAR_WIDTH, AVATAR_HEIGHT)
                    };
                    slot_dims.dims.insert(slot, (effective_w, effective_h));

                    clog_info!(
                        "🎨 Slot {}: resized to {}x{}",
                        slot,
                        effective_w,
                        effective_h
                    );
                }
            }
            AvatarCommand::Shutdown => {
                clog_info!("🎨 Bevy renderer shutting down");
                return;
            }
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constants() {
        assert!(
            MAX_AVATAR_SLOTS >= 14,
            "Need at least 14 slots for all personas"
        );
        assert_eq!(AVATAR_WIDTH, 640);
        assert_eq!(AVATAR_HEIGHT, 360);
        assert!(
            AVATAR_FPS >= 10.0 && AVATAR_FPS <= 60.0,
            "FPS must be 10-60 — below 10 looks choppy, above 60 wastes GPU"
        );
    }
}
