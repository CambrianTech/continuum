//! Scene objects — anything that lives in a scene.
//!
//! Each variant owns its specific state. Adding a new object type =
//! adding a variant here + its state struct.

use bevy::prelude::*;

use super::avatar::AvatarObject;

/// A typed object within a scene.
pub enum SceneObject {
    /// An animated VRM/glTF character with morph targets, bones, speech.
    Avatar(AvatarObject),
    /// A static 3D model — prop, furniture, vehicle, terrain feature.
    StaticMesh(StaticMeshObject),
    /// A full environment — room, outdoor scene, skybox + ground.
    Environment(EnvironmentObject),
}

impl SceneObject {
    /// The ECS entity for this object (if spawned).
    pub fn entity(&self) -> Option<Entity> {
        match self {
            SceneObject::Avatar(a) => a.entity,
            SceneObject::StaticMesh(m) => m.entity,
            SceneObject::Environment(e) => e.entity,
        }
    }

    /// Set the ECS entity after spawning.
    pub fn set_entity(&mut self, entity: Entity) {
        match self {
            SceneObject::Avatar(a) => a.entity = Some(entity),
            SceneObject::StaticMesh(m) => m.entity = Some(entity),
            SceneObject::Environment(e) => e.entity = Some(entity),
        }
    }

    /// True if this object has been loaded/spawned in the ECS.
    pub fn is_loaded(&self) -> bool {
        match self {
            SceneObject::Avatar(a) => a.state.model_loaded,
            SceneObject::StaticMesh(m) => m.entity.is_some(),
            SceneObject::Environment(e) => e.entity.is_some(),
        }
    }

    /// Get this object as an avatar, if it is one.
    pub fn as_avatar(&self) -> Option<&AvatarObject> {
        match self {
            SceneObject::Avatar(a) => Some(a),
            _ => None,
        }
    }

    /// Get this object as a mutable avatar, if it is one.
    pub fn as_avatar_mut(&mut self) -> Option<&mut AvatarObject> {
        match self {
            SceneObject::Avatar(a) => Some(a),
            _ => None,
        }
    }

    /// Get this object as a static mesh, if it is one.
    pub fn as_static_mesh(&self) -> Option<&StaticMeshObject> {
        match self {
            SceneObject::StaticMesh(m) => Some(m),
            _ => None,
        }
    }

    /// Get this object as a mutable static mesh, if it is one.
    pub fn as_static_mesh_mut(&mut self) -> Option<&mut StaticMeshObject> {
        match self {
            SceneObject::StaticMesh(m) => Some(m),
            _ => None,
        }
    }

    /// Get this object as an environment, if it is one.
    pub fn as_environment(&self) -> Option<&EnvironmentObject> {
        match self {
            SceneObject::Environment(e) => Some(e),
            _ => None,
        }
    }

    /// Get this object as a mutable environment, if it is one.
    pub fn as_environment_mut(&mut self) -> Option<&mut EnvironmentObject> {
        match self {
            SceneObject::Environment(e) => Some(e),
            _ => None,
        }
    }
}

// =============================================================================
// Static Mesh — prop, terrain, vehicle, etc.
// =============================================================================

/// A static (non-animated) 3D model in the scene.
pub struct StaticMeshObject {
    /// ECS entity (child of scene root). None until spawned.
    pub entity: Option<Entity>,
    /// Path to the glTF/GLB file.
    pub model_path: String,
    /// Handle to the loaded asset.
    pub handle: Option<Handle<Scene>>,
    /// Position/rotation/scale within the scene.
    pub local_transform: Transform,
}

impl StaticMeshObject {
    pub fn new(model_path: String, transform: Transform) -> Self {
        Self {
            entity: None,
            model_path,
            handle: None,
            local_transform: transform,
        }
    }
}

// =============================================================================
// Environment — backgrounds, rooms, skyboxes
// =============================================================================

/// An environment enclosure — a room, outdoor scene, or skybox.
/// Typically loaded as a full glTF scene that provides the "stage."
pub struct EnvironmentObject {
    /// ECS entity (child of scene root). None until spawned.
    pub entity: Option<Entity>,
    /// Path to the environment glTF/GLB.
    pub model_path: String,
    /// Handle to the loaded asset.
    pub handle: Option<Handle<Scene>>,
    /// Position/rotation/scale within the scene.
    pub local_transform: Transform,
}

impl EnvironmentObject {
    pub fn new(model_path: String, transform: Transform) -> Self {
        Self {
            entity: None,
            model_path,
            handle: None,
            local_transform: transform,
        }
    }
}
