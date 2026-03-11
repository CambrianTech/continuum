//! Render slots and slot registry — viewport management.
//!
//! A RenderSlot is pre-allocated render infrastructure (render target, readback).
//! It contains one scene with any number of typed objects.
//! The SlotRegistry is the Bevy Resource that holds all slots.

use bevy::asset::Handle;
use bevy::prelude::*;
use std::collections::HashMap;

use super::avatar::AvatarObject;
use super::object::SceneObject;
use crate::live::video::bevy_renderer::{AVATAR_HEIGHT, AVATAR_WIDTH};

// =============================================================================
// Render Slot
// =============================================================================

/// A render slot is pre-allocated render infrastructure (render target, readback).
/// It contains one scene with any number of objects. The camera is a child of
/// the scene root, but the slot tracks render target handles for HD pool
/// management and GPU bridge lookups.
pub struct RenderSlot {
    pub slot_id: u8,
    /// The scene root entity. None = slot is idle, no scene spawned.
    pub scene_root: Option<Entity>,
    /// Camera entity (child of scene root). Needed for is_active toggling.
    pub camera_entity: Option<Entity>,
    /// Readback observer entity (separate from scene hierarchy).
    pub readback_entity: Entity,
    /// Currently active render target (may be HD if spotlight assigned).
    pub render_target: Handle<Image>,
    /// The slot's own low-res render target (640×360).
    pub default_render_target: Handle<Image>,
    pub dimensions: (u32, u32),
    /// All objects in this scene, keyed by a stable string ID.
    /// For avatars, the key is typically the persona identity.
    /// For props/environments, application-defined.
    pub objects: HashMap<String, SceneObject>,
}

impl RenderSlot {
    /// Create a new idle render slot with no scene.
    pub fn new(
        slot_id: u8,
        readback_entity: Entity,
        render_target: Handle<Image>,
    ) -> Self {
        Self {
            slot_id,
            scene_root: None,
            camera_entity: None,
            readback_entity,
            render_target: render_target.clone(),
            default_render_target: render_target,
            dimensions: (AVATAR_WIDTH, AVATAR_HEIGHT),
            objects: HashMap::new(),
        }
    }

    /// Get the render target's AssetId for render-world lookups.
    pub fn render_target_id(&self) -> bevy::asset::AssetId<Image> {
        self.render_target.id()
    }

    /// True if this slot has a scene with at least one loaded object.
    pub fn is_active(&self) -> bool {
        self.objects.values().any(|obj| obj.is_loaded())
    }

    /// True if any avatar in this scene is speaking.
    pub fn is_speaking(&self) -> bool {
        self.avatars().any(|(_, a)| a.is_speaking())
    }

    // --- Typed avatar accessors ---

    /// Iterate all avatars in this scene.
    pub fn avatars(&self) -> impl Iterator<Item = (&str, &AvatarObject)> {
        self.objects.iter().filter_map(|(id, obj)| {
            obj.as_avatar().map(|a| (id.as_str(), a))
        })
    }

    /// Mutably iterate all avatars in this scene.
    pub fn avatars_mut(&mut self) -> impl Iterator<Item = (&str, &mut AvatarObject)> {
        self.objects.iter_mut().filter_map(|(id, obj)| {
            obj.as_avatar_mut().map(|a| (id.as_str(), a))
        })
    }

    /// Get the primary (first) avatar. For single-avatar slots this is THE avatar.
    pub fn primary_avatar(&self) -> Option<&AvatarObject> {
        self.avatars().next().map(|(_, a)| a)
    }

    /// Get the primary avatar mutably.
    pub fn primary_avatar_mut(&mut self) -> Option<&mut AvatarObject> {
        self.objects.values_mut().find_map(|obj| obj.as_avatar_mut())
    }

    /// Get a specific avatar by its object ID.
    pub fn avatar(&self, id: &str) -> Option<&AvatarObject> {
        self.objects.get(id).and_then(|obj| obj.as_avatar())
    }

    /// Get a specific avatar mutably.
    pub fn avatar_mut(&mut self, id: &str) -> Option<&mut AvatarObject> {
        self.objects.get_mut(id).and_then(|obj| obj.as_avatar_mut())
    }

    /// Count of loaded avatars in this scene.
    pub fn avatar_count(&self) -> usize {
        self.avatars().filter(|(_, a)| a.state.model_loaded).count()
    }

    // --- Generic object management ---

    /// Add an object to this scene. Returns the previous object at that ID, if any.
    pub fn add_object(&mut self, id: String, object: SceneObject) -> Option<SceneObject> {
        self.objects.insert(id, object)
    }

    /// Remove an object from the scene. Caller must despawn its entity.
    pub fn remove_object(&mut self, id: &str) -> Option<SceneObject> {
        self.objects.remove(id)
    }

    /// Tear down the scene — despawning the root recursively cleans everything.
    /// Clears all slot state except render infrastructure.
    pub fn teardown(&mut self, commands: &mut Commands) {
        if let Some(root) = self.scene_root.take() {
            commands.entity(root).despawn();
        }
        self.camera_entity = None;
        self.objects.clear();
    }
}

// =============================================================================
// Slot Registry
// =============================================================================

/// Global registry of all render slots.
#[derive(Resource, Default)]
pub struct SlotRegistry {
    pub slots: HashMap<u8, RenderSlot>,
}

impl SlotRegistry {
    /// Iterate all active slots (have at least one loaded object).
    pub fn active_slots(&self) -> impl Iterator<Item = (&u8, &RenderSlot)> {
        self.slots.iter().filter(|(_, s)| s.is_active())
    }

    /// Mutable iterate all active slots.
    pub fn active_slots_mut(&mut self) -> impl Iterator<Item = (&u8, &mut RenderSlot)> {
        self.slots.iter_mut().filter(|(_, s)| s.is_active())
    }

    /// Count of slots with loaded models.
    pub fn loaded_count(&self) -> u8 {
        self.slots.values().filter(|s| s.is_active()).count() as u8
    }

    /// Count of currently speaking slots.
    pub fn speaking_count(&self) -> u8 {
        self.slots.values().filter(|s| s.is_speaking()).count() as u8
    }

    /// Total avatar count across all slots.
    pub fn total_avatars(&self) -> usize {
        self.slots.values().map(|s| s.avatar_count()).sum()
    }
}
