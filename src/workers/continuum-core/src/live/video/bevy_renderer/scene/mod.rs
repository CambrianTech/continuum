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
//! - `object` — SceneObject enum + variant structs
//! - `avatar` — AvatarState, animation state types, morph targets, bones
//! - `lighting` — LightRig configurations and spawn functions
//! - `builder` — SceneConfig, build_scene(), marker components

mod avatar;
mod builder;
mod lighting;
mod object;
pub mod room;
mod slot;

pub use avatar::*;
pub use builder::*;
pub use lighting::*;
pub use object::*;
pub use room::{RoomConfig, select_scene_for_identity, scene_model_path};
pub use slot::*;
