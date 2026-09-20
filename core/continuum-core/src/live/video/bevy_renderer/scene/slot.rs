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
    /// Locked head world-Y from skeleton discovery. Camera uses this instead
    /// of the live head position so breathing/sway don't bob the camera.
    pub camera_head_y: Option<f32>,
    /// All objects in this scene, keyed by a stable string ID.
    /// For avatars, the key is the persona identity.
    /// For props, application-defined. The key IS the object's id.
    pub objects: HashMap<String, Box<dyn SceneObject>>,
}

impl RenderSlot {
    /// Create a new idle render slot with no scene.
    pub fn new(slot_id: u8, readback_entity: Entity, render_target: Handle<Image>) -> Self {
        Self {
            slot_id,
            scene_root: None,
            camera_entity: None,
            camera_head_y: None,
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

    // --- Generic typed accessors (the ONE downcast seam) ---

    /// Borrow an object of concrete type `T` by id, if present and of that type.
    pub fn object_as<T: 'static>(&self, id: &str) -> Option<&T> {
        self.objects
            .get(id)
            .and_then(|obj| obj.as_any().downcast_ref::<T>())
    }

    /// Mutably borrow an object of concrete type `T` by id.
    pub fn object_as_mut<T: 'static>(&mut self, id: &str) -> Option<&mut T> {
        self.objects
            .get_mut(id)
            .and_then(|obj| obj.as_any_mut().downcast_mut::<T>())
    }

    /// Iterate all objects of concrete type `T` in this scene, with their ids.
    pub fn objects_of<T: 'static>(&self) -> impl Iterator<Item = (&str, &T)> {
        self.objects
            .iter()
            .filter_map(|(id, obj)| obj.as_any().downcast_ref::<T>().map(|t| (id.as_str(), t)))
    }

    /// Mutably iterate all objects of concrete type `T` in this scene.
    pub fn objects_of_mut<T: 'static>(&mut self) -> impl Iterator<Item = (&str, &mut T)> {
        self.objects.iter_mut().filter_map(|(id, obj)| {
            obj.as_any_mut()
                .downcast_mut::<T>()
                .map(|t| (id.as_str(), t))
        })
    }

    // --- Typed avatar accessors (thin wrappers over the generic seam;
    //     signatures preserved so existing call sites are untouched) ---

    /// Iterate all avatars in this scene.
    pub fn avatars(&self) -> impl Iterator<Item = (&str, &AvatarObject)> {
        self.objects_of::<AvatarObject>()
    }

    /// Mutably iterate all avatars in this scene.
    pub fn avatars_mut(&mut self) -> impl Iterator<Item = (&str, &mut AvatarObject)> {
        self.objects_of_mut::<AvatarObject>()
    }

    /// Get the primary (first) avatar. For single-avatar slots this is THE avatar.
    pub fn primary_avatar(&self) -> Option<&AvatarObject> {
        self.avatars().next().map(|(_, a)| a)
    }

    /// Get the primary avatar mutably.
    pub fn primary_avatar_mut(&mut self) -> Option<&mut AvatarObject> {
        self.objects
            .values_mut()
            .find_map(|obj| obj.as_any_mut().downcast_mut::<AvatarObject>())
    }

    /// Get a specific avatar by its object ID.
    pub fn avatar(&self, id: &str) -> Option<&AvatarObject> {
        self.object_as::<AvatarObject>(id)
    }

    /// Get a specific avatar mutably.
    pub fn avatar_mut(&mut self, id: &str) -> Option<&mut AvatarObject> {
        self.object_as_mut::<AvatarObject>(id)
    }

    /// Count of loaded avatars in this scene.
    pub fn avatar_count(&self) -> usize {
        self.avatars().filter(|(_, a)| a.state.model_loaded).count()
    }

    // --- Generic object management ---

    /// Add an object to this scene. Returns the previous object at that ID, if any.
    pub fn add_object(
        &mut self,
        id: String,
        object: Box<dyn SceneObject>,
    ) -> Option<Box<dyn SceneObject>> {
        self.objects.insert(id, object)
    }

    /// Remove an object from the scene. Caller must despawn its entity.
    pub fn remove_object(&mut self, id: &str) -> Option<Box<dyn SceneObject>> {
        self.objects.remove(id)
    }

    /// Tear down the scene — despawning the root recursively cleans everything.
    /// Clears all slot state except render infrastructure.
    pub fn teardown(&mut self, commands: &mut Commands) {
        if let Some(root) = self.scene_root.take() {
            commands.entity(root).despawn();
        }
        self.camera_entity = None;
        self.camera_head_y = None;
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
