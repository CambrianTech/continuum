//! Room environments — asset-based and procedural scene backgrounds.
//!
//! The scene system is a generic Bevy render engine. It renders any combination
//! of objects (avatars, props, vehicles, environments) to render targets.
//! Consumers (LiveKit, screenshots, previews) read those render targets.
//!
//! This module handles ENVIRONMENT population: loading glTF room/world assets
//! or generating procedural geometry when no asset is available.
//!
//! ## Architecture
//!
//! - `RoomConfig` — ECS component on scene root, declares what environment to load
//! - `RoomStyle` — procedural fallback style (deterministic from identity)
//! - `populate_rooms` — Bevy system, loads environment assets or spawns procedural geometry
//! - All environment entities are children of scene root → recursive despawn cleans up
//!
//! ## Asset-Based Environments
//!
//! When `RoomConfig.environment_asset` is set, the system loads a glTF scene
//! and spawns it as a child of the scene root. The glTF can contain anything:
//! office furniture, outdoor parks, spaceships, whatever.
//!
//! ## Procedural Fallback
//!
//! When no asset is specified, procedural geometry is generated based on
//! `RoomStyle` (deterministic from persona identity). This ensures every
//! scene always has SOME visible environment, even without custom assets.

use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;

use super::builder::SceneMarker;

// =============================================================================
// Room Style (procedural fallback)
// =============================================================================

/// Procedural environment style. Deterministic from persona identity hash.
/// Used when no glTF environment asset is provided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoomStyle {
    /// Office cubicle — back wall, floor, desk surface, side walls.
    Office,
    /// Creative studio — wider space, art-wall tones, large work surface.
    Studio,
    /// Lounge — warmer tones, alcove feel, coffee-table height desk.
    Lounge,
}

impl RoomStyle {
    /// Deterministic room style from persona identity string.
    pub fn from_identity(identity: &str) -> Self {
        let hash = identity
            .bytes()
            .fold(0u64, |acc, b| acc.wrapping_mul(37).wrapping_add(b as u64));
        match hash % 3 {
            0 => RoomStyle::Office,
            1 => RoomStyle::Studio,
            _ => RoomStyle::Lounge,
        }
    }
}

// =============================================================================
// Room Config (ECS Component)
// =============================================================================

/// Declares what environment a scene should have. Attached to the scene root
/// entity. The `populate_rooms` system consumes this to load/generate the room.
#[derive(Component)]
pub struct RoomConfig {
    /// Optional path to a glTF/GLB environment asset. When set, the asset
    /// is loaded and spawned as a child of the scene root. The glTF can
    /// contain any geometry: rooms, outdoor scenes, vehicles, abstract spaces.
    pub environment_asset: Option<String>,
    /// Procedural fallback style (used when `environment_asset` is None).
    pub style: RoomStyle,
    /// Base color derived from persona identity — drives procedural materials.
    pub base_color: Color,
    /// RenderLayers for this scene's geometry.
    pub layer: RenderLayers,
}

/// Marker: this scene's environment has been spawned (asset loaded or procedural generated).
#[derive(Component)]
pub struct RoomPopulated;

/// Marker on environment entities so they can be queried/replaced independently.
#[derive(Component)]
pub struct EnvironmentGeometry;

// =============================================================================
// Procedural Geometry Constants
// =============================================================================

/// Camera is at (0, 1.50, -0.55) looking toward +Z. Avatar at origin.
mod layout {
    // Back wall — visible behind the avatar
    pub const WALL_Z: f32 = 0.4;
    pub const WALL_WIDTH: f32 = 3.0;
    pub const WALL_HEIGHT: f32 = 3.0;
    pub const WALL_THICKNESS: f32 = 0.02;

    // Floor
    pub const FLOOR_Y: f32 = -0.01;
    pub const FLOOR_WIDTH: f32 = 3.0;
    pub const FLOOR_DEPTH: f32 = 2.0;

    // Desk surface
    pub const DESK_Y: f32 = 0.75;
    pub const DESK_Z: f32 = 0.10;
    pub const DESK_WIDTH: f32 = 1.2;
    pub const DESK_DEPTH: f32 = 0.6;
    pub const DESK_THICKNESS: f32 = 0.04;

    // Side walls
    pub const SIDE_WALL_X: f32 = 1.0;
    pub const SIDE_WALL_DEPTH: f32 = 1.5;
    pub const SIDE_WALL_HEIGHT: f32 = 3.0;
}

// =============================================================================
// Material Derivation
// =============================================================================

/// Surface materials derived from the persona's identity color.
struct RoomMaterials {
    wall: Handle<StandardMaterial>,
    floor: Handle<StandardMaterial>,
    desk: Handle<StandardMaterial>,
    side_wall: Handle<StandardMaterial>,
}

fn derive_materials(
    base: Color,
    style: RoomStyle,
    assets: &mut Assets<StandardMaterial>,
) -> RoomMaterials {
    let c = base.to_srgba();
    let (r, g, b) = (c.red, c.green, c.blue);

    let wall_color = match style {
        RoomStyle::Office => Color::srgb(
            (r * 1.8 + 0.08).min(0.35),
            (g * 1.8 + 0.08).min(0.35),
            (b * 1.8 + 0.08).min(0.35),
        ),
        RoomStyle::Studio => Color::srgb(
            (r * 2.0 + 0.06).min(0.30),
            (g * 1.5 + 0.06).min(0.25),
            (b * 2.0 + 0.06).min(0.30),
        ),
        RoomStyle::Lounge => Color::srgb(
            (r * 2.2 + 0.10).min(0.35),
            (g * 2.0 + 0.08).min(0.30),
            (b * 1.5 + 0.05).min(0.25),
        ),
    };

    let floor_color = Color::srgb(
        (r * 0.6 + 0.02).min(0.15),
        (g * 0.6 + 0.02).min(0.15),
        (b * 0.6 + 0.02).min(0.15),
    );

    let desk_color = match style {
        RoomStyle::Office => Color::srgb(
            (r * 0.3 + 0.18).min(0.35),
            (g * 0.3 + 0.12).min(0.25),
            (b * 0.3 + 0.06).min(0.15),
        ),
        RoomStyle::Studio => Color::srgb(
            (r * 0.4 + 0.15).min(0.30),
            (g * 0.4 + 0.10).min(0.22),
            (b * 0.4 + 0.08).min(0.18),
        ),
        RoomStyle::Lounge => Color::srgb(
            (r * 0.3 + 0.20).min(0.38),
            (g * 0.3 + 0.14).min(0.28),
            (b * 0.2 + 0.05).min(0.12),
        ),
    };

    let side_color = Color::srgb(
        (r * 1.2 + 0.05).min(0.25),
        (g * 1.2 + 0.05).min(0.25),
        (b * 1.2 + 0.05).min(0.25),
    );

    RoomMaterials {
        wall: assets.add(StandardMaterial {
            base_color: wall_color,
            perceptual_roughness: 0.9,
            metallic: 0.0,
            reflectance: 0.1,
            ..default()
        }),
        floor: assets.add(StandardMaterial {
            base_color: floor_color,
            perceptual_roughness: 0.85,
            metallic: 0.0,
            reflectance: 0.15,
            ..default()
        }),
        desk: assets.add(StandardMaterial {
            base_color: desk_color,
            perceptual_roughness: 0.6,
            metallic: 0.05,
            reflectance: 0.3,
            ..default()
        }),
        side_wall: assets.add(StandardMaterial {
            base_color: side_color,
            perceptual_roughness: 0.92,
            metallic: 0.0,
            reflectance: 0.08,
            ..default()
        }),
    }
}

// =============================================================================
// Population System
// =============================================================================

/// Bevy system: detects scenes with `RoomConfig` (without `RoomPopulated`)
/// and spawns the environment — either loading a glTF asset or generating
/// procedural geometry.
///
/// All spawned entities are children of the scene root, inheriting its
/// RenderLayers and getting cleaned up on teardown (recursive despawn).
pub fn populate_rooms(
    mut commands: Commands,
    new_rooms: Query<(Entity, &RoomConfig), (With<SceneMarker>, Without<RoomPopulated>)>,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (scene_entity, config) in new_rooms.iter() {
        if let Some(ref asset_path) = config.environment_asset {
            // Asset-based environment: load glTF and spawn as child
            spawn_asset_environment(
                &mut commands,
                scene_entity,
                asset_path,
                &config.layer,
                &asset_server,
            );
        } else {
            // Procedural fallback: generate geometry from style + color
            spawn_procedural_room(
                &mut commands,
                scene_entity,
                config.style,
                config.base_color,
                &config.layer,
                &mut meshes,
                &mut materials,
            );
        }

        commands.entity(scene_entity).insert(RoomPopulated);
    }
}

// =============================================================================
// Asset-Based Environment
// =============================================================================

/// Load a glTF scene and spawn it as a child of the scene root.
/// The glTF can contain anything — office, park, spaceship, arena.
fn spawn_asset_environment(
    commands: &mut Commands,
    scene_root: Entity,
    asset_path: &str,
    layer: &RenderLayers,
    asset_server: &AssetServer,
) {
    let scene_path = if asset_path.contains('#') {
        asset_path.to_string()
    } else {
        format!("{asset_path}#Scene0")
    };

    let scene_handle: Handle<Scene> = asset_server.load(&scene_path);
    let env_entity = commands
        .spawn((
            SceneRoot(scene_handle),
            Transform::default(),
            layer.clone(),
            EnvironmentGeometry,
        ))
        .id();
    commands.entity(scene_root).add_child(env_entity);
}

// =============================================================================
// Procedural Environment
// =============================================================================

/// Generate procedural room geometry based on style and color.
fn spawn_procedural_room(
    commands: &mut Commands,
    scene_root: Entity,
    style: RoomStyle,
    base_color: Color,
    layer: &RenderLayers,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let mats = derive_materials(base_color, style, materials);

    match style {
        RoomStyle::Office => spawn_office(commands, scene_root, layer, &mats, meshes),
        RoomStyle::Studio => spawn_studio(commands, scene_root, layer, &mats, meshes),
        RoomStyle::Lounge => spawn_lounge(commands, scene_root, layer, &mats, meshes),
    }
}

/// Helper: spawn a mesh entity as a child of the scene root.
fn spawn_room_mesh(
    commands: &mut Commands,
    scene_root: Entity,
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
    transform: Transform,
    layer: &RenderLayers,
) {
    let entity = commands
        .spawn((
            Mesh3d(mesh),
            MeshMaterial3d(material),
            transform,
            layer.clone(),
            EnvironmentGeometry,
        ))
        .id();
    commands.entity(scene_root).add_child(entity);
}

// =============================================================================
// Procedural Room Variants
// =============================================================================

fn spawn_office(
    commands: &mut Commands,
    root: Entity,
    layer: &RenderLayers,
    mats: &RoomMaterials,
    meshes: &mut Assets<Mesh>,
) {
    use layout::*;

    // Back wall
    spawn_room_mesh(
        commands, root,
        meshes.add(Cuboid::new(WALL_WIDTH, WALL_HEIGHT, WALL_THICKNESS)),
        mats.wall.clone(),
        Transform::from_xyz(0.0, WALL_HEIGHT / 2.0, WALL_Z),
        layer,
    );

    // Floor
    spawn_room_mesh(
        commands, root,
        meshes.add(Cuboid::new(FLOOR_WIDTH, 0.02, FLOOR_DEPTH)),
        mats.floor.clone(),
        Transform::from_xyz(0.0, FLOOR_Y, WALL_Z / 2.0),
        layer,
    );

    // Desk
    spawn_room_mesh(
        commands, root,
        meshes.add(Cuboid::new(DESK_WIDTH, DESK_THICKNESS, DESK_DEPTH)),
        mats.desk.clone(),
        Transform::from_xyz(0.0, DESK_Y, DESK_Z),
        layer,
    );

    // Side walls
    let side_mesh = meshes.add(Cuboid::new(WALL_THICKNESS, SIDE_WALL_HEIGHT, SIDE_WALL_DEPTH));
    spawn_room_mesh(
        commands, root,
        side_mesh.clone(),
        mats.side_wall.clone(),
        Transform::from_xyz(-SIDE_WALL_X, SIDE_WALL_HEIGHT / 2.0, WALL_Z / 2.0),
        layer,
    );
    spawn_room_mesh(
        commands, root,
        side_mesh,
        mats.side_wall.clone(),
        Transform::from_xyz(SIDE_WALL_X, SIDE_WALL_HEIGHT / 2.0, WALL_Z / 2.0),
        layer,
    );
}

fn spawn_studio(
    commands: &mut Commands,
    root: Entity,
    layer: &RenderLayers,
    mats: &RoomMaterials,
    meshes: &mut Assets<Mesh>,
) {
    use layout::*;

    // Wide back wall
    spawn_room_mesh(
        commands, root,
        meshes.add(Cuboid::new(WALL_WIDTH * 1.3, WALL_HEIGHT, WALL_THICKNESS)),
        mats.wall.clone(),
        Transform::from_xyz(0.0, WALL_HEIGHT / 2.0, WALL_Z + 0.1),
        layer,
    );

    // Wide floor
    spawn_room_mesh(
        commands, root,
        meshes.add(Cuboid::new(FLOOR_WIDTH * 1.3, 0.02, FLOOR_DEPTH * 1.2)),
        mats.floor.clone(),
        Transform::from_xyz(0.0, FLOOR_Y, WALL_Z / 2.0),
        layer,
    );

    // Large work surface
    spawn_room_mesh(
        commands, root,
        meshes.add(Cuboid::new(DESK_WIDTH * 1.4, DESK_THICKNESS, DESK_DEPTH * 1.1)),
        mats.desk.clone(),
        Transform::from_xyz(0.0, DESK_Y, DESK_Z),
        layer,
    );
}

fn spawn_lounge(
    commands: &mut Commands,
    root: Entity,
    layer: &RenderLayers,
    mats: &RoomMaterials,
    meshes: &mut Assets<Mesh>,
) {
    use layout::*;

    let h = WALL_HEIGHT * 0.85;

    // Back wall — closer, shorter
    spawn_room_mesh(
        commands, root,
        meshes.add(Cuboid::new(WALL_WIDTH * 0.9, h, WALL_THICKNESS)),
        mats.wall.clone(),
        Transform::from_xyz(0.0, h / 2.0, WALL_Z - 0.05),
        layer,
    );

    // Floor
    spawn_room_mesh(
        commands, root,
        meshes.add(Cuboid::new(FLOOR_WIDTH * 0.9, 0.02, FLOOR_DEPTH)),
        mats.floor.clone(),
        Transform::from_xyz(0.0, FLOOR_Y, WALL_Z / 2.0),
        layer,
    );

    // Coffee table
    spawn_room_mesh(
        commands, root,
        meshes.add(Cuboid::new(DESK_WIDTH * 0.8, DESK_THICKNESS * 1.5, DESK_DEPTH * 0.7)),
        mats.desk.clone(),
        Transform::from_xyz(0.0, DESK_Y * 0.85, DESK_Z + 0.05),
        layer,
    );

    // Alcove walls
    let sh = SIDE_WALL_HEIGHT * 0.85;
    let side_mesh = meshes.add(Cuboid::new(WALL_THICKNESS, sh, SIDE_WALL_DEPTH * 0.8));
    spawn_room_mesh(
        commands, root,
        side_mesh.clone(),
        mats.side_wall.clone(),
        Transform::from_xyz(-(SIDE_WALL_X * 0.8), sh / 2.0, WALL_Z / 2.0),
        layer,
    );
    spawn_room_mesh(
        commands, root,
        side_mesh,
        mats.side_wall.clone(),
        Transform::from_xyz(SIDE_WALL_X * 0.8, sh / 2.0, WALL_Z / 2.0),
        layer,
    );
}
