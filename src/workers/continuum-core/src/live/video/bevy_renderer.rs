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
//! Performance: 14 avatars × 640×360 @ ~7fps effective readback (15fps Bevy tick).
//! On Apple Silicon (shared memory + GPU bridge), readback is zero-copy IOSurface.

use crate::gpu::make_entry;
use crate::gpu::memory_manager::{GpuAllocationGuard, GpuMemoryManager, GpuPriority, GpuSubsystem};
use crate::{clog_info, clog_warn};
use bevy::app::ScheduleRunnerPlugin;
use bevy::asset::LoadState;
use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::{RenderLayers, SetViewVisibility};
use bevy::camera::RenderTarget;
use bevy::mesh::morph::MorphWeights;
use bevy::prelude::*;
use bevy::render::gpu_readback::{Readback, ReadbackComplete};
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use bevy::scene::SceneInstanceReady;
use crossbeam_channel::{Receiver, Sender};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use crate::live::avatar::RgbaFrame;

// GpuConvertPlugin DISABLED: Metal compute shader runs on a separate command queue
// from Bevy/wgpu, with no GPU synchronization guarantee that the render pass has
// completed before the compute shader reads the texture. This causes alternating
// correct/stale frames (the strobe). The ReadbackComplete → try_write_bridge path
// is correct and should be used until proper GPU sync is implemented.
//
// To re-enable: use wgpu's command encoder (same queue) instead of a separate
// Metal command queue, or add MTLFence/MTLEvent synchronization.

/// Debug logging — no-op in production. Enable via BEVY_AVATAR_DEBUG=1 env var.
fn bevy_debug(_msg: &str) {
    // Intentionally empty — file I/O per call was killing performance
}

/// Maximum number of concurrent avatar render slots.
/// 16 supports up to 16 AI personas with 3D avatars simultaneously.
/// At 15fps Bevy / ~7fps effective readback × 640×360 × 4 bytes = ~6.5 MB/s per slot.
/// On Apple Silicon shared memory this is essentially a memcpy (GPU bridge zero-copy).
/// Bevy supports 32 render layers; we use layers 1-16, leaving headroom.
pub const MAX_AVATAR_SLOTS: u8 = 16;

/// Default render resolution per avatar.
/// 640×360 for grid tiles — matches typical display size (small thumbnails).
/// The adaptive resize system scales spotlight/active speaker to HD (1280×720)
/// via the ResolutionTier data channel from the browser. Starting low keeps GPU
/// load manageable with 14+ simultaneous avatars on a single GPU.
pub const AVATAR_WIDTH: u32 = 640;
pub const AVATAR_HEIGHT: u32 = 360;

/// Target framerate for avatar rendering.
/// 15fps Bevy tick → ~7-8fps effective readback (Readback is one-shot, re-inserted
/// next frame). Avatar animations (blinking, breathing, idle gestures) look smooth
/// at 15fps — they're slow, continuous motions. Lip sync uses pre-computed weight
/// sequences sampled by Bevy's internal clock, so mouth animation interpolates
/// smoothly regardless of frame rate. 15fps cuts ALL Bevy overhead in half vs 30fps:
/// half the ECS schedule runs, half the render graph traversals, half the GPU passes.
const AVATAR_FPS: f64 = 15.0;

// ============================================================================
// Public API — BevyAvatarSystem singleton
// ============================================================================

/// GPU memory manager for render VRAM tracking.
/// Set once from ipc/mod.rs during server startup, before any avatars load.
static RENDERER_GPU_MANAGER: OnceLock<Arc<GpuMemoryManager>> = OnceLock::new();

/// Provide the GPU memory manager to the renderer subsystem.
pub fn set_gpu_manager(mgr: Arc<GpuMemoryManager>) {
    let _ = RENDERER_GPU_MANAGER.set(mgr);
}

/// Access the GPU memory manager (if set).
fn gpu_manager() -> Option<&'static Arc<GpuMemoryManager>> {
    RENDERER_GPU_MANAGER.get()
}

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

/// A complete speech animation clip — the synchronized package.
///
/// Bundles mouth weights + duration into one unit so all animation attributes
/// (mouth, head nod, future: gestures) play from a single timeline.
/// Like a game engine animation clip — queued, played, auto-stopped.
#[derive(Debug, Clone)]
pub struct SpeechAnimationClip {
    /// Per-window mouth open weights (lerped between consecutive samples).
    pub mouth_weights: Vec<f32>,
    /// Interval between mouth weight samples (ms). Typically 66ms (~15Hz).
    pub interval_ms: u32,
    /// Total audio duration (ms). Bevy auto-clears Speaking flag when this expires.
    pub duration_ms: u64,
}

/// Emotional expression state for avatar facial animation.
/// Maps to VRM expression blend shape presets. Neutral = no expression active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Emotion {
    #[default]
    Neutral,
    Happy,
    Sad,
    Angry,
    Surprised,
    Relaxed,
}

/// Body gesture for avatar upper-body animation.
/// Driven by speech content analysis — gestures fire alongside emotions
/// since they animate different body parts (arms vs face).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Gesture {
    #[default]
    None,
    /// Friendly wave — right arm up, forearm oscillates
    Wave,
    /// Thinking pose — right hand near chin, head tilts
    Think,
    /// Emphatic head nod — stronger than speech nod
    Nod,
    /// Shoulders up, arms slightly out — uncertainty
    Shrug,
    /// Right arm extended forward — directing attention
    Point,
    /// Both arms slightly out, palms up — explaining
    OpenHands,
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
    /// Set a pre-computed sequence of mouth weights for amplitude-responsive lip sync.
    /// Bevy samples from this sequence on its own clock — no tokio timing dependency.
    /// This eliminates starvation from tokio runtime contention (14+ concurrent agents).
    SetMouthWeightSequence {
        slot: u8,
        weights: Vec<f32>,
        interval_ms: u32,
    },
    /// Play a complete speech animation clip — unified package for synchronized playback.
    /// Bundles speaking flag + mouth weights + duration into ONE command.
    /// Bevy auto-stops when duration expires (no tokio::spawn needed).
    PlaySpeech { slot: u8, clip: SpeechAnimationClip },
    /// Stop a speech animation immediately (e.g. interrupted by new speech).
    StopSpeech { slot: u8 },
    /// Set emotional expression on an avatar. Weight controls intensity (0.0-1.0).
    /// Transition smooths over transition_ms. Auto-decays to neutral after ~5s.
    SetEmotion {
        slot: u8,
        emotion: Emotion,
        weight: f32,
        transition_ms: u32,
    },
    /// Trigger a body gesture. Duration controls how long the gesture plays.
    /// Gesture animation uses arm bones — can overlap with speech (head nod + arm gesture).
    SetGesture {
        slot: u8,
        gesture: Gesture,
        duration_ms: u32,
    },
    /// Set the cognitive state for an avatar slot (evaluating, generating, idle).
    /// Drives looping gesture animations while the state persists.
    SetCognitiveState {
        slot: u8,
        state: crate::live::session::cognitive_animation::CognitiveState,
    },
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
    /// Frame-ready notifiers, one per slot. Fired by the readback observer
    /// when a frame is written (channel or GPU bridge). Video loops await
    /// these instead of sleep-polling — frame arrival IS the clock.
    frame_notifiers: Vec<std::sync::Arc<tokio::sync::Notify>>,
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

        // Create frame channels and notifiers for each slot
        let mut frame_senders = Vec::with_capacity(MAX_AVATAR_SLOTS as usize);
        let mut frame_receivers = Vec::with_capacity(MAX_AVATAR_SLOTS as usize);
        let mut frame_notifiers = Vec::with_capacity(MAX_AVATAR_SLOTS as usize);
        for _ in 0..MAX_AVATAR_SLOTS {
            let (tx, rx) = crossbeam_channel::bounded(4); // absorbs timing jitter without frame drops
            frame_senders.push(tx);
            frame_receivers.push(rx);
            frame_notifiers.push(std::sync::Arc::new(tokio::sync::Notify::new()));
        }

        // Clone notifiers for Bevy thread — it fires these on readback completion
        let notifiers_for_bevy: Vec<std::sync::Arc<tokio::sync::Notify>> =
            frame_notifiers.to_vec();

        std::thread::Builder::new()
            .name("bevy-avatar-renderer".into())
            .spawn(move || {
                run_bevy_app(command_rx, frame_senders, notifiers_for_bevy, ready_clone);
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
            clog_warn!(
                "🎨 Bevy renderer did not report ready within 5s — may have failed to init GPU"
            );
        } else {
            bevy_debug("Bevy confirmed READY");
        }

        Self {
            command_tx,
            frame_receivers,
            frame_notifiers,
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

    /// Get the frame-ready notifier for a specific avatar slot.
    /// Video loops await this instead of sleep-polling — wakes immediately
    /// when the readback observer writes a frame.
    pub fn frame_notifier(&self, slot: u8) -> Option<std::sync::Arc<tokio::sync::Notify>> {
        self.frame_notifiers.get(slot as usize).cloned()
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
        let _ = self
            .command_tx
            .send(AvatarCommand::SetSpeaking { slot, speaking });
    }

    /// Register a persona identity → slot mapping (called when a BevyChannelRenderer is created).
    pub fn register_identity(&self, identity: &str, slot: u8) {
        self.identity_to_slot
            .lock()
            .unwrap()
            .insert(identity.to_string(), slot);
        bevy_debug(&format!(
            "Registered identity '{}' → slot {}",
            &identity[..8.min(identity.len())],
            slot
        ));
    }

    /// Remove a persona identity from the slot map (called when SlotGuard drops).
    pub fn unregister_identity(&self, identity: &str) {
        self.identity_to_slot.lock().unwrap().remove(identity);
        bevy_debug(&format!(
            "Unregistered identity '{}'",
            &identity[..8.min(identity.len())]
        ));
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
            if speaking {
                clog_warn!(
                    "🎨 set_speaking: identity '{}' not registered (no slot)",
                    &identity[..8.min(identity.len())]
                );
            }
            false
        }
    }

    /// Set mouth open weight for amplitude-responsive lip sync.
    /// Weight should be 0.0 (closed) to 1.0 (fully open).
    pub fn set_mouth_weight(&self, slot: u8, weight: f32) {
        let _ = self
            .command_tx
            .send(AvatarCommand::SetMouthWeight { slot, weight });
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

    /// Send a pre-computed sequence of mouth weights to Bevy.
    /// Bevy samples from this on its own clock — immune to tokio runtime starvation.
    /// This is the preferred path: all weights computed upfront, timing is Bevy's problem.
    pub fn set_mouth_weight_sequence_by_identity(
        &self,
        identity: &str,
        weights: Vec<f32>,
        interval_ms: u32,
    ) -> bool {
        if let Some(&slot) = self.identity_to_slot.lock().unwrap().get(identity) {
            let _ = self.command_tx.send(AvatarCommand::SetMouthWeightSequence {
                slot,
                weights,
                interval_ms,
            });
            true
        } else {
            clog_warn!(
                "🎨 set_mouth_weight_sequence: identity '{}' not registered",
                &identity[..8.min(identity.len())]
            );
            false
        }
    }

    /// Play a complete speech animation clip — the unified path.
    /// ONE command bundles speaking flag + mouth weights + auto-stop duration.
    /// Returns false if identity has no registered slot.
    pub fn play_speech_by_identity(&self, identity: &str, clip: SpeechAnimationClip) -> bool {
        if let Some(&slot) = self.identity_to_slot.lock().unwrap().get(identity) {
            let _ = self
                .command_tx
                .send(AvatarCommand::PlaySpeech { slot, clip });
            true
        } else {
            clog_warn!(
                "🎨 play_speech: identity '{}' not registered (no slot)",
                &identity[..8.min(identity.len())]
            );
            false
        }
    }

    /// Stop a speech animation immediately (e.g. new utterance interrupts old one).
    pub fn stop_speech_by_identity(&self, identity: &str) -> bool {
        if let Some(&slot) = self.identity_to_slot.lock().unwrap().get(identity) {
            let _ = self.command_tx.send(AvatarCommand::StopSpeech { slot });
            true
        } else {
            false
        }
    }

    /// Resize a slot's render target to new dimensions.
    pub fn resize_slot(&self, slot: u8, width: u32, height: u32) {
        let _ = self.command_tx.send(AvatarCommand::Resize {
            slot,
            width,
            height,
        });
    }

    /// Set emotional expression by persona identity (user_id).
    /// Returns false if identity has no registered slot.
    pub fn set_emotion_by_identity(
        &self,
        identity: &str,
        emotion: Emotion,
        weight: f32,
        transition_ms: u32,
    ) -> bool {
        if let Some(&slot) = self.identity_to_slot.lock().unwrap().get(identity) {
            let _ = self.command_tx.send(AvatarCommand::SetEmotion {
                slot,
                emotion,
                weight,
                transition_ms,
            });
            true
        } else {
            false
        }
    }

    /// Trigger a body gesture by persona identity (user_id).
    /// Returns false if identity has no registered slot.
    pub fn set_gesture_by_identity(
        &self,
        identity: &str,
        gesture: Gesture,
        duration_ms: u32,
    ) -> bool {
        if let Some(&slot) = self.identity_to_slot.lock().unwrap().get(identity) {
            let _ = self.command_tx.send(AvatarCommand::SetGesture {
                slot,
                gesture,
                duration_ms,
            });
            true
        } else {
            false
        }
    }

    /// Set cognitive state by persona identity (user_id).
    /// Drives looping gesture animations while the cognitive state persists.
    /// Returns false if identity has no registered slot.
    pub fn set_cognitive_state_by_identity(
        &self,
        identity: &str,
        state: crate::live::session::cognitive_animation::CognitiveState,
    ) -> bool {
        if let Some(&slot) = self.identity_to_slot.lock().unwrap().get(identity) {
            let _ = self
                .command_tx
                .send(AvatarCommand::SetCognitiveState { slot, state });
            true
        } else {
            false
        }
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

/// Per-slot frame-ready notifiers. Fired by the readback observer after writing
/// a frame (channel or GPU bridge path). Video loops await these — frame arrival
/// is the clock, not sleep-polling.
#[derive(Resource)]
pub(crate) struct FrameNotifiers(pub Vec<std::sync::Arc<tokio::sync::Notify>>);

/// Shared ready flag — set after Bevy Startup systems complete.
#[derive(Resource)]
struct ReadyFlag(std::sync::Arc<std::sync::atomic::AtomicBool>);

/// Tracks the state of each avatar render slot.
#[derive(Resource)]
pub(crate) struct SlotRegistry {
    pub slots: HashMap<u8, SlotState>,
}

pub(crate) struct SlotState {
    pub camera_entity: Entity,
    pub _readback_entity: Entity,
    pub scene_entity: Option<Entity>,
    /// Currently active render target (either default low-res or borrowed HD from pool).
    _render_target: Handle<Image>,
    /// The slot's own low-res render target (640×360). Always retained for fallback
    /// when an HD target is returned to the pool.
    default_render_target: Handle<Image>,
    pub active: bool,
    /// True once SceneInstanceReady fires — model meshes are spawned and the camera
    /// has rendered at least one valid frame. Readback only starts after this flag is set,
    /// preventing the bright-green uninitialized-texture flash on slot activation.
    pub model_loaded: bool,
    /// Handle to the loaded Gltf asset — used for morph target name discovery.
    gltf_handle: Option<Handle<bevy::gltf::Gltf>>,
    /// Path to the model file — used for VRM extension parsing.
    model_path: Option<String>,
}

impl SlotState {
    /// Get the render target's AssetId for render-world lookups.
    pub fn render_target_id(&self) -> bevy::asset::AssetId<Image> {
        self._render_target.id()
    }
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

/// Marker for the shared directional light so force_light_visibility can find it.
#[derive(Component)]
struct AvatarSceneLight;

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
    // VRM expression presets — emotional blend shapes
    happy_index: Option<usize>,
    sad_index: Option<usize>,
    angry_index: Option<usize>,
    surprised_index: Option<usize>,
    relaxed_index: Option<usize>,
    // Eye gaze blend shapes (VRM lookAt presets)
    look_up: Option<usize>,
    look_down: Option<usize>,
    look_left: Option<usize>,
    look_right: Option<usize>,
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
    spine: Option<BoneInfo>,
    left_shoulder: Option<BoneInfo>,
    right_shoulder: Option<BoneInfo>,
    // Arm bones for gesture animation
    left_upper_arm: Option<BoneInfo>,
    right_upper_arm: Option<BoneInfo>,
    left_lower_arm: Option<BoneInfo>,
    right_lower_arm: Option<BoneInfo>,
    // Eye bones for bone-based gaze (VRM lookAtTypeName: "Bone")
    left_eye: Option<BoneInfo>,
    right_eye: Option<BoneInfo>,
    // Hand bones — discovered now for animation systems to reference.
    // Currently used only for bone discovery logging; hand/finger animation is next.
    #[allow(dead_code)]
    left_hand: Option<BoneInfo>,
    #[allow(dead_code)]
    right_hand: Option<BoneInfo>,
    /// VRM lookAt configuration — eye rotation ranges for bone-based gaze.
    /// Parsed from extensions.VRM.firstPerson.lookAtHorizontalInner/Outer/VerticalUp/Down.
    look_at_config: Option<VrmLookAtConfig>,
}

struct BoneInfo {
    entity: Entity,
    /// Actual local-space rest translation (from skeleton bind pose)
    rest_translation: Vec3,
    /// Actual local-space rest rotation (from skeleton bind pose)
    rest_rotation: Quat,
}

/// VRM lookAt configuration for bone-based eye gaze.
/// Parsed from VRM extensions — defines how far eye bones can rotate in each direction.
/// Output values are the actual eye bone rotation in degrees for the corresponding
/// input range (typically full ±90° input → 8-12° actual eye rotation).
#[derive(Debug, Clone, Copy)]
struct VrmLookAtConfig {
    /// Max eye bone Y-rotation (radians) for looking left/right (inward)
    horizontal_inner_deg: f32,
    /// Max eye bone Y-rotation (radians) for looking left/right (outward)
    horizontal_outer_deg: f32,
    /// Max eye bone X-rotation (radians) for looking up
    vertical_up_deg: f32,
    /// Max eye bone X-rotation (radians) for looking down
    vertical_down_deg: f32,
}

impl Default for VrmLookAtConfig {
    fn default() -> Self {
        // Sensible defaults for typical VRM models (8° horizontal, 10° vertical)
        Self {
            horizontal_inner_deg: 8.0,
            horizontal_outer_deg: 8.0,
            vertical_up_deg: 10.0,
            vertical_down_deg: 10.0,
        }
    }
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
    /// Current head Y-rotation toward speaker (smoothly interpolated)
    head_turn_current: f32,
    /// Target head Y-rotation toward speaker
    head_turn_target: f32,
}

/// Per-slot render target dimensions. Updated when a slot is resized.
/// The ReadbackComplete observer reads from this instead of using constants,
/// allowing each slot to have a different resolution after adaptive resize.
#[derive(Resource)]
pub(crate) struct SlotDimensions {
    pub dims: HashMap<u8, (u32, u32)>,
}

impl Default for SlotDimensions {
    fn default() -> Self {
        let mut dims = HashMap::new();
        for slot in 0..MAX_AVATAR_SLOTS {
            dims.insert(slot, (AVATAR_WIDTH, AVATAR_HEIGHT));
        }
        Self { dims }
    }
}

/// An active speech clip playing on Bevy's timeline.
/// All animation attributes (mouth, head nod, future gestures) sample from this.
struct ActiveClip {
    mouth_weights: Vec<f32>,
    interval_ms: u32,
    duration_ms: u64,
    /// Bevy elapsed_secs when playback started.
    start_time: f32,
}

/// All active speech animations, keyed by slot.
/// Single source of truth for speech-related animation — replaces the old fragmented
/// MouthWeights + Speaking component approach.
///
/// Design: like a game engine's animation state machine.
/// - PlaySpeech inserts a clip (starts mouth + head nod + auto-stop timer)
/// - StopSpeech removes it (immediate interrupt)
/// - animate_speaking reads from here — one resource, one timeline per slot
#[derive(Resource, Default)]
struct ActiveSpeechClips {
    clips: HashMap<u8, ActiveClip>,
    /// Atomic stats — accumulated in the render loop, flushed periodically.
    /// Never log from the hot path; read+reset from a low-frequency system.
    clips_started: u32,
    clips_auto_stopped: u32,
    clips_interrupted: u32,
}

/// Legacy per-slot mouth weights (for SetMouthWeight individual command).
/// Kept for backward compatibility but PlaySpeech is the preferred path.
#[derive(Resource, Default)]
struct LegacyMouthWeights {
    weights: HashMap<u8, f32>,
}

/// Per-slot emotional expression state with smooth transitions and auto-decay.
#[derive(Resource, Default)]
struct EmotionState {
    slots: HashMap<u8, SlotEmotionState>,
}

struct SlotEmotionState {
    current: Emotion,
    current_weight: f32,
    target: Emotion,
    target_weight: f32,
    /// Weight change per second (derived from transition_ms)
    transition_rate: f32,
    /// Seconds remaining until fade to neutral (~5s auto-decay)
    decay_timer: f32,
}

impl Default for SlotEmotionState {
    fn default() -> Self {
        Self {
            current: Emotion::Neutral,
            current_weight: 0.0,
            target: Emotion::Neutral,
            target_weight: 0.0,
            transition_rate: 3.0, // default: 333ms transition
            decay_timer: 0.0,
        }
    }
}

/// Auto-decay duration — emotion fades to neutral after this many seconds without refresh.
const EMOTION_DECAY_SECS: f32 = 5.0;
/// During active speech, expression weight is attenuated to this fraction
/// to avoid fighting mouth animation (mouth shapes + full expression looks wrong).
const SPEECH_ATTENUATION: f32 = 0.3;

/// Per-slot body gesture state with attack/sustain/release phases.
#[derive(Resource, Default)]
struct ActiveGestures {
    slots: HashMap<u8, SlotGestureAnimState>,
}

/// Phase of a gesture animation's lifecycle.
#[derive(Debug, Clone, Copy, PartialEq)]
enum GesturePhase {
    /// Easing in from rest to gesture pose
    Attack,
    /// Holding gesture pose with micro-oscillation
    Sustain,
    /// Easing out from gesture pose back to rest
    Release,
}

struct SlotGestureAnimState {
    gesture: Gesture,
    phase: GesturePhase,
    /// Total duration of the gesture in seconds
    duration_secs: f32,
    /// Elapsed time since gesture started
    elapsed: f32,
    /// Current blend weight (0.0=rest, 1.0=full gesture)
    weight: f32,
}

/// Duration of attack/release phases for gesture easing (seconds).
const GESTURE_EASE_SECS: f32 = 0.3;

/// Per-slot cognitive animation state — tracks active cognitive state and re-roll timing.
#[derive(Resource, Default)]
struct CognitiveAnimState {
    slots: HashMap<u8, SlotCognitiveState>,
}

struct SlotCognitiveState {
    state: crate::live::session::cognitive_animation::CognitiveState,
    config: crate::live::session::cognitive_animation::CognitiveAnimationConfig,
    /// Elapsed time since last gesture re-roll
    time_since_reroll: f32,
}

/// Render cadence — staggered camera activation for GPU load distribution.
///
/// Game engine LOD principle: not every object renders every frame.
/// - Speaking slots: camera active every frame (15fps → ~7fps effective readback)
/// - Idle slots: camera active every Nth frame, staggered by slot index
///
/// This distributes GPU render passes across frames. With 14 idle slots and
/// cadence=3, each frame only renders ~5 idle + speaking slots (~6-7 total),
/// cutting GPU work in half while maintaining visible animation quality.
///
/// Animation systems (blinking, breathing, gestures) still run every frame,
/// updating bone transforms and morph weights. The camera just captures the
/// accumulated state less frequently for idle slots.
#[derive(Resource)]
struct RenderSchedule {
    frame_count: u32,
    /// How many frames between renders for idle slots. Lower = smoother but more GPU work.
    /// 1 = every frame renders (15fps Bevy tick). No cadence stagger needed because
    /// 15fps already halved GPU work vs 30fps. Cadence>1 at 15fps causes visible
    /// strobe artifacts (camera on/off toggling creates readback timing mismatches).
    idle_cadence: u32,
}

impl Default for RenderSchedule {
    fn default() -> Self {
        Self {
            frame_count: 0,
            // 1 = all slots render every frame at 15fps Bevy tick.
            // 15fps already halved GPU work vs 30fps — no cadence stagger needed.
            // Cadence>1 at 15fps caused visible strobe (camera on/off toggling
            // creates readback timing mismatches between render and readback passes).
            idle_cadence: 1,
        }
    }
}

/// HD render target resolution (for spotlight / active speaker).
const HD_WIDTH: u32 = 1280;
const HD_HEIGHT: u32 = 720;
/// Maximum number of simultaneous HD render targets. Like Nintendo cartridge RAM —
/// pre-allocate a fixed budget, swap pointers, never allocate at runtime.
const MAX_HD_SLOTS: usize = 3;

/// Pre-allocated pool of HD render targets. Only MAX_HD_SLOTS exist at any time.
/// When a slot needs HD (spotlight/active speaker), it borrows from the pool.
/// When demoted, it returns the target and falls back to its default low-res target.
/// No runtime texture allocation — just pointer swaps.
#[derive(Resource)]
struct HdRenderTargetPool {
    /// Available HD targets not currently assigned to any slot.
    available: Vec<Handle<Image>>,
    /// Which slot is borrowing which HD target. When the slot is done, the
    /// target goes back to `available`.
    assigned: HashMap<u8, Handle<Image>>,
}

/// GPU allocation guards for renderer VRAM tracking.
/// One aggregate guard for all pre-allocated render targets (low-res + HD pool),
/// plus per-slot guards for loaded VRM/glTF model VRAM.
#[derive(Resource, Default)]
struct GpuGuards {
    /// Guard for all pre-allocated render targets (16 low-res + HD pool).
    /// Allocated once at startup, never released until shutdown.
    /// RAII: held alive so its Drop releases VRAM — not read directly.
    _render_targets: Option<GpuAllocationGuard>,
    /// Per-slot guards for loaded VRM model VRAM estimates.
    /// Removed on `AvatarCommand::Unload` (drop releases VRAM).
    model_guards: HashMap<u8, GpuAllocationGuard>,
}

fn run_bevy_app(
    command_rx: Receiver<AvatarCommand>,
    frame_senders: Vec<Sender<RgbaFrame>>,
    frame_notifiers: Vec<std::sync::Arc<tokio::sync::Notify>>,
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
                }),
                // Note: PipelinedRenderingPlugin is NOT in DefaultPlugins when bevy_winit
                // is absent (our headless config). No need to disable it.
        )
        // ScheduleRunnerPlugin drives the frame loop at our target FPS
        .add_plugins(ScheduleRunnerPlugin::run_loop(Duration::from_secs_f64(
            1.0 / AVATAR_FPS,
        )))
        // GPU→GPU RGBA→NV12 compute DISABLED — see GpuConvertPlugin comment above.
        // Slots use CPU readback → try_write_bridge → IOSurface → GpuBridgePublisher.
        // Bevy 0.18: TransformTreeChanged is a new marker component required by Transform
        // (via #[require(TransformTreeChanged)]). Scene spawner panics if it's not registered
        // in the type registry. reflect_auto_register should handle this but doesn't.
        .register_type::<bevy::transform::components::TransformTreeChanged>()
        .add_systems(Startup, (setup_render_slots, signal_ready).chain())
        // process_commands + monitor_load_states ALWAYS run (receive Load commands, track scene readiness).
        // Animation/render systems gated by has_active_slots — skip when no models loaded.
        .add_systems(
            Update,
            (
                process_commands,
                monitor_load_states,
                touch_ambient_light,
            ),
        )
        .add_systems(
            Update,
            (
                manage_render_cadence,
                ensure_continuous_readback,
                discover_morph_targets,
                animate_idle,
                animate_speaking,
                animate_expression,
                animate_blinking,
                animate_breathing,
                animate_idle_gestures,
                animate_eye_gaze,
                drive_cognitive_gestures,
                animate_body_gestures,
            )
                .run_if(has_active_slots),
        )
        // Force light visibility AFTER Bevy's CheckVisibility (runs in PostUpdate).
        // Without this, the directional light's ViewVisibility may be false in headless
        // mode, causing extract_lights to skip it on some frames → lighting strobe.
        .add_systems(
            PostUpdate,
            force_light_visibility
                .after(bevy::camera::visibility::VisibilitySystems::CheckVisibility),
        )
        .run();
}

/// Spawn a readback entity for a given render target + slot.
///
/// CRITICAL: Bevy's `Readback` component is one-shot — it fires `ReadbackComplete`
/// once and is consumed. For continuous video frames, we must re-insert the
/// `Readback` component after each completion. This gives us readback every
/// other frame (~7fps at 15fps Bevy) due to Commands' deferred execution.
///
/// If `start_active` is true, the `Readback` component is inserted immediately
/// (for resize/reload scenarios where the slot is already active).
/// If false, only the marker + observer are spawned — `ensure_continuous_readback`
/// inserts `Readback` once the slot becomes active. This prevents wasted GPU
/// readback on inactive slots at boot.
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

                // Look up this slot's current dimensions (may have been resized)
                let (slot_w, slot_h) = slot_dims.dims
                    .get(&slot_id)
                    .copied()
                    .unwrap_or((AVATAR_WIDTH, AVATAR_HEIGHT));

                // Log first readback per slot + pixel diversity diagnostic
                static FIRST_READBACK: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(0);
                #[allow(clippy::declare_interior_mutable_const)]
                const ATOMIC_ZERO: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
                #[allow(clippy::borrow_interior_mutable_const)]
                static FRAME_COUNTER: [std::sync::atomic::AtomicU32; 16] = [ATOMIC_ZERO; 16];
                let mask = 1u16 << slot_id;
                let prev = FIRST_READBACK.fetch_or(mask, std::sync::atomic::Ordering::Relaxed);
                if prev & mask == 0 {
                    clog_info!("🎨 Slot {}: first ReadbackComplete ({} bytes, {}×{})", slot_id, pixel_bytes.len(), slot_w, slot_h);
                }

                let frame_n = FRAME_COUNTER[slot_id as usize].fetch_add(1, std::sync::atomic::Ordering::Relaxed);

                // Health check at frame 150 and 300 — LOG ONLY, no fallback.
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
                        // Signal video loop — frame arrival is the clock
                        if let Some(notify) = notifiers.0.get(slot_id as usize) {
                            notify.notify_one();
                        }
                        return; // Frame written to IOSurface, skip channel
                    }
                }

                // Channel path (non-macOS, or no GPU bridge registered for this slot)
                if let Some(tx) = channels.0.get(slot_id as usize) {
                    match tx.try_send(RgbaFrame {
                        width: slot_w,
                        height: slot_h,
                        data: pixel_bytes.to_vec(),
                    }) {
                        Ok(()) => {
                            // Signal video loop — frame arrival is the clock
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
    // Global ambient light — brightness forced-touched every frame by
    // `touch_ambient_light` system to ensure render-world extraction never skips it.
    // (Bevy's extraction uses `is_changed()` which can drop the resource on frames
    // where it hasn't been mutated, causing lighting flicker on offscreen targets.)
    commands.insert_resource(GlobalAmbientLight {
        color: Color::WHITE,
        brightness: 300.0,
        affects_lightmapped_meshes: false,
    });

    // Single shared directional light visible on ALL render layers.
    // Shadows disabled = no 10-light limit. One light for all slots is efficient.
    // (Pipelined rendering is disabled, so extraction is deterministic — no flicker.)
    {
        let all_layers: Vec<usize> = (1..=(MAX_AVATAR_SLOTS as usize)).collect();
        commands.spawn((
            DirectionalLight {
                illuminance: 12000.0,
                shadows_enabled: false,
                ..default()
            },
            // Light from camera direction (negative Z) to illuminate faces
            Transform::from_rotation(Quat::from_euler(
                EulerRot::XYZ,
                -0.4,
                std::f32::consts::PI,
                0.0,
            )),
            RenderLayers::from_layers(&all_layers),
            // Marker for force_light_visibility system
            AvatarSceneLight,
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
        rt_image.texture_descriptor.usage = TextureUsages::RENDER_ATTACHMENT
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

        // Shared directional light above handles all layers — no per-slot light needed.

        // GPU readback entity — spawned WITHOUT Readback component initially.
        // Inactive slots (no model loaded) should not trigger GPU readback.
        // ensure_continuous_readback inserts Readback when the slot becomes active.
        // The ReadbackComplete observer IS attached now (for when readback fires later).
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

    // Pre-allocate HD render target pool — Nintendo-style fixed memory budget.
    // Only MAX_HD_SLOTS HD textures exist. Spotlight borrows one; rest stay at low-res.
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

    // Track total render target VRAM allocation:
    // 16 low-res targets (640×360×4) + 3 HD targets (1280×720×4)
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
        "🎨 Bevy renderer ready: {} slots × {}×{} @{}fps ({} HD targets pooled at {}×{})",
        MAX_AVATAR_SLOTS,
        AVATAR_WIDTH,
        AVATAR_HEIGHT,
        AVATAR_FPS,
        MAX_HD_SLOTS,
        HD_WIDTH,
        HD_HEIGHT
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
        clog_info!(
            "🎨 File check '{}': exists={}",
            test_path,
            std::path::Path::new(test_path).exists()
        );
    }

    // Check parent Gltf handles (the .glb file itself)
    for entry in pending.gltf_handles.iter_mut() {
        if entry.logged_final {
            continue;
        }
        match asset_server.load_state(entry.handle.id()) {
            LoadState::Loaded => {
                // Gltf parsed successfully — inspect what scenes it contains
                if let Some(gltf) = gltf_assets.get(entry.handle.id()) {
                    let named: Vec<&Box<str>> = gltf.named_scenes.keys().collect();
                    clog_info!(
                        "🎨 ✅ Gltf LOADED slot {}: {} — {} scenes, named: {:?}",
                        entry.slot,
                        entry.path,
                        gltf.scenes.len(),
                        named,
                    );
                } else {
                    clog_info!(
                        "🎨 ✅ Gltf LOADED slot {}: {} (not yet in Assets<Gltf>)",
                        entry.slot,
                        entry.path
                    );
                }
                entry.logged_final = true;
            }
            LoadState::Failed(ref err) => {
                clog_warn!(
                    "🎨 ❌ Gltf FAILED slot {}: {} — error: {:?}",
                    entry.slot,
                    entry.path,
                    err
                );
                entry.logged_final = true;
            }
            _ => {
                // Log periodically while still loading
                static GLTF_LOADING_TICKS: std::sync::atomic::AtomicU64 =
                    std::sync::atomic::AtomicU64::new(0);
                let tick = GLTF_LOADING_TICKS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if tick == 30 || tick == 150 || tick == 300 {
                    bevy_debug(&format!(
                        "⏳ Gltf still loading slot {}: {} (tick {})",
                        entry.slot, entry.path, tick
                    ));
                }
            }
        }
    }

    // Check Scene sub-asset handles (#Scene0)
    for entry in pending.scene_handles.iter_mut() {
        if entry.logged_final {
            continue;
        }
        match asset_server.load_state(entry.handle.id()) {
            LoadState::Loaded => {
                clog_info!("🎨 ✅ Scene LOADED slot {}: {}", entry.slot, entry.path);
                entry.logged_final = true;
            }
            LoadState::Failed(ref err) => {
                clog_warn!(
                    "🎨 ❌ Scene FAILED slot {}: {} — error: {:?}",
                    entry.slot,
                    entry.path,
                    err
                );
                entry.logged_final = true;
            }
            _ => {
                static SCENE_LOADING_TICKS: std::sync::atomic::AtomicU64 =
                    std::sync::atomic::AtomicU64::new(0);
                let tick = SCENE_LOADING_TICKS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if tick == 30 || tick == 150 || tick == 300 {
                    bevy_debug(&format!(
                        "⏳ Scene still loading slot {}: {} (tick {})",
                        entry.slot, entry.path, tick
                    ));
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

/// Run condition: true when at least one slot has a loaded model.
/// Gates all animation/render systems — zero GPU work when no avatars are loaded.
fn has_active_slots(registry: Res<SlotRegistry>) -> bool {
    registry
        .slots
        .values()
        .any(|s| s.active && s.model_loaded)
}

/// Staggered render cadence — controls which cameras render each frame.
///
/// Game engine LOD: active speakers render every frame (smooth lip sync),
/// idle slots render every Nth frame (staggered by slot index for even distribution).
/// Animation systems still run every frame for all slots (bone/morph updates are cheap).
/// Only the GPU render pass (rasterization) is staggered.
///
/// With idle_cadence=1 at 15fps, all 14 slots render every frame.
/// 15fps already halved GPU work vs 30fps (half the ECS runs, render passes, readbacks).
/// Combined with lower default resolution (640×360), total GPU work drops ~2x vs 30fps.
/// Touch GlobalAmbientLight every frame to keep it marked as changed.
/// Bevy's render-world extraction of GlobalAmbientLight uses `is_changed()` —
/// if the resource hasn't been mutated since last frame, the render world drops it,
/// causing intermittent lighting loss on offscreen render targets.
/// This no-op mutation keeps the change flag set so extraction never skips it.
fn touch_ambient_light(mut ambient: ResMut<GlobalAmbientLight>) {
    // Force Bevy's change detection by writing the same value back.
    // ResMut::set_changed() is the explicit way to mark as changed.
    ambient.set_changed();
}

/// Force the directional light to be visible every frame.
///
/// Bevy's `extract_lights` skips directional lights where `ViewVisibility::get()` is false.
/// In headless rendering (no window/viewport), Bevy's `CheckVisibility` system may not
/// reliably mark our light as visible, causing intermittent lighting loss on offscreen
/// render targets. This system runs in PostUpdate (after visibility propagation) and
/// forces the light's ViewVisibility to visible, ensuring it's always extracted.
fn force_light_visibility(
    mut lights: Query<&mut ViewVisibility, With<AvatarSceneLight>>,
) {
    for mut vis in &mut lights {
        vis.set_visible();
    }
}

fn manage_render_cadence(
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

        // Speaking slots always render. Idle slots render every Nth frame,
        // staggered by slot index so render load distributes evenly.
        let should_render = is_speaking || (frame % cadence == (*slot as u32 % cadence));

        if let Ok(mut camera) = cameras.get_mut(state.camera_entity) {
            camera.is_active = should_render;
        }
    }
}

/// Re-insert `Readback` on entities that lost it after `ReadbackComplete`.
///
/// Bevy's `Readback` is one-shot: the render pipeline removes the component after
/// the GPU readback completes. This system runs every frame and re-inserts the
/// `Readback` component on readback entities that no longer have it, enabling
/// continuous frame delivery at ~half the Bevy loop rate (every other frame).
///
/// Only readbacks slots whose camera is active this frame (per render cadence).
/// Uses `ReadbackMarker` to distinguish readback entities from cameras/scenes.
#[allow(clippy::type_complexity)]
fn ensure_continuous_readback(
    query: Query<(Entity, &AvatarSlotId), (With<ReadbackMarker>, Without<Readback>)>,
    registry: Res<SlotRegistry>,
    cameras: Query<&Camera>,
    mut commands: Commands,
) {
    for (entity, slot_id) in &query {
        if let Some(state) = registry.slots.get(&slot_id.0) {
            // Skip slots that aren't ready for readback:
            // - !active: no Load command received yet
            // - !model_loaded: SceneInstanceReady hasn't fired yet
            // - camera not active this frame (render cadence says skip)
            if !state.active || !state.model_loaded {
                continue;
            }
            // GPU bridge slots still use CPU readback — the ReadbackComplete observer
            // writes RGBA→NV12 to the IOSurface via try_write_bridge(). The Metal
            // GPU compute path (GpuConvertPlugin) is disabled until proper GPU
            // synchronization is implemented (separate command queue = race condition).
            // Only readback if the camera rendered this frame (per render cadence).
            // Reading back a stale render target wastes GPU bandwidth for no new data.
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

/// Process commands from the main application.
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
                // Record identity + model path for health check logging
                health.identities.insert(slot, identity.clone());
                health.model_paths.insert(slot, model_path.clone());

                if let Some(state) = registry.slots.get_mut(&slot) {
                    let layer = RenderLayers::layer((slot + 1) as usize);

                    // Remove previous scene if any
                    if let Some(old_entity) = state.scene_entity.take() {
                        commands.entity(old_entity).despawn();
                    }
                    // Release previous model's GPU guard (if reloading)
                    gpu_guards.model_guards.remove(&slot);

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

                    // SceneInstanceReady fires once the scene is fully spawned.
                    // Propagate RenderLayers to all descendants (Bevy doesn't inherit them).
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
                            let child_count = count_descendants(root, &children_query);
                            propagate_render_layers(root, &layer_for_observer, &children_query, &mut cmds);
                            dump_bone_names(root, &children_query, &names);
                            fix_tpose_arms(root, &children_query, &names, &mut transforms);
                            discover_upper_body_bones(root, slot_for_observer, &model_path_for_observer, &children_query, &names, &transforms, &mut bone_registry);

                            // Mark model as loaded — readback can now begin for this slot.
                            // Scene meshes are spawned, camera has rendered at least one frame
                            // with the clear color. No more green-flash from uninitialized textures.
                            if let Some(state) = slot_registry.slots.get_mut(&slot_for_observer) {
                                state.model_loaded = true;
                            }

                            // Track VRM model VRAM via GPU memory manager.
                            // Estimate from file size — glTF/VRM meshes + textures decompress
                            // to roughly file-size VRAM.
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

                    // Activate camera
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
                    // Return HD target to pool if this slot had one
                    if let Some(hd_target) = hd_pool.assigned.remove(&slot) {
                        hd_pool.available.push(hd_target);
                        // Restore camera to default low-res target
                        commands
                            .entity(state.camera_entity)
                            .insert(RenderTarget::Image(
                                state.default_render_target.clone().into(),
                            ));
                        state._render_target = state.default_render_target.clone();
                        slot_dims.dims.insert(slot, (AVATAR_WIDTH, AVATAR_HEIGHT));
                    }
                    // Release GPU allocation guard for this model (drop releases VRAM)
                    gpu_guards.model_guards.remove(&slot);
                    // Clean up animation state for this slot
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
                // Legacy path — use PlaySpeech instead for synchronized animation.
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
                // Also set Speaking component for head nod
                if let Some(state) = registry.slots.get(&slot) {
                    if let Some(scene_entity) = state.scene_entity {
                        commands.entity(scene_entity).insert(Speaking);
                    }
                }
                speech_clips.clips_started += 1;
            }
            AvatarCommand::PlaySpeech { slot, clip } => {
                // Unified path — one command starts everything.
                // Insert clip (mouth weights + auto-stop timer)
                speech_clips.clips.insert(
                    slot,
                    ActiveClip {
                        mouth_weights: clip.mouth_weights,
                        interval_ms: clip.interval_ms,
                        duration_ms: clip.duration_ms,
                        start_time: time.elapsed_secs(),
                    },
                );
                // Set Speaking component for head nod
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
                    100.0 // instant
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
                        // Current gesture finishes naturally via release phase
                    }
                    _ => {
                        cognitive_anim.slots.insert(slot, SlotCognitiveState {
                            state,
                            config: crate::live::session::cognitive_animation::CognitiveAnimationConfig::default(),
                            time_since_reroll: 999.0, // Force immediate first gesture
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
                        // Promote to HD — borrow from pool
                        if let Some(hd_target) = hd_pool.available.pop() {
                            hd_pool.assigned.insert(slot, hd_target.clone());
                            clog_info!(
                                "🎨 Slot {}: promoted to HD ({}×{}, {} HD targets remaining)",
                                slot,
                                HD_WIDTH,
                                HD_HEIGHT,
                                hd_pool.available.len()
                            );
                            hd_target
                        } else {
                            clog_warn!("🎨 Slot {}: HD pool exhausted ({} already assigned), staying at current res",
                                slot, hd_pool.assigned.len());
                            continue;
                        }
                    } else if !is_hd_request && currently_hd {
                        // Demote from HD — return target to pool, use default
                        if let Some(hd_target) = hd_pool.assigned.remove(&slot) {
                            hd_pool.available.push(hd_target);
                            clog_info!(
                                "🎨 Slot {}: demoted to low-res ({}×{}, {} HD targets available)",
                                slot,
                                AVATAR_WIDTH,
                                AVATAR_HEIGHT,
                                hd_pool.available.len()
                            );
                        }
                        state.default_render_target.clone()
                    } else if is_hd_request && currently_hd {
                        // Already HD — no change needed
                        continue;
                    } else {
                        // Low-res to low-res — use default target (no allocation)
                        state.default_render_target.clone()
                    };

                    // Swap camera to the new render target
                    commands
                        .entity(state.camera_entity)
                        .insert(RenderTarget::Image(new_rt_handle.clone().into()));

                    // Despawn old readback entity and spawn new one with new handle
                    commands.entity(state._readback_entity).despawn();
                    let new_readback =
                        spawn_readback_entity(&mut commands, new_rt_handle.clone(), slot);

                    // Update slot state
                    state._readback_entity = new_readback;
                    state._render_target = new_rt_handle;

                    // Update per-slot dimensions
                    let (effective_w, effective_h) = if is_hd_request {
                        (HD_WIDTH, HD_HEIGHT)
                    } else {
                        (AVATAR_WIDTH, AVATAR_HEIGHT)
                    };
                    slot_dims.dims.insert(slot, (effective_w, effective_h));

                    clog_info!(
                        "🎨 Slot {}: resized to {}×{}",
                        slot,
                        effective_w,
                        effective_h
                    );
                }
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
        let mesh_names: Vec<String> = morph_query
            .get(morph_entity)
            .ok()
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
            // Standard glTF + VRoid naming conventions — match by name.
            // VRoid Studio uses "Fcl_" prefix: Fcl_MTH_A (mouth), Fcl_EYE_Close (blink), etc.
            // CRITICAL: Use ends_with, not contains, to avoid "mth_angry" matching before "mth_a".
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
                    mouth_open_index = Some(i);
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
                    blink_index = Some(i);
                }
                if blink_left_index.is_none()
                    && (lower == "blinkleft"
                        || lower == "blink_l"
                        || lower == "fcl_eye_close_l"
                        || lower.contains("eye_close_l")
                        || lower.contains("eye_close_left"))
                {
                    blink_left_index = Some(i);
                }
                if blink_right_index.is_none()
                    && (lower == "blinkright"
                        || lower == "blink_r"
                        || lower == "fcl_eye_close_r"
                        || lower.contains("eye_close_r")
                        || lower.contains("eye_close_right"))
                {
                    blink_right_index = Some(i);
                }
                // VRM expression presets — emotional blend shapes
                // VRoid: Fcl_EYE_Joy, Fcl_MTH_Angry, Fcl_ALL_Fun, etc.
                // Standard glTF / VRM: happy, joy, sad, sorrow, angry, surprised, fun, relaxed
                if happy_index.is_none()
                    && (lower == "happy"
                        || lower == "joy"
                        || lower.ends_with("_joy")
                        || lower.ends_with("_happy")
                        || lower == "fcl_all_joy"
                        || lower == "fcl_eye_joy")
                {
                    happy_index = Some(i);
                }
                if sad_index.is_none()
                    && (lower == "sad"
                        || lower == "sorrow"
                        || lower.ends_with("_sad")
                        || lower.ends_with("_sorrow")
                        || lower == "fcl_all_sorrow"
                        || lower == "fcl_eye_sorrow")
                {
                    sad_index = Some(i);
                }
                if angry_index.is_none()
                    && (lower == "angry"
                        || lower.ends_with("_angry")
                        || lower == "fcl_all_angry"
                        || lower == "fcl_mth_angry")
                {
                    angry_index = Some(i);
                }
                if surprised_index.is_none()
                    && (lower == "surprised"
                        || lower == "fun"
                        || lower.ends_with("_surprised")
                        || lower.ends_with("_fun")
                        || lower == "fcl_all_fun"
                        || lower == "fcl_brw_surprised")
                {
                    surprised_index = Some(i);
                }
                if relaxed_index.is_none()
                    && (lower == "relaxed"
                        || lower.ends_with("_relaxed")
                        || lower == "fcl_all_relaxed")
                {
                    relaxed_index = Some(i);
                }
                // Eye gaze blend shapes (VRM lookAt presets)
                // VRM 1.0: "lookUp", "lookDown", "lookLeft", "lookRight"
                // VRoid: "Fcl_EYE_LookUp", "Fcl_EYE_LookDown", etc.
                if look_up.is_none()
                    && (lower == "lookup"
                        || lower == "look_up"
                        || lower.ends_with("lookup")
                        || lower == "fcl_eye_lookup")
                {
                    look_up = Some(i);
                }
                if look_down.is_none()
                    && (lower == "lookdown"
                        || lower == "look_down"
                        || lower.ends_with("lookdown")
                        || lower == "fcl_eye_lookdown")
                {
                    look_down = Some(i);
                }
                if look_left.is_none()
                    && (lower == "lookleft"
                        || lower == "look_left"
                        || lower.ends_with("lookleft")
                        || lower == "fcl_eye_lookleft")
                {
                    look_left = Some(i);
                }
                if look_right.is_none()
                    && (lower == "lookright"
                        || lower == "look_right"
                        || lower.ends_with("lookright")
                        || lower == "fcl_eye_lookright")
                {
                    look_right = Some(i);
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
                    if blink_left_index.is_none() && (preset == "blink_l" || preset == "blinkleft")
                    {
                        if let Some(bind) = shape.binds.first() {
                            blink_left_index = Some(bind.index);
                        }
                    }
                    if blink_right_index.is_none()
                        && (preset == "blink_r" || preset == "blinkright")
                    {
                        if let Some(bind) = shape.binds.first() {
                            blink_right_index = Some(bind.index);
                        }
                    }
                    // VRM expression presets — emotional blend shapes
                    // VRM 0.x presets: "joy"/"happy", "sorrow"/"sad", "angry", "fun"/"surprised", "relaxed"
                    // VRM 1.0 presets: "happy", "sad", "angry", "surprised", "relaxed"
                    if happy_index.is_none() && (preset == "joy" || preset == "happy") {
                        if let Some(bind) = shape.binds.first() {
                            happy_index = Some(bind.index);
                        }
                    }
                    if sad_index.is_none() && (preset == "sorrow" || preset == "sad") {
                        if let Some(bind) = shape.binds.first() {
                            sad_index = Some(bind.index);
                        }
                    }
                    if angry_index.is_none() && preset == "angry" {
                        if let Some(bind) = shape.binds.first() {
                            angry_index = Some(bind.index);
                        }
                    }
                    if surprised_index.is_none() && (preset == "fun" || preset == "surprised") {
                        if let Some(bind) = shape.binds.first() {
                            surprised_index = Some(bind.index);
                        }
                    }
                    if relaxed_index.is_none() && preset == "relaxed" {
                        if let Some(bind) = shape.binds.first() {
                            relaxed_index = Some(bind.index);
                        }
                    }
                    // Eye gaze VRM presets
                    if look_up.is_none() && (preset == "lookup" || preset == "lookUp") {
                        if let Some(bind) = shape.binds.first() {
                            look_up = Some(bind.index);
                        }
                    }
                    if look_down.is_none() && (preset == "lookdown" || preset == "lookDown") {
                        if let Some(bind) = shape.binds.first() {
                            look_down = Some(bind.index);
                        }
                    }
                    if look_left.is_none() && (preset == "lookleft" || preset == "lookLeft") {
                        if let Some(bind) = shape.binds.first() {
                            look_left = Some(bind.index);
                        }
                    }
                    if look_right.is_none() && (preset == "lookright" || preset == "lookRight") {
                        if let Some(bind) = shape.binds.first() {
                            look_right = Some(bind.index);
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

/// Animate mouth morph targets + subtle head nod during speech.
///
/// Reads from ActiveSpeechClips (unified timeline per slot) for synchronized
/// mouth + head animation. Auto-stops when clip duration expires — no external
/// tokio::spawn needed.
///
/// Like a game engine animation system: one clip drives all speech-related
/// attributes from a single timeline. Clips can be interrupted by new speech
/// or StopSpeech commands.
#[allow(clippy::too_many_arguments)]
fn animate_speaking(
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

    // Auto-stop expired clips and remove Speaking component.
    // This replaces the tokio::spawn approach — Bevy manages its own lifecycle.
    let mut expired: Vec<u8> = Vec::new();
    for (&slot, clip) in &speech_clips.clips {
        let elapsed_ms = ((now - clip.start_time) * 1000.0) as u64;
        if elapsed_ms > clip.duration_ms + 200 {
            // 200ms grace for audio pipeline latency
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

    // Periodic stats flush — accumulate in memory, log every ~20s (300 frames at 15fps).
    // Never log from the hot path per frame.
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

            // Mouth weight from clip (priority) or legacy path or sine fallback.
            let mouth_weight = if let Some(clip) = speech_clips.clips.get(slot) {
                let elapsed = now - clip.start_time;
                let t = elapsed * 1000.0 / clip.interval_ms as f32;
                let idx = t as usize;
                if idx >= clip.mouth_weights.len() {
                    0.0
                } else if idx + 1 < clip.mouth_weights.len() {
                    // Lerp between consecutive weights for smooth continuous movement
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
                // Sine fallback (Speaking set but no clip data — should be brief)
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

        // Head nod during speech — driven by clip or Speaking flag.
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

/// Animate emotional expressions via discovered VRM blend shapes.
///
/// Smoothly transitions between emotions with lerp. Auto-decays to neutral
/// after EMOTION_DECAY_SECS without refresh. During active speech, expression
/// weight is attenuated to SPEECH_ATTENUATION to avoid fighting mouth animation.
fn animate_expression(
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

        // Auto-decay: count down timer, then fade to neutral
        if state.decay_timer > 0.0 {
            state.decay_timer -= dt;
            if state.decay_timer <= 0.0 {
                state.target = Emotion::Neutral;
                state.target_weight = 0.0;
                state.transition_rate = 1.0; // gentle 1s fade-out
            }
        }

        // Cross-fade: if switching to a different emotion, fade old out first
        if state.target != state.current && state.current_weight > 0.01 {
            // Fade current emotion out
            state.current_weight = (state.current_weight - state.transition_rate * dt).max(0.0);
            if state.current_weight <= 0.01 {
                state.current_weight = 0.0;
                state.current = state.target;
            }
        } else {
            // Same emotion (or old is faded) — lerp toward target
            state.current = state.target;
            if state.current_weight < state.target_weight {
                state.current_weight =
                    (state.current_weight + state.transition_rate * dt).min(state.target_weight);
            } else if state.current_weight > state.target_weight {
                state.current_weight =
                    (state.current_weight - state.transition_rate * dt).max(state.target_weight);
            }
        }

        // Speech attenuation: reduce expression during active speech
        let is_speaking = speaking_slots.contains(slot) || speech_clips.clips.contains_key(slot);
        let effective_weight = if is_speaking {
            state.current_weight * SPEECH_ATTENUATION
        } else {
            state.current_weight
        };

        if state.current == Emotion::Neutral || effective_weight < 0.001 {
            continue;
        }

        // Apply the blend shape weight for the current emotion
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
            state.blink_frames_remaining = 3; // 3 frames at 15fps = 200ms blink
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

        // Skip idle gestures while a body gesture is active (arm gesture takes priority).
        // But DON'T skip for speaking — we still want head turn toward speaker for non-speakers,
        // and forward-facing presenter pose for speakers.
        if active_gestures.slots.contains_key(slot) {
            continue;
        }

        let slot_bones = match bone_registry.slots.get(slot) {
            Some(b) => b,
            None => continue,
        };

        // Initialize gesture state with unique phase offset per slot
        let gesture = gesture_state
            .slots
            .entry(*slot)
            .or_insert_with(|| SlotGestureState {
                phase: *slot as f32 * 2.37,
                head_turn_current: 0.0,
                head_turn_target: 0.0,
            });

        let t = time.elapsed_secs() + gesture.phase;

        // Compute head turn target toward active speaker.
        // Non-speaking: turn head toward whoever is speaking.
        // Speaking: face forward (slight drift, "presenter" pose).
        if is_speaking {
            gesture.head_turn_target = 0.0; // Face camera
        } else if !speaking_slots.is_empty() {
            // Average direction toward all active speakers
            let turn_bias: f32 = speaking_slots
                .iter()
                .map(|&s| {
                    let diff = s as f32 - *slot as f32;
                    diff.signum() * 0.15 // ~8.5° per speaker direction
                })
                .sum::<f32>()
                .clamp(-0.25, 0.25); // Max ~14° turn
            gesture.head_turn_target = turn_bias;
        } else {
            gesture.head_turn_target = 0.0; // No speaker, face forward
        }

        // Smooth interpolation: exponential decay for natural head movement.
        // current = lerp(current, target, 1 - e^(-dt * speed))
        // speed=3.0 → 95% there in ~1 second
        let lerp_factor = 1.0 - (-dt * 3.0_f32).exp();
        gesture.head_turn_current +=
            (gesture.head_turn_target - gesture.head_turn_current) * lerp_factor;

        // Skip idle oscillation while speaking (head nod in animate_speaking takes priority)
        if is_speaking {
            continue;
        }

        // 1. Neck micro-tilt + speaker-directed head turn.
        //    COMPOSES delta rotation onto the bone's rest rotation (not replacing it!)
        //    Combines 3 sine waves at incommensurate frequencies (non-repeating pattern)
        //    plus the smooth head turn toward active speaker.
        if let Some(ref neck) = slot_bones.neck {
            if let Ok(mut transform) = transforms.get_mut(neck.entity) {
                let tilt_x = (t * 0.15).sin() * 0.03           // Very slow nod
                    + (t * 0.23).cos() * 0.02                   // Slower lateral component
                    + (t * 0.37).sin() * 0.01; // Subtle high-freq detail
                let tilt_z = (t * 0.12).cos() * 0.025           // Slow lateral head tilt
                    + (t * 0.31).sin() * 0.015; // Detail frequency
                                                // Head turn: idle drift + speaker-directed turn
                let idle_turn = (t * 0.08).sin() * 0.02;
                let turn_y = idle_turn + gesture.head_turn_current;

                let delta = Quat::from_euler(EulerRot::XYZ, tilt_x, turn_y, tilt_z);
                transform.rotation = neck.rest_rotation * delta;
            }
        }

        // 2. Shoulder micro-shifts — opposite phase for natural weight distribution.
        //    Adds tiny Y delta to the bone's actual local rest translation.
        if let Some(ref left_shoulder) = slot_bones.left_shoulder {
            if let Ok(mut transform) = transforms.get_mut(left_shoulder.entity) {
                let shift = (t * 0.4).sin() * 0.002             // Primary breathing frequency
                    + (t * 0.17).cos() * 0.001; // Slow drift
                transform.translation.y = left_shoulder.rest_translation.y + shift;
            }
        }
        if let Some(ref right_shoulder) = slot_bones.right_shoulder {
            if let Ok(mut transform) = transforms.get_mut(right_shoulder.entity) {
                // Opposite phase from left shoulder (natural body mechanics)
                let shift =
                    (t * 0.4 + std::f32::consts::PI).sin() * 0.002 + (t * 0.17 + 1.0).cos() * 0.001;
                transform.translation.y = right_shoulder.rest_translation.y + shift;
            }
        }
    }
}

/// Subtle breathing animation — gentle spine/chest oscillation + weight shift.
/// Uses cached spine bone from BoneRegistry (no per-frame tree traversal).
/// Composes delta rotation onto the bone's rest rotation (preserves bind pose).
fn animate_breathing(
    time: Res<Time>,
    registry: Res<SlotRegistry>,
    bone_registry: Res<BoneRegistry>,
    mut transforms: Query<&mut Transform>,
) {
    for (slot, state) in &registry.slots {
        if !state.active {
            continue;
        }

        // Use cached spine bone from BoneRegistry (eliminates 420 tree traversals/sec)
        let spine = match bone_registry.slots.get(slot).and_then(|b| b.spine.as_ref()) {
            Some(s) => s,
            None => continue,
        };

        if let Ok(mut transform) = transforms.get_mut(spine.entity) {
            let t = time.elapsed_secs() + *slot as f32 * 1.1; // Phase offset per slot

            // Breathing: Y scale variation (chest expanding/contracting)
            // Increased from ±0.003 to ±0.005 for visibility at 15fps readback
            let breath = (t * 0.8 * std::f32::consts::TAU).sin() * 0.005;
            transform.scale.y = 1.0 + breath;

            // Weight shift: slow lateral sway — compose delta onto rest rotation
            // (was: overwriting rotation entirely, breaking bind pose)
            let sway = (t * 0.12).sin() * 0.012;
            let delta = Quat::from_rotation_z(sway);
            transform.rotation = spine.rest_rotation * delta;
        }
    }
}

/// Animate eye gaze via look blend shapes. Creates a "living eyes" effect with:
/// 1. Idle drift: slow random movement using layered oscillators
/// 2. Look toward speaker: when another slot is speaking, eyes drift that direction
/// 3. Look at camera when speaking: engaging the viewer during own speech
///
/// Uses lookUp/lookDown/lookLeft/lookRight blend shapes discovered from VRM presets.
fn animate_eye_gaze(
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

    // Iterate over all active slots (not just those with morph targets)
    for (slot, state) in &registry.slots {
        if !state.active {
            continue;
        }

        let is_speaking = speaking_slots.contains(slot);
        let phase = *slot as f32 * 2.73; // Unique offset per slot

        // Compute gaze target (x = left/right, y = up/down), range -1..1
        let (gaze_x, gaze_y) = if is_speaking {
            // Look at camera when speaking (small drift for naturalness)
            let drift_x = (t * 0.3 + phase).sin() * 0.05;
            let drift_y = (t * 0.25 + phase).cos() * 0.03;
            (drift_x, drift_y)
        } else {
            // Find which direction a speaker is (relative to this slot's position)
            let speaker_bias: f32 = speaking_slots
                .iter()
                .map(|&s| {
                    let diff = s as f32 - *slot as f32;
                    diff.signum() * 0.15 // Subtle bias toward speaker direction
                })
                .sum::<f32>()
                .clamp(-0.3, 0.3);

            // Idle gaze drift — layered oscillators for organic movement
            let drift_x = (t * 0.13 + phase).sin() * 0.12
                + (t * 0.07 + phase * 0.7).cos() * 0.08
                + speaker_bias;
            let drift_y = (t * 0.11 + phase).cos() * 0.08 + (t * 0.19 + phase * 1.3).sin() * 0.05;
            (drift_x.clamp(-0.4, 0.4), drift_y.clamp(-0.3, 0.3))
        };

        // Path 1: Bone-based eye gaze (VRM lookAtTypeName: "Bone")
        // Eye bones rotate directly — this is what most VRM models actually use.
        let mut used_bone_gaze = false;
        if let Some(slot_bones) = bone_registry.slots.get(slot) {
            if slot_bones.left_eye.is_some() && slot_bones.right_eye.is_some() {
                let config = slot_bones.look_at_config.unwrap_or_default();

                // Map gaze_x/gaze_y (-1..1) to eye bone rotation.
                // Use average of inner/outer for horizontal (both eyes look the same direction).
                let h_deg = (config.horizontal_inner_deg + config.horizontal_outer_deg) * 0.5;
                let v_up_deg = config.vertical_up_deg;
                let v_down_deg = config.vertical_down_deg;

                // Horizontal: Y-rotation (positive = look right in VRM's -Z forward convention)
                let yaw_rad = gaze_x * h_deg.to_radians();
                // Vertical: X-rotation (negative = look up, positive = look down)
                let pitch_rad = if gaze_y >= 0.0 {
                    -gaze_y * v_up_deg.to_radians() // Looking up
                } else {
                    -gaze_y * v_down_deg.to_radians() // Looking down (gaze_y is negative)
                };

                let gaze_delta = Quat::from_euler(EulerRot::XYZ, pitch_rad, yaw_rad, 0.0);

                // Apply to both eyes (conjugate gaze — both look at same point)
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

        // Path 2: Blend shape gaze (fallback for models without eye bones)
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

                    // Horizontal: negative = look left, positive = look right
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

                    // Vertical: negative = look down, positive = look up
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

/// Smoothstep easing function (cubic Hermite interpolation). Maps 0→0, 1→1
/// with zero derivative at both endpoints — natural acceleration/deceleration.
fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Cognitive gesture driver — selects and triggers gestures based on cognitive state.
///
/// Runs BEFORE animate_body_gestures. Checks each slot's cognitive state and,
/// when the re-roll interval expires and no gesture is active, selects a new
/// weighted-random gesture. Yields to speech gestures (skips when Speaking).
fn drive_cognitive_gestures(
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

        // Skip if currently speaking (speech gestures have priority)
        if speaking_slots.contains(slot) {
            continue;
        }

        // Skip if a gesture is still playing (let it finish naturally)
        if active_gestures.slots.contains_key(slot) {
            continue;
        }

        // Check if it's time to re-roll
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
///
/// Each gesture has three phases: Attack (ease in), Sustain (hold with micro-oscillation),
/// Release (ease out). Arm rotations compose onto rest_rotation from BoneInfo —
/// never replaces the bind pose. Idle gestures automatically resume when gesture ends.
///
/// Gesture and speech are complementary: speech drives head nod + mouth, gestures drive arms.
/// Both can be active simultaneously without conflict.
fn animate_body_gestures(
    time: Res<Time>,
    bone_registry: Res<BoneRegistry>,
    mut active_gestures: ResMut<ActiveGestures>,
    mut transforms: Query<&mut Transform>,
) {
    let dt = time.delta_secs();
    let now = time.elapsed_secs();

    // Collect slots to remove after iteration (gesture completed)
    let mut finished: Vec<u8> = Vec::new();

    for (slot, anim) in active_gestures.slots.iter_mut() {
        anim.elapsed += dt;

        // Phase transitions
        let attack_end = GESTURE_EASE_SECS;
        let sustain_end = anim.duration_secs - GESTURE_EASE_SECS;
        let total_end = anim.duration_secs;

        if anim.elapsed >= total_end {
            finished.push(*slot);
            continue;
        }

        // Compute blend weight based on phase
        anim.weight = if anim.elapsed < attack_end {
            // Attack: ease in
            anim.phase = GesturePhase::Attack;
            smoothstep(anim.elapsed / GESTURE_EASE_SECS)
        } else if anim.elapsed < sustain_end {
            // Sustain: full weight with subtle oscillation
            anim.phase = GesturePhase::Sustain;
            1.0
        } else {
            // Release: ease out
            anim.phase = GesturePhase::Release;
            let release_progress = (anim.elapsed - sustain_end) / GESTURE_EASE_SECS;
            1.0 - smoothstep(release_progress)
        };

        let slot_bones = match bone_registry.slots.get(slot) {
            Some(b) => b,
            None => continue,
        };

        let w = anim.weight;
        let t = now + *slot as f32 * 1.7; // Phase offset per slot for micro-oscillation

        match anim.gesture {
            Gesture::Wave => {
                // Right upper arm up ~90°, right forearm oscillates ±20° at 2Hz
                if let Some(ref rua) = slot_bones.right_upper_arm {
                    if let Ok(mut transform) = transforms.get_mut(rua.entity) {
                        // Rotate up (negative Z = up for right arm, already T-pose fixed)
                        let up_angle = -1.2 * w; // ~69° up from resting
                        let delta = Quat::from_rotation_z(up_angle);
                        transform.rotation = rua.rest_rotation * delta;
                    }
                }
                if let Some(ref rla) = slot_bones.right_lower_arm {
                    if let Ok(mut transform) = transforms.get_mut(rla.entity) {
                        // Oscillating wave motion on forearm
                        let wave = if anim.phase == GesturePhase::Sustain {
                            (t * 2.0 * std::f32::consts::TAU).sin() * 0.35
                        } else {
                            0.0
                        };
                        let bend = (-0.5 + wave) * w; // Bent + wave
                        let delta = Quat::from_rotation_z(bend);
                        transform.rotation = rla.rest_rotation * delta;
                    }
                }
            }
            Gesture::Think => {
                // Right arm forward and bent — hand near chin
                if let Some(ref rua) = slot_bones.right_upper_arm {
                    if let Ok(mut transform) = transforms.get_mut(rua.entity) {
                        let forward = Quat::from_rotation_x(-0.8 * w); // Forward ~45°
                        let inward = Quat::from_rotation_z(-0.3 * w); // Slightly inward
                        let delta = forward * inward;
                        transform.rotation = rua.rest_rotation * delta;
                    }
                }
                if let Some(ref rla) = slot_bones.right_lower_arm {
                    if let Ok(mut transform) = transforms.get_mut(rla.entity) {
                        let bend = Quat::from_rotation_z(2.0 * w); // Tight bend ~115°
                        transform.rotation = rla.rest_rotation * bend;
                    }
                }
                // Slight head tilt for "thinking" look
                if let Some(ref head) = slot_bones.head {
                    if let Ok(mut transform) = transforms.get_mut(head.entity) {
                        let tilt = Quat::from_euler(EulerRot::XYZ, 0.05 * w, 0.0, 0.08 * w);
                        transform.rotation = head.rest_rotation * tilt;
                    }
                }
            }
            Gesture::Nod => {
                // Emphatic head nod — stronger than speech nod
                if let Some(ref head) = slot_bones.head {
                    if let Ok(mut transform) = transforms.get_mut(head.entity) {
                        let nod = (t * 1.5 * std::f32::consts::TAU).sin() * 0.12 * w;
                        let delta = Quat::from_rotation_x(nod);
                        transform.rotation = head.rest_rotation * delta;
                    }
                }
            }
            Gesture::Shrug => {
                // Both shoulders up, both arms slightly out
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
                // Arms slightly out
                if let Some(ref lua) = slot_bones.left_upper_arm {
                    if let Ok(mut transform) = transforms.get_mut(lua.entity) {
                        let out = Quat::from_rotation_z(-0.35 * w); // Left arm out
                        transform.rotation = lua.rest_rotation * out;
                    }
                }
                if let Some(ref rua) = slot_bones.right_upper_arm {
                    if let Ok(mut transform) = transforms.get_mut(rua.entity) {
                        let out = Quat::from_rotation_z(0.35 * w); // Right arm out
                        transform.rotation = rua.rest_rotation * out;
                    }
                }
            }
            Gesture::Point => {
                // Right arm extended forward
                if let Some(ref rua) = slot_bones.right_upper_arm {
                    if let Ok(mut transform) = transforms.get_mut(rua.entity) {
                        let forward = Quat::from_rotation_x(-1.05 * w); // Forward ~60°
                        transform.rotation = rua.rest_rotation * forward;
                    }
                }
                if let Some(ref rla) = slot_bones.right_lower_arm {
                    if let Ok(mut transform) = transforms.get_mut(rla.entity) {
                        // Straighten forearm (undo the resting bend)
                        let straighten = Quat::from_rotation_z(0.26 * w); // Cancel ~15° resting bend
                        transform.rotation = rla.rest_rotation * straighten;
                    }
                }
            }
            Gesture::OpenHands => {
                // Both arms slightly out and forward, forearms open
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
                // Slight oscillation during sustain
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
    let arm_names: Vec<&str> = bone_names
        .iter()
        .filter(|n| n.to_lowercase().contains("arm"))
        .map(|s| s.as_str())
        .collect();
    if !arm_names.is_empty() {
        bevy_debug(&format!("Arm bones found: {:?}", arm_names));
    } else {
        // Log all names if no arm bones found (different naming convention)
        bevy_debug(&format!(
            "No 'arm' bones found. All {} named entities: {:?}",
            bone_names.len(),
            bone_names.iter().take(50).collect::<Vec<_>>()
        ));
    }
}

fn collect_names(
    entity: Entity,
    children: &Query<&Children>,
    names: &Query<&Name>,
    out: &mut Vec<String>,
) {
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

    if adjustments.is_empty() {
        clog_warn!("🎨 T-pose fix: no arm bones found — model may use unknown naming convention");
    } else {
        clog_info!(
            "🎨 T-pose fix: {} arm bone adjustments applied",
            adjustments.len()
        );
    }

    for (bone_entity, rotation) in adjustments {
        if let Ok(mut transform) = transforms.get_mut(bone_entity) {
            transform.rotation *= rotation;
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
        let name_lower = name_str.to_lowercase();

        // Detect upper arm bones and rotate downward (~65 degrees).
        // VRoid: "J_Bip_L_UpperArm", "J_Sec_L_UpperArm"
        // Mixamo: "mixamorig:LeftArm", "vis_char_056:mixamorig:LeftArm"
        // 100Avatars/Generic: "LeftUpperArm", "Left_UpperArm", "leftupperarm"
        // Blender Rigify: "upper_arm.L", "upper_arm.R" (dot-suffix laterality)
        // Webaverse/quappa: "Upperarm_L", "Upperarm_R" (underscore-suffix laterality)
        // Hand-rigged: "Left arm", "Right arm" (space-separated, exact match)
        // Humanoid: any name ending in "upperarm" or "upper_arm" with left/L hint
        let is_left_upper = name_str.contains("J_Bip_L_UpperArm")
            || name_str.contains("J_Sec_L_UpperArm")
            || (name_str.contains("mixamorig:LeftArm") && !name_str.contains("ForeArm"))
            || name_str == "LeftUpperArm"
            || name_str == "Left_UpperArm"
            || name_str == "Left arm"
            || name_str == "Upperarm_L"
            || name_str == "upper_arm.L"
            || (name_lower.ends_with("upperarm")
                && (name_lower.contains("left") || name_lower.contains("_l_")))
            || (name_lower.ends_with("upper_arm")
                && (name_lower.contains("left") || name_lower.contains("_l_")));
        let is_right_upper = name_str.contains("J_Bip_R_UpperArm")
            || name_str.contains("J_Sec_R_UpperArm")
            || (name_str.contains("mixamorig:RightArm") && !name_str.contains("ForeArm"))
            || name_str == "RightUpperArm"
            || name_str == "Right_UpperArm"
            || name_str == "Right arm"
            || name_str == "Upperarm_R"
            || name_str == "upper_arm.R"
            || (name_lower.ends_with("upperarm")
                && (name_lower.contains("right") || name_lower.contains("_r_")))
            || (name_lower.ends_with("upper_arm")
                && (name_lower.contains("right") || name_lower.contains("_r_")));

        // Detect lower arm / forearm bones — slight bend for natural look.
        // VRoid: "J_Bip_L_LowerArm"
        // Mixamo: "mixamorig:LeftForeArm"
        // 100Avatars/Generic: "LeftLowerArm", "Left_LowerArm", "leftlowerarm"
        // Blender Rigify: "lower_arm.L", "lower_arm.R"
        // Webaverse/quappa: "Lowerarm_L", "Lowerarm_R"
        // Hand-rigged: "Left elbow", "Right elbow" (elbow = forearm bone in simple rigs)
        let is_left_lower = name_str.contains("J_Bip_L_LowerArm")
            || name_str.contains("J_Sec_L_LowerArm")
            || name_str.contains("mixamorig:LeftForeArm")
            || name_str == "LeftLowerArm"
            || name_str == "Left_LowerArm"
            || name_str == "LeftForeArm"
            || name_str == "Left elbow"
            || name_str == "Lowerarm_L"
            || name_str == "lower_arm.L"
            || (name_lower.ends_with("lowerarm")
                && (name_lower.contains("left") || name_lower.contains("_l_")))
            || (name_lower.ends_with("forearm")
                && (name_lower.contains("left") || name_lower.contains("_l_")));
        let is_right_lower = name_str.contains("J_Bip_R_LowerArm")
            || name_str.contains("J_Sec_R_LowerArm")
            || name_str.contains("mixamorig:RightForeArm")
            || name_str == "RightLowerArm"
            || name_str == "Right_LowerArm"
            || name_str == "RightForeArm"
            || name_str == "Right elbow"
            || name_str == "Lowerarm_R"
            || name_str == "lower_arm.R"
            || (name_lower.ends_with("lowerarm")
                && (name_lower.contains("right") || name_lower.contains("_r_")))
            || (name_lower.ends_with("forearm")
                && (name_lower.contains("right") || name_lower.contains("_r_")));

        if is_left_upper {
            // Rotate left upper arm down ~65 degrees (positive Z in VRM local space)
            adjustments.push((entity, Quat::from_rotation_z(1.13)));
            bevy_debug(&format!(
                "T-pose fix: left upper arm '{}' → rotate Z +65°",
                name_str
            ));
        } else if is_right_upper {
            // Rotate right upper arm down ~65 degrees (negative Z in VRM local space)
            adjustments.push((entity, Quat::from_rotation_z(-1.13)));
            bevy_debug(&format!(
                "T-pose fix: right upper arm '{}' → rotate Z -65°",
                name_str
            ));
        } else if is_left_lower {
            // Slight bend at elbow for natural look (~15 degrees)
            adjustments.push((entity, Quat::from_rotation_z(0.26)));
            bevy_debug(&format!(
                "T-pose fix: left lower arm '{}' → rotate Z +15°",
                name_str
            ));
        } else if is_right_lower {
            adjustments.push((entity, Quat::from_rotation_z(-0.26)));
            bevy_debug(&format!(
                "T-pose fix: right lower arm '{}' → rotate Z -15°",
                name_str
            ));
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
    let chunk_length = u32::from_le_bytes([
        chunk_header[0],
        chunk_header[1],
        chunk_header[2],
        chunk_header[3],
    ]) as usize;
    let chunk_type = u32::from_le_bytes([
        chunk_header[4],
        chunk_header[5],
        chunk_header[6],
        chunk_header[7],
    ]);
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
fn parse_vrm0x_blend_shapes(
    root: &serde_json::Value,
    glb_path: &str,
) -> Option<Vec<VrmBlendShape>> {
    let groups = root
        .get("extensions")?
        .get("VRM")?
        .get("blendShapeMaster")?
        .get("blendShapeGroups")?
        .as_array()?;

    let mut shapes = Vec::new();
    for group in groups {
        let name = group
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let preset_name = group
            .get("presetName")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let binds_arr = group.get("binds").and_then(|v| v.as_array());
        let mut binds = Vec::new();
        if let Some(binds_arr) = binds_arr {
            for bind in binds_arr {
                let mesh = bind.get("mesh").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                let index = bind.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                let weight = bind.get("weight").and_then(|v| v.as_f64()).unwrap_or(100.0) as f32;
                binds.push(VrmBlendShapeBind {
                    mesh,
                    index,
                    weight,
                });
            }
        }
        shapes.push(VrmBlendShape {
            name,
            preset_name,
            binds,
        });
    }

    bevy_debug(&format!(
        "VRM 0.x blend shapes from '{}': {} groups — {:?}",
        glb_path,
        shapes.len(),
        shapes
            .iter()
            .map(|s| format!("{}({})[{}binds]", s.name, s.preset_name, s.binds.len()))
            .collect::<Vec<_>>()
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
                binds.push(VrmBlendShapeBind {
                    mesh,
                    index,
                    weight,
                });
            }
        }
        shapes.push(VrmBlendShape {
            name: preset_name.clone(),
            preset_name: preset_name.clone(),
            binds,
        });
    }

    bevy_debug(&format!(
        "VRM 1.0 expressions from '{}': {} presets — {:?}",
        glb_path,
        shapes.len(),
        shapes
            .iter()
            .map(|s| format!("{}[{}binds]", s.preset_name, s.binds.len()))
            .collect::<Vec<_>>()
    ));

    Some(shapes)
}

/// Parse VRM humanoid bone mapping from the .glb JSON extensions.
/// Returns a map of VRM bone name (e.g. "leftEye") → glTF node name.
/// Works with both VRM 0.x (extensions.VRM.humanoid.humanBones) and
/// VRM 1.0 (extensions.VRMC_vrm.humanoid.humanBones).
fn parse_vrm_humanoid_bones(glb_path: &str) -> HashMap<String, String> {
    let root = match read_glb_json(glb_path) {
        Some(r) => r,
        None => return HashMap::new(),
    };

    // Get the nodes array for resolving node index → node name
    let nodes = root.get("nodes").and_then(|v| v.as_array());

    let resolve_node_name = |node_index: u64| -> Option<String> {
        nodes
            .and_then(|n| n.get(node_index as usize))
            .and_then(|node| node.get("name"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    };

    let mut bone_map = HashMap::new();

    // Try VRM 0.x: extensions.VRM.humanoid.humanBones (array of {bone, node, ...})
    if let Some(human_bones) = root
        .get("extensions")
        .and_then(|e| e.get("VRM"))
        .and_then(|v| v.get("humanoid"))
        .and_then(|h| h.get("humanBones"))
        .and_then(|b| b.as_array())
    {
        for bone_entry in human_bones {
            let bone_name = bone_entry.get("bone").and_then(|v| v.as_str());
            let node_idx = bone_entry.get("node").and_then(|v| v.as_u64());
            if let (Some(name), Some(idx)) = (bone_name, node_idx) {
                if let Some(node_name) = resolve_node_name(idx) {
                    bone_map.insert(name.to_string(), node_name);
                }
            }
        }
        if !bone_map.is_empty() {
            bevy_debug(&format!(
                "VRM 0.x humanoid bones from '{}': {} bones",
                glb_path,
                bone_map.len()
            ));
            return bone_map;
        }
    }

    // Try VRM 1.0: extensions.VRMC_vrm.humanoid.humanBones (object of {boneName: {node: idx}})
    if let Some(human_bones) = root
        .get("extensions")
        .and_then(|e| e.get("VRMC_vrm"))
        .and_then(|v| v.get("humanoid"))
        .and_then(|h| h.get("humanBones"))
        .and_then(|b| b.as_object())
    {
        for (bone_name, bone_data) in human_bones {
            let node_idx = bone_data.get("node").and_then(|v| v.as_u64());
            if let Some(idx) = node_idx {
                if let Some(node_name) = resolve_node_name(idx) {
                    bone_map.insert(bone_name.clone(), node_name);
                }
            }
        }
        if !bone_map.is_empty() {
            bevy_debug(&format!(
                "VRM 1.0 humanoid bones from '{}': {} bones",
                glb_path,
                bone_map.len()
            ));
        }
    }

    bone_map
}

/// Parse VRM lookAt configuration from the .glb JSON extensions.
/// Returns the eye rotation ranges used for bone-based gaze.
fn parse_vrm_look_at_config(glb_path: &str) -> Option<VrmLookAtConfig> {
    let root = read_glb_json(glb_path)?;

    // VRM 0.x: extensions.VRM.firstPerson.lookAtTypeName + lookAtHorizontalInner/Outer/VerticalUp/Down
    if let Some(first_person) = root
        .get("extensions")
        .and_then(|e| e.get("VRM"))
        .and_then(|v| v.get("firstPerson"))
    {
        let look_at_type = first_person
            .get("lookAtTypeName")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // Only parse bone config if lookAtTypeName is "Bone" (not "BlendShape")
        if look_at_type != "Bone" {
            bevy_debug(&format!(
                "VRM lookAt type '{}' — not bone-based",
                look_at_type
            ));
            return None;
        }

        let get_output = |key: &str| -> f32 {
            first_person
                .get(key)
                .and_then(|v| v.get("yRange"))
                .and_then(|v| v.as_f64())
                .unwrap_or(8.0) as f32
        };

        let config = VrmLookAtConfig {
            horizontal_inner_deg: get_output("lookAtHorizontalInner"),
            horizontal_outer_deg: get_output("lookAtHorizontalOuter"),
            vertical_up_deg: get_output("lookAtVerticalUp"),
            vertical_down_deg: get_output("lookAtVerticalDown"),
        };
        bevy_debug(&format!(
            "VRM lookAt config: inner={:.1}° outer={:.1}° up={:.1}° down={:.1}°",
            config.horizontal_inner_deg,
            config.horizontal_outer_deg,
            config.vertical_up_deg,
            config.vertical_down_deg
        ));
        return Some(config);
    }

    // VRM 1.0: extensions.VRMC_vrm.lookAt.type + rangeMapHorizontalInner/Outer/VerticalUp/Down
    if let Some(look_at) = root
        .get("extensions")
        .and_then(|e| e.get("VRMC_vrm"))
        .and_then(|v| v.get("lookAt"))
    {
        let look_at_type = look_at.get("type").and_then(|v| v.as_str()).unwrap_or("");

        if look_at_type != "bone" {
            bevy_debug(&format!(
                "VRMC lookAt type '{}' — not bone-based",
                look_at_type
            ));
            return None;
        }

        let get_output = |key: &str| -> f32 {
            look_at
                .get(key)
                .and_then(|v| v.get("outputScale"))
                .and_then(|v| v.as_f64())
                .unwrap_or(8.0) as f32
        };

        let config = VrmLookAtConfig {
            horizontal_inner_deg: get_output("rangeMapHorizontalInner"),
            horizontal_outer_deg: get_output("rangeMapHorizontalOuter"),
            vertical_up_deg: get_output("rangeMapVerticalUp"),
            vertical_down_deg: get_output("rangeMapVerticalDown"),
        };
        return Some(config);
    }

    None
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
    model_path: &str,
    children: &Query<&Children>,
    names: &Query<&Name>,
    transforms: &Query<&mut Transform>,
    bone_registry: &mut ResMut<BoneRegistry>,
) {
    let head_names = ["J_Bip_C_Head", "mixamorig:Head", "Head"];
    let neck_names = ["J_Bip_C_Neck", "mixamorig:Neck", "Neck"];
    let spine_names = ["J_Bip_C_Spine", "mixamorig:Spine", "Spine"];
    let left_shoulder_names = [
        "J_Bip_L_Shoulder",
        "mixamorig:LeftShoulder",
        "LeftShoulder",
        "Left shoulder",
        "Shoulder_L",
        "Shoulder.L",
    ];
    let right_shoulder_names = [
        "J_Bip_R_Shoulder",
        "mixamorig:RightShoulder",
        "RightShoulder",
        "Right shoulder",
        "Shoulder_R",
        "Shoulder.R",
    ];
    // Arm bone naming conventions across model sources:
    // VRoid (J_Bip_*), Mixamo (mixamorig:*), Generic (LeftUpperArm),
    // Blender Rigify (upper_arm.L), Webaverse/quappa (Upperarm_L), Hand-rigged (Left arm)
    // Note: mixamorig patterns use contains() so prefixed names like "vis_char_056:mixamorig:LeftArm" match.
    let left_upper_arm_names = [
        "J_Bip_L_UpperArm",
        "mixamorig:LeftArm",
        "LeftUpperArm",
        "Left_UpperArm",
        "Left arm",
        "Upperarm_L",
        "upper_arm.L",
    ];
    let right_upper_arm_names = [
        "J_Bip_R_UpperArm",
        "mixamorig:RightArm",
        "RightUpperArm",
        "Right_UpperArm",
        "Right arm",
        "Upperarm_R",
        "upper_arm.R",
    ];
    let left_lower_arm_names = [
        "J_Bip_L_LowerArm",
        "mixamorig:LeftForeArm",
        "LeftLowerArm",
        "Left_LowerArm",
        "LeftForeArm",
        "Left elbow",
        "Lowerarm_L",
        "lower_arm.L",
    ];
    let right_lower_arm_names = [
        "J_Bip_R_LowerArm",
        "mixamorig:RightForeArm",
        "RightLowerArm",
        "Right_LowerArm",
        "RightForeArm",
        "Right elbow",
        "Lowerarm_R",
        "lower_arm.R",
    ];

    // Helper: find bone and capture its actual local-space rest transform.
    // This is critical — bone transforms are LOCAL (relative to parent bone),
    // NOT world space. Gesture animations must compose with these rest values.
    let discover = |target_names: &[&str], label: &str| -> Option<BoneInfo> {
        find_bone_by_name(root, children, names, target_names).and_then(|entity| {
            if let Ok(t) = transforms.get(entity) {
                bevy_debug(&format!(
                    "{} bone slot {}: entity {:?}, local_pos={:?}, local_rot={:?}",
                    label, slot, entity, t.translation, t.rotation
                ));
                Some(BoneInfo {
                    entity,
                    rest_translation: t.translation,
                    rest_rotation: t.rotation,
                })
            } else {
                bevy_debug(&format!(
                    "{} bone slot {}: entity {:?} — no Transform!",
                    label, slot, entity
                ));
                None
            }
        })
    };

    let head = discover(&head_names, "Head");
    let neck = discover(&neck_names, "Neck");
    let spine = discover(&spine_names, "Spine");
    let left_shoulder = discover(&left_shoulder_names, "L.Shoulder");
    let right_shoulder = discover(&right_shoulder_names, "R.Shoulder");
    let left_upper_arm = discover(&left_upper_arm_names, "L.UpperArm");
    let right_upper_arm = discover(&right_upper_arm_names, "R.UpperArm");
    let left_lower_arm = discover(&left_lower_arm_names, "L.LowerArm");
    let right_lower_arm = discover(&right_lower_arm_names, "R.LowerArm");

    // Discover eye and hand bones.
    // First try name-based discovery (works for all model types).
    let left_eye_names = [
        "J_Adj_L_FaceEye",
        "mixamorig:LeftEye",
        "LeftEye",
        "Eye_L",
        "eye.L",
    ];
    let right_eye_names = [
        "J_Adj_R_FaceEye",
        "mixamorig:RightEye",
        "RightEye",
        "Eye_R",
        "eye.R",
    ];
    let left_hand_names = [
        "J_Bip_L_Hand",
        "mixamorig:LeftHand",
        "LeftHand",
        "Hand_L",
        "hand.L",
    ];
    let right_hand_names = [
        "J_Bip_R_Hand",
        "mixamorig:RightHand",
        "RightHand",
        "Hand_R",
        "hand.R",
    ];

    let mut left_eye = discover(&left_eye_names, "L.Eye");
    let mut right_eye = discover(&right_eye_names, "R.Eye");
    let mut left_hand = discover(&left_hand_names, "L.Hand");
    let mut right_hand = discover(&right_hand_names, "R.Hand");

    // Parse VRM humanoid bone mapping for any bones that name-based discovery missed.
    // The VRM extension has an authoritative mapping: VRM bone name → glTF node index → node name.
    let vrm_bones = parse_vrm_humanoid_bones(model_path);
    if !vrm_bones.is_empty() {
        // Helper: discover a bone from VRM mapping by looking up its node name
        let vrm_discover = |vrm_name: &str, label: &str| -> Option<BoneInfo> {
            vrm_bones.get(vrm_name).and_then(|node_name| {
                find_bone_by_name(root, children, names, &[node_name.as_str()]).and_then(|entity| {
                    if let Ok(t) = transforms.get(entity) {
                        bevy_debug(&format!(
                            "{} bone slot {} (VRM '{}'→'{}'): entity {:?}",
                            label, slot, vrm_name, node_name, entity
                        ));
                        Some(BoneInfo {
                            entity,
                            rest_translation: t.translation,
                            rest_rotation: t.rotation,
                        })
                    } else {
                        None
                    }
                })
            })
        };

        // Fill in missing bones from VRM mapping
        if left_eye.is_none() {
            left_eye = vrm_discover("leftEye", "L.Eye");
        }
        if right_eye.is_none() {
            right_eye = vrm_discover("rightEye", "R.Eye");
        }
        if left_hand.is_none() {
            left_hand = vrm_discover("leftHand", "L.Hand");
        }
        if right_hand.is_none() {
            right_hand = vrm_discover("rightHand", "R.Hand");
        }
    }

    // Parse VRM lookAt config for bone-based eye gaze rotation ranges
    let look_at_config = parse_vrm_look_at_config(model_path);

    let upper_body_count = [&head, &neck, &spine, &left_shoulder, &right_shoulder]
        .iter()
        .filter(|b| b.is_some())
        .count();
    let arm_count = [
        &left_upper_arm,
        &right_upper_arm,
        &left_lower_arm,
        &right_lower_arm,
    ]
    .iter()
    .filter(|b| b.is_some())
    .count();
    let eye_count = [&left_eye, &right_eye]
        .iter()
        .filter(|b| b.is_some())
        .count();
    let hand_count = [&left_hand, &right_hand]
        .iter()
        .filter(|b| b.is_some())
        .count();
    clog_info!("🎨 Bone discovery slot {}: {}/5 upper body (head={} neck={} spine={} lsh={} rsh={}), {}/4 arms (lua={} rua={} lla={} rla={}), eyes={}/2, hands={}/2, lookAt={}",
        slot, upper_body_count,
        head.is_some(), neck.is_some(), spine.is_some(),
        left_shoulder.is_some(), right_shoulder.is_some(),
        arm_count,
        left_upper_arm.is_some(), right_upper_arm.is_some(),
        left_lower_arm.is_some(), right_lower_arm.is_some(),
        eye_count, hand_count, look_at_config.is_some());

    bone_registry.slots.insert(
        slot,
        SlotBones {
            head,
            neck,
            spine,
            left_shoulder,
            right_shoulder,
            left_upper_arm,
            right_upper_arm,
            left_lower_arm,
            right_lower_arm,
            left_eye,
            right_eye,
            left_hand,
            right_hand,
            look_at_config,
        },
    );
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
        assert!(
            MAX_AVATAR_SLOTS >= 14,
            "Need at least 14 slots for all personas"
        );
        // Default: 640×360 for non-spotlight tiles. HD pool provides 1280×720 for spotlight.
        assert_eq!(AVATAR_WIDTH, 640);
        assert_eq!(AVATAR_HEIGHT, 360);
        assert!(
            AVATAR_FPS >= 10.0 && AVATAR_FPS <= 60.0,
            "FPS must be 10-60 — below 10 looks choppy, above 60 wastes GPU"
        );
    }
}
