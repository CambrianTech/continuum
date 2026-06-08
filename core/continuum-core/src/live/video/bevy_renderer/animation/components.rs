//! Animation Components — attach to any entity to animate it.
//!
//! These are the primitives. Each one is self-contained state for one
//! animation concern. Systems find entities via Query<&mut ComponentType>.
//! No slot IDs, no HashMaps, no global resources.

use bevy::mesh::morph::MorphWeights;
use bevy::prelude::*;

use super::super::scene::avatar::{BoneInfo, VrmLookAtConfig};
use super::prng::SlotRng;

// =============================================================================
// Morph Target Layout — discovered blend shape indices
// =============================================================================

/// Morph target indices discovered from a loaded mesh.
/// Attach to the entity that owns the MorphWeights.
#[derive(Component)]
pub struct MorphTargets {
    pub mouth_open: Option<usize>,
    pub blink: Option<usize>,
    pub blink_left: Option<usize>,
    pub blink_right: Option<usize>,
    pub happy: Option<usize>,
    pub sad: Option<usize>,
    pub angry: Option<usize>,
    pub surprised: Option<usize>,
    pub relaxed: Option<usize>,
    pub look_up: Option<usize>,
    pub look_down: Option<usize>,
    pub look_left: Option<usize>,
    pub look_right: Option<usize>,
}

impl MorphTargets {
    pub fn has_blink(&self) -> bool {
        self.blink.is_some() || (self.blink_left.is_some() && self.blink_right.is_some())
    }

    pub fn has_gaze(&self) -> bool {
        self.look_up.is_some()
            || self.look_down.is_some()
            || self.look_left.is_some()
            || self.look_right.is_some()
    }
}

// =============================================================================
// Skeleton — discovered bones
// =============================================================================

/// Upper-body skeleton for an animated entity.
/// Attach to the root entity of a character.
#[derive(Component)]
pub struct Skeleton {
    pub head: Option<BoneRef>,
    pub neck: Option<BoneRef>,
    pub spine: Option<BoneRef>,
    pub left_shoulder: Option<BoneRef>,
    pub right_shoulder: Option<BoneRef>,
    pub left_upper_arm: Option<BoneRef>,
    pub right_upper_arm: Option<BoneRef>,
    pub left_lower_arm: Option<BoneRef>,
    pub right_lower_arm: Option<BoneRef>,
    pub left_eye: Option<BoneRef>,
    pub right_eye: Option<BoneRef>,
    pub left_hand: Option<BoneRef>,
    pub right_hand: Option<BoneRef>,
    pub look_at_config: Option<VrmLookAtConfig>,
}

/// Reference to a bone entity with its rest pose.
#[derive(Clone)]
pub struct BoneRef {
    pub entity: Entity,
    pub rest_translation: Vec3,
    pub rest_rotation: Quat,
}

impl From<&BoneInfo> for BoneRef {
    fn from(info: &BoneInfo) -> Self {
        Self {
            entity: info.entity,
            rest_translation: info.rest_translation,
            rest_rotation: info.rest_rotation,
        }
    }
}

// =============================================================================
// Animation Components
// =============================================================================

/// Blink animation. Attach to any entity with MorphTargets that has blink indices.
#[derive(Component)]
pub struct BlinkAnimation {
    pub next_blink_time: f32,
    pub blink_frames_remaining: u8,
}

impl BlinkAnimation {
    pub fn new(elapsed: f32, seed: u8) -> Self {
        let mut rng = SlotRng::new(elapsed, seed);
        Self {
            next_blink_time: elapsed + 0.5 + rng.range(0.0, 4.0),
            blink_frames_remaining: 0,
        }
    }
}

/// Breathing animation. Attach to any entity with a Skeleton that has a spine.
#[derive(Component)]
pub struct BreathingAnimation {
    pub phase_offset: f32,
}

impl BreathingAnimation {
    pub fn new(seed: u8) -> Self {
        let mut rng = SlotRng::new(seed as f32 * 137.0, seed);
        Self {
            phase_offset: rng.range(0.0, 20.0),
        }
    }
}

/// Idle micro-movements (neck tilt, shoulder shift, head-turn toward speaker).
#[derive(Component)]
pub struct IdleMotion {
    pub phase: f32,
    pub head_turn_current: f32,
    pub head_turn_target: f32,
}

impl IdleMotion {
    pub fn new(seed: u8) -> Self {
        let mut rng = SlotRng::new(seed as f32 * 251.0, seed);
        Self {
            phase: rng.range(0.0, 30.0),
            head_turn_current: 0.0,
            head_turn_target: 0.0,
        }
    }
}

/// Marker: this entity is currently speaking.
#[derive(Component)]
pub struct Speaking;

/// Active speech clip playing on this entity.
#[derive(Component)]
pub struct SpeechClip {
    pub mouth_weights: Vec<f32>,
    pub interval_ms: u32,
    pub duration_ms: u64,
    pub start_time: f32,
}

/// Legacy per-frame mouth weight (for SetMouthWeight command).
#[derive(Component)]
pub struct MouthWeight(pub f32);

/// Emotional expression with smooth transitions and auto-decay.
#[derive(Component)]
pub struct EmotionAnimation {
    pub current: Emotion,
    pub current_weight: f32,
    pub target: Emotion,
    pub target_weight: f32,
    pub transition_rate: f32,
    pub decay_timer: f32,
}

impl Default for EmotionAnimation {
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

/// Emotional expression state.
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

pub const EMOTION_DECAY_SECS: f32 = 5.0;
pub const SPEECH_ATTENUATION: f32 = 0.3;

/// Body gesture with attack/sustain/release envelope.
#[derive(Component)]
pub struct GestureAnimation {
    pub gesture: Gesture,
    pub phase: GesturePhase,
    pub duration_secs: f32,
    pub elapsed: f32,
    pub weight: f32,
}

/// Body gesture type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Gesture {
    #[default]
    None,
    Wave,
    Think,
    Nod,
    Shrug,
    Point,
    OpenHands,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GesturePhase {
    Attack,
    Sustain,
    Release,
}

pub const GESTURE_EASE_SECS: f32 = 0.3;

/// Cognitive gesture driver — selects gestures based on AI cognitive state.
#[derive(Component)]
pub struct CognitiveGesture {
    pub state: crate::live::session::cognitive_animation::CognitiveState,
    pub config: crate::live::session::cognitive_animation::CognitiveAnimationConfig,
    pub time_since_reroll: f32,
}

/// Eye gaze animation (bone-based or blend-shape-based).
#[derive(Component)]
pub struct EyeGaze {
    pub phase: f32,
}

impl EyeGaze {
    pub fn new(seed: u8) -> Self {
        let mut rng = SlotRng::new(seed as f32 * 311.0, seed);
        Self {
            phase: rng.range(0.0, 40.0),
        }
    }
}

/// Links a morph-target mesh entity to the avatar root entity that owns it.
/// Inserted on the avatar root so systems can find the mesh entity to write MorphWeights.
#[derive(Component)]
pub struct MorphMeshLink(pub Entity);

/// Path to the loaded model file — needed for VRM extension parsing during discovery.
#[derive(Component)]
pub struct ModelPath(pub String);

/// Slot ID — kept for render infrastructure (camera routing, readback, frame channels).
/// NOT used by animation systems.
#[derive(Component, Clone, Copy)]
pub struct SlotId(pub u8);

/// Camera lock to head — stores the locked Y position.
#[derive(Component)]
pub struct CameraHeadLock {
    pub head_y: Option<f32>,
}

// =============================================================================
// Helpers
// =============================================================================

/// Set a morph weight by optional index, with bounds check.
#[inline(always)]
pub fn set_morph(w: &mut [f32], idx: Option<usize>, val: f32) {
    if let Some(i) = idx {
        if i < w.len() {
            w[i] = val;
        }
    }
}

/// Smoothstep easing.
#[inline]
pub fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
