// Scene module = a backend-neutral scene invariant (`SceneDescription`) with
// THREE producers and ONE consumer. Today only one producer (`birther`) and the
// one consumer (`instantiate`) are wired into the live Load path; the file
// producer (`library::resolve_scene` + `EMBEDDED_SCENES`), the fluent producer
// (`builder_api::SceneBuilder`), the physics install path (`physics::…::set`),
// the second `SceneObject` outlier (`PropSceneObject`), and the generic slot
// helpers are validated by unit tests but not yet called by a command — the
// deliberate outlier-validate-then-STOP seam Slices 2–3 consume. dead_code is
// allowed here for that ahead-of-consumption API surface; remove this once the
// scene file-loader + builder are wired to commands.
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
//! The scene **description** is the backend-neutral invariant, produced three
//! ways and consumed one way:
//!
//! - `description` — SceneDescription: the backend-neutral scene-graph data (the invariant)
//! - `library` — file producer: resolve a scene reference to a SceneDescription (embed/override/fail-loud)
//! - `builder_api` — fluent producer: SceneBuilder → the same SceneDescription
//! - `birther` — procedural producer: birth a scene from a persona identity
//! - `instantiate` — the single consumer: walk a SceneDescription into a live Bevy graph
//! - `object` — SceneObject trait + concrete impls (avatar, prop)
//! - `physics` — PhysicsBackend adapter seam (data now, no simulation in the base engine)
//! - `avatar` — AvatarState, animation state types, morph targets, bones
//! - `animation` — AnimationConfig component + profiles (portrait, full-body, minimal)
//! - `builder` — scene marker components + identity-derived room color (shared bits)
//! - `room` — environment catalog + RoomConfig / populate_rooms

pub mod animation;
pub(crate) mod avatar;
pub mod birther;
mod builder;
pub mod builder_api;
pub mod description;
pub mod instantiate;
pub mod library;
mod object;
pub mod physics;
pub mod room;
mod slot;

pub use animation::AnimationConfig;
pub use avatar::*;
pub use birther::birth_scene_for_identity;
pub use builder::*;
pub use builder_api::SceneBuilder;
pub use description::*;
pub use instantiate::{
    build_scene_from_description, spawn_global_lights, InstantiateParams, SceneInstance,
};
pub use library::resolve_scene;
pub use object::*;
pub use physics::*;
pub use room::{scene_model_path, select_scene_for_identity, RoomConfig};
pub use slot::*;
