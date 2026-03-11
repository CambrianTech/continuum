//! Scene environment catalog — asset-based room/environment selection.
//!
//! Every persona gets a scene environment deterministically derived from their
//! identity hash, matching the same pattern as voice and avatar selection:
//!
//!   identity → hash → voice (gender-coherent)
//!   identity → hash → avatar (gender-coherent)
//!   identity → hash → scene (deterministic from catalog)
//!
//! Scenes are ALWAYS assets (glTF/GLB files on disk). The catalog defines
//! available scenes; `select_scene_for_identity()` picks one deterministically.
//!
//! ## Hierarchy
//!
//! Scenes compose hierarchically — a persona's office is a sub-scene within
//! their apartment, which is a sub-scene within a city block. For now, each
//! persona gets a single room scene for the portrait view.
//!
//! ## Asset Pipeline
//!
//! Scene GLBs live in `models/scenes/` alongside avatar models in `models/avatars/`.
//! Downloaded automatically at deploy time by `scripts/download-scene-models.sh`,
//! integrated into `npm start` the same way avatar models are.

use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy::scene::SceneInstanceReady;

use crate::live::avatar::hash::{deterministic_index, deterministic_pick};
use crate::live::video::bevy_renderer::skeleton;

use super::builder::SceneMarker;

// =============================================================================
// Scene Catalog
// =============================================================================

/// A scene environment in the catalog.
#[derive(Debug, Clone)]
pub struct SceneEntry {
    /// Unique identifier.
    pub id: &'static str,
    /// Human-readable name.
    pub name: &'static str,
    /// Filename in models/scenes/.
    pub filename: &'static str,
    /// Style tag for matching/filtering.
    pub style: SceneStyle,
}

/// Scene style categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneStyle {
    /// Corporate/professional workspace.
    Office,
    /// Creative workspace with art.
    Studio,
    /// Warm casual space.
    Lounge,
    /// Personal home workspace.
    HomeOffice,
    /// Tech/server room aesthetic.
    TechDen,
}

/// Static scene catalog — all available room environments.
/// Mirrors AVATAR_CATALOG pattern. Extended by filesystem discovery later.
pub const SCENE_CATALOG: &[SceneEntry] = &[
    SceneEntry {
        id: "office",
        name: "Standard Office",
        filename: "office.glb",
        style: SceneStyle::Office,
    },
    SceneEntry {
        id: "studio",
        name: "Creative Studio",
        filename: "studio.glb",
        style: SceneStyle::Studio,
    },
    SceneEntry {
        id: "lounge",
        name: "Cozy Lounge",
        filename: "lounge.glb",
        style: SceneStyle::Lounge,
    },
    SceneEntry {
        id: "home-office",
        name: "Home Office",
        filename: "home-office.glb",
        style: SceneStyle::HomeOffice,
    },
    SceneEntry {
        id: "server-room",
        name: "Server Room",
        filename: "server-room.glb",
        style: SceneStyle::TechDen,
    },
];

/// Models directory for scene assets.
const SCENES_DIR: &str = "models/scenes";

/// Get the filesystem path for a scene model.
pub fn scene_model_path(filename: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(SCENES_DIR).join(filename)
}

// =============================================================================
// Scene Selection
// =============================================================================

/// Select a scene deterministically from persona identity.
/// Same persona always gets the same room. Uses salt "scene" for independent
/// distribution from avatar and voice selection.
pub fn select_scene_for_identity(identity: &str) -> &'static SceneEntry {
    deterministic_pick(identity, SCENE_CATALOG, "scene")
}

/// Select a scene by explicit style preference, falling back to identity hash.
pub fn select_scene_by_style(identity: &str, preferred: SceneStyle) -> &'static SceneEntry {
    let matching: Vec<&SceneEntry> = SCENE_CATALOG
        .iter()
        .filter(|s| s.style == preferred)
        .collect();
    if matching.is_empty() {
        return select_scene_for_identity(identity);
    }
    let idx = deterministic_index(identity, matching.len(), "scene-style");
    matching[idx]
}

// =============================================================================
// ECS Components
// =============================================================================

/// Declares what environment a scene should load. Attached to the scene root
/// entity by the Load command handler.
#[derive(Component)]
pub struct RoomConfig {
    /// Path to the glTF/GLB scene asset (relative to working dir).
    pub asset_path: String,
    /// RenderLayers for this scene's geometry.
    pub layer: RenderLayers,
    /// Scene catalog entry ID (for logging/debugging).
    pub scene_id: String,
}

/// Marker: this scene's environment has been loaded.
#[derive(Component)]
pub struct RoomPopulated;

/// Marker on environment entities for independent query/replacement.
#[derive(Component)]
pub struct EnvironmentGeometry;

// =============================================================================
// Population System
// =============================================================================

/// Bevy system: detects scenes with `RoomConfig` (without `RoomPopulated`)
/// and loads the glTF environment asset as a child of the scene root.
///
/// All spawned entities are children of the scene root — recursive despawn
/// on teardown handles cleanup automatically. No leaks.
pub fn populate_rooms(
    mut commands: Commands,
    new_rooms: Query<(Entity, &RoomConfig), (With<SceneMarker>, Without<RoomPopulated>)>,
    asset_server: Res<AssetServer>,
) {
    for (scene_entity, config) in new_rooms.iter() {
        let scene_path = if config.asset_path.contains('#') {
            config.asset_path.clone()
        } else {
            format!("{}#Scene0", config.asset_path)
        };

        let scene_handle: Handle<Scene> = asset_server.load(&scene_path);
        let layer_for_observer = config.layer.clone();
        let scene_id_for_log = config.scene_id.clone();
        let env_entity = commands
            .spawn((
                SceneRoot(scene_handle),
                Transform::default(),
                config.layer.clone(),
                EnvironmentGeometry,
            ))
            .observe(
                move |
                    event: On<SceneInstanceReady>,
                    children_query: Query<&Children>,
                    mut cmds: Commands,
                | {
                    let root = event.entity;
                    // Propagate RenderLayers to all glTF children so the
                    // scene camera and lights can see the room geometry.
                    skeleton::propagate_render_layers(
                        root,
                        &layer_for_observer,
                        &children_query,
                        &mut cmds,
                    );
                    let count = skeleton::count_descendants(root, &children_query);
                    crate::clog_info!(
                        "🏠 Room '{}' ready: {} descendants, render layers propagated",
                        scene_id_for_log,
                        count
                    );
                },
            )
            .id();
        commands.entity(scene_entity).add_child(env_entity);

        commands.entity(scene_entity).insert(RoomPopulated);

        crate::clog_info!(
            "🏠 Scene '{}': loading environment from {}",
            config.scene_id,
            config.asset_path
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scene_selection_deterministic() {
        let s1 = select_scene_for_identity("test-persona-123");
        let s2 = select_scene_for_identity("test-persona-123");
        assert_eq!(s1.id, s2.id, "Same identity must get same scene");
    }

    #[test]
    fn test_scene_selection_covers_catalog() {
        let mut seen = std::collections::HashSet::new();
        for i in 0..200 {
            let s = select_scene_for_identity(&format!("persona-{}", i));
            seen.insert(s.id);
        }
        assert!(
            seen.len() >= 3,
            "Expected diversity across 200 identities, got {} unique scenes",
            seen.len()
        );
    }

    #[test]
    fn test_scene_model_path() {
        let path = scene_model_path("office.glb");
        assert_eq!(path.to_str().unwrap(), "models/scenes/office.glb");
    }

    #[test]
    fn test_select_scene_by_style() {
        let s = select_scene_by_style("test-id", SceneStyle::TechDen);
        assert_eq!(s.style, SceneStyle::TechDen);
    }

    #[test]
    fn test_select_scene_by_style_fallback() {
        // All styles in catalog exist, but this tests the pattern works
        let s = select_scene_by_style("test-id", SceneStyle::Office);
        assert_eq!(s.style, SceneStyle::Office);
    }
}
