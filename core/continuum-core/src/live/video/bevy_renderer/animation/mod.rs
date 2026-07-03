//! Animation systems — Component-based, Query-driven.
//!
//! Every animation concern is a Component on the entity being animated.
//! Systems operate on Queries — they don't know or care what the entity IS,
//! only what Components it HAS. An avatar, dog, NPC, or ceiling fan can all
//! blink if they have BlinkAnimation + MorphTargets.
//!
//! NO global HashMap<u8, State> resources. NO slot IDs in animation logic.

use bevy::prelude::*;

pub mod animator;
mod blinking;
mod body_gestures;
mod breathing;
mod cadence;
mod camera;
mod components;
mod expression;
mod eye_gaze;
mod idle_gestures;
mod morph_discovery;
pub mod pose;
pub(super) mod prng;
mod procedural;
pub mod registry;
mod speaking;

// The animation Components — attach to any entity to animate it.
pub(super) use components::*;

// The Animator seam (Slice 2): the applier + per-slot registry + supervisor.
// `ExternalPose` is re-exported here for the built-in writers' `Without<>` filter.
pub(super) use pose::{apply_external_pose, ExternalPose};
pub(super) use registry::{drive_animators, select_animator_for_identity, AnimatorRegistry};

/// Ordered animation phases. `Intent` (supervisor decides pose vs built-in) →
/// `Pose` (built-in writers + `apply_external_pose` write Transforms/morphs) →
/// `Readback` (capture the rendered frame). `.chain()`ed for a total order
/// between phases, with parallelism preserved *within* each phase. This replaces
/// the old unordered `Update` tuple, whose last-writer-wins only worked because
/// the built-in writers happened to touch disjoint bones.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum AnimationSet {
    Intent,
    Pose,
    Readback,
}

// Re-export system functions for app.rs registration.
pub(super) use blinking::animate_blinking;
pub(super) use body_gestures::{animate_body_gestures, drive_cognitive_gestures};
pub(super) use breathing::animate_breathing;
pub(super) use cadence::manage_render_cadence;
pub(super) use camera::animate_idle;
pub(super) use expression::animate_expression;
pub(super) use eye_gaze::animate_eye_gaze;
pub(super) use idle_gestures::animate_idle_gestures;
pub(super) use morph_discovery::discover_morph_targets;
pub(super) use speaking::animate_speaking;
