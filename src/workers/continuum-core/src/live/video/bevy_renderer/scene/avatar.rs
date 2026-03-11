//! Avatar — animated character state for scene objects.
//!
//! All per-avatar mutable state: model references, discovered mesh data,
//! skeleton bones, and animation state (blink, speech, emotion, gesture,
//! cognitive). Animation systems access this through the scene's SlotRegistry.
//!
//! Note: Many fields are unused while animation systems still use separate
//! Bevy Resources (BoneRegistry, SlotMorphTargets, etc.). These will be
//! consumed when animation migrates to read from AvatarState directly.
#![allow(dead_code)]

use bevy::asset::Handle;
use bevy::gltf::Gltf;
use bevy::prelude::*;

// =============================================================================
// Avatar Object
// =============================================================================

/// An avatar within a scene — VRM/glTF model with full animation state.
pub struct AvatarObject {
    /// ECS entity (child of scene root). None until spawned.
    pub entity: Option<Entity>,
    /// Animation and model state.
    pub state: AvatarState,
}

impl AvatarObject {
    pub fn new(model_path: String, display_name: String, identity: String) -> Self {
        Self {
            entity: None,
            state: AvatarState {
                model_loaded: false,
                gltf_handle: None,
                model_path: Some(model_path),
                display_name: Some(display_name),
                identity: Some(identity),
                morph_targets: None,
                bones: None,
                blink: BlinkState::default(),
                idle_gesture: IdleGestureState::default(),
                speech: SpeechState::default(),
                emotion: EmotionState::default(),
                gesture: None,
                cognitive: None,
            },
        }
    }

    pub fn is_speaking(&self) -> bool {
        self.state.is_speaking()
    }
}

// =============================================================================
// Avatar State — all mutable per-character data
// =============================================================================

/// All mutable state for one animated avatar.
/// Animation systems read/write this directly through the scene registry.
pub struct AvatarState {
    pub model_loaded: bool,
    pub gltf_handle: Option<Handle<Gltf>>,
    pub model_path: Option<String>,
    pub display_name: Option<String>,
    /// Persona identity (user_id).
    pub identity: Option<String>,

    // --- Discovered mesh data ---
    pub morph_targets: Option<MorphTargetLayout>,

    // --- Discovered skeleton ---
    pub bones: Option<AvatarBones>,

    // --- Animation state ---
    pub blink: BlinkState,
    pub idle_gesture: IdleGestureState,
    pub speech: SpeechState,
    pub emotion: EmotionState,
    pub gesture: Option<GestureAnimState>,
    pub cognitive: Option<CognitiveAnimState>,
}

impl AvatarState {
    pub fn is_speaking(&self) -> bool {
        self.speech.clip.is_some() || self.speech.legacy_mouth_weight > 0.0
    }
}

// =============================================================================
// Morph Target Layout
// =============================================================================

/// Morph target indices for a loaded avatar model.
pub struct MorphTargetLayout {
    /// Entity that has the MorphWeights component (the face mesh).
    pub mesh_entity: Entity,
    pub mouth_open_index: Option<usize>,
    pub blink_index: Option<usize>,
    pub blink_left_index: Option<usize>,
    pub blink_right_index: Option<usize>,
    /// VRM expression presets.
    pub happy_index: Option<usize>,
    pub sad_index: Option<usize>,
    pub angry_index: Option<usize>,
    pub surprised_index: Option<usize>,
    pub relaxed_index: Option<usize>,
    /// Eye gaze blend shapes.
    pub look_up: Option<usize>,
    pub look_down: Option<usize>,
    pub look_left: Option<usize>,
    pub look_right: Option<usize>,
}

// =============================================================================
// Skeleton / Bones
// =============================================================================

/// Discovered skeleton bones for upper-body animation.
pub struct AvatarBones {
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
    pub left_hand: Option<BoneInfo>,
    pub right_hand: Option<BoneInfo>,
    /// VRM lookAt configuration for bone-based gaze.
    pub look_at_config: Option<VrmLookAtConfig>,
}

pub struct BoneInfo {
    pub entity: Entity,
    pub rest_translation: Vec3,
    pub rest_rotation: Quat,
}

/// VRM lookAt configuration for bone-based eye gaze.
#[derive(Debug, Clone, Copy)]
pub struct VrmLookAtConfig {
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

// =============================================================================
// Animation State Types
// =============================================================================

#[derive(Default)]
pub struct BlinkState {
    pub next_blink_time: f32,
    pub blink_frames_remaining: u8,
}

#[derive(Default)]
pub struct IdleGestureState {
    pub phase: f32,
    pub head_turn_current: f32,
    pub head_turn_target: f32,
}

#[derive(Default)]
pub struct SpeechState {
    pub clip: Option<ActiveSpeechClip>,
    pub legacy_mouth_weight: f32,
}

pub struct ActiveSpeechClip {
    pub mouth_weights: Vec<f32>,
    pub interval_ms: u32,
    pub duration_ms: u64,
    pub start_time: f32,
}

pub struct EmotionState {
    pub current: crate::live::video::bevy_renderer::Emotion,
    pub current_weight: f32,
    pub target: crate::live::video::bevy_renderer::Emotion,
    pub target_weight: f32,
    pub transition_rate: f32,
    pub decay_timer: f32,
}

impl Default for EmotionState {
    fn default() -> Self {
        Self {
            current: crate::live::video::bevy_renderer::Emotion::Neutral,
            current_weight: 0.0,
            target: crate::live::video::bevy_renderer::Emotion::Neutral,
            target_weight: 0.0,
            transition_rate: 3.0,
            decay_timer: 0.0,
        }
    }
}

pub const EMOTION_DECAY_SECS: f32 = 5.0;
pub const SPEECH_ATTENUATION: f32 = 0.3;

pub struct GestureAnimState {
    pub gesture: crate::live::video::bevy_renderer::Gesture,
    pub phase: GesturePhase,
    pub duration_secs: f32,
    pub elapsed: f32,
    pub weight: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GesturePhase {
    Attack,
    Sustain,
    Release,
}

pub const GESTURE_EASE_SECS: f32 = 0.3;

pub struct CognitiveAnimState {
    pub state: crate::live::session::cognitive_animation::CognitiveState,
    pub config: crate::live::session::cognitive_animation::CognitiveAnimationConfig,
    pub time_since_reroll: f32,
}
