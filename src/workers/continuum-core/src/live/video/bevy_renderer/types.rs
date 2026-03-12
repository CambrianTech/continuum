//! Bevy renderer types — components, resources, enums, and structs.
//!
//! Type definitions for the avatar rendering system.
//! Scene graph types (SceneObject, AvatarState, etc.) live in the `scene` module.
//! This module holds Bevy ECS Resources that wrap per-slot animation state,
//! plus public API types (AvatarCommand, Emotion, Gesture).

use bevy::asset::Handle;
use bevy::gltf::Gltf;
use bevy::prelude::*;
use crossbeam_channel::{Receiver, Sender};
use std::collections::HashMap;
use std::sync::Arc;

use crate::gpu::memory_manager::GpuAllocationGuard;
use crate::live::avatar::RgbaFrame;

// Re-export scene types used by animation and skeleton modules.
pub(super) use super::scene::{AvatarBones, MorphTargetLayout, SlotRegistry};

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
        /// Persona identity (user_id) — used for procedural room color generation.
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
    /// Unload all loaded slots that are NOT currently speaking.
    /// Used by memory pressure system at critical level to reclaim model memory.
    UnloadIdle,
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
    /// Desired idle cadence — written by MemoryReporter, read by Bevy system.
    pub desired_idle_cadence: std::sync::atomic::AtomicU32,
}

impl BevyMemoryStats {
    pub fn new() -> Self {
        Self {
            active_slots: std::sync::atomic::AtomicU8::new(0),
            loaded_models: std::sync::atomic::AtomicU8::new(0),
            speaking_slots: std::sync::atomic::AtomicU8::new(0),
            render_target_bytes: std::sync::atomic::AtomicU64::new(0),
            pending_loads: std::sync::atomic::AtomicU32::new(0),
            desired_idle_cadence: std::sync::atomic::AtomicU32::new(1),
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

// =============================================================================
// ECS Marker Components
// =============================================================================

/// Component marking which avatar slot an entity belongs to.
#[derive(Component, Clone, Copy)]
pub(super) struct AvatarSlotId(#[allow(dead_code)] pub u8);

/// Marker component for readback entities.
#[derive(Component)]
pub(super) struct ReadbackMarker;

/// Component marking an avatar that is currently speaking.
#[derive(Component)]
pub(super) struct Speaking;

// =============================================================================
// Operational Resources
// =============================================================================

/// Tracks slot metadata for health check logging.
#[derive(Resource, Default)]
pub(super) struct SlotHealthStatus {
    pub identities: HashMap<u8, String>,
    pub model_paths: HashMap<u8, String>,
}

/// Minimum seconds between consecutive snapshot captures.
/// Prevents burst when many avatars load simultaneously.
/// Can be lowered for active sessions or raised under memory pressure.
pub(super) const SNAPSHOT_COOLDOWN_SECS: u64 = 2;

/// Minimum seconds after model load before snapshot is eligible.
pub(super) const SNAPSHOT_MIN_AGE_SECS: u64 = 4;

/// Refresh interval for existing snapshots (seconds). 0 = never refresh.
pub(super) const SNAPSHOT_MAX_AGE_SECS: u64 = 3600;

/// Background snapshot tracker — opportunistically saves profile pics from
/// already-rendered frames. Game engine screenshot pattern:
///
/// 1. Time-based eligibility (not frame-based) — waits for model to settle
/// 2. Single-flight — only one capture in progress at any time
/// 3. Global cooldown — 2s minimum between captures (prevents burst on mass load)
/// 4. For GPU bridge slots: temporarily inserts Readback for one frame
///
/// Priority: HIGH if no snapshot file exists, LOW for periodic refresh.
/// Max age before refresh: 1 hour for active slots.
#[derive(Resource)]
pub(super) struct SnapshotTracker {
    /// Slots that have already been captured this session (avoid repeated work).
    pub captured: HashMap<u8, std::time::Instant>,
    /// When each slot's model finished loading (SceneInstanceReady).
    pub loaded_at: HashMap<u8, std::time::Instant>,
    /// Minimum seconds after load before first capture (let model settle).
    pub min_age_secs: u64,
    /// Max age before re-capturing (seconds). 0 = never refresh.
    pub max_age_secs: u64,
    /// Last time ANY capture was initiated — global cooldown to prevent bursts.
    pub last_capture_time: Option<std::time::Instant>,
    /// Cooldown between captures (seconds).
    pub capture_cooldown_secs: u64,
    /// GPU bridge slot awaiting a temporary Readback for snapshot.
    /// `ensure_continuous_readback` keeps Readback alive for this slot.
    /// Cleared after ReadbackComplete fires and captures the frame.
    pub pending_readback_slot: Option<u8>,
}

impl Default for SnapshotTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl SnapshotTracker {
    pub fn new() -> Self {
        Self {
            captured: HashMap::new(),
            loaded_at: HashMap::new(),
            min_age_secs: SNAPSHOT_MIN_AGE_SECS,
            max_age_secs: SNAPSHOT_MAX_AGE_SECS,
            last_capture_time: None,
            capture_cooldown_secs: SNAPSHOT_COOLDOWN_SECS,
            pending_readback_slot: None,
        }
    }

    /// Mark a slot as having its model fully loaded (call from SceneInstanceReady).
    pub fn mark_loaded(&mut self, slot: u8) {
        self.loaded_at.insert(slot, std::time::Instant::now());
    }

    /// Mark a slot as unloaded (call from Unload command).
    pub fn mark_unloaded(&mut self, slot: u8) {
        self.loaded_at.remove(&slot);
        if self.pending_readback_slot == Some(slot) {
            self.pending_readback_slot = None;
        }
    }

    /// Check if this slot needs a snapshot capture.
    /// Returns true if: model settled, cooldown elapsed, and file missing/stale.
    pub fn needs_capture(&self, slot: u8, identity: &str) -> bool {
        // Model not loaded yet
        let loaded = match self.loaded_at.get(&slot) {
            Some(t) => t,
            None => return false,
        };

        // Too early — model still settling after load
        if loaded.elapsed().as_secs() < self.min_age_secs {
            return false;
        }

        // Already captured this session recently
        if let Some(captured_at) = self.captured.get(&slot) {
            if captured_at.elapsed().as_secs() < self.max_age_secs {
                return false;
            }
        }

        // Global cooldown — prevent bursts when many slots load at once
        if let Some(last) = self.last_capture_time {
            if last.elapsed().as_secs() < self.capture_cooldown_secs {
                return false;
            }
        }

        let avatar_dir = Self::avatar_dir();
        let png_path = avatar_dir.join(format!("{identity}.png"));

        if !png_path.exists() {
            // HIGH priority — no snapshot at all
            return true;
        }

        // LOW priority — check file age for refresh
        if let Ok(metadata) = std::fs::metadata(&png_path) {
            if let Ok(modified) = metadata.modified() {
                if let Ok(age) = modified.elapsed() {
                    return age.as_secs() > self.max_age_secs;
                }
            }
        }

        false
    }

    /// Save an RGBA frame as PNG in the background. Fire-and-forget.
    /// Updates cooldown timestamp — no other capture can start for `capture_cooldown_secs`.
    pub fn capture_background(
        &mut self,
        slot: u8,
        identity: String,
        width: u32,
        height: u32,
        rgba_data: Vec<u8>,
    ) {
        let now = std::time::Instant::now();
        self.captured.insert(slot, now);
        self.last_capture_time = Some(now);

        // Clear pending readback if this was the bridge slot we were waiting for.
        if self.pending_readback_slot == Some(slot) {
            self.pending_readback_slot = None;
        }

        // Fire-and-forget on a background thread — never blocks the render loop.
        std::thread::spawn(move || {
            let avatar_dir = Self::avatar_dir();
            if std::fs::create_dir_all(&avatar_dir).is_err() {
                return;
            }
            let png_path = avatar_dir.join(format!("{identity}.png"));

            if let Some(img) = image::ImageBuffer::<image::Rgba<u8>, Vec<u8>>::from_raw(
                width, height, rgba_data,
            ) {
                if img.save(&png_path).is_ok() {
                    crate::clog_info!(
                        "📸 Snapshot saved for '{}': {}",
                        identity,
                        png_path.display()
                    );
                }
            }
        });
    }

    fn avatar_dir() -> std::path::PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        std::path::PathBuf::from(home).join(".continuum").join("avatars")
    }
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

// =============================================================================
// Animation State Resources (per-slot HashMaps)
// =============================================================================

/// Per-slot morph target layout discovered at scene load time.
#[derive(Resource, Default)]
pub(super) struct SlotMorphTargets {
    pub layouts: HashMap<u8, MorphTargetLayout>,
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
    pub slots: HashMap<u8, AvatarBones>,
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

// =============================================================================
// Render Infrastructure
// =============================================================================

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

/// Cached set of currently-speaking slot IDs — computed once per frame,
/// consumed by multiple animation systems (speaking, expression, idle gestures, eye gaze, cognitive).
#[derive(Resource, Default)]
pub(super) struct SpeakingSlots(pub std::collections::HashSet<u8>);
