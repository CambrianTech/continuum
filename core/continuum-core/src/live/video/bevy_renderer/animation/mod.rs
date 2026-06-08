//! Animation systems — Component-based, Query-driven.
//!
//! Every animation concern is a Component on the entity being animated.
//! Systems operate on Queries — they don't know or care what the entity IS,
//! only what Components it HAS. An avatar, dog, NPC, or ceiling fan can all
//! blink if they have BlinkAnimation + MorphTargets.
//!
//! NO global HashMap<u8, State> resources. NO slot IDs in animation logic.

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
pub(super) mod prng;
mod speaking;

// The animation Components — attach to any entity to animate it.
pub(super) use components::*;

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
