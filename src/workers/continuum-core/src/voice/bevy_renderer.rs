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
//! Performance: 14 avatars × 320×240 @ 30fps = ~123 MB/s GPU readback.
//! On Apple Silicon (shared memory), this is essentially a memcpy.

use bevy::prelude::*;
use bevy::app::ScheduleRunnerPlugin;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::gpu_readback::{Readback, ReadbackComplete};
use bevy::render::view::RenderLayers;
use bevy::asset::LoadState;
use bevy::scene::SceneInstanceReady;
use crossbeam_channel::{Receiver, Sender};
use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::Duration;
use tracing::{info, warn};

use super::avatar_renderer::RgbaFrame;

/// Debug logging to file — bypasses tracing subscriber issues on background threads.
/// Writes to /tmp/bevy-avatar-debug.log for direct inspection.
fn bevy_debug(msg: &str) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true).append(true)
        .open("/tmp/bevy-avatar-debug.log")
    {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let _ = writeln!(f, "[{}] {}", ts, msg);
    }
}

/// Maximum number of concurrent avatar render slots.
/// 24 supports up to 24 AI personas with 3D avatars simultaneously.
/// At 5fps × 640×480 × 4 bytes = ~6.1 MB/s readback per slot (147 MB/s total).
/// Bevy supports 32 render layers; we use layers 1-24, leaving headroom.
pub const MAX_AVATAR_SLOTS: u8 = 24;

/// Render resolution per avatar.
/// 640×480 balances quality vs GPU readback bandwidth.
const AVATAR_WIDTH: u32 = 640;
const AVATAR_HEIGHT: u32 = 480;

/// Target framerate for avatar rendering.
/// 5fps is sufficient for mostly-static avatars with idle sway.
/// Higher rates (30fps) starve the tokio runtime and cause IPC timeouts.
const AVATAR_FPS: f64 = 5.0;

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
        info!("🎨 Starting Bevy headless avatar renderer ({MAX_AVATAR_SLOTS} slots, {AVATAR_WIDTH}x{AVATAR_HEIGHT} @{AVATAR_FPS}fps)");
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
    },
    /// Remove the model from a render slot.
    Unload { slot: u8 },
    /// Set whether the avatar is currently speaking (for expression animation).
    SetSpeaking { slot: u8, speaking: bool },
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
                info!("🎨 Bevy renderer confirmed ready");
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }

        if !ready.load(std::sync::atomic::Ordering::Acquire) {
            bevy_debug("WARN: Bevy did not report ready within 5s — may have failed to init GPU");
            warn!("🎨 Bevy renderer did not report ready within 5s — may have failed to init GPU");
        } else {
            bevy_debug("Bevy confirmed READY");
        }

        Self {
            command_tx,
            frame_receivers,
            ready,
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
    pub fn load_model(&self, slot: u8, model_path: &str, display_name: &str) {
        if slot >= MAX_AVATAR_SLOTS {
            warn!("Avatar slot {slot} exceeds max {MAX_AVATAR_SLOTS}");
            return;
        }
        let _ = self.command_tx.send(AvatarCommand::Load {
            slot,
            model_path: model_path.to_string(),
            display_name: display_name.to_string(),
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
}

/// Component marking which avatar slot an entity belongs to.
/// The inner u8 is the slot index — used by Bevy's ECS query system even though
/// Rust's dead-code analysis doesn't see it read (it's matched via Query filters).
#[derive(Component, Clone, Copy)]
struct AvatarSlotId(#[allow(dead_code)] u8);

/// Component marking an avatar that is currently speaking.
#[derive(Component)]
struct Speaking;

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
        // DefaultPlugins with no window — the official Bevy headless rendering approach.
        // WindowPlugin registers Events<WindowResized> etc. needed by camera_system,
        // but primary_window: None means no actual OS window is created.
        // ExitCondition::DontExit prevents "No windows open, exiting" (Issue #16807).
        .add_plugins(
            DefaultPlugins
                .set(bevy::window::WindowPlugin {
                    primary_window: None,
                    exit_condition: bevy::window::ExitCondition::DontExit,
                    close_when_requested: false,
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
        .add_systems(Startup, (setup_render_slots, signal_ready).chain())
        .add_systems(Update, (process_commands, monitor_load_states, animate_idle))
        .run();
}

/// Create render targets, cameras, and lights for all avatar slots.
fn setup_render_slots(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut registry: ResMut<SlotRegistry>,
    _frame_channels: Res<FrameChannels>,
) {
    // Global ambient light (all layers)
    commands.insert_resource(AmbientLight {
        color: Color::WHITE,
        brightness: 300.0,
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
        let camera_entity = commands
            .spawn((
                Camera3d::default(),
                Camera {
                    order: slot as isize,
                    clear_color: ClearColorConfig::Custom(Color::srgb(0.1, 0.1, 0.18)),
                    target: bevy::render::camera::RenderTarget::Image(rt_handle.clone()),
                    is_active: false,
                    ..default()
                },
                bevy::core_pipeline::tonemapping::Tonemapping::None,
                // Disable MSAA — prevents macOS Metal validation error (Issue #16590)
                bevy::render::view::Msaa::Off,
                // VRM models face -Z direction. Camera must be at negative Z to see
                // the face. Head-and-shoulders framing at ~1.2m distance.
                Transform::from_xyz(0.0, 1.4, -1.2)
                    .looking_at(Vec3::new(0.0, 1.35, 0.0), Vec3::Y),
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
        let readback_entity = commands
            .spawn((
                Readback::texture(rt_handle.clone()),
                AvatarSlotId(slot),
            ))
            .observe(
                move |trigger: Trigger<ReadbackComplete>,
                      channels: Res<FrameChannels>| {
                    let pixel_bytes: &[u8] = trigger.event();

                    // Log first readback frame per slot + periodic updates
                    static FRAME_COUNTS: std::sync::OnceLock<std::sync::Mutex<[u64; 32]>> = std::sync::OnceLock::new();
                    let counts = FRAME_COUNTS.get_or_init(|| std::sync::Mutex::new([0u64; 32]));
                    if let Ok(mut c) = counts.lock() {
                        c[slot_id as usize] += 1;
                        let n = c[slot_id as usize];
                        if n == 1 || n == 10 || n == 100 || n % 1000 == 0 {
                            // Sample CENTER pixels (where 3D content renders, not top-left corner)
                            let center_offset = ((AVATAR_HEIGHT / 2) * AVATAR_WIDTH + AVATAR_WIDTH / 2 - 50) as usize * 4;
                            let sample_end = std::cmp::min(center_offset + 400, pixel_bytes.len());
                            let non_bg = pixel_bytes[center_offset..sample_end].chunks(4)
                                .filter(|px| px.len() >= 3 && !(px[0] < 30 && px[1] < 30 && px[2] < 50))
                                .count();
                            bevy_debug(&format!(
                                "Readback slot {}: frame #{}, {} bytes, non-bg center pixels: {}",
                                slot_id, n, pixel_bytes.len(), non_bg
                            ));
                        }
                    }

                    if let Some(tx) = channels.0.get(slot_id as usize) {
                        let _ = tx.try_send(RgbaFrame {
                            width: AVATAR_WIDTH,
                            height: AVATAR_HEIGHT,
                            data: pixel_bytes.to_vec(),
                        });
                    }
                },
            )
            .id();

        registry.slots.insert(
            slot,
            SlotState {
                camera_entity,
                _readback_entity: readback_entity,
                scene_entity: None,
                _render_target: rt_handle,
                active: false,
            },
        );
    }

    info!(
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
            bevy_debug(&format!("Asset server cwd: {:?}", cwd));
        }
        let test_path = "models/avatars/vroid-female-base.glb";
        bevy_debug(&format!("File check '{}': exists={}", test_path, std::path::Path::new(test_path).exists()));
    }

    // Check parent Gltf handles (the .glb file itself)
    for entry in pending.gltf_handles.iter_mut() {
        if entry.logged_final { continue; }
        match asset_server.load_state(entry.handle.id()) {
            LoadState::Loaded => {
                // Gltf parsed successfully — inspect what scenes it contains
                if let Some(gltf) = gltf_assets.get(entry.handle.id()) {
                    let named: Vec<&Box<str>> = gltf.named_scenes.keys().collect();
                    bevy_debug(&format!(
                        "✅ Gltf LOADED slot {}: {} — {} scenes, named: {:?}, default_scene: {:?}",
                        entry.slot, entry.path, gltf.scenes.len(), named,
                        gltf.default_scene.as_ref().map(|h| format!("{:?}", h.id()))
                    ));
                } else {
                    bevy_debug(&format!(
                        "✅ Gltf LOADED slot {}: {} (not yet in Assets<Gltf>)",
                        entry.slot, entry.path
                    ));
                }
                entry.logged_final = true;
            }
            LoadState::Failed(ref err) => {
                bevy_debug(&format!(
                    "❌ Gltf FAILED slot {}: {} — error: {:?}",
                    entry.slot, entry.path, err
                ));
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
                bevy_debug(&format!("✅ Scene LOADED slot {}: {}", entry.slot, entry.path));
                entry.logged_final = true;
            }
            LoadState::Failed(ref err) => {
                bevy_debug(&format!(
                    "❌ Scene FAILED slot {}: {} — error: {:?}",
                    entry.slot, entry.path, err
                ));
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

/// Process commands from the main application.
fn process_commands(
    command_channel: Res<CommandChannel>,
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut registry: ResMut<SlotRegistry>,
    mut cameras: Query<&mut Camera>,
    mut pending: ResMut<PendingLoads>,
) {
    while let Ok(cmd) = command_channel.0.try_recv() {
        match cmd {
            AvatarCommand::Load {
                slot,
                model_path,
                display_name,
            } => {
                if let Some(state) = registry.slots.get_mut(&slot) {
                    let layer = RenderLayers::layer((slot + 1) as usize);

                    // Remove previous scene if any
                    if let Some(old_entity) = state.scene_entity.take() {
                        commands.entity(old_entity).despawn_recursive();
                    }

                    // Load VRM/glTF model on this slot's render layer.
                    // VRM files are binary glTF but Bevy's GltfPlugin only registers
                    // for .gltf/.glb extensions. We use .glb symlinks (created alongside
                    // the .vrm originals) so the asset server recognizes the format.
                    let load_path = if model_path.ends_with(".vrm") {
                        model_path.replace(".vrm", ".glb")
                    } else {
                        model_path.clone()
                    };
                    let asset_path = format!("{}#Scene0", load_path);

                    // Spawn scene with an observer that propagates RenderLayers
                    // to all descendant entities once the scene finishes loading.
                    // Bevy does NOT inherit RenderLayers from parents — each mesh
                    // entity needs its own RenderLayers or the camera won't see it.
                    bevy_debug(&format!("Load slot {}: path='{}' asset_path='{}'", slot, model_path, asset_path));

                    // Load scene handle AND track parent Gltf handle for diagnostics.
                    // If parent Gltf loads but Scene fails, the label is wrong.
                    // If parent Gltf fails, the file parsing is the issue.
                    let scene_handle: Handle<Scene> = asset_server.load(&asset_path);
                    let gltf_handle: Handle<bevy::gltf::Gltf> = asset_server.load(&load_path);
                    pending.scene_handles.push(PendingLoadEntry {
                        slot,
                        handle: scene_handle.clone(),
                        path: asset_path.clone(),
                        logged_final: false,
                    });
                    pending.gltf_handles.push(PendingLoadEntry {
                        slot,
                        handle: gltf_handle,
                        path: load_path.clone(),
                        logged_final: false,
                    });

                    let layer_for_observer = layer.clone();
                    let scene_entity = commands
                        .spawn((
                            SceneRoot(scene_handle),
                            Transform::default(),
                            layer,
                        ))
                        .observe(move |
                            trigger: Trigger<SceneInstanceReady>,
                            children_query: Query<&Children>,
                            names: Query<&Name>,
                            mut transforms: Query<&mut Transform>,
                            mut cmds: Commands,
                        | {
                            let root = trigger.entity();
                            let child_count = count_descendants(root, &children_query);
                            propagate_render_layers(root, &layer_for_observer, &children_query, &mut cmds);
                            // Dump all bone names for debugging T-pose fix coverage
                            dump_bone_names(root, &children_query, &names);
                            // Fix T-pose: rotate arm bones to natural resting position
                            fix_tpose_arms(root, &children_query, &names, &mut transforms);
                            bevy_debug(&format!("SceneInstanceReady: entity {:?}, propagated layers to {} descendants", root, child_count));
                        })
                        .id();
                    state.scene_entity = Some(scene_entity);
                    state.active = true;

                    // Activate camera
                    if let Ok(mut camera) = cameras.get_mut(state.camera_entity) {
                        camera.is_active = true;
                    }

                    info!(
                        "🎨 Slot {}: loaded '{}' from {}",
                        slot, display_name, model_path
                    );
                }
            }
            AvatarCommand::Unload { slot } => {
                if let Some(state) = registry.slots.get_mut(&slot) {
                    if let Some(entity) = state.scene_entity.take() {
                        commands.entity(entity).despawn_recursive();
                    }
                    state.active = false;
                    if let Ok(mut camera) = cameras.get_mut(state.camera_entity) {
                        camera.is_active = false;
                    }
                    info!("🎨 Slot {}: unloaded", slot);
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
            AvatarCommand::Shutdown => {
                info!("🎨 Bevy renderer shutting down");
                // Don't process::exit — that kills the entire Rust worker.
                // Just stop processing commands. The Bevy loop continues but
                // inactive cameras render nothing. In practice, shutdown means
                // the process is ending and the thread will be joined by OS.
                return;
            }
        }
    }
}

/// Simple idle animation — gentle camera sway for visual interest.
fn animate_idle(
    time: Res<Time>,
    registry: Res<SlotRegistry>,
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
            transform.translation.x = sway_x;
            transform.translation.y = 1.4 + sway_y; // Match base camera height
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
        for &child in child_list.iter() {
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
        for &child in child_list.iter() {
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
        for &child in child_list.iter() {
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
        for &child in child_list.iter() {
            commands.entity(child).insert(layer.clone());
            propagate_render_layers(child, layer, children, commands);
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
        assert!(MAX_AVATAR_SLOTS >= 14, "Need at least 14 slots for all personas");
        assert_eq!(AVATAR_WIDTH, 640);
        assert_eq!(AVATAR_HEIGHT, 480);
        assert!(AVATAR_FPS >= 1.0 && AVATAR_FPS <= 30.0, "FPS should be reasonable (1-30)");
    }
}
