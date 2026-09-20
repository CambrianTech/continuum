//! `scene::birther` — the procedural producer of a [`SceneDescription`] from a
//! persona identity. The other two producers are the file loader
//! ([`super::library::resolve_scene`]) and the fluent
//! [`super::builder_api::SceneBuilder`]; all three emit the identical type, which
//! [`super::instantiate`] then walks into a live graph.
//!
//! This replaces the const-hash-pick logic that used to live inline in the
//! `commands.rs` Load arm: the scene choice (backdrop tint, room environment,
//! portrait framing) becomes *data generation* rather than imperative spawning.
//!
//! ## No light nodes
//!
//! The birther emits NO [`NodePayload::Light`] nodes. The Bevy backend supplies
//! lighting through a shared global rig ([`super::instantiate::spawn_global_lights`])
//! to stay within Bevy's directional-light budget; a per-scene light node would
//! fail loud at instantiation ([[fallbacks-are-illegal-fail-loud]]).
//!
//! ## The regression this guards
//!
//! The avatar node's `id` is the persona `identity`. Instantiate keys the boxed
//! `AvatarObject` and the `SceneInstanceReady` observer on that same `id`, so a
//! birthed avatar's `model_loaded` flips correctly. If this ever stops being the
//! identity, the slot never goes active (see the instantiate module docs).

use bevy::prelude::{Transform, Vec3};

use super::builder::room_color_from_identity;
use super::description::{
    AnimationProfileKind, AssetRef, AvatarPayload, CameraPayload, EnvironmentPayload, NodePayload,
    SceneDescription, SceneNode, TransformDesc, SCENE_DESCRIPTION_VERSION,
};
use super::room::{scene_model_path, select_scene_for_identity};
use crate::live::video::bevy_renderer::skeleton;

/// Birth a portrait scene for a persona: an identity-tinted backdrop, a
/// head-locked portrait camera, the identity's deterministic room environment,
/// and the avatar itself. Deterministic — the same `(identity, model_path,
/// display_name)` always yields the identical [`SceneDescription`].
pub fn birth_scene_for_identity(
    identity: &str,
    model_path: &str,
    display_name: &str,
) -> SceneDescription {
    let backdrop = room_color_from_identity(identity).into();

    // Default portrait framing — matches the pre-data-driven `build_scene`
    // default: eye-level, pulled back on -Z, looking at just below the head.
    // Computed with Bevy's `looking_at` then projected to the neutral
    // `TransformDesc` so the framing is baked into the description as data.
    let camera_xf: TransformDesc = Transform::from_xyz(
        0.0,
        skeleton::REFERENCE_HEAD_Y,
        skeleton::REFERENCE_CAMERA_Z,
    )
    .looking_at(
        Vec3::new(0.0, skeleton::REFERENCE_HEAD_Y - 0.02, 0.0),
        Vec3::Y,
    )
    .into();

    let scene_entry = select_scene_for_identity(identity);
    let env_asset = scene_model_path(scene_entry.filename)
        .to_string_lossy()
        .to_string();

    let root = SceneNode::group("scene")
        .with_child(
            SceneNode::leaf(
                "camera",
                NodePayload::Camera(CameraPayload { head_lock: true }),
            )
            .with_transform(camera_xf),
        )
        .with_child(SceneNode::leaf(
            "environment",
            NodePayload::Environment(EnvironmentPayload {
                scene_id: scene_entry.id.to_string(),
                asset: AssetRef::path(env_asset),
            }),
        ))
        // Avatar node id == identity — the instantiate/observer/objects-map key.
        .with_child(SceneNode::leaf(
            identity,
            NodePayload::Avatar(AvatarPayload {
                asset: AssetRef::path(model_path),
                display_name: display_name.to_string(),
                animation: AnimationProfileKind::Portrait,
            }),
        ));

    SceneDescription {
        version: SCENE_DESCRIPTION_VERSION,
        backdrop,
        root,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the birther becoming non-deterministic — the same
    // identity producing a different scene, which would break replay/caching and
    // the "same persona always gets the same room" invariant.
    #[test]
    fn birthing_is_deterministic_for_an_identity() {
        let a = birth_scene_for_identity("asha", "models/avatars/asha.vrm", "Asha");
        let b = birth_scene_for_identity("asha", "models/avatars/asha.vrm", "Asha");
        assert_eq!(a, b, "same identity must birth an identical scene");
    }

    // what this catches: the avatar node's id drifting away from the persona
    // identity — the exact mismatch that leaves model_loaded stuck false forever
    // (the observer keys off this id). Guards the instantiate contract.
    #[test]
    fn avatar_node_id_is_the_identity() {
        let scene = birth_scene_for_identity("asha", "models/avatars/asha.vrm", "Asha");
        let avatar = scene
            .root
            .children
            .iter()
            .find(|n| matches!(n.payload, NodePayload::Avatar(_)))
            .expect("birthed scene must contain an avatar node");
        assert_eq!(avatar.id, "asha", "avatar node id must be the identity");
    }

    // what this catches: the birther regressing to emit per-scene light nodes,
    // which instantiate would then reject with a fail-loud error — the birther
    // must rely on the shared global rig, never author lights itself.
    #[test]
    fn birther_emits_no_light_nodes() {
        let scene = birth_scene_for_identity("asha", "models/avatars/asha.vrm", "Asha");
        let has_light = scene
            .root
            .children
            .iter()
            .any(|n| matches!(n.payload, NodePayload::Light(_)));
        assert!(
            !has_light,
            "birther must emit no light nodes (global rig supplies lighting)"
        );
    }
}
