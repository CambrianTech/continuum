// Scene infrastructure — many types/methods exist for future use (multi-avatar scenes,
// props, environments). Suppress dead_code for the entire scene module.
#![allow(dead_code)]

//! Scene Graph — generic Bevy hierarchy-based scene management.
//!
//! A scene is a PARENT ENTITY in the Bevy ECS. Everything in the scene
//! (camera, lights, models, props, environments) is a CHILD entity.
//! Despawning the parent recursively cleans up everything (Bevy 0.18+).
//!
//! The scene system is GENERIC — it knows about objects, not specifically
//! about avatars. An avatar is one type of `SceneObject`. A static mesh,
//! environment, or particle system are others.
//!
//! ## Module Structure
//!
//! - `animation` — AnimationConfig component + profiles (portrait, full-body, minimal)
//! - `description` — SceneDescription: the backend-neutral, representation-neutral scene-graph data
//! - `object` — SceneObject trait + concrete impls (avatar, prop)
//! - `physics` — PhysicsBackend adapter seam (data now, no simulation in the base engine)
//! - `avatar` — AvatarState, animation state types, morph targets, bones
//! - `lighting` — LightRig configurations and spawn functions
//! - `builder` — SceneConfig, build_scene(), marker components

pub mod animation;
pub(crate) mod avatar;
mod builder;
pub mod description;
pub mod library;
mod lighting;
mod object;
pub mod physics;
pub mod room;
mod slot;

pub use animation::AnimationConfig;
pub use avatar::*;
pub use builder::*;
pub use description::*;
pub use library::resolve_scene;
pub use lighting::*;
pub use object::*;
pub use physics::*;
pub use room::{scene_model_path, select_scene_for_identity, RoomConfig};
pub use slot::*;
