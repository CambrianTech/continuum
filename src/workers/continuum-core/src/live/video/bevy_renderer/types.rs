//! Bevy renderer types — all components, resources, enums, and structs.
//!
//! Centralized type definitions for the avatar rendering system.
//! Other sub-modules import from here.

use bevy::asset::Handle;
use bevy::gltf::Gltf;
use bevy::prelude::*;
use crossbeam_channel::{Receiver, Sender};
use std::collections::HashMap;
use std::sync::Arc;

use crate::gpu::memory_manager::GpuAllocationGuard;
use crate::live::avatar::RgbaFrame;

// =============================================================================
// Public Types (used by external modules)
// =============================================================================

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
    SetMouthWeight { slot: u8, weight: f32 },
    /// Set a pre-computed sequence of mouth weights for amplitude-responsive lip sync.
    SetMouthWeightSequence {
        slot: u8,
        weights: Vec<f32>,
        interval_ms: u32,
    },
    /// Play a complete speech animation clip — unified package for synchronized playback.
    PlaySpeech { slot: u8, clip: SpeechAnimationClip },
    /// Stop a speech animation immediately (e.g. interrupted by new speech).
    StopSpeech { slot: u8 },
    /// Set emotional expression on an avatar.
    SetEmotion {
        slot: u8,
        emotion: Emotion,
        weight: f32,
        transition_ms: u32,
    },
    /// Trigger a body gesture.
    SetGesture {
        slot: u8,
        gesture: Gesture,
        duration_ms: u32,
    },
    /// Set the cognitive state for an avatar slot.
    SetCognitiveState {
        slot: u8,
        state: crate::live::session::cognitive_animation::CognitiveState,
    },
    /// Resize a slot's render target to new dimensions.
    Resize { slot: u8, width: u32, height: u32 },
    /// Shut down the renderer gracefully.
    Shutdown,
}

/// Atomic stats updated by Bevy systems, read by MemoryReporter.
/// No locks — atomics only. Safe to read from any thread at any time.
pub struct BevyMemoryStats {
    /// Number of slots with active == true
    pub active_slots: std::sync::atomic::AtomicU8,
    /// Number of slots with model_loaded == true
    pub loaded_models: std::sync::atomic::AtomicU8,
    /// Number of slots currently speaking
    pub speaking_slots: std::sync::atomic::AtomicU8,
    /// Approximate bytes: render targets (width × height × 4 × slot_count)
    pub render_target_bytes: std::sync::atomic::AtomicU64,
    /// Pending load entries count
    pub pending_loads: std::sync::atomic::AtomicU32,
}

impl BevyMemoryStats {
    pub fn new() -> Self {
        Self {
            active_slots: std::sync::atomic::AtomicU8::new(0),
            loaded_models: std::sync::atomic::AtomicU8::new(0),
            speaking_slots: std::sync::atomic::AtomicU8::new(0),
            render_target_bytes: std::sync::atomic::AtomicU64::new(0),
            pending_loads: std::sync::atomic::AtomicU32::new(0),
        }
    }
}

// =============================================================================
// Bevy Resources (internal to renderer)
// =============================================================================

/// Channel for receiving commands from the main application.
#[derive(Resource)]
pub(super) struct CommandChannel(pub Receiver<AvatarCommand>);

/// Channels for sending rendered frames back to LiveKit video loops.
#[derive(Resource)]
pub(super) struct FrameChannels(pub Vec<Sender<RgbaFrame>>);

/// Per-slot frame-ready notifiers. Fired by the readback observer after writing
/// a frame (channel or GPU bridge path). Video loops await these.
#[derive(Resource)]
pub(crate) struct FrameNotifiers(pub Vec<Arc<tokio::sync::Notify>>);

/// Shared ready flag — set after Bevy Startup systems complete.
#[derive(Resource)]
pub(super) struct ReadyFlag(pub Arc<std::sync::atomic::AtomicBool>);

/// Tracks the state of each avatar render slot.
#[derive(Resource)]
pub(crate) struct SlotRegistry {
    pub slots: HashMap<u8, SlotState>,
}

pub(crate) struct SlotState {
    pub camera_entity: Entity,
    pub _readback_entity: Entity,
    pub scene_entity: Option<Entity>,
    /// Currently active render target.
    pub _render_target: Handle<Image>,
    /// The slot's own low-res render target (640×360).
    pub default_render_target: Handle<Image>,
    pub active: bool,
    /// True once SceneInstanceReady fires — model meshes are spawned.
    pub model_loaded: bool,
    /// Handle to the loaded Gltf asset — used for morph target name discovery.
    pub gltf_handle: Option<Handle<Gltf>>,
    /// Path to the model file — used for VRM extension parsing.
    pub model_path: Option<String>,
}

impl SlotState {
    /// Get the render target's AssetId for render-world lookups.
    pub fn render_target_id(&self) -> bevy::asset::AssetId<Image> {
        self._render_target.id()
    }
}

/// Component marking which avatar slot an entity belongs to.
#[derive(Component, Clone, Copy)]
pub(super) struct AvatarSlotId(#[allow(dead_code)] pub u8);

/// Marker component for readback entities.
#[derive(Component)]
pub(super) struct ReadbackMarker;

/// Marker for the shared directional light so force_light_visibility can find it.
#[derive(Component)]
pub(super) struct AvatarSceneLight;

/// Component marking an avatar that is currently speaking.
#[derive(Component)]
pub(super) struct Speaking;

/// Tracks slot metadata for health check logging.
#[derive(Resource, Default)]
pub(super) struct SlotHealthStatus {
    pub identities: HashMap<u8, String>,
    pub model_paths: HashMap<u8, String>,
}

/// Tracks asset handles for load state monitoring.
#[derive(Resource, Default)]
pub(super) struct PendingLoads {
    pub scene_handles: Vec<PendingLoadEntry<Scene>>,
    pub gltf_handles: Vec<PendingLoadEntry<Gltf>>,
}

pub(super) struct PendingLoadEntry<T: bevy::asset::Asset> {
    pub slot: u8,
    pub handle: Handle<T>,
    pub path: String,
    pub logged_final: bool,
}

/// Per-slot morph target layout discovered at scene load time.
#[derive(Resource, Default)]
pub(super) struct SlotMorphTargets {
    pub layouts: HashMap<u8, MorphTargetLayout>,
}

pub(super) struct MorphTargetLayout {
    /// Entity that has the MorphWeights component (the face mesh)
    pub mesh_entity: Entity,
    /// Index of the "aa" / "A" mouth-open morph target
    pub mouth_open_index: Option<usize>,
    /// Index of blink morph target (both eyes)
    pub blink_index: Option<usize>,
    pub blink_left_index: Option<usize>,
    pub blink_right_index: Option<usize>,
    // VRM expression presets
    pub happy_index: Option<usize>,
    pub sad_index: Option<usize>,
    pub angry_index: Option<usize>,
    pub surprised_index: Option<usize>,
    pub relaxed_index: Option<usize>,
    // Eye gaze blend shapes
    pub look_up: Option<usize>,
    pub look_down: Option<usize>,
    pub look_left: Option<usize>,
    pub look_right: Option<usize>,
}

/// Per-slot blink animation state.
#[derive(Resource, Default)]
pub(super) struct BlinkState {
    pub slots: HashMap<u8, SlotBlinkState>,
}

pub(super) struct SlotBlinkState {
    pub next_blink_time: f32,
    pub blink_frames_remaining: u8,
}

/// Per-slot bone registry — tracks discovered skeleton bones for animation.
#[derive(Resource, Default)]
pub(super) struct BoneRegistry {
    pub slots: HashMap<u8, SlotBones>,
}

pub(super) struct SlotBones {
    pub head: Option<BoneInfo>,
    pub neck: Option<BoneInfo>,
    pub spine: Option<BoneInfo>,
    pub left_shoulder: Option<BoneInfo>,
    pub right_shoulder: Option<BoneInfo>,
    pub left_upper_arm: Option<BoneInfo>,
    pub right_upper_arm: Option<BoneInfo>,
    pub left_lower_arm: Option<BoneInfo>,
    pub right_lower_arm: Option<BoneInfo>,
    pub left_eye: Option<BoneInfo>,
    pub right_eye: Option<BoneInfo>,
    #[allow(dead_code)]
    pub left_hand: Option<BoneInfo>,
    #[allow(dead_code)]
    pub right_hand: Option<BoneInfo>,
    /// VRM lookAt configuration — eye rotation ranges for bone-based gaze.
    pub look_at_config: Option<VrmLookAtConfig>,
}

pub(super) struct BoneInfo {
    pub entity: Entity,
    /// Actual local-space rest translation (from skeleton bind pose)
    pub rest_translation: Vec3,
    /// Actual local-space rest rotation (from skeleton bind pose)
    pub rest_rotation: Quat,
}

/// VRM lookAt configuration for bone-based eye gaze.
#[derive(Debug, Clone, Copy)]
pub(super) struct VrmLookAtConfig {
    pub horizontal_inner_deg: f32,
    pub horizontal_outer_deg: f32,
    pub vertical_up_deg: f32,
    pub vertical_down_deg: f32,
}

impl Default for VrmLookAtConfig {
    fn default() -> Self {
        Self {
            horizontal_inner_deg: 8.0,
            horizontal_outer_deg: 8.0,
            vertical_up_deg: 10.0,
            vertical_down_deg: 10.0,
        }
    }
}

/// Per-slot idle gesture animation state.
#[derive(Resource, Default)]
pub(super) struct IdleGestureState {
    pub slots: HashMap<u8, SlotGestureState>,
}

pub(super) struct SlotGestureState {
    pub phase: f32,
    pub head_turn_current: f32,
    pub head_turn_target: f32,
}

/// Per-slot render target dimensions.
#[derive(Resource)]
pub(crate) struct SlotDimensions {
    pub dims: HashMap<u8, (u32, u32)>,
}

impl Default for SlotDimensions {
    fn default() -> Self {
        let mut dims = HashMap::new();
        for slot in 0..super::MAX_AVATAR_SLOTS {
            dims.insert(slot, (super::AVATAR_WIDTH, super::AVATAR_HEIGHT));
        }
        Self { dims }
    }
}

/// An active speech clip playing on Bevy's timeline.
pub(super) struct ActiveClip {
    pub mouth_weights: Vec<f32>,
    pub interval_ms: u32,
    pub duration_ms: u64,
    pub start_time: f32,
}

/// All active speech animations, keyed by slot.
#[derive(Resource, Default)]
pub(super) struct ActiveSpeechClips {
    pub clips: HashMap<u8, ActiveClip>,
    pub clips_started: u32,
    pub clips_auto_stopped: u32,
    pub clips_interrupted: u32,
}

/// Legacy per-slot mouth weights (for SetMouthWeight individual command).
#[derive(Resource, Default)]
pub(super) struct LegacyMouthWeights {
    pub weights: HashMap<u8, f32>,
}

/// Per-slot emotional expression state with smooth transitions and auto-decay.
#[derive(Resource, Default)]
pub(super) struct EmotionState {
    pub slots: HashMap<u8, SlotEmotionState>,
}

pub(super) struct SlotEmotionState {
    pub current: Emotion,
    pub current_weight: f32,
    pub target: Emotion,
    pub target_weight: f32,
    pub transition_rate: f32,
    pub decay_timer: f32,
}

impl Default for SlotEmotionState {
    fn default() -> Self {
        Self {
            current: Emotion::Neutral,
            current_weight: 0.0,
            target: Emotion::Neutral,
            target_weight: 0.0,
            transition_rate: 3.0,
            decay_timer: 0.0,
        }
    }
}

/// Auto-decay duration for emotions.
pub(super) const EMOTION_DECAY_SECS: f32 = 5.0;
/// Speech attenuation for expression weight.
pub(super) const SPEECH_ATTENUATION: f32 = 0.3;

/// Per-slot body gesture state with attack/sustain/release phases.
#[derive(Resource, Default)]
pub(super) struct ActiveGestures {
    pub slots: HashMap<u8, SlotGestureAnimState>,
}

/// Phase of a gesture animation's lifecycle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum GesturePhase {
    Attack,
    Sustain,
    Release,
}

pub(super) struct SlotGestureAnimState {
    pub gesture: Gesture,
    pub phase: GesturePhase,
    pub duration_secs: f32,
    pub elapsed: f32,
    pub weight: f32,
}

/// Duration of attack/release phases for gesture easing (seconds).
pub(super) const GESTURE_EASE_SECS: f32 = 0.3;

/// Per-slot cognitive animation state.
#[derive(Resource, Default)]
pub(super) struct CognitiveAnimState {
    pub slots: HashMap<u8, SlotCognitiveState>,
}

pub(super) struct SlotCognitiveState {
    pub state: crate::live::session::cognitive_animation::CognitiveState,
    pub config: crate::live::session::cognitive_animation::CognitiveAnimationConfig,
    pub time_since_reroll: f32,
}

/// Render cadence — staggered camera activation for GPU load distribution.
#[derive(Resource)]
pub(super) struct RenderSchedule {
    pub frame_count: u32,
    pub idle_cadence: u32,
}

impl Default for RenderSchedule {
    fn default() -> Self {
        Self {
            frame_count: 0,
            idle_cadence: 1,
        }
    }
}

/// HD render target resolution.
pub(super) const HD_WIDTH: u32 = 1280;
pub(super) const HD_HEIGHT: u32 = 720;
pub(super) const MAX_HD_SLOTS: usize = 3;

/// Pre-allocated pool of HD render targets.
#[derive(Resource)]
pub(super) struct HdRenderTargetPool {
    pub available: Vec<Handle<Image>>,
    pub assigned: HashMap<u8, Handle<Image>>,
}

/// GPU allocation guards for renderer VRAM tracking.
#[derive(Resource, Default)]
pub(super) struct GpuGuards {
    pub _render_targets: Option<GpuAllocationGuard>,
    pub model_guards: HashMap<u8, GpuAllocationGuard>,
}

/// Bevy resource wrapping the shared atomic stats for cross-thread reporting.
#[derive(Resource)]
pub(super) struct SharedMemoryStats(pub Arc<BevyMemoryStats>);
