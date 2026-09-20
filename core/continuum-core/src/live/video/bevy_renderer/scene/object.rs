//! Scene objects — anything that lives in a scene.
//!
//! `SceneObject` is a TRAIT, not a closed enum: a scene holds
//! `Box<dyn SceneObject>` values keyed by a stable string id. Adding a new
//! kind of thing to a scene = one more `impl SceneObject`, with zero edits to
//! the slot, the enum, or a hand-written downcast matrix. This is the
//! polymorphism-over-enums doctrine (cv::Algorithm-style) applied to the scene
//! graph, and it is what lets a future Unreal backend or a generated prop drop
//! in as an adapter rather than a new variant everyone must match on.
//!
//! Two maximally-different impls validate the trait, then we STOP:
//!   - outlier A: [`AvatarObject`] — animated VRM with skeleton, morphs, speech
//!   - outlier B: [`PropSceneObject`] — a static GLB with none of that
//!
//! The environment (room/backdrop) is deliberately NOT a `SceneObject`: it is
//! an ECS `RoomConfig` child of the scene root (`scene::room::populate_rooms`),
//! recursively despawned with the root. That is why the old `EnvironmentObject`
//! variant was dead — don't reintroduce it here.

use bevy::prelude::*;
use std::any::Any;

use super::avatar::AvatarObject;

/// Anything that can live in a scene slot. Object-safe by construction (no
/// generics, no `Self` returns) so it can be stored as `Box<dyn SceneObject>`.
///
/// `id` is intentionally NOT on the trait: the slot's `HashMap` key IS the id
/// (one source of truth — the compression principle), so storing it again on
/// every object would be redundant state that can drift from the key.
pub trait SceneObject: Send + Sync {
    /// The ECS entity for this object, if spawned.
    fn entity(&self) -> Option<Entity>;

    /// Record the ECS entity after spawning.
    fn set_entity(&mut self, entity: Entity);

    /// True once this object is fully loaded/spawned in the ECS. The exact
    /// meaning is object-specific (an avatar needs its glTF scene ready and
    /// bones discovered; a prop only needs its entity spawned).
    fn is_loaded(&self) -> bool;

    /// Downcast seam — the ONE place typed access is recovered. Callers use
    /// the slot's `object_as::<T>` / `objects_of::<T>` helpers, never this
    /// directly.
    fn as_any(&self) -> &dyn Any;

    /// Mutable downcast seam.
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

// =============================================================================
// Outlier A — Avatar (animated VRM: skeleton, morph targets, speech, emotion)
// =============================================================================

impl SceneObject for AvatarObject {
    fn entity(&self) -> Option<Entity> {
        self.entity
    }

    fn set_entity(&mut self, entity: Entity) {
        self.entity = Some(entity);
    }

    fn is_loaded(&self) -> bool {
        // An avatar isn't "loaded" until its glTF scene is ready and its
        // skeleton discovered — tracked by the SceneInstanceReady observer.
        self.state.model_loaded
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

// =============================================================================
// Outlier B — Prop (static, non-animated glTF/GLB: furniture, vehicle, terrain)
// =============================================================================

/// A static (non-animated) 3D model in the scene — the maximally-different
/// outlier that proves the trait: no skeleton, no morphs, no speech, "loaded"
/// means nothing more than "its entity has been spawned."
pub struct PropSceneObject {
    /// ECS entity (child of scene root). None until spawned.
    pub entity: Option<Entity>,
    /// Path to the glTF/GLB asset.
    pub model_path: String,
    /// Handle to the loaded scene asset (kept alive so the asset isn't dropped).
    pub handle: Option<Handle<Scene>>,
}

impl PropSceneObject {
    pub fn new(model_path: String) -> Self {
        Self {
            entity: None,
            model_path,
            handle: None,
        }
    }
}

impl SceneObject for PropSceneObject {
    fn entity(&self) -> Option<Entity> {
        self.entity
    }

    fn set_entity(&mut self, entity: Entity) {
        self.entity = Some(entity);
    }

    fn is_loaded(&self) -> bool {
        self.entity.is_some()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
