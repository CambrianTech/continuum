//! Scene builder — spawns the root entity hierarchy (camera + lights).
//!
//! `build_scene()` creates the parent entity and its infrastructure children.
//! Scene objects (avatars, props, environments) are added separately via
//! the RenderSlot's object management methods.

use bevy::asset::Handle;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::ClearColorConfig;
use bevy::prelude::*;

use super::lighting::LightRig;
use crate::live::video::bevy_renderer::skeleton;

/// Marker component on the root entity of a scene.
#[derive(Component)]
pub struct SceneMarker {
    pub slot_id: u8,
}

/// Marker for scene-owned lights (for visibility forcing).
#[derive(Component)]
pub struct SceneLight;

/// Configuration for building a new scene.
pub struct SceneConfig {
    pub slot_id: u8,
    pub render_target: Handle<Image>,
    pub background_color: Color,
    /// RenderLayers for this scene (slot_id + 1).
    pub layer: RenderLayers,
    /// Which lighting rig to use.
    pub light_rig: LightRig,
    /// Camera transform. None = default portrait framing.
    pub camera_transform: Option<Transform>,
}

/// Generate a unique room background color from a persona identity hash.
/// Produces dark, slightly saturated tones — each avatar's room has a distinct mood.
pub fn room_color_from_identity(identity: &str) -> Color {
    let hash = identity.bytes().fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u64));
    let hue = (hash % 360) as f32;
    let saturation = 0.15 + (((hash >> 8) % 20) as f32 / 100.0);
    let lightness = 0.08 + (((hash >> 16) % 10) as f32 / 100.0);
    Color::hsl(hue, saturation, lightness)
}

/// Builds a complete scene hierarchy: root → camera.
/// Lights are global (shared across all scenes) to stay within Bevy's
/// directional light limit. Per-scene lights can be added later with
/// a light budget system.
/// Returns the root entity and camera entity.
pub fn build_scene(commands: &mut Commands, config: &SceneConfig) -> (Entity, Entity) {
    let mut camera_entity = Entity::PLACEHOLDER;

    let camera_transform = config.camera_transform.unwrap_or_else(|| {
        Transform::from_xyz(
            0.0,
            skeleton::REFERENCE_HEAD_Y,
            skeleton::REFERENCE_CAMERA_Z,
        )
        .looking_at(
            Vec3::new(0.0, skeleton::REFERENCE_HEAD_Y - 0.02, 0.0),
            Vec3::Y,
        )
    });

    let root = commands
        .spawn((
            SceneMarker { slot_id: config.slot_id },
            Transform::default(),
            Visibility::default(),
            config.layer.clone(),
        ))
        .with_children(|parent| {
            camera_entity = parent
                .spawn((
                    Camera3d::default(),
                    Camera {
                        order: config.slot_id as isize,
                        clear_color: ClearColorConfig::Custom(config.background_color),
                        is_active: false,
                        ..default()
                    },
                    bevy::camera::RenderTarget::Image(config.render_target.clone().into()),
                    bevy::core_pipeline::tonemapping::Tonemapping::None,
                    Msaa::Off,
                    camera_transform,
                    config.layer.clone(),
                ))
                .id();
        })
        .id();

    (root, camera_entity)
}

/// Spawn global portrait lights visible on all avatar layers.
/// Uses 3-point portrait rig — shared across all scenes to stay within
/// Bevy's 10 directional light limit.
pub fn spawn_global_lights(commands: &mut Commands, max_slots: u8) {
    let all_layers: Vec<usize> = (1..=(max_slots as usize)).collect();
    let layers = bevy::camera::visibility::RenderLayers::from_layers(&all_layers);

    // Key light — upper-left, strong
    commands.spawn((
        DirectionalLight {
            illuminance: 25000.0,
            shadows_enabled: false,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(
            EulerRot::XYZ,
            -0.4,
            std::f32::consts::PI + 0.3,
            0.0,
        )),
        layers.clone(),
        SceneLight,
    ));

    // Fill light — lower-right, softer
    commands.spawn((
        DirectionalLight {
            illuminance: 10000.0,
            shadows_enabled: false,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(
            EulerRot::XYZ,
            -0.1,
            std::f32::consts::PI - 0.4,
            0.0,
        )),
        layers.clone(),
        SceneLight,
    ));

    // Rim light — from behind and above, edge separation
    commands.spawn((
        DirectionalLight {
            illuminance: 12000.0,
            shadows_enabled: false,
            color: Color::srgb(0.85, 0.9, 1.0),
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(
            EulerRot::XYZ,
            -0.6,
            0.2,
            0.0,
        )),
        layers,
        SceneLight,
    ));
}
