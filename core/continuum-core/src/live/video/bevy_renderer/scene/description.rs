//! `SceneDescription` — the backend-neutral, representation-neutral scene
//! invariant.
//!
//! This is the ONE type produced three ways (a RON file on disk, the fluent
//! [`super::builder_api::SceneBuilder`], or the procedural [`super::birther`])
//! and consumed one way (instantiated into a live render graph by
//! [`super::instantiate`]). It is plain serde data with its own small POD
//! transform/color types — deliberately **no engine types in its fields** —
//! so a future render backend (Unreal, a Gaussian-splat renderer) instantiates
//! the *same* description into its own graph.
//!
//! ## Scene graph (hierarchy + inheritance)
//!
//! A scene is a **tree of [`SceneNode`]s** rooted at `SceneDescription::root`.
//! Each node carries a local [`TransformDesc`] and a [`NodePayload`] (what it
//! *is*), and `children` that inherit its composed transform — the classic
//! Unity/Godot/Unreal scene graph. A `Group` payload is a pure
//! transform/organizational node: the mechanism of hierarchy and the OOP
//! "base fields on the node, kind-specific fields in the payload" split. This
//! is what makes "a very complex scene" expressible as data.
//!
//! ## Representation neutrality (mesh · VRM · Gaussian splats · generated)
//!
//! Renderable payloads reference their asset through an [`AssetRef`], never a
//! baked engine handle, and the asset's *representation* is an open
//! [`AssetKind`] (triangle mesh / humanoid rig today; **Gaussian splat** clouds
//! and **generated** assets as first-class named kinds the backend picks a
//! loader for). The scene graph does not assume triangle meshes — a splat
//! avatar or a splat prop is the same tree with a different `AssetKind`.
//!
//! Engine types are recovered only at instantiation via the `From`/`into`
//! conversions in this file, the single seam where POD data becomes engine
//! state.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Bump when the on-disk schema changes incompatibly. `resolve_scene` fails
/// loud on an unknown/newer version rather than silently misreading a file.
pub const SCENE_DESCRIPTION_VERSION: u32 = 1;

// =============================================================================
// POD spatial / color primitives (no engine types in the fields)
// =============================================================================

/// A 3-component vector (translation or scale). POD; converts into `Vec3`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/scene/")]
pub struct Vec3Desc {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3Desc {
    pub const ZERO: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    pub const ONE: Self = Self {
        x: 1.0,
        y: 1.0,
        z: 1.0,
    };

    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }
}

impl From<Vec3Desc> for Vec3 {
    fn from(v: Vec3Desc) -> Self {
        Vec3::new(v.x, v.y, v.z)
    }
}

impl From<Vec3> for Vec3Desc {
    fn from(v: Vec3) -> Self {
        Self {
            x: v.x,
            y: v.y,
            z: v.z,
        }
    }
}

/// A quaternion rotation (xyzw). POD; converts into `Quat`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/scene/")]
pub struct QuatDesc {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl QuatDesc {
    pub const IDENTITY: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 0.0,
        w: 1.0,
    };

    /// Build from XYZ Euler angles (radians) — the idiom the light rig uses.
    pub fn from_euler_xyz(x: f32, y: f32, z: f32) -> Self {
        Quat::from_euler(EulerRot::XYZ, x, y, z).into()
    }
}

impl Default for QuatDesc {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl From<QuatDesc> for Quat {
    fn from(q: QuatDesc) -> Self {
        Quat::from_xyzw(q.x, q.y, q.z, q.w)
    }
}

impl From<Quat> for QuatDesc {
    fn from(q: Quat) -> Self {
        Self {
            x: q.x,
            y: q.y,
            z: q.z,
            w: q.w,
        }
    }
}

/// A linear RGBA color. POD; converts into Bevy `Color`. Canonicalized to
/// linear space so the round-trip through `Color` is lossless regardless of how
/// the source color was authored (hsl/srgb/etc).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/scene/")]
pub struct ColorDesc {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl ColorDesc {
    pub const WHITE: Self = Self {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    };
    pub const BLACK: Self = Self {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };
}

impl From<ColorDesc> for Color {
    fn from(c: ColorDesc) -> Self {
        Color::linear_rgba(c.r, c.g, c.b, c.a)
    }
}

impl From<Color> for ColorDesc {
    fn from(c: Color) -> Self {
        let lin = c.to_linear();
        Self {
            r: lin.red,
            g: lin.green,
            b: lin.blue,
            a: lin.alpha,
        }
    }
}

/// A full transform (translation + rotation + scale). POD; converts into
/// `Transform`. `Default` is the identity transform (scale ONE, not ZERO).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/scene/")]
pub struct TransformDesc {
    pub translation: Vec3Desc,
    pub rotation: QuatDesc,
    pub scale: Vec3Desc,
}

impl Default for TransformDesc {
    fn default() -> Self {
        Self {
            translation: Vec3Desc::ZERO,
            rotation: QuatDesc::IDENTITY,
            scale: Vec3Desc::ONE,
        }
    }
}

impl TransformDesc {
    /// A translation-only transform (identity rotation, unit scale).
    pub fn from_translation(translation: Vec3Desc) -> Self {
        Self {
            translation,
            ..Default::default()
        }
    }

    /// A rotation-only transform (identity translation, unit scale) — the shape
    /// directional lights use.
    pub fn from_rotation(rotation: QuatDesc) -> Self {
        Self {
            rotation,
            ..Default::default()
        }
    }
}

impl From<TransformDesc> for Transform {
    fn from(t: TransformDesc) -> Self {
        Transform {
            translation: t.translation.into(),
            rotation: t.rotation.into(),
            scale: t.scale.into(),
        }
    }
}

impl From<Transform> for TransformDesc {
    fn from(t: Transform) -> Self {
        Self {
            translation: t.translation.into(),
            rotation: t.rotation.into(),
            scale: t.scale.into(),
        }
    }
}

// =============================================================================
// Asset references — representation-neutral (mesh · VRM · splat · generated)
// =============================================================================

/// The *representation* of a renderable asset. The scene graph is neutral over
/// this: a backend picks a loader by kind. `None` on an [`AssetRef`] means
/// "infer from the source extension". Gaussian splats and generated assets are
/// named here so the model anticipates them without a schema change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/scene/")]
pub enum AssetKind {
    /// Triangle mesh glTF/GLB.
    Mesh,
    /// Humanoid-rigged glTF/VRM (has a skeleton the animator can drive).
    Humanoid,
    /// A Gaussian-splat radiance cloud (`.ply`/`.splat`). Design-for: a splat
    /// render backend loads this; the scene graph places it like any node.
    GaussianSplat,
    /// A procedurally/AI-generated asset (Nano-Banana et al.) resolved from a
    /// content-addressed blob store rather than a repo path.
    Generated,
}

/// A reference to a renderable asset — a path today, a content-addressed id
/// later. Never a baked engine handle, so the description stays portable across
/// backends and machines.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/scene/")]
pub struct AssetRef {
    /// Where the asset lives: a repo/asset path, or a content-addressed id for
    /// generated/downloaded assets.
    pub source: String,
    /// Explicit representation, or `None` to infer from the `source` extension
    /// (`.glb`/`.gltf` = Mesh, `.vrm` = Humanoid, `.ply`/`.splat` =
    /// GaussianSplat).
    #[serde(default)]
    pub kind: Option<AssetKind>,
}

impl AssetRef {
    /// A path-backed asset with inferred kind.
    pub fn path(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            kind: None,
        }
    }

    /// A path-backed asset with an explicit representation.
    pub fn of(source: impl Into<String>, kind: AssetKind) -> Self {
        Self {
            source: source.into(),
            kind: Some(kind),
        }
    }
}

// =============================================================================
// Lights
// =============================================================================

/// The kind of light + its kind-specific parameters. Ambient is a scene-global
/// resource in Bevy; the others are entities.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/scene/")]
pub enum LightKind {
    /// Uniform base illumination (Bevy `AmbientLight` resource). `intensity` is
    /// brightness; the node transform is ignored.
    Ambient,
    /// Sun-like parallel light (Bevy `DirectionalLight`). `intensity` is
    /// illuminance (lux); the node transform supplies the direction.
    Directional,
    /// Omnidirectional point light. `intensity` is lumens.
    Point { range: f32 },
    /// Cone light. `intensity` is lumens.
    Spot {
        range: f32,
        inner_angle: f32,
        outer_angle: f32,
    },
}

/// A light payload. Backend-neutral; the Bevy backend currently instantiates a
/// shared *global* rig ([`default_portrait_lights`]) to respect Bevy's
/// 10-directional-light budget, but a per-scene light node is the portable
/// intent a per-scene-capable backend (Unreal) or a budgeting system honors.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/scene/")]
pub struct LightDesc {
    pub kind: LightKind,
    pub color: ColorDesc,
    pub intensity: f32,
}

/// The canonical 3-point portrait rig + ambient, as DATA — the single source of
/// the lighting values (they used to be hardcoded inline in
/// `spawn_global_lights`). Returned as `(LightDesc, TransformDesc)` pairs; the
/// birther turns each into a `Light` node, and the Bevy backend instantiates
/// them once globally.
pub fn default_portrait_lights() -> Vec<(LightDesc, TransformDesc)> {
    use std::f32::consts::PI;
    vec![
        // Ambient — base illumination so no face is completely dark.
        (
            LightDesc {
                kind: LightKind::Ambient,
                color: ColorDesc::WHITE,
                intensity: 500.0,
            },
            TransformDesc::default(),
        ),
        // Key — upper-right-front, strong primary illumination.
        (
            LightDesc {
                kind: LightKind::Directional,
                color: ColorDesc::WHITE,
                intensity: 30000.0,
            },
            TransformDesc::from_rotation(QuatDesc::from_euler_xyz(-0.5, PI - 0.4, 0.0)),
        ),
        // Fill — front-left, softer to balance.
        (
            LightDesc {
                kind: LightKind::Directional,
                color: ColorDesc::WHITE,
                intensity: 15000.0,
            },
            TransformDesc::from_rotation(QuatDesc::from_euler_xyz(-0.2, PI + 0.4, 0.0)),
        ),
        // Rim — behind and above, cool edge separation.
        (
            LightDesc {
                kind: LightKind::Directional,
                color: Color::srgb(0.85, 0.9, 1.0).into(),
                intensity: 12000.0,
            },
            TransformDesc::from_rotation(QuatDesc::from_euler_xyz(-0.6, 0.2, 0.0)),
        ),
    ]
}

// =============================================================================
// Animation profile selector (1:1 with the AnimationConfig::* constructors)
// =============================================================================

/// Which built-in animation profile an avatar uses. Maps 1:1 onto the existing
/// `AnimationConfig::portrait/full_body/minimal` constructors at instantiation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
#[ts(export, export_to = "../../../protocol/typescript/scene/")]
pub enum AnimationProfileKind {
    #[default]
    Portrait,
    FullBody,
    Minimal,
}

// =============================================================================
// Physics (designed-for seam — data now, PhysicsBackend in physics.rs)
// =============================================================================

/// How a node participates in physics. Data only — instantiation is gated on a
/// `PhysicsBackend` being present (see `super::physics`); the base engine ships
/// with a no-op backend, so physics data is inert until a backend is attached.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/scene/")]
pub enum RigidBodyKind {
    /// Immovable (terrain, walls).
    Static,
    /// Simulated (falls, collides, pushed).
    Dynamic,
    /// Moved by code, pushes dynamics but isn't pushed back.
    Kinematic,
}

/// A collision shape for a node. `ConvexFromAsset` derives the shape from the
/// node's renderable asset at instantiation.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/scene/")]
pub enum ColliderDesc {
    Box { half_extents: Vec3Desc },
    Sphere { radius: f32 },
    Capsule { half_height: f32, radius: f32 },
    ConvexFromAsset,
}

/// Optional physics on a node — the designed-for seam that lets a complex scene
/// carry rigid bodies + colliders without the base engine simulating them yet.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/scene/")]
pub struct PhysicsDesc {
    pub body: RigidBodyKind,
    pub collider: ColliderDesc,
    #[serde(default)]
    pub mass: Option<f32>,
}

// =============================================================================
// Node payloads (data mirrors of the runtime SceneObject impls + non-objects)
// =============================================================================

/// An animated avatar. `AvatarPayload` uses an [`AssetRef`] so a humanoid glTF,
/// a VRM, or (design-for) a rigged Gaussian-splat avatar are the same payload
/// with a different [`AssetKind`] — the animator drives whichever the backend
/// loaded.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/scene/")]
pub struct AvatarPayload {
    pub asset: AssetRef,
    pub display_name: String,
    #[serde(default)]
    pub animation: AnimationProfileKind,
}

/// A static (non-animated) renderable — a mesh prop today, a splat cloud or a
/// generated asset tomorrow (the [`AssetKind`] decides).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/scene/")]
pub struct PropPayload {
    pub asset: AssetRef,
}

/// The room/backdrop environment — a glTF loaded as an ECS `RoomConfig` child
/// of the scene root (mirrors the runtime `RoomConfig`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/scene/")]
pub struct EnvironmentPayload {
    pub scene_id: String,
    pub asset: AssetRef,
}

/// A camera. Its view transform is the node's transform; this payload carries
/// only camera-specific behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
#[ts(export, export_to = "../../../protocol/typescript/scene/")]
pub struct CameraPayload {
    /// Lock the camera's height to the discovered head world-Y so breathing/sway
    /// don't bob the shot (the existing portrait behavior).
    #[serde(default)]
    pub head_lock: bool,
}

/// What a [`SceneNode`] *is*. Adding a new renderable kind = a new variant here
/// + a `SceneObject` impl + a loader; the tree, transforms, physics, and
/// hierarchy are unchanged. `Group` is the transform-only inheritance node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/scene/")]
pub enum NodePayload {
    /// A pure transform/organizational node — the mechanism of hierarchy.
    Group,
    Avatar(AvatarPayload),
    Prop(PropPayload),
    Light(LightDesc),
    Camera(CameraPayload),
    Environment(EnvironmentPayload),
}

// =============================================================================
// The scene graph
// =============================================================================

/// One node in the scene graph: a stable id, a local transform, what it is, an
/// optional physics body, and children that inherit its composed transform.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/scene/")]
pub struct SceneNode {
    /// Stable id. For an avatar this MUST equal the persona identity AND the key
    /// the load observer captures, or `model_loaded` never flips true.
    pub id: String,
    #[serde(default)]
    pub transform: TransformDesc,
    pub payload: NodePayload,
    #[serde(default)]
    pub physics: Option<PhysicsDesc>,
    #[serde(default)]
    pub children: Vec<SceneNode>,
}

impl SceneNode {
    /// A leaf node (no children, no physics) with an identity transform.
    pub fn leaf(id: impl Into<String>, payload: NodePayload) -> Self {
        Self {
            id: id.into(),
            transform: TransformDesc::default(),
            payload,
            physics: None,
            children: Vec::new(),
        }
    }

    /// An empty `Group` node — a transform-only parent for hierarchy.
    pub fn group(id: impl Into<String>) -> Self {
        Self::leaf(id, NodePayload::Group)
    }

    /// Builder: set the local transform.
    pub fn with_transform(mut self, transform: TransformDesc) -> Self {
        self.transform = transform;
        self
    }

    /// Builder: attach a child.
    pub fn with_child(mut self, child: SceneNode) -> Self {
        self.children.push(child);
        self
    }

    /// Builder: attach physics.
    pub fn with_physics(mut self, physics: PhysicsDesc) -> Self {
        self.physics = Some(physics);
        self
    }

    /// Depth-first pre-order iterator over this node and all descendants.
    pub fn iter(&self) -> impl Iterator<Item = &SceneNode> {
        let mut stack = vec![self];
        std::iter::from_fn(move || {
            let node = stack.pop()?;
            // push children reversed so pre-order visits them left-to-right
            stack.extend(node.children.iter().rev());
            Some(node)
        })
    }
}

/// A complete, backend-neutral description of a scene: a scene-global backdrop
/// clear color plus the scene-graph root. Cameras, lights, avatars, props, and
/// environment all live as nodes in the tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/scene/")]
pub struct SceneDescription {
    pub version: u32,
    pub backdrop: ColorDesc,
    pub root: SceneNode,
}

impl SceneDescription {
    /// An empty scene (bare `Group` root) at the current schema version — the
    /// base the builder and birther extend.
    pub fn empty() -> Self {
        Self {
            version: SCENE_DESCRIPTION_VERSION,
            backdrop: ColorDesc::BLACK,
            root: SceneNode::group("root"),
        }
    }

    /// Depth-first pre-order iterator over every node in the scene.
    pub fn nodes(&self) -> impl Iterator<Item = &SceneNode> {
        self.root.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a field/enum whose serde impl isn't symmetric — a
    // scene written to RON must parse back byte-identical, or saved scenes
    // silently corrupt on reload. Exercises the recursive node tree, every
    // payload variant, an AssetKind, and a physics body.
    #[test]
    fn ron_round_trips_a_hierarchical_scene() {
        let (key_light, key_xf) = default_portrait_lights()[1];
        let scene = SceneDescription {
            version: SCENE_DESCRIPTION_VERSION,
            backdrop: ColorDesc {
                r: 0.1,
                g: 0.2,
                b: 0.3,
                a: 1.0,
            },
            root: SceneNode::group("root")
                .with_child(
                    SceneNode::leaf(
                        "camera",
                        NodePayload::Camera(CameraPayload { head_lock: true }),
                    )
                    .with_transform(TransformDesc::from_translation(
                        Vec3Desc::new(0.0, 1.5, 2.0),
                    )),
                )
                .with_child(
                    SceneNode::leaf("key", NodePayload::Light(key_light)).with_transform(key_xf),
                )
                .with_child(
                    // a Group holding an avatar + a splat prop child — proves
                    // hierarchy, AssetKind neutrality, and physics together.
                    SceneNode::group("stage")
                        .with_child(SceneNode::leaf(
                            "asha",
                            NodePayload::Avatar(AvatarPayload {
                                asset: AssetRef::of("models/avatars/asha.vrm", AssetKind::Humanoid),
                                display_name: "Asha".to_string(),
                                animation: AnimationProfileKind::Portrait,
                            }),
                        ))
                        .with_child(
                            SceneNode::leaf(
                                "cloud",
                                NodePayload::Prop(PropPayload {
                                    asset: AssetRef::of(
                                        "props/cloud.ply",
                                        AssetKind::GaussianSplat,
                                    ),
                                }),
                            )
                            .with_physics(PhysicsDesc {
                                body: RigidBodyKind::Static,
                                collider: ColliderDesc::Sphere { radius: 0.5 },
                                mass: None,
                            }),
                        ),
                ),
        };

        let ron = ron::ser::to_string(&scene).expect("serialize");
        let parsed: SceneDescription = ron::from_str(&ron).expect("deserialize");
        assert_eq!(scene, parsed);
    }

    // what this catches: TransformDesc::default silently regressing to ZERO
    // scale (which renders every model invisible) instead of unit scale.
    #[test]
    fn default_transform_is_identity_not_zero_scale() {
        assert_eq!(TransformDesc::default().scale, Vec3Desc::ONE);
        let t: Transform = TransformDesc::default().into();
        assert_eq!(t.scale, Vec3::ONE);
    }

    // what this catches: the pre-order tree walk missing nodes or looping —
    // nodes() must visit every node in a nested tree exactly once.
    #[test]
    fn nodes_visits_every_node_once() {
        let scene = SceneDescription {
            version: SCENE_DESCRIPTION_VERSION,
            backdrop: ColorDesc::BLACK,
            root: SceneNode::group("root")
                .with_child(SceneNode::group("a").with_child(SceneNode::group("a1")))
                .with_child(SceneNode::group("b")),
        };
        let ids: Vec<&str> = scene.nodes().map(|n| n.id.as_str()).collect();
        assert_eq!(ids.len(), 4);
        for id in ["root", "a", "a1", "b"] {
            assert!(ids.contains(&id), "missing {id}");
        }
    }
}
