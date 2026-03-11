//! Light rigs — configurable lighting setups for scenes.
//!
//! Each rig spawns lights as children of the scene root, scoped to
//! the scene's RenderLayers so lighting doesn't bleed across scenes.

use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;

use super::SceneLight;

/// Predefined lighting configurations for different scene types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LightRig {
    /// 3-point portrait rig (key, fill, rim). Good for talking heads.
    Portrait,
    /// Outdoor rig — strong directional sun + soft ambient sky fill.
    Outdoor,
    /// Flat studio rig — even illumination from multiple angles.
    Studio,
}

impl Default for LightRig {
    fn default() -> Self {
        Self::Portrait
    }
}

/// Spawn lights as children of the scene root based on the selected rig.
pub fn spawn_light_rig(parent: &mut ChildSpawnerCommands, layer: &RenderLayers, rig: LightRig) {
    match rig {
        LightRig::Portrait => spawn_portrait_lights(parent, layer),
        LightRig::Outdoor => spawn_outdoor_lights(parent, layer),
        LightRig::Studio => spawn_studio_lights(parent, layer),
    }
}

/// 3-point portrait lighting: key (upper-left), fill (lower-right), rim (behind).
fn spawn_portrait_lights(parent: &mut ChildSpawnerCommands, layer: &RenderLayers) {
    // Key light — upper-left, strong primary illumination
    parent.spawn((
        DirectionalLight {
            illuminance: 25000.0,
            shadows_enabled: false,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(
            EulerRot::XYZ,
            -0.4,                             // 23° down
            std::f32::consts::PI + 0.3,       // slightly left of camera
            0.0,
        )),
        layer.clone(),
        SceneLight,
    ));

    // Fill light — lower-right, softer to reduce harsh shadows
    parent.spawn((
        DirectionalLight {
            illuminance: 10000.0,
            shadows_enabled: false,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(
            EulerRot::XYZ,
            -0.1,                             // nearly level
            std::f32::consts::PI - 0.4,       // from the right
            0.0,
        )),
        layer.clone(),
        SceneLight,
    ));

    // Rim light — from behind and above, edge separation
    parent.spawn((
        DirectionalLight {
            illuminance: 12000.0,
            shadows_enabled: false,
            color: Color::srgb(0.85, 0.9, 1.0),
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(
            EulerRot::XYZ,
            -0.6,                             // 34° down from above
            0.2,                              // from behind, slightly offset
            0.0,
        )),
        layer.clone(),
        SceneLight,
    ));
}

/// Outdoor lighting: strong sun from above, soft sky fill.
fn spawn_outdoor_lights(parent: &mut ChildSpawnerCommands, layer: &RenderLayers) {
    // Sun — high and slightly offset
    parent.spawn((
        DirectionalLight {
            illuminance: 50000.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(
            EulerRot::XYZ,
            -0.8,                             // 46° down
            std::f32::consts::PI + 0.5,       // from the left
            0.0,
        )),
        layer.clone(),
        SceneLight,
    ));

    // Sky fill — soft from above
    parent.spawn((
        DirectionalLight {
            illuminance: 5000.0,
            shadows_enabled: false,
            color: Color::srgb(0.7, 0.8, 1.0),
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(
            EulerRot::XYZ,
            -1.2,                             // steep overhead
            0.0,
            0.0,
        )),
        layer.clone(),
        SceneLight,
    ));
}

/// Studio lighting: flat, even illumination from multiple angles.
fn spawn_studio_lights(parent: &mut ChildSpawnerCommands, layer: &RenderLayers) {
    let angles: [(f32, f32, f32); 4] = [
        (-0.3, std::f32::consts::PI + 0.5, 15000.0),   // front-left
        (-0.3, std::f32::consts::PI - 0.5, 15000.0),   // front-right
        (-0.5, 0.4, 8000.0),                            // back-left
        (-0.5, -0.4, 8000.0),                           // back-right
    ];

    for (pitch, yaw, illuminance) in angles {
        parent.spawn((
            DirectionalLight {
                illuminance,
                shadows_enabled: false,
                ..default()
            },
            Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, pitch, yaw, 0.0)),
            layer.clone(),
            SceneLight,
        ));
    }
}
