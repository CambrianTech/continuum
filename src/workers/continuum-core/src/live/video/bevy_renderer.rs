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
//!           └── Avatar slot 13: Camera → RenderTarget → Readback → channel
//!
//! Performance: 14 avatars × 1280×720 @ 30fps = ~1.5 GB/s GPU readback.
//! On Apple Silicon (shared memory), this is essentially a memcpy.

use bevy::prelude::*;
use bevy::app::ScheduleRunnerPlugin;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use bevy::asset::RenderAssetUsages;
use bevy::render::gpu_readback::{Readback, ReadbackComplete};
use bevy::camera::visibility::RenderLayers;
use bevy::camera::RenderTarget;
use bevy::asset::LoadState;
use bevy::mesh::morph::MorphWeights;
use bevy::scene::SceneInstanceReady;
use crossbeam_channel::{Receiver, Sender};
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;
use std::time::Duration;
use crate::{clog_info, clog_warn};

use crate::live::avatar::RgbaFrame;

/// Debug logging — no-op in production. Enable via BEVY_AVATAR_DEBUG=1 env var.
fn bevy_debug(_msg: &str) {
    // Intentionally empty — file I/O per call was killing performance
}

/// Maximum number of concurrent avatar render slots.
/// 24 supports up to 24 AI personas with 3D avatars simultaneously.
/// At 30fps × 640×480 × 4 bytes = ~36.9 MB/s readback per slot.
/// Bevy supports 32 render layers; we use layers 1-24, leaving headroom.
pub const MAX_AVATAR_SLOTS: u8 = 16;

/// Render resolution per avatar.
/// 640×480 (VGA) — good quality for grid tiles, ~75% less GPU than 720p.
/// Adaptive resolution (ResolutionTier) will eventually drive per-slot sizes,
/// but VGA is the minimum floor. Spotlight tiles can go higher later.
const AVATAR_WIDTH: u32 = 640;
const AVATAR_HEIGHT: u32 = 480;

/// Target framerate for avatar rendering.
/// 15fps — adequate for lip sync and head animation while reducing GPU load ~40%.
/// 16 slots × VGA × 15fps ≈ 29 MB/s readback (vs 47 MB/s at 24fps).
/// Prevents sustained-load CPU/GPU saturation with 15+ concurrent avatars.
const AVATAR_FPS: f64 = 15.0;

// ============================================================================
// Public API — BevyAvatarSystem singleton
// ============================================================================

/// Global singleton for the Bevy avatar rendering system.
static BEVY_SYSTEM: OnceLock<BevyAvatarSystem> = OnceLock::new();

/// Get or initialize the global BevyAvatarSystem.
/// The Bevy app is lazily started on first access.
pub fn get_or_init() -> &'static BevyAvatarSystem {
    BEVY_SYSTEM.get_or_init(|| {
        bevy_debug(&format!("Starting Bevy headless renderer ({MAX_AVATAR_SLOTS} slots, {AVATAR_WIDTH}x{AVATAR_HEIGHT} @{AVATAR_FPS}fps)"));
        clog_info!("🎨 Starting Bevy headless avatar renderer ({MAX_AVATAR_SLOTS} slots, {AVATAR_WIDTH}x{AVATAR_HEIGHT} @{AVATAR_FPS}fps)");
        BevyAvatarSystem::start()
    })
}

/// Get the BevyAvatarSystem if it has already been initialized.
/// Returns None if get_or_init() has never been called.
pub fn try_get() -> Option<&'static BevyAvatarSystem> {
    BEVY_SYSTEM.get()
}

/// Commands sent to the Bevy renderer thread.
#[derive(Debug)]
pub enum AvatarCommand {
    /// Load a VRM/glTF model into a render slot.
    Load {
        slot: u8,
        model_path: String,
        display_name: String,
        /// Persona identity (user_id) — used for procedural fallback color generation.
        identity: String,
    },
    /// Remove the model from a render slot.
    Unload { slot: u8 },
    /// Set whether the avatar is currently speaking (for expression animation).
    SetSpeaking { slot: u8, speaking: bool },
    /// Set mouth open weight from audio amplitude (0.0 = closed, 1.0 = fully open).
    /// Overrides the default sine oscillation for amplitude-responsive lip sync.
    SetMouthWeight { slot: u8, weight: f32 },
    /// Resize a slot's render target to new dimensions.
    /// Recreates the render target image, camera target, and readback entity.
    Resize { slot: u8, width: u32, height: u32 },
    /// Shut down the renderer gracefully.
    Shutdown,
}

/// The singleton Bevy avatar rendering system.
/// Manages a headless Bevy app on a dedicated thread that renders
/// all avatar models and delivers RGBA frames via channels.
pub struct BevyAvatarSystem {
    command_tx: Sender<AvatarCommand>,
    /// Frame receivers, one per slot. Each LiveKit agent gets one.
    frame_receivers: Vec<Receiver<RgbaFrame>>,
    /// Set to true once the Bevy app completes startup (setup_render_slots runs).
    ready: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Maps persona identity (user_id) → Bevy render slot index.
    /// Populated by register_identity() when a renderer is created.
    identity_to_slot: std::sync::Mutex<HashMap<String, u8>>,
}

impl BevyAvatarSystem {
    /// Start the Bevy renderer on a dedicated OS thread.
    fn start() -> Self {
        let (command_tx, command_rx) = crossbeam_channel::unbounded();
        let ready = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let ready_clone = ready.clone();

        // Create frame channels for each slot
        let mut frame_senders = Vec::with_capacity(MAX_AVATAR_SLOTS as usize);
        let mut frame_receivers = Vec::with_capacity(MAX_AVATAR_SLOTS as usize);
        for _ in 0..MAX_AVATAR_SLOTS {
            let (tx, rx) = crossbeam_channel::bounded(2); // small buffer, drop stale frames
            frame_senders.push(tx);
            frame_receivers.push(rx);
        }

        std::thread::Builder::new()
            .name("bevy-avatar-renderer".into())
            .spawn(move || {
                run_bevy_app(command_rx, frame_senders, ready_clone);
            })
            .expect("Failed to spawn Bevy avatar renderer thread");

        // Wait up to 5 seconds for Bevy to initialize
        for _ in 0..50 {
            if ready.load(std::sync::atomic::Ordering::Acquire) {
                clog_info!("🎨 Bevy renderer confirmed ready");
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }

        if !ready.load(std::sync::atomic::Ordering::Acquire) {
            bevy_debug("WARN: Bevy did not report ready within 5s — may have failed to init GPU");
            clog_warn!("🎨 Bevy renderer did not report ready within 5s — may have failed to init GPU");
        } else {
            bevy_debug("Bevy confirmed READY");
        }

        Self {
            command_tx,
            frame_receivers,
            ready,
            identity_to_slot: std::sync::Mutex::new(HashMap::new()),
        }
    }

    /// Check if the Bevy renderer is alive and ready to accept commands.
    pub fn is_ready(&self) -> bool {
        self.ready.load(std::sync::atomic::Ordering::Acquire)
    }

    /// Get the frame receiver for a specific avatar slot.
    /// Returns None if slot is out of range.
    pub fn frame_receiver(&self, slot: u8) -> Option<&Receiver<RgbaFrame>> {
        self.frame_receivers.get(slot as usize)
    }

    /// Load a VRM/glTF model into a render slot.
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

    /// Remove the model from a render slot.
    pub fn unload_model(&self, slot: u8) {
        let _ = self.command_tx.send(AvatarCommand::Unload { slot });
    }

    /// Update speaking state for expression animation.
    pub fn set_speaking(&self, slot: u8, speaking: bool) {
        let _ = self.command_tx.send(AvatarCommand::SetSpeaking { slot, speaking });
    }

    /// Register a persona identity → slot mapping (called when a BevyChannelRenderer is created).
    pub fn register_identity(&self, identity: &str, slot: u8) {
        self.identity_to_slot.lock().unwrap().insert(identity.to_string(), slot);
        bevy_debug(&format!("Registered identity '{}' → slot {}", &identity[..8.min(identity.len())], slot));
    }

    /// Remove a persona identity from the slot map (called when SlotGuard drops).
    pub fn unregister_identity(&self, identity: &str) {
        self.identity_to_slot.lock().unwrap().remove(identity);
        bevy_debug(&format!("Unregistered identity '{}'", &identity[..8.min(identity.len())]));
    }

    /// Get a snapshot of the identity → slot map (for dedup checks in render_loop).
    pub fn identity_to_slot_map(&self) -> HashMap<String, u8> {
        self.identity_to_slot.lock().unwrap().clone()
    }

    /// Update speaking state by persona identity (user_id).
    /// Returns false if identity has no registered slot.
    pub fn set_speaking_by_identity(&self, identity: &str, speaking: bool) -> bool {
        if let Some(&slot) = self.identity_to_slot.lock().unwrap().get(identity) {
            self.set_speaking(slot, speaking);
            true
        } else {
            false
        }
    }

    /// Set mouth open weight for amplitude-responsive lip sync.
    /// Weight should be 0.0 (closed) to 1.0 (fully open).
    pub fn set_mouth_weight(&self, slot: u8, weight: f32) {
        let _ = self.command_tx.send(AvatarCommand::SetMouthWeight { slot, weight });
    }

    /// Set mouth weight by persona identity (user_id).
    /// Returns false if identity has no registered slot.
    pub fn set_mouth_weight_by_identity(&self, identity: &str, weight: f32) -> bool {
        if let Some(&slot) = self.identity_to_slot.lock().unwrap().get(identity) {
            self.set_mouth_weight(slot, weight);
            true
        } else {
            false
        }
    }

    /// Resize a slot's render target to new dimensions.
    pub fn resize_slot(&self, slot: u8, width: u32, height: u32) {
        let _ = self.command_tx.send(AvatarCommand::Resize { slot, width, height });
    }

    /// Resize by persona identity (user_id).
    /// Returns false if identity has no registered slot.
    pub fn resize_by_identity(&self, identity: &str, width: u32, height: u32) -> bool {
        if let Some(&slot) = self.identity_to_slot.lock().unwrap().get(identity) {
            self.resize_slot(slot, width, height);
            true
        } else {
            false
        }
    }
}

// ============================================================================
// Bevy App (runs on dedicated OS thread)
// ============================================================================

/// Channel for receiving commands from the main application.
#[derive(Resource)]
struct CommandChannel(Receiver<AvatarCommand>);

/// Channels for sending rendered frames back to LiveKit video loops.
#[derive(Resource)]
struct FrameChannels(Vec<Sender<RgbaFrame>>);

/// Shared ready flag — set after Bevy Startup systems complete.
#[derive(Resource)]
struct ReadyFlag(std::sync::Arc<std::sync::atomic::AtomicBool>);

/// Tracks the state of each avatar render slot.
#[derive(Resource)]
struct SlotRegistry {
    slots: HashMap<u8, SlotState>,
}

struct SlotState {
    camera_entity: Entity,
    _readback_entity: Entity,
    scene_entity: Option<Entity>,
    _render_target: Handle<Image>,
    active: bool,
    /// Handle to the loaded Gltf asset — used for morph target name discovery.
    gltf_handle: Option<Handle<bevy::gltf::Gltf>>,
    /// Path to the model file — used for VRM extension parsing.
    model_path: Option<String>,
}

/// Component marking which avatar slot an entity belongs to.
/// The inner u8 is the slot index — used by Bevy's ECS query system even though
/// Rust's dead-code analysis doesn't see it read (it's matched via Query filters).
#[derive(Component, Clone, Copy)]
struct AvatarSlotId(#[allow(dead_code)] u8);

/// Marker component for readback entities (distinguishes from camera/scene entities
/// which also have AvatarSlotId). Used by `ensure_continuous_readback` to find
/// entities that need their `Readback` component re-inserted.
#[derive(Component)]
struct ReadbackMarker;

/// Component marking an avatar that is currently speaking.
#[derive(Component)]
struct Speaking;

/// Tracks slot metadata for health check logging.
#[derive(Resource, Default)]
struct SlotHealthStatus {
    /// Identity string per slot.
    identities: HashMap<u8, String>,
    /// Model path per slot.
    model_paths: HashMap<u8, String>,
}



/// Tracks asset handles for load state monitoring.
/// Monitors both the parent Gltf asset (parsing) and the Scene sub-asset (extraction).
#[derive(Resource, Default)]
struct PendingLoads {
    scene_handles: Vec<PendingLoadEntry<Scene>>,
    gltf_handles: Vec<PendingLoadEntry<bevy::gltf::Gltf>>,
}

struct PendingLoadEntry<T: bevy::asset::Asset> {
    slot: u8,
    handle: Handle<T>,
    path: String,
    logged_final: bool,
}

/// Per-slot morph target layout discovered at scene load time.
/// VRM models have blend shapes on their face mesh; we discover indices
/// at load time so animation systems can set weights efficiently.
#[derive(Resource, Default)]
struct SlotMorphTargets {
    layouts: HashMap<u8, MorphTargetLayout>,
}

struct MorphTargetLayout {
    /// Entity that has the MorphWeights component (the face mesh)
    mesh_entity: Entity,
    /// Index of the "aa" / "A" mouth-open morph target
    mouth_open_index: Option<usize>,
    /// Index of blink morph target (both eyes)
    blink_index: Option<usize>,
    /// Index of left eye blink
    blink_left_index: Option<usize>,
    /// Index of right eye blink
    blink_right_index: Option<usize>,
}

/// Per-slot blink animation state.
#[derive(Resource, Default)]
struct BlinkState {
    slots: HashMap<u8, SlotBlinkState>,
}

struct SlotBlinkState {
    next_blink_time: f32,
    blink_frames_remaining: u8,
}

/// Per-slot bone registry — tracks discovered skeleton bones for animation systems.
/// Replaces the single-bone HeadBones with full upper-body bone tracking for
/// idle gestures, speaking animations, and camera targeting.
#[derive(Resource, Default)]
struct BoneRegistry {
    slots: HashMap<u8, SlotBones>,
}

struct SlotBones {
    head: Option<BoneInfo>,
    neck: Option<BoneInfo>,
    left_shoulder: Option<BoneInfo>,
    right_shoulder: Option<BoneInfo>,
}

struct BoneInfo {
    entity: Entity,
    /// Actual local-space rest translation (from skeleton bind pose)
    rest_translation: Vec3,
    /// Actual local-space rest rotation (from skeleton bind pose)
    rest_rotation: Quat,
}

/// Per-slot idle gesture animation state.
/// Tracks unique phase offsets per slot so avatars don't move in sync.
#[derive(Resource, Default)]
struct IdleGestureState {
    slots: HashMap<u8, SlotGestureState>,
}

struct SlotGestureState {
    /// Unique phase offset derived from slot index
    phase: f32,
}

/// Per-slot mouth weights from audio amplitude analysis.
/// Updated by SetMouthWeight commands from the livekit_agent audio pipeline.
/// When present, animate_speaking() uses this instead of sine oscillation.
#[derive(Resource, Default)]
struct MouthWeights {
    weights: HashMap<u8, f32>,
}


fn run_bevy_app(
    command_rx: Receiver<AvatarCommand>,
    frame_senders: Vec<Sender<RgbaFrame>>,
    ready_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
) {
    // Bevy's FileAssetReader resolves asset paths relative to the BINARY location
    // (current_exe().parent()), NOT the process cwd. Since our binary lives at
    // src/workers/target/release/ but models live at src/models/, we must
    // explicitly set the asset base path to the process cwd.
    let asset_base = std::env::current_dir()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    bevy_debug(&format!("Asset base path: {}", asset_base));

    App::new()
        .insert_resource(CommandChannel(command_rx))
        .insert_resource(FrameChannels(frame_senders))
        .insert_resource(ReadyFlag(ready_flag))
        .insert_resource(SlotRegistry {
            slots: HashMap::new(),
        })
        .insert_resource(PendingLoads::default())
        .insert_resource(SlotMorphTargets::default())
        .insert_resource(BlinkState::default())
        .insert_resource(BoneRegistry::default())
        .insert_resource(IdleGestureState::default())
        .insert_resource(MouthWeights::default())
        .insert_resource(SlotHealthStatus::default())
        // DefaultPlugins with no window — the official Bevy headless rendering approach.
        // WindowPlugin registers Events<WindowResized> etc. needed by camera_system,
        // but primary_window: None means no actual OS window is created.
        // ExitCondition::DontExit prevents "No windows open, exiting" (Issue #16807).
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
                })
        )
        // ScheduleRunnerPlugin drives the frame loop at our target FPS
        .add_plugins(ScheduleRunnerPlugin::run_loop(
            Duration::from_secs_f64(1.0 / AVATAR_FPS),
        ))
        // Bevy 0.18: TransformTreeChanged is a new marker component required by Transform
        // (via #[require(TransformTreeChanged)]). Scene spawner panics if it's not registered
        // in the type registry. reflect_auto_register should handle this but doesn't.
        .register_type::<bevy::transform::components::TransformTreeChanged>()
        .add_systems(Startup, (setup_render_slots, signal_ready).chain())
        .add_systems(Update, (
            process_commands,
            ensure_continuous_readback,
            monitor_load_states,
            discover_morph_targets,
            animate_idle,
            animate_speaking,
            animate_blinking,
            animate_breathing,
            animate_idle_gestures,
        ))
        .run();
}

/// Spawn a readback entity for a given render target + slot.
///
/// CRITICAL: Bevy's `Readback` component is one-shot — it fires `ReadbackComplete`
/// once and is consumed. For continuous video frames, we must re-insert the
/// `Readback` component after each completion. This gives us readback every
/// other frame (~7.5fps at 15fps Bevy) due to Commands' deferred execution.
fn spawn_readback_entity(
    commands: &mut Commands,
    rt_handle: Handle<Image>,
    slot_id: u8,
) -> Entity {
    commands
        .spawn((
            Readback::texture(rt_handle),
            AvatarSlotId(slot_id),
            ReadbackMarker,
        ))
        .observe(
            move |event: On<ReadbackComplete>,
                  channels: Res<FrameChannels>,
                  health: Res<SlotHealthStatus>| {
                let pixel_bytes: &[u8] = &event.data;
                // Log first readback per slot + pixel diversity diagnostic
                static FIRST_READBACK: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(0);
                static FRAME_COUNTER: [std::sync::atomic::AtomicU32; 16] = {
                    const INIT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
                    [INIT; 16]
                };
                let mask = 1u16 << slot_id;
                let prev = FIRST_READBACK.fetch_or(mask, std::sync::atomic::Ordering::Relaxed);
                if prev & mask == 0 {
                    clog_info!("🎨 Slot {}: first ReadbackComplete ({} bytes)", slot_id, pixel_bytes.len());
                }

                // Health check at frame 150 and 300 — LOG ONLY, no fallback.
                // If a model is broken, we want to SEE it and fix the root cause.
                let frame_n = FRAME_COUNTER[slot_id as usize].fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if frame_n == 150 || frame_n == 300 {
                    let test_frame = crate::live::avatar::RgbaFrame {
                        width: AVATAR_WIDTH,
                        height: AVATAR_HEIGHT,
                        data: pixel_bytes.to_vec(),
                    };
                    let analysis = crate::live::avatar::frame_analysis::analyze(&test_frame);
                    let verdict = analysis.verdict();
                    match verdict {
                        crate::live::avatar::HealthVerdict::Healthy => {
                            clog_info!("🎨 Slot {} frame {}: ✅ HEALTHY — coverage={:.0}%, colors={}, roughness={:.1}, symmetry={:.2}",
                                slot_id, frame_n, analysis.coverage * 100.0, analysis.color_diversity,
                                analysis.edge_roughness, analysis.symmetry);
                        }
                        _ => {
                            clog_warn!("🎨 Slot {} frame {}: ❌ {:?} — coverage={:.0}%, colors={}, roughness={:.1}, white={:.0}%, symmetry={:.2}",
                                slot_id, frame_n, verdict, analysis.coverage * 100.0, analysis.color_diversity,
                                analysis.edge_roughness, analysis.white_ratio * 100.0, analysis.symmetry);
                            if let Some(model_path) = health.model_paths.get(&slot_id) {
                                clog_warn!("🎨 Slot {}: model '{}' rendered unhealthy", slot_id, model_path);
                            }
                        }
                    }
                }
                // GPU bridge zero-copy path (macOS only): write RGBA→NV12 directly
                // to pre-allocated IOSurface. Eliminates pixel_bytes.to_vec() (1.2MB)
                // and per-frame CVPixelBufferCreate (460KB).
                #[cfg(target_os = "macos")]
                {
                    if crate::live::avatar::publishers::gpu_bridge::try_write_bridge(slot_id, pixel_bytes) {
                        return; // Frame written to IOSurface, skip channel
                    }
                }

                // Channel path (non-macOS, or no GPU bridge registered for this slot)
                if let Some(tx) = channels.0.get(slot_id as usize) {
                    let _ = tx.try_send(RgbaFrame {
                        width: AVATAR_WIDTH,
                        height: AVATAR_HEIGHT,
                        data: pixel_bytes.to_vec(),
                    });
                }
                // NOTE: Readback re-insertion is handled by ensure_continuous_readback system,
                // NOT here. Observer Commands race with Bevy's internal Readback removal.
            },
        )
        .id()
}

/// Create render targets, cameras, and lights for all avatar slots.
fn setup_render_slots(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut registry: ResMut<SlotRegistry>,
    _frame_channels: Res<FrameChannels>,
) {
    // Global ambient light (all layers)
    // Bevy 0.18: AmbientLight is now per-camera; GlobalAmbientLight is the resource version
    commands.insert_resource(GlobalAmbientLight {
        color: Color::WHITE,
        brightness: 300.0,
        affects_lightmapped_meshes: false,
    });

    // Single shared directional light visible on ALL render layers.
    // Bevy limits directional lights to 10 — one per slot would exceed this.
    {
        let all_layers: Vec<usize> = (1..=(MAX_AVATAR_SLOTS as usize)).collect();
        commands.spawn((
            DirectionalLight {
                illuminance: 12000.0,
                shadows_enabled: false,
                ..default()
            },
            // Light from camera direction (negative Z) to illuminate faces
            Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.4, std::f32::consts::PI, 0.0)),
            RenderLayers::from_layers(&all_layers),
        ));
    }

    for slot in 0..MAX_AVATAR_SLOTS {
        let layer = RenderLayers::layer((slot + 1) as usize);

        // Create render target texture
        let size = Extent3d {
            width: AVATAR_WIDTH,
            height: AVATAR_HEIGHT,
            depth_or_array_layers: 1,
        };
        let mut rt_image = Image::new_fill(
            size,
            TextureDimension::D2,
            &[26, 26, 46, 255], // Dark background matching our theme
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        );
        rt_image.texture_descriptor.usage =
            TextureUsages::RENDER_ATTACHMENT
            | TextureUsages::COPY_SRC      // Required for GPU readback
            | TextureUsages::COPY_DST      // Required for clear operations
            | TextureUsages::TEXTURE_BINDING;
        let rt_handle = images.add(rt_image);

        // Camera — starts inactive, activated when a model is loaded
        // Bevy 0.18: RenderTarget is a separate component (no longer a Camera field)
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
                // VRM models face -Z direction. Camera centered on face.
                // Initial position uses reference values; animate_idle() adjusts
                // dynamically once the head bone is discovered.
                Transform::from_xyz(0.0, REFERENCE_HEAD_Y, REFERENCE_CAMERA_Z)
                    .looking_at(Vec3::new(0.0, REFERENCE_HEAD_Y - 0.02, 0.0), Vec3::Y),
                layer.clone(),
                AvatarSlotId(slot),
            ))
            .id();

        // No per-slot directional light — shared light above handles all layers.
        // This avoids Bevy's 10 directional light limit.

        // GPU readback — fires ReadbackComplete observer every rendered frame.
        // ReadbackComplete derefs to Vec<u8> containing raw pixel data in the
        // render target's texture format (Rgba8UnormSrgb → RGBA bytes).
        let slot_id = slot;
        let readback_entity = spawn_readback_entity(&mut commands, rt_handle.clone(), slot_id);

        registry.slots.insert(
            slot,
            SlotState {
                camera_entity,
                _readback_entity: readback_entity,
                scene_entity: None,
                _render_target: rt_handle,
                active: false,
                gltf_handle: None,
                model_path: None,
            },
        );
    }

    clog_info!(
        "🎨 Bevy renderer ready: {} slots × {}×{} @{}fps",
        MAX_AVATAR_SLOTS, AVATAR_WIDTH, AVATAR_HEIGHT, AVATAR_FPS
    );
}

/// Monitor load states of all pending asset handles.
/// Logs transitions to Loaded or Failed, with error details on failure.
/// Also tracks the parent Gltf asset separately from the Scene sub-asset
/// to distinguish "glTF parsing failed" from "scene extraction failed".
fn monitor_load_states(
    asset_server: Res<AssetServer>,
    mut pending: ResMut<PendingLoads>,
    gltf_assets: Res<Assets<bevy::gltf::Gltf>>,
) {
    // Log cwd once
    static CWD_LOGGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if !CWD_LOGGED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        if let Ok(cwd) = std::env::current_dir() {
            clog_info!("🎨 Asset server cwd: {:?}", cwd);
        }
        let test_path = "models/avatars/vroid-female-base.glb";
        clog_info!("🎨 File check '{}': exists={}", test_path, std::path::Path::new(test_path).exists());
    }

    // Check parent Gltf handles (the .glb file itself)
    for entry in pending.gltf_handles.iter_mut() {
        if entry.logged_final { continue; }
        match asset_server.load_state(entry.handle.id()) {
            LoadState::Loaded => {
                // Gltf parsed successfully — inspect what scenes it contains
                if let Some(gltf) = gltf_assets.get(entry.handle.id()) {
                    let named: Vec<&Box<str>> = gltf.named_scenes.keys().collect();
                    clog_info!(
                        "🎨 ✅ Gltf LOADED slot {}: {} — {} scenes, named: {:?}",
                        entry.slot, entry.path, gltf.scenes.len(), named,
                    );
                } else {
                    clog_info!(
                        "🎨 ✅ Gltf LOADED slot {}: {} (not yet in Assets<Gltf>)",
                        entry.slot, entry.path
                    );
                }
                entry.logged_final = true;
            }
            LoadState::Failed(ref err) => {
                clog_warn!(
                    "🎨 ❌ Gltf FAILED slot {}: {} — error: {:?}",
                    entry.slot, entry.path, err
                );
                entry.logged_final = true;
            }
            _ => {
                // Log periodically while still loading
                static GLTF_LOADING_TICKS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
                let tick = GLTF_LOADING_TICKS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if tick == 30 || tick == 150 || tick == 300 {
                    bevy_debug(&format!("⏳ Gltf still loading slot {}: {} (tick {})", entry.slot, entry.path, tick));
                }
            }
        }
    }

    // Check Scene sub-asset handles (#Scene0)
    for entry in pending.scene_handles.iter_mut() {
        if entry.logged_final { continue; }
        match asset_server.load_state(entry.handle.id()) {
            LoadState::Loaded => {
                clog_info!("🎨 ✅ Scene LOADED slot {}: {}", entry.slot, entry.path);
                entry.logged_final = true;
            }
            LoadState::Failed(ref err) => {
                clog_warn!(
                    "🎨 ❌ Scene FAILED slot {}: {} — error: {:?}",
                    entry.slot, entry.path, err
                );
                entry.logged_final = true;
            }
            _ => {
                static SCENE_LOADING_TICKS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
                let tick = SCENE_LOADING_TICKS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if tick == 30 || tick == 150 || tick == 300 {
                    bevy_debug(&format!("⏳ Scene still loading slot {}: {} (tick {})", entry.slot, entry.path, tick));
                }
            }
        }
    }
}

/// Signal to the main thread that Bevy has finished initializing.
fn signal_ready(flag: Res<ReadyFlag>) {
    flag.0.store(true, std::sync::atomic::Ordering::Release);
    bevy_debug("signal_ready: Bevy startup systems completed, flag set to true");
}

/// Re-insert `Readback` on entities that lost it after `ReadbackComplete`.
///
/// Bevy's `Readback` is one-shot: the render pipeline removes the component after
/// the GPU readback completes. This system runs every frame and re-inserts the
/// `Readback` component on readback entities that no longer have it, enabling
/// continuous frame delivery at ~half the Bevy loop rate (every other frame).
///
/// Uses `ReadbackMarker` to distinguish readback entities from cameras/scenes.
fn ensure_continuous_readback(
    query: Query<(Entity, &AvatarSlotId), (With<ReadbackMarker>, Without<Readback>)>,
    registry: Res<SlotRegistry>,
    mut commands: Commands,
) {
    for (entity, slot_id) in &query {
        if let Some(state) = registry.slots.get(&slot_id.0) {
            commands.entity(entity).insert(
                Readback::texture(state._render_target.clone())
            );
        }
    }
}

/// Process commands from the main application.
fn process_commands(
    command_channel: Res<CommandChannel>,
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut registry: ResMut<SlotRegistry>,
    mut cameras: Query<&mut Camera>,
    mut pending: ResMut<PendingLoads>,
    mut mouth_weights: ResMut<MouthWeights>,
    mut health: ResMut<SlotHealthStatus>,
) {
    while let Ok(cmd) = command_channel.0.try_recv() {
        match cmd {
            AvatarCommand::Load {
                slot,
                model_path,
                display_name,
                identity,
            } => {
                // Record identity + model path for health check logging
                health.identities.insert(slot, identity.clone());
                health.model_paths.insert(slot, model_path.clone());

                if let Some(state) = registry.slots.get_mut(&slot) {
                    let layer = RenderLayers::layer((slot + 1) as usize);

                    // Remove previous scene if any
                    if let Some(old_entity) = state.scene_entity.take() {
                        commands.entity(old_entity).despawn();
                    }

                    state.gltf_handle = None;
                    state.model_path = Some(model_path.clone());

                    // Load via Bevy's standard glTF loader.
                    // VRM 0.x files are binary glTF — Bevy's GltfPlugin only registers
                    // for .gltf/.glb extensions. We use .glb symlinks so asset server
                    // recognizes the format.
                    let load_path = if model_path.ends_with(".vrm") {
                        model_path.replacen(".vrm", ".glb", 1)
                    } else {
                        model_path.clone()
                    };

                    let asset_path = format!("{}#Scene0", load_path);
                    let scene_handle: Handle<Scene> = asset_server.load(&asset_path);
                    let gltf_handle: Handle<bevy::gltf::Gltf> = asset_server.load(&load_path);
                    clog_info!("🎨 Slot {}: loading '{}' from {}", slot, display_name, load_path);
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
                    let scene_entity = commands.spawn((
                        SceneRoot(scene_handle),
                        Transform::default(),
                        layer.clone(),
                        AvatarSlotId(slot),
                    )).id();

                    // SceneInstanceReady fires once the scene is fully spawned.
                    // Propagate RenderLayers to all descendants (Bevy doesn't inherit them).
                    let layer_for_observer = layer;
                    let slot_for_observer = slot;
                    commands.entity(scene_entity).observe(
                        move |
                            event: On<SceneInstanceReady>,
                            children_query: Query<&Children>,
                            names: Query<&Name>,
                            mut transforms: Query<&mut Transform>,
                            mut cmds: Commands,
                            mut bone_registry: ResMut<BoneRegistry>,
                        | {
                            let root = event.entity;
                            let child_count = count_descendants(root, &children_query);
                            propagate_render_layers(root, &layer_for_observer, &children_query, &mut cmds);
                            dump_bone_names(root, &children_query, &names);
                            fix_tpose_arms(root, &children_query, &names, &mut transforms);
                            discover_upper_body_bones(root, slot_for_observer, &children_query, &names, &transforms, &mut bone_registry);
                            clog_info!("🎨 SceneInstanceReady: slot {}, entity {:?}, {} descendants — render layers propagated", slot_for_observer, root, child_count);
                        }
                    );

                    state.scene_entity = Some(scene_entity);
                    state.active = true;

                    // Activate camera
                    if let Ok(mut camera) = cameras.get_mut(state.camera_entity) {
                        camera.is_active = true;
                    }

                    clog_info!(
                        "🎨 Slot {}: loaded '{}' from {}",
                        slot, display_name, load_path
                    );
                }
            }
            AvatarCommand::Unload { slot } => {
                if let Some(state) = registry.slots.get_mut(&slot) {
                    if let Some(entity) = state.scene_entity.take() {
                        commands.entity(entity).despawn();
                    }
                    state.active = false;
                    state.gltf_handle = None;
                    state.model_path = None;
                    if let Ok(mut camera) = cameras.get_mut(state.camera_entity) {
                        camera.is_active = false;
                    }
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
                mouth_weights.weights.insert(slot, weight);
            }
            AvatarCommand::Resize { slot, width, height } => {
                // TODO: Implement render target resize once Bevy readback resource
                // borrowing is resolved. For now, all slots render at AVATAR_WIDTH×AVATAR_HEIGHT.
                clog_info!("🎨 Slot {}: resize requested to {}x{} (not yet implemented)", slot, width, height);
            }
            AvatarCommand::Shutdown => {
                clog_info!("🎨 Bevy renderer shutting down");
                // Don't process::exit — that kills the entire Rust worker.
                // Just stop processing commands. The Bevy loop continues but
                // inactive cameras render nothing. In practice, shutdown means
                // the process is ending and the thread will be joined by OS.
                return;
            }
        }
    }
}

/// Reference head Y for VRM standard models (~1.50m).
/// Camera Z distance is scaled proportionally from this baseline.
const REFERENCE_HEAD_Y: f32 = 1.50;

/// Baseline camera Z distance for the reference head height.
/// At this distance with Bevy's default ~45° vertical FOV, a standard VRM
/// face fills roughly 60% of the frame vertically — good head+shoulders framing.
const REFERENCE_CAMERA_Z: f32 = -0.55;

/// Compute camera Z distance from head world-Y position.
///
/// Simple proportional scaling: smaller models (lower head Y) get a closer camera.
/// This is an approximation of the full projection math:
///   apparent_size = object_size / distance * focal_length
/// Since all VRM models have similar head-to-body proportions, scaling Z
/// linearly with head_y / reference_head_y gives correct framing.
fn camera_z_for_head(head_y: f32) -> f32 {
    // Scale factor: how big is this model relative to the VRM standard?
    let scale = (head_y / REFERENCE_HEAD_Y).clamp(0.5, 2.0);
    // Closer for smaller models, farther for larger — proportional
    REFERENCE_CAMERA_Z * scale
}

/// Idle animation — gentle camera sway + head-targeted framing.
/// Uses discovered head bone world position to center camera on face
/// and dynamically adjusts Z distance based on model proportions.
fn animate_idle(
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
            let t = time.elapsed_secs() + *slot as f32 * 0.7; // Phase offset per slot
            let sway_x = (t * 0.3).sin() * 0.02;
            let sway_y = (t * 0.2).cos() * 0.01;

            // Dynamic head tracking: center camera on face (eyes), not skull base.
            // Head bone is at base of skull; eyes are ~0.06 above that.
            // Camera Z distance scales with model size (simple proportional trig).
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
            // Re-orient to look at head position (with sway applied)
            let look_target = Vec3::new(0.0, look_y, 0.0);
            *transform = transform.looking_at(look_target, Vec3::Y);
        }
    }
}

// ============================================================================
// Animation Systems — Morph Targets, Blinking, Breathing, Speaking
// ============================================================================

/// Discover morph target indices from loaded mesh assets.
/// Runs every frame but only acts on slots that haven't been discovered yet.
/// Morph target names are stored on the Mesh asset via set_morph_target_names()
/// during glTF loading — accessed via MorphWeights::first_mesh().
fn discover_morph_targets(
    registry: Res<SlotRegistry>,
    meshes: Res<Assets<Mesh>>,
    morph_query: Query<(Entity, &MorphWeights)>,
    children_query: Query<&Children>,
    mut morph_targets: ResMut<SlotMorphTargets>,
) {
    for (slot, state) in &registry.slots {
        // Skip if already discovered or no scene loaded
        if morph_targets.layouts.contains_key(slot) || !state.active {
            continue;
        }
        let scene_entity = match state.scene_entity {
            Some(e) => e,
            None => continue,
        };

        // Find the first entity with MorphWeights in this slot's scene hierarchy
        let morph_entity = find_morph_entity(scene_entity, &children_query, &morph_query);
        let morph_entity = match morph_entity {
            Some(e) => e,
            None => continue,
        };

        // Get morph target names from the Mesh asset via MorphWeights::first_mesh()
        let mesh_names: Vec<String> = morph_query.get(morph_entity).ok()
            .and_then(|(_, weights)| weights.first_mesh())
            .and_then(|mesh_handle| meshes.get(mesh_handle))
            .and_then(|mesh| mesh.morph_target_names())
            .map(|names| names.to_vec())
            .unwrap_or_default();

        // Build name → index mapping.
        // First try standard glTF morph target names from the Mesh asset.
        // If empty (VRM files store names in VRM extension, not glTF targetNames),
        // fall back to parsing the VRM extension from the .glb file directly.
        let mut mouth_open_index = None;
        let mut blink_index = None;
        let mut blink_left_index = None;
        let mut blink_right_index = None;

        if !mesh_names.is_empty() {
            // Standard glTF + VRoid naming conventions — match by name.
            // VRoid Studio uses "Fcl_" prefix: Fcl_MTH_A (mouth), Fcl_EYE_Close (blink), etc.
            // CRITICAL: Use ends_with, not contains, to avoid "mth_angry" matching before "mth_a".
            for (i, name) in mesh_names.iter().enumerate() {
                let lower = name.to_lowercase();
                if mouth_open_index.is_none() && (
                    lower == "aa" || lower == "a" ||
                    lower.ends_with("_mth_a") || lower.ends_with("mth_a") ||
                    lower.ends_with("_v_aa") || lower == "v_aa" ||
                    lower.ends_with("mouth_open") || lower.ends_with("jawopen") ||
                    lower == "fcl_mth_a"
                ) {
                    mouth_open_index = Some(i);
                }
                if blink_index.is_none() && (
                    lower == "blink" ||
                    lower == "fcl_eye_close" ||
                    (lower.contains("eye_close") && !lower.contains("_l") && !lower.contains("_r") && !lower.contains("left") && !lower.contains("right")) ||
                    lower == "vrc.blink"
                ) {
                    blink_index = Some(i);
                }
                if blink_left_index.is_none() && (
                    lower == "blinkleft" || lower == "blink_l" ||
                    lower == "fcl_eye_close_l" ||
                    lower.contains("eye_close_l") || lower.contains("eye_close_left")
                ) {
                    blink_left_index = Some(i);
                }
                if blink_right_index.is_none() && (
                    lower == "blinkright" || lower == "blink_r" ||
                    lower == "fcl_eye_close_r" ||
                    lower.contains("eye_close_r") || lower.contains("eye_close_right")
                ) {
                    blink_right_index = Some(i);
                }
            }
        } else if let Some(model_path) = &state.model_path {
            // VRM path — parse blend shape groups from the VRM extension in the .glb file
            if let Some(vrm_shapes) = parse_vrm_blend_shapes(model_path) {
                for shape in &vrm_shapes {
                    let preset = shape.preset_name.to_lowercase();
                    if mouth_open_index.is_none() && (preset == "a" || preset == "aa") {
                        // VRM "a" preset = mouth open. Use first bind's morph target index.
                        if let Some(bind) = shape.binds.first() {
                            mouth_open_index = Some(bind.index);
                        }
                    }
                    if blink_index.is_none() && preset == "blink" {
                        if let Some(bind) = shape.binds.first() {
                            blink_index = Some(bind.index);
                        }
                        // Some VRM models bind L+R separately under "blink"
                        if shape.binds.len() >= 2 {
                            blink_left_index = Some(shape.binds[0].index);
                            blink_right_index = Some(shape.binds[1].index);
                        }
                    }
                    if blink_left_index.is_none() && (preset == "blink_l" || preset == "blinkleft") {
                        if let Some(bind) = shape.binds.first() {
                            blink_left_index = Some(bind.index);
                        }
                    }
                    if blink_right_index.is_none() && (preset == "blink_r" || preset == "blinkright") {
                        if let Some(bind) = shape.binds.first() {
                            blink_right_index = Some(bind.index);
                        }
                    }
                }
                clog_info!("🎨 VRM blend shapes slot {}: {} groups parsed", slot, vrm_shapes.len());
            }
        }

        let weight_count = morph_query.get(morph_entity).ok()
            .map(|(_, w)| w.weights().len())
            .unwrap_or(0);

        clog_info!(
            "🎨 Morph discovery slot {}: {} weights, {} names, mouth={:?}, blink={:?}, blink_l={:?}, blink_r={:?}",
            slot, weight_count, mesh_names.len(), mouth_open_index, blink_index, blink_left_index, blink_right_index,
        );

        morph_targets.layouts.insert(*slot, MorphTargetLayout {
            mesh_entity: morph_entity,
            mouth_open_index,
            blink_index,
            blink_left_index,
            blink_right_index,
        });
    }
}

/// Animate mouth morph targets + subtle head nod when a slot is speaking.
/// Uses audio-amplitude MouthWeights when available for responsive lip sync,
/// falling back to sine oscillation when no amplitude data is present.
fn animate_speaking(
    time: Res<Time>,
    speaking_query: Query<&AvatarSlotId, With<Speaking>>,
    morph_targets: Res<SlotMorphTargets>,
    bone_registry: Res<BoneRegistry>,
    mouth_weights: Res<MouthWeights>,
    mut morph_weights: Query<&mut MorphWeights>,
    mut transforms: Query<&mut Transform>,
) {
    // Collect which slots are currently speaking
    let speaking_slots: HashSet<u8> = speaking_query.iter().map(|id| id.0).collect();

    for (slot, layout) in &morph_targets.layouts {
        let is_speaking = speaking_slots.contains(slot);

        if let Ok(mut weights) = morph_weights.get_mut(layout.mesh_entity) {
            let w = weights.weights_mut();
            if is_speaking {
                // Prefer amplitude-based weight from audio pipeline when available
                let mouth_weight = if let Some(&amplitude) = mouth_weights.weights.get(slot) {
                    // Amplitude already normalized to 0.0-1.0 by the sender.
                    // Full range for clearly visible mouth movement.
                    amplitude.clamp(0.0, 1.0)
                } else {
                    // Fallback: sine oscillation (no amplitude data)
                    let t = time.elapsed_secs();
                    ((t * 3.0 * std::f32::consts::TAU).sin() * 0.4 + 0.5).clamp(0.1, 0.9)
                };
                if let Some(idx) = layout.mouth_open_index {
                    if idx < w.len() {
                        w[idx] = mouth_weight;
                    }
                }
            } else {
                // Not speaking — close mouth
                if let Some(idx) = layout.mouth_open_index {
                    if idx < w.len() {
                        w[idx] = 0.0;
                    }
                }
            }
        }

        // Subtle head nod during speech (±2 degrees pitch oscillation at ~1.5Hz).
        // Composes delta onto rest rotation — never replaces the bone's bind pose.
        if let Some(slot_bones) = bone_registry.slots.get(slot) {
            if let Some(ref head) = slot_bones.head {
                if let Ok(mut transform) = transforms.get_mut(head.entity) {
                    if is_speaking {
                        let t = time.elapsed_secs() + *slot as f32 * 1.3;
                        let nod = (t * 1.5 * std::f32::consts::TAU).sin() * 0.035; // ~2 degrees
                        let tilt = (t * 0.9).sin() * 0.02; // slight lateral tilt
                        let delta = Quat::from_euler(EulerRot::XYZ, nod, 0.0, tilt);
                        transform.rotation = head.rest_rotation * delta;
                    } else {
                        // Return to rest rotation (smoothly)
                        transform.rotation = transform.rotation.slerp(head.rest_rotation, 0.3);
                    }
                }
            }
        }
    }
}

/// Animate random eye blinks across all active avatar slots.
fn animate_blinking(
    time: Res<Time>,
    morph_targets: Res<SlotMorphTargets>,
    mut blink_state: ResMut<BlinkState>,
    mut morph_weights: Query<&mut MorphWeights>,
) {
    let elapsed = time.elapsed_secs();

    for (slot, layout) in &morph_targets.layouts {
        // Initialize blink state if not present
        let state = blink_state.slots.entry(*slot).or_insert_with(|| {
            // Randomize initial blink time per slot using slot index as seed
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

        // Check if it's time to start a new blink
        if state.blink_frames_remaining == 0 && elapsed >= state.next_blink_time {
            state.blink_frames_remaining = 2; // 2 frames at 5fps ≈ 400ms blink
            // Next blink in 2-6 seconds (pseudo-random using elapsed time)
            let pseudo_rand = ((elapsed * 1000.0 + *slot as f32 * 137.0) % 4000.0) / 1000.0;
            state.next_blink_time = elapsed + 2.0 + pseudo_rand;
        }

        if let Ok(mut weights) = morph_weights.get_mut(layout.mesh_entity) {
            let w = weights.weights_mut();
            let blink_weight = if state.blink_frames_remaining > 0 {
                state.blink_frames_remaining -= 1;
                1.0 // Eyes closed
            } else {
                0.0 // Eyes open
            };

            // Apply to unified blink or L/R separately
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

/// Idle gesture system — subtle upper-body micro-movements for "alive, not static" feel.
///
/// Uses layered multi-frequency oscillators (like Fourier decomposition) to create
/// organic, non-repetitive motion. Each slot gets a unique phase offset so avatars
/// don't move in sync. Gestures pause when the avatar is speaking (speaking head nod
/// takes priority).
///
/// Animations:
/// 1. Neck micro-tilt — slow lateral and forward head movement
/// 2. Shoulder micro-shifts — tiny alternating up/down (breathing-like)
/// 3. Weight shift — spine lateral tilt (very slow, barely perceptible)
fn animate_idle_gestures(
    time: Res<Time>,
    registry: Res<SlotRegistry>,
    bone_registry: Res<BoneRegistry>,
    speaking_query: Query<&AvatarSlotId, With<Speaking>>,
    mut gesture_state: ResMut<IdleGestureState>,
    mut transforms: Query<&mut Transform>,
) {
    let speaking_slots: HashSet<u8> = speaking_query.iter().map(|id| id.0).collect();

    for (slot, state) in &registry.slots {
        if !state.active {
            continue;
        }

        // Skip gestures while speaking — head nod takes priority
        if speaking_slots.contains(slot) {
            continue;
        }

        let slot_bones = match bone_registry.slots.get(slot) {
            Some(b) => b,
            None => continue,
        };

        // Initialize gesture state with unique phase offset per slot
        let gesture = gesture_state.slots.entry(*slot).or_insert_with(|| {
            SlotGestureState {
                phase: *slot as f32 * 2.37, // Golden ratio-ish offset for visual variety
            }
        });

        let t = time.elapsed_secs() + gesture.phase;

        // 1. Neck micro-tilt — layered frequencies for organic motion.
        //    COMPOSES delta rotation onto the bone's rest rotation (not replacing it!)
        //    Combines 3 sine waves at incommensurate frequencies (non-repeating pattern)
        if let Some(ref neck) = slot_bones.neck {
            if let Ok(mut transform) = transforms.get_mut(neck.entity) {
                let tilt_x = (t * 0.15).sin() * 0.03           // Very slow nod
                    + (t * 0.23).cos() * 0.02                   // Slower lateral component
                    + (t * 0.37).sin() * 0.01;                  // Subtle high-freq detail
                let tilt_z = (t * 0.12).cos() * 0.025           // Slow lateral head tilt
                    + (t * 0.31).sin() * 0.015;                 // Detail frequency
                let turn_y = (t * 0.08).sin() * 0.02;           // Very slow head turn

                let delta = Quat::from_euler(EulerRot::XYZ, tilt_x, turn_y, tilt_z);
                transform.rotation = neck.rest_rotation * delta;
            }
        }

        // 2. Shoulder micro-shifts — opposite phase for natural weight distribution.
        //    Adds tiny Y delta to the bone's actual local rest translation.
        if let Some(ref left_shoulder) = slot_bones.left_shoulder {
            if let Ok(mut transform) = transforms.get_mut(left_shoulder.entity) {
                let shift = (t * 0.4).sin() * 0.002             // Primary breathing frequency
                    + (t * 0.17).cos() * 0.001;                 // Slow drift
                transform.translation.y = left_shoulder.rest_translation.y + shift;
            }
        }
        if let Some(ref right_shoulder) = slot_bones.right_shoulder {
            if let Ok(mut transform) = transforms.get_mut(right_shoulder.entity) {
                // Opposite phase from left shoulder (natural body mechanics)
                let shift = (t * 0.4 + std::f32::consts::PI).sin() * 0.002
                    + (t * 0.17 + 1.0).cos() * 0.001;
                transform.translation.y = right_shoulder.rest_translation.y + shift;
            }
        }
    }
}

/// Subtle breathing animation — gentle spine/chest oscillation + weight shift.
fn animate_breathing(
    time: Res<Time>,
    registry: Res<SlotRegistry>,
    children_query: Query<&Children>,
    names: Query<&Name>,
    mut transforms: Query<&mut Transform>,
) {
    for (slot, state) in &registry.slots {
        if !state.active {
            continue;
        }
        let scene_entity = match state.scene_entity {
            Some(e) => e,
            None => continue,
        };

        // Find spine bone and apply subtle vertical oscillation + lateral weight shift
        if let Some(spine_entity) = find_bone_by_name(scene_entity, &children_query, &names, &["J_Bip_C_Spine", "mixamorig:Spine", "Spine"]) {
            if let Ok(mut transform) = transforms.get_mut(spine_entity) {
                let t = time.elapsed_secs() + *slot as f32 * 1.1; // Phase offset
                let breath = (t * 0.8 * std::f32::consts::TAU).sin() * 0.003;
                // Breathing: Y scale variation (chest expanding)
                transform.scale.y = 1.0 + breath;
                // Weight shift: very slow lateral sway (barely perceptible)
                let sway = (t * 0.12).sin() * 0.01;
                transform.rotation = Quat::from_rotation_z(sway);
            }
        }
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Count all descendant entities recursively (for debug logging).
fn count_descendants(entity: Entity, children: &Query<&Children>) -> usize {
    let mut count = 0;
    if let Ok(child_list) = children.get(entity) {
        for child in child_list.iter() {
            count += 1 + count_descendants(child, children);
        }
    }
    count
}

/// Dump all named entities in a scene hierarchy (for debugging bone name conventions).
fn dump_bone_names(entity: Entity, children: &Query<&Children>, names: &Query<&Name>) {
    let mut bone_names = Vec::new();
    collect_names(entity, children, names, &mut bone_names);
    // Only log names containing "arm" or "Arm" to reduce noise
    let arm_names: Vec<&str> = bone_names.iter()
        .filter(|n| n.to_lowercase().contains("arm"))
        .map(|s| s.as_str())
        .collect();
    if !arm_names.is_empty() {
        bevy_debug(&format!("Arm bones found: {:?}", arm_names));
    } else {
        // Log all names if no arm bones found (different naming convention)
        bevy_debug(&format!("No 'arm' bones found. All {} named entities: {:?}",
            bone_names.len(),
            bone_names.iter().take(50).collect::<Vec<_>>()));
    }
}

fn collect_names(entity: Entity, children: &Query<&Children>, names: &Query<&Name>, out: &mut Vec<String>) {
    if let Ok(name) = names.get(entity) {
        out.push(name.as_str().to_string());
    }
    if let Ok(child_list) = children.get(entity) {
        for child in child_list.iter() {
            collect_names(child, children, names, out);
        }
    }
}

/// Fix T-pose by rotating arm bones to a natural resting position.
/// VRM models load in T-pose (arms straight out). This finds arm bone entities
/// by name and applies rotation to bring them to a relaxed pose.
///
/// VRM bone naming conventions:
/// - VRoid Studio: "J_Bip_L_UpperArm", "J_Bip_R_UpperArm", "J_Bip_L_LowerArm", etc.
/// - 100Avatars: "LeftUpperArm", "RightUpperArm", "LeftLowerArm", etc.
/// - Standard glTF: varies, but usually contains "Arm" in the name
fn fix_tpose_arms(
    entity: Entity,
    children: &Query<&Children>,
    names: &Query<&Name>,
    transforms: &mut Query<&mut Transform>,
) {
    // Collect bone adjustments needed (entity, rotation)
    let mut adjustments = Vec::new();
    collect_arm_adjustments(entity, children, names, &mut adjustments);

    for (bone_entity, rotation) in adjustments {
        if let Ok(mut transform) = transforms.get_mut(bone_entity) {
            transform.rotation = transform.rotation * rotation;
        }
    }
}

/// Recursively search for arm bones and collect transform adjustments.
fn collect_arm_adjustments(
    entity: Entity,
    children: &Query<&Children>,
    names: &Query<&Name>,
    adjustments: &mut Vec<(Entity, Quat)>,
) {
    if let Ok(name) = names.get(entity) {
        let name_str = name.as_str();
        let _name_lower = name_str.to_lowercase();

        // Detect upper arm bones and rotate downward (~65 degrees).
        // VRoid: "J_Bip_L_UpperArm", "J_Sec_L_UpperArm"
        // Mixamo: "mixamorig:LeftArm", "vis_char_056:mixamorig:LeftArm"
        let is_left_upper = name_str.contains("J_Bip_L_UpperArm")
            || name_str.contains("J_Sec_L_UpperArm")
            || (name_str.contains("mixamorig:LeftArm") && !name_str.contains("ForeArm"));
        let is_right_upper = name_str.contains("J_Bip_R_UpperArm")
            || name_str.contains("J_Sec_R_UpperArm")
            || (name_str.contains("mixamorig:RightArm") && !name_str.contains("ForeArm"));

        // Detect lower arm / forearm bones — slight bend for natural look.
        // VRoid: "J_Bip_L_LowerArm"
        // Mixamo: "mixamorig:LeftForeArm"
        let is_left_lower = name_str.contains("J_Bip_L_LowerArm")
            || name_str.contains("J_Sec_L_LowerArm")
            || name_str.contains("mixamorig:LeftForeArm");
        let is_right_lower = name_str.contains("J_Bip_R_LowerArm")
            || name_str.contains("J_Sec_R_LowerArm")
            || name_str.contains("mixamorig:RightForeArm");

        if is_left_upper {
            // Rotate left upper arm down ~65 degrees (positive Z in VRM local space)
            adjustments.push((entity, Quat::from_rotation_z(1.13)));
            bevy_debug(&format!("T-pose fix: left upper arm '{}' → rotate Z +65°", name_str));
        } else if is_right_upper {
            // Rotate right upper arm down ~65 degrees (negative Z in VRM local space)
            adjustments.push((entity, Quat::from_rotation_z(-1.13)));
            bevy_debug(&format!("T-pose fix: right upper arm '{}' → rotate Z -65°", name_str));
        } else if is_left_lower {
            // Slight bend at elbow for natural look (~15 degrees)
            adjustments.push((entity, Quat::from_rotation_z(0.26)));
            bevy_debug(&format!("T-pose fix: left lower arm '{}' → rotate Z +15°", name_str));
        } else if is_right_lower {
            adjustments.push((entity, Quat::from_rotation_z(-0.26)));
            bevy_debug(&format!("T-pose fix: right lower arm '{}' → rotate Z -15°", name_str));
        }
    }

    if let Ok(child_list) = children.get(entity) {
        for child in child_list.iter() {
            collect_arm_adjustments(child, children, names, adjustments);
        }
    }
}

/// Recursively propagate RenderLayers to all descendant entities of a scene root.
/// Bevy's RenderLayers are per-entity (not inherited), so every mesh, armature node,
/// and visual child must explicitly have the correct layer or the camera won't see it.
fn propagate_render_layers(
    entity: Entity,
    layer: &RenderLayers,
    children: &Query<&Children>,
    commands: &mut Commands,
) {
    if let Ok(child_list) = children.get(entity) {
        for child in child_list.iter() {
            commands.entity(child).insert(layer.clone());
            propagate_render_layers(child, layer, children, commands);
        }
    }
}

// ============================================================================
// VRM Version Detection
// ============================================================================

// ============================================================================
// VRM Extension Parsing
// ============================================================================

/// A VRM blend shape group — maps a named expression to morph target indices.
struct VrmBlendShape {
    #[allow(dead_code)]
    name: String,
    preset_name: String,
    binds: Vec<VrmBlendShapeBind>,
}

/// A single morph target binding within a VRM blend shape group.
struct VrmBlendShapeBind {
    #[allow(dead_code)]
    mesh: usize,
    index: usize,
    #[allow(dead_code)]
    weight: f32,
}

/// Parse VRM blend shape groups from a .glb file's JSON chunk.
/// Supports both VRM 0.x (extensions.VRM.blendShapeMaster) and
/// VRM 1.0 (extensions.VRMC_vrm.expressions.preset).
/// Returns None if the file can't be read or doesn't have VRM extensions.
fn parse_vrm_blend_shapes(glb_path: &str) -> Option<Vec<VrmBlendShape>> {
    let root = read_glb_json(glb_path)?;

    // Try VRM 0.x first (extensions.VRM.blendShapeMaster.blendShapeGroups)
    if let Some(shapes) = parse_vrm0x_blend_shapes(&root, glb_path) {
        return Some(shapes);
    }

    // Try VRM 1.0 (extensions.VRMC_vrm.expressions.preset)
    if let Some(shapes) = parse_vrmc_expressions(&root, glb_path) {
        return Some(shapes);
    }

    None
}

/// Read the JSON chunk from a .glb file.
fn read_glb_json(glb_path: &str) -> Option<serde_json::Value> {
    use std::io::Read;

    let mut file = std::fs::File::open(glb_path).ok()?;

    // GLB header: magic(4) + version(4) + length(4)
    let mut header = [0u8; 12];
    file.read_exact(&mut header).ok()?;
    let magic = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
    if magic != 0x46546C67 {
        return None;
    }

    // JSON chunk header: length(4) + type(4)
    let mut chunk_header = [0u8; 8];
    file.read_exact(&mut chunk_header).ok()?;
    let chunk_length = u32::from_le_bytes([chunk_header[0], chunk_header[1], chunk_header[2], chunk_header[3]]) as usize;
    let chunk_type = u32::from_le_bytes([chunk_header[4], chunk_header[5], chunk_header[6], chunk_header[7]]);
    if chunk_type != 0x4E4F534A {
        return None;
    }

    let mut json_data = vec![0u8; chunk_length];
    file.read_exact(&mut json_data).ok()?;
    let json_str = std::str::from_utf8(&json_data).ok()?;
    serde_json::from_str(json_str).ok()
}

/// Parse VRM 0.x blend shape groups (extensions.VRM.blendShapeMaster.blendShapeGroups).
/// Used by older VRoid Studio exports and most existing VRM models.
fn parse_vrm0x_blend_shapes(root: &serde_json::Value, glb_path: &str) -> Option<Vec<VrmBlendShape>> {
    let groups = root
        .get("extensions")?
        .get("VRM")?
        .get("blendShapeMaster")?
        .get("blendShapeGroups")?
        .as_array()?;

    let mut shapes = Vec::new();
    for group in groups {
        let name = group.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let preset_name = group.get("presetName").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let binds_arr = group.get("binds").and_then(|v| v.as_array());
        let mut binds = Vec::new();
        if let Some(binds_arr) = binds_arr {
            for bind in binds_arr {
                let mesh = bind.get("mesh").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                let index = bind.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                let weight = bind.get("weight").and_then(|v| v.as_f64()).unwrap_or(100.0) as f32;
                binds.push(VrmBlendShapeBind { mesh, index, weight });
            }
        }
        shapes.push(VrmBlendShape { name, preset_name, binds });
    }

    bevy_debug(&format!("VRM 0.x blend shapes from '{}': {} groups — {:?}",
        glb_path, shapes.len(),
        shapes.iter().map(|s| format!("{}({})[{}binds]", s.name, s.preset_name, s.binds.len())).collect::<Vec<_>>()
    ));

    Some(shapes)
}

/// Parse VRM 1.0 expressions (extensions.VRMC_vrm.expressions.preset).
/// Used by VRoid Hub exports and newer VRoid Studio models.
/// Maps VRMC preset expressions to the same VrmBlendShape format as VRM 0.x.
fn parse_vrmc_expressions(root: &serde_json::Value, glb_path: &str) -> Option<Vec<VrmBlendShape>> {
    let preset = root
        .get("extensions")?
        .get("VRMC_vrm")?
        .get("expressions")?
        .get("preset")?
        .as_object()?;

    let mut shapes = Vec::new();
    for (preset_name, expr) in preset {
        let morph_binds = expr.get("morphTargetBinds").and_then(|v| v.as_array());
        let mut binds = Vec::new();
        if let Some(morph_binds) = morph_binds {
            for bind in morph_binds {
                // VRM 1.0 uses "node" (mesh node index) and "index" (morph target index)
                let mesh = bind.get("node").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                let index = bind.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                let weight = bind.get("weight").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
                binds.push(VrmBlendShapeBind { mesh, index, weight });
            }
        }
        shapes.push(VrmBlendShape {
            name: preset_name.clone(),
            preset_name: preset_name.clone(),
            binds,
        });
    }

    bevy_debug(&format!("VRM 1.0 expressions from '{}': {} presets — {:?}",
        glb_path, shapes.len(),
        shapes.iter().map(|s| format!("{}[{}binds]", s.preset_name, s.binds.len())).collect::<Vec<_>>()
    ));

    Some(shapes)
}

/// Find the first entity with MorphWeights in a scene hierarchy.
fn find_morph_entity(
    root: Entity,
    children: &Query<&Children>,
    morph_query: &Query<(Entity, &MorphWeights)>,
) -> Option<Entity> {
    // Check root itself
    if morph_query.get(root).is_ok() {
        return Some(root);
    }
    // Recurse into children
    if let Ok(child_list) = children.get(root) {
        for child in child_list.iter() {
            if let Some(found) = find_morph_entity(child, children, morph_query) {
                return Some(found);
            }
        }
    }
    None
}

/// Discover upper-body bone entities from scene hierarchy for animation systems.
///
/// Finds head, neck, and shoulder bones using multiple naming conventions:
/// - VRM/VRoid: "J_Bip_C_Head", "J_Bip_C_Neck", "J_Bip_L_Shoulder", etc.
/// - Mixamo: "mixamorig:Head", "mixamorig:Neck", "mixamorig:LeftShoulder", etc.
/// - Generic: "Head", "Neck", "LeftShoulder", etc.
///
/// Note: bone transforms are in LOCAL space. We use estimated world-space heights
/// for camera targeting (walking the full bone chain is complex and model-specific).
fn discover_upper_body_bones(
    root: Entity,
    slot: u8,
    children: &Query<&Children>,
    names: &Query<&Name>,
    transforms: &Query<&mut Transform>,
    bone_registry: &mut ResMut<BoneRegistry>,
) {
    let head_names = ["J_Bip_C_Head", "mixamorig:Head", "Head"];
    let neck_names = ["J_Bip_C_Neck", "mixamorig:Neck", "Neck"];
    let left_shoulder_names = ["J_Bip_L_Shoulder", "mixamorig:LeftShoulder", "LeftShoulder"];
    let right_shoulder_names = ["J_Bip_R_Shoulder", "mixamorig:RightShoulder", "RightShoulder"];

    // Helper: find bone and capture its actual local-space rest transform.
    // This is critical — bone transforms are LOCAL (relative to parent bone),
    // NOT world space. Gesture animations must compose with these rest values.
    let discover = |target_names: &[&str], label: &str| -> Option<BoneInfo> {
        find_bone_by_name(root, children, names, target_names).and_then(|entity| {
            if let Ok(t) = transforms.get(entity) {
                bevy_debug(&format!("{} bone slot {}: entity {:?}, local_pos={:?}, local_rot={:?}",
                    label, slot, entity, t.translation, t.rotation));
                Some(BoneInfo {
                    entity,
                    rest_translation: t.translation,
                    rest_rotation: t.rotation,
                })
            } else {
                bevy_debug(&format!("{} bone slot {}: entity {:?} — no Transform!", label, slot, entity));
                None
            }
        })
    };

    let head = discover(&head_names, "Head");
    let neck = discover(&neck_names, "Neck");
    let left_shoulder = discover(&left_shoulder_names, "L.Shoulder");
    let right_shoulder = discover(&right_shoulder_names, "R.Shoulder");

    let found_count = [&head, &neck, &left_shoulder, &right_shoulder].iter()
        .filter(|b| b.is_some()).count();
    bevy_debug(&format!("Bone discovery slot {}: {}/4 bones found", slot, found_count));

    bone_registry.slots.insert(slot, SlotBones {
        head,
        neck,
        left_shoulder,
        right_shoulder,
    });
}

/// Find a bone entity by matching against a list of known names.
fn find_bone_by_name(
    root: Entity,
    children: &Query<&Children>,
    names: &Query<&Name>,
    target_names: &[&str],
) -> Option<Entity> {
    if let Ok(name) = names.get(root) {
        let name_str = name.as_str();
        for target in target_names {
            if name_str.contains(target) {
                return Some(root);
            }
        }
    }
    if let Ok(child_list) = children.get(root) {
        for child in child_list.iter() {
            if let Some(found) = find_bone_by_name(child, children, names, target_names) {
                return Some(found);
            }
        }
    }
    None
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constants() {
        assert!(MAX_AVATAR_SLOTS >= 14, "Need at least 14 slots for all personas");
        assert_eq!(AVATAR_WIDTH, 640);
        assert_eq!(AVATAR_HEIGHT, 480);
        assert!(AVATAR_FPS >= 1.0 && AVATAR_FPS <= 30.0, "FPS should be reasonable (1-30)");
    }
}
