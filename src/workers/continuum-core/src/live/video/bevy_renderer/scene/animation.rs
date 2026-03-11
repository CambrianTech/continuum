//! Animation configuration for scene objects.
//!
//! Animation parameters are DATA, not code. Any scene object (avatar, prop,
//! NPC, pet, vehicle) can have an AnimationConfig component. The Bevy
//! animation systems read these parameters rather than using hard-coded values.
//!
//! This separation means:
//! - Different scenes can use different animation profiles
//! - Sentinel pipelines can adjust animation at runtime
//! - A sleeping persona has near-zero amplitudes
//! - A Sims character walking has larger body movement than a webcam portrait
//! - Non-avatar objects (ceiling fan, flickering light) use the same pattern

use bevy::prelude::*;

// =============================================================================
// Animation Profile Constants
// =============================================================================

/// Webcam portrait — subtle upper-body movement for video call framing.
pub const PORTRAIT_PROFILE: AnimationProfile = AnimationProfile {
    // Breathing
    breathing_scale_amplitude: 0.0025,
    breathing_frequency: 0.8,
    spine_sway_amplitude: 0.006,
    spine_sway_frequency: 0.12,

    // Idle neck micro-movement
    neck_tilt_x_amplitude: 0.015,
    neck_tilt_z_amplitude: 0.012,
    neck_turn_amplitude: 0.01,

    // Speaking
    speaking_nod_amplitude: 0.018,
    speaking_tilt_amplitude: 0.01,

    // Shoulders
    shoulder_shift_amplitude: 0.001,

    // Gestures
    gesture_nod_amplitude: 0.06,
    gesture_think_head_tilt: 0.05,
    gesture_think_head_roll: 0.08,
    gesture_ease_secs: 0.25,
};

/// Full-body — larger movement for Sims-like scenes or third-person views.
#[allow(dead_code)]
pub const FULL_BODY_PROFILE: AnimationProfile = AnimationProfile {
    breathing_scale_amplitude: 0.004,
    breathing_frequency: 0.7,
    spine_sway_amplitude: 0.015,
    spine_sway_frequency: 0.10,

    neck_tilt_x_amplitude: 0.025,
    neck_tilt_z_amplitude: 0.02,
    neck_turn_amplitude: 0.02,

    speaking_nod_amplitude: 0.03,
    speaking_tilt_amplitude: 0.02,

    shoulder_shift_amplitude: 0.002,

    gesture_nod_amplitude: 0.10,
    gesture_think_head_tilt: 0.08,
    gesture_think_head_roll: 0.12,
    gesture_ease_secs: 0.3,
};

/// Minimal — for sleeping, meditating, or background characters.
#[allow(dead_code)]
pub const MINIMAL_PROFILE: AnimationProfile = AnimationProfile {
    breathing_scale_amplitude: 0.002,
    breathing_frequency: 0.5,
    spine_sway_amplitude: 0.0,
    spine_sway_frequency: 0.0,

    neck_tilt_x_amplitude: 0.0,
    neck_tilt_z_amplitude: 0.0,
    neck_turn_amplitude: 0.0,

    speaking_nod_amplitude: 0.0,
    speaking_tilt_amplitude: 0.0,

    shoulder_shift_amplitude: 0.0,

    gesture_nod_amplitude: 0.0,
    gesture_think_head_tilt: 0.0,
    gesture_think_head_roll: 0.0,
    gesture_ease_secs: 0.25,
};

// =============================================================================
// Animation Profile
// =============================================================================

/// Defines all animation amplitudes and frequencies for a scene object.
/// Stored as a simple struct — no heap allocation, trivially copyable.
#[derive(Debug, Clone, Copy)]
pub struct AnimationProfile {
    // -- Breathing --
    /// Spine scale.y oscillation amplitude.
    pub breathing_scale_amplitude: f32,
    /// Breathing oscillation frequency (Hz, multiplied by TAU).
    pub breathing_frequency: f32,
    /// Spine Z-rotation sway amplitude (radians).
    pub spine_sway_amplitude: f32,
    /// Spine sway frequency (Hz).
    pub spine_sway_frequency: f32,

    // -- Idle neck --
    /// Neck X-tilt amplitude (radians) — the "nodding" micro-movement.
    pub neck_tilt_x_amplitude: f32,
    /// Neck Z-tilt amplitude (radians) — the "tilting" micro-movement.
    pub neck_tilt_z_amplitude: f32,
    /// Neck Y-turn amplitude (radians) — subtle left/right drift.
    pub neck_turn_amplitude: f32,

    // -- Speaking --
    /// Head nod amplitude while speaking (radians).
    pub speaking_nod_amplitude: f32,
    /// Head tilt amplitude while speaking (radians).
    pub speaking_tilt_amplitude: f32,

    // -- Shoulders --
    /// Shoulder micro-shift amplitude (meters).
    pub shoulder_shift_amplitude: f32,

    // -- Gestures --
    /// Gesture::Nod amplitude (radians).
    pub gesture_nod_amplitude: f32,
    /// Gesture::Think head X-tilt (radians).
    pub gesture_think_head_tilt: f32,
    /// Gesture::Think head Z-roll (radians).
    pub gesture_think_head_roll: f32,
    /// Attack/release easing duration for gestures (seconds).
    pub gesture_ease_secs: f32,
}

// =============================================================================
// ECS Component
// =============================================================================

/// Attached to any scene object that should animate. The Bevy animation
/// systems read this component's profile to determine amplitudes.
///
/// Default profile is PORTRAIT_PROFILE (webcam video call).
#[derive(Component)]
pub struct AnimationConfig {
    pub profile: AnimationProfile,
    /// Per-object frequency variation factor (randomized at spawn).
    /// Prevents all objects from moving in lockstep.
    pub freq_variation: f32,
}

impl Default for AnimationConfig {
    fn default() -> Self {
        Self {
            profile: PORTRAIT_PROFILE,
            freq_variation: 1.0,
        }
    }
}

impl AnimationConfig {
    pub fn portrait(slot: u8) -> Self {
        Self {
            profile: PORTRAIT_PROFILE,
            // Deterministic per-slot variation: ±30% frequency spread.
            freq_variation: 1.0 + (slot as f32 * 0.37).sin() * 0.3,
        }
    }

    #[allow(dead_code)]
    pub fn full_body(slot: u8) -> Self {
        Self {
            profile: FULL_BODY_PROFILE,
            freq_variation: 1.0 + (slot as f32 * 0.37).sin() * 0.3,
        }
    }

    #[allow(dead_code)]
    pub fn minimal() -> Self {
        Self {
            profile: MINIMAL_PROFILE,
            freq_variation: 1.0,
        }
    }
}
