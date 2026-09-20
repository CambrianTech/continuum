//! `scene::builder_api` — a fluent [`SceneBuilder`] producing the SAME
//! [`SceneDescription`] the file loader ([`super::library::resolve_scene`]) and
//! the [`super::birther`] produce. One invariant, three producers, one consumer
//! ([`super::instantiate`]).
//!
//! This is the hand-authoring ergonomic front door: a test, a tool, or future
//! editor code builds a scene fluently instead of hand-writing RON or nesting
//! [`SceneNode`]s directly. Because it emits the identical type, a builder-built
//! scene and a parsed-from-RON scene are byte-for-byte comparable (the
//! round-trip test below).

use super::description::{
    AnimationProfileKind, AssetRef, AvatarPayload, CameraPayload, ColorDesc, EnvironmentPayload,
    NodePayload, PropPayload, SceneDescription, SceneNode, TransformDesc,
    SCENE_DESCRIPTION_VERSION,
};

/// Fluent builder for a [`SceneDescription`]. Every `with_*` method appends a
/// child of the scene root and returns `self`; [`SceneBuilder::build`] finalizes.
pub struct SceneBuilder {
    backdrop: ColorDesc,
    children: Vec<SceneNode>,
}

impl SceneBuilder {
    /// A new builder with a black backdrop and no children.
    pub fn new() -> Self {
        Self {
            backdrop: ColorDesc::BLACK,
            children: Vec::new(),
        }
    }

    /// Set the scene backdrop (clear) color.
    pub fn backdrop(mut self, color: ColorDesc) -> Self {
        self.backdrop = color;
        self
    }

    /// Add a camera node with the given head-lock behavior and framing.
    pub fn with_camera(mut self, head_lock: bool, transform: TransformDesc) -> Self {
        self.children.push(
            SceneNode::leaf("camera", NodePayload::Camera(CameraPayload { head_lock }))
                .with_transform(transform),
        );
        self
    }

    /// Add an avatar node. `id` is the load identity — it MUST be unique and is
    /// the key the instantiator uses for the object map and the load observer.
    pub fn with_avatar(
        mut self,
        id: impl Into<String>,
        asset: impl Into<String>,
        display_name: impl Into<String>,
        animation: AnimationProfileKind,
        transform: TransformDesc,
    ) -> Self {
        self.children.push(
            SceneNode::leaf(
                id,
                NodePayload::Avatar(AvatarPayload {
                    asset: AssetRef::path(asset),
                    display_name: display_name.into(),
                    animation,
                }),
            )
            .with_transform(transform),
        );
        self
    }

    /// Add a static prop node (glTF/GLB, no skeleton).
    pub fn with_prop(
        mut self,
        id: impl Into<String>,
        asset: impl Into<String>,
        transform: TransformDesc,
    ) -> Self {
        self.children.push(
            SceneNode::leaf(
                id,
                NodePayload::Prop(PropPayload {
                    asset: AssetRef::path(asset),
                }),
            )
            .with_transform(transform),
        );
        self
    }

    /// Add the scene environment (backdrop room geometry).
    pub fn with_environment(
        mut self,
        scene_id: impl Into<String>,
        asset: impl Into<String>,
    ) -> Self {
        self.children.push(SceneNode::leaf(
            "environment",
            NodePayload::Environment(EnvironmentPayload {
                scene_id: scene_id.into(),
                asset: AssetRef::path(asset),
            }),
        ));
        self
    }

    /// Append an already-constructed node (escape hatch for groups/subtrees the
    /// typed helpers don't cover).
    pub fn with_node(mut self, node: SceneNode) -> Self {
        self.children.push(node);
        self
    }

    /// Finalize into a [`SceneDescription`] rooted at a `"scene"` group.
    pub fn build(self) -> SceneDescription {
        let root = self
            .children
            .into_iter()
            .fold(SceneNode::group("scene"), |root, child| {
                root.with_child(child)
            });
        SceneDescription {
            version: SCENE_DESCRIPTION_VERSION,
            backdrop: self.backdrop,
            root,
        }
    }
}

impl Default for SceneBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live::video::bevy_renderer::scene::description::Vec3Desc;

    fn sample() -> SceneDescription {
        SceneBuilder::new()
            .backdrop(ColorDesc {
                r: 0.1,
                g: 0.2,
                b: 0.3,
                a: 1.0,
            })
            .with_camera(
                true,
                TransformDesc::from_translation(Vec3Desc::new(0.0, 1.5, 2.0)),
            )
            .with_environment("office", "models/scenes/office.glb")
            .with_avatar(
                "asha",
                "models/avatars/asha.vrm",
                "Asha",
                AnimationProfileKind::Portrait,
                TransformDesc::default(),
            )
            .build()
    }

    // what this catches: the RON serde round-trip drifting — a SceneDescription
    // that serializes but does not parse back identically would silently corrupt
    // on-disk scenes and break the file<->builder<->birther equivalence.
    #[test]
    fn ron_round_trips_a_builder_scene() {
        let scene = sample();
        let text = ron::ser::to_string(&scene).expect("serialize");
        let parsed: SceneDescription = ron::from_str(&text).expect("parse");
        assert_eq!(scene, parsed, "RON round-trip must be identity");
    }

    // what this catches: the builder producing a different shape than a
    // hand-constructed description for the same inputs — the builder must be pure
    // sugar over the same SceneDescription the loader/birther emit.
    #[test]
    fn builder_equals_hand_constructed() {
        let built = SceneBuilder::new()
            .with_avatar(
                "asha",
                "models/avatars/asha.vrm",
                "Asha",
                AnimationProfileKind::Portrait,
                TransformDesc::default(),
            )
            .build();

        let hand = SceneDescription {
            version: SCENE_DESCRIPTION_VERSION,
            backdrop: ColorDesc::BLACK,
            root: SceneNode::group("scene").with_child(SceneNode::leaf(
                "asha",
                NodePayload::Avatar(AvatarPayload {
                    asset: AssetRef::path("models/avatars/asha.vrm"),
                    display_name: "Asha".to_string(),
                    animation: AnimationProfileKind::Portrait,
                }),
            )),
        };

        assert_eq!(built, hand, "builder must equal the hand-constructed scene");
    }
}
