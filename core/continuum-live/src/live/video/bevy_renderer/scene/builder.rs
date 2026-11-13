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
    // Studio backdrop polish ([[persona-visual-identity]] render-well): a CLEAN,
    // low-saturation identity tint — a subtle wash over a controlled dark value, not
    // the old murky mid-saturation "dark wall" (sat 0.15–0.35 @ near-black 0.08–0.18
    // read as muddy). Lower saturation kills the murk; a slightly lifted value gives
    // a slate/charcoal-with-a-hint-of-hue that still lets a lit avatar pop while
    // looking deliberate. Each persona keeps a unique, recognizable tint.
    let saturation = 0.07 + (((hash >> 8) % 9) as f32 / 100.0); // 0.07–0.16
    let lightness = 0.12 + (((hash >> 16) % 7) as f32 / 100.0); // 0.12–0.19
    Color::hsl(hue, saturation, lightness)
}
