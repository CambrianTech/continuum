//! Scene marker components + identity-derived room color.
//!
//! Scene instantiation itself lives in [`super::instantiate`] — this file holds
//! only the small pieces that both the instantiator and the birther share: the
//! two marker components tagging scene-owned entities, and the deterministic
//! per-identity room backdrop color.

use bevy::prelude::*;

/// Marker component on the root entity of a scene.
#[derive(Component)]
pub struct SceneMarker {
    pub slot_id: u8,
}

/// Marker for scene-owned lights (for visibility forcing).
#[derive(Component)]
pub struct SceneLight;

/// Generate a unique room background color from a persona identity hash.
/// Produces dark, slightly saturated tones — each avatar's room has a distinct mood.
pub fn room_color_from_identity(identity: &str) -> Color {
    let hash = identity
        .bytes()
        .fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u64));
    let hue = (hash % 360) as f32;
    let saturation = 0.15 + (((hash >> 8) % 20) as f32 / 100.0);
    let lightness = 0.08 + (((hash >> 16) % 10) as f32 / 100.0);
    Color::hsl(hue, saturation, lightness)
}
