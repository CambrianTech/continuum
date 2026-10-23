//! `animation::pose` — the portable pose value an [`super::animator::Animator`]
//! produces and the one applier ([`apply_external_pose`]) consumes.
//!
//! [`SkeletonPose`] carries NO Bevy `Entity` and NO Bevy component types — only
//! POD math ([`QuatDesc`]/[`TransformDesc`] from `scene::description`) keyed by
//! canonical bone/morph NAME. That entity-freedom is the whole point: the *same*
//! value can later arrive over a network (Slice 3 VLA) — a pose computed on a
//! remote 5090 deserializes here unchanged and resolves name→entity locally.
//!
//! [`ExternalPose`]/[`PoseSource`] are the Bevy-side wrappers (Components) that
//! carry a `SkeletonPose` on an entity; the built-in animation systems gate off
//! (`Without<ExternalPose>`) when one is present so there is never a double-write.

use std::collections::HashMap;

use bevy::mesh::morph::MorphWeights;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::super::scene::description::{QuatDesc, TransformDesc};
use super::components::{set_morph, MorphMeshLink, MorphTargets, Skeleton};

// =============================================================================
// Canonical bone / morph vocabulary (VRM humanoid names — the wire contract)
// =============================================================================

/// The canonical bone names a [`SkeletonPose`] addresses. These are the VRM
/// humanoid bone identifiers; [`Skeleton::bone`] maps each to its `BoneRef`.
/// A pose naming a bone this skeleton lacks is silently skipped (an absent
/// optional bone is a legitimate per-avatar fact, not an error — the retarget
/// layer in Slice 3 is where unmappable *actions* fail loud).
pub mod bone {
    pub const HEAD: &str = "head";
    pub const NECK: &str = "neck";
    pub const SPINE: &str = "spine";
    pub const LEFT_SHOULDER: &str = "leftShoulder";
    pub const RIGHT_SHOULDER: &str = "rightShoulder";
    pub const LEFT_UPPER_ARM: &str = "leftUpperArm";
    pub const RIGHT_UPPER_ARM: &str = "rightUpperArm";
    pub const LEFT_LOWER_ARM: &str = "leftLowerArm";
    pub const RIGHT_LOWER_ARM: &str = "rightLowerArm";
    pub const LEFT_EYE: &str = "leftEye";
    pub const RIGHT_EYE: &str = "rightEye";
    pub const LEFT_HAND: &str = "leftHand";
    pub const RIGHT_HAND: &str = "rightHand";

    /// Every canonical bone name — used by tests to guard the name↔field map.
    pub const ALL: &[&str] = &[
        HEAD,
        NECK,
        SPINE,
        LEFT_SHOULDER,
        RIGHT_SHOULDER,
        LEFT_UPPER_ARM,
        RIGHT_UPPER_ARM,
        LEFT_LOWER_ARM,
        RIGHT_LOWER_ARM,
        LEFT_EYE,
        RIGHT_EYE,
        LEFT_HAND,
        RIGHT_HAND,
    ];
}

/// The canonical morph (blend-shape) channel names a [`SkeletonPose`] addresses.
/// [`MorphTargets::resolve`] maps each to a discovered morph index (or `None`).
pub mod morph {
    pub const MOUTH_OPEN: &str = "mouthOpen";
    pub const BLINK: &str = "blink";
    pub const BLINK_LEFT: &str = "blinkLeft";
    pub const BLINK_RIGHT: &str = "blinkRight";
    pub const HAPPY: &str = "happy";
    pub const SAD: &str = "sad";
    pub const ANGRY: &str = "angry";
    pub const SURPRISED: &str = "surprised";
    pub const RELAXED: &str = "relaxed";
    pub const LOOK_UP: &str = "lookUp";
    pub const LOOK_DOWN: &str = "lookDown";
    pub const LOOK_LEFT: &str = "lookLeft";
    pub const LOOK_RIGHT: &str = "lookRight";

    /// Every canonical morph name — used by tests to guard the name↔field map.
    pub const ALL: &[&str] = &[
        MOUTH_OPEN,
        BLINK,
        BLINK_LEFT,
        BLINK_RIGHT,
        HAPPY,
        SAD,
        ANGRY,
        SURPRISED,
        RELAXED,
        LOOK_UP,
        LOOK_DOWN,
        LOOK_LEFT,
        LOOK_RIGHT,
    ];
}

// =============================================================================
// The portable pose value (no Bevy Entity — network-serializable)
// =============================================================================

/// How a single bone's local pose is expressed. Three idioms because different
/// producers speak differently:
/// - [`BonePose::Delta`] — `rest_rotation × delta` (gestures: "rotate N° from rest")
/// - [`BonePose::Absolute`] — replace rotation outright (a VLA emits absolute joint targets)
/// - [`BonePose::Full`] — replace the whole local transform (breathing writes `scale.y`)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/animation/")]
pub enum BonePose {
    Delta(QuatDesc),
    Absolute(QuatDesc),
    Full(TransformDesc),
}

/// A full-body pose: named-bone poses + named morph weights. Entity-free and
/// serde/TS — the network-portable animation frame. An absent bone/morph means
/// "leave it as the built-ins (or the previous pose) left it".
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/animation/")]
pub struct SkeletonPose {
    /// canonical bone name → pose.
    pub bones: HashMap<String, BonePose>,
    /// canonical morph channel name → weight in [0,1].
    pub morphs: HashMap<String, f32>,
}

impl SkeletonPose {
    pub fn new() -> Self {
        Self::default()
    }

    /// Builder: set one bone's pose.
    pub fn with_bone(mut self, name: impl Into<String>, pose: BonePose) -> Self {
        self.bones.insert(name.into(), pose);
        self
    }

    /// Builder: set one morph channel's weight.
    pub fn with_morph(mut self, name: impl Into<String>, weight: f32) -> Self {
        self.morphs.insert(name.into(), weight);
        self
    }
}

// =============================================================================
// Bevy-side wrappers (Components) + the applier system
// =============================================================================

/// Which producer currently owns an entity's animation. Per-entity because a
/// grid of 16 avatars may each differ (some procedural, some VLA-driven).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PoseSource {
    /// The built-in animation systems compute this entity's motion.
    #[default]
    Procedural,
    /// An external animator (VLA) owns this entity via [`ExternalPose`].
    External,
}

/// The latest externally-produced pose for an entity. Presence of this component
/// gates the built-in animation systems OFF (`Without<ExternalPose>`) so the
/// applier is the sole writer for this entity — no last-writer-wins race.
///
/// `generation` monotonically increases per new pose so a consumer can detect
/// staleness (a VLA holding its last pose while "thinking" keeps the same gen).
#[derive(Component, Debug, Clone)]
pub struct ExternalPose {
    pub pose: SkeletonPose,
    pub generation: u64,
}

/// Apply externally-produced poses. Runs in the `Pose` set alongside the
/// built-in writers; it only touches entities marked [`PoseSource::External`]
/// (which the built-ins skip via `Without<ExternalPose>`), so the two never
/// write the same `Transform`/`MorphWeights` in one tick.
pub(in crate::live::video::bevy_renderer) fn apply_external_pose(
    query: Query<(
        &ExternalPose,
        &PoseSource,
        &Skeleton,
        Option<&MorphTargets>,
        Option<&MorphMeshLink>,
    )>,
    mut transforms: Query<&mut Transform>,
    mut morph_weights: Query<&mut MorphWeights>,
) {
    for (external, source, skeleton, morph_targets, mesh_link) in &query {
        if *source != PoseSource::External {
            continue;
        }

        // Bones: name → BoneRef → its Transform.
        for (name, bone_pose) in &external.pose.bones {
            let Some(bone) = skeleton.bone(name) else {
                continue; // this avatar lacks that bone — legitimate, skip
            };
            let Ok(mut tf) = transforms.get_mut(bone.entity) else {
                continue;
            };
            match bone_pose {
                BonePose::Delta(q) => tf.rotation = bone.rest_rotation * Quat::from(*q),
                BonePose::Absolute(q) => tf.rotation = Quat::from(*q),
                BonePose::Full(t) => *tf = Transform::from(*t),
            }
        }

        // Morphs: name → discovered index → weight on the linked mesh.
        if let (Some(targets), Some(link)) = (morph_targets, mesh_link) {
            if let Ok(mut weights) = morph_weights.get_mut(link.0) {
                let w = weights.weights_mut();
                for (name, weight) in &external.pose.morphs {
                    set_morph(w, targets.resolve(name), *weight);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::breathing::animate_breathing;
    use super::super::components::{BoneRef, BreathingAnimation};
    use super::*;
    use crate::live::video::bevy_renderer::scene::description::Vec3Desc;

    fn empty_skeleton() -> Skeleton {
        Skeleton {
            head: None,
            neck: None,
            spine: None,
            left_shoulder: None,
            right_shoulder: None,
            left_upper_arm: None,
            right_upper_arm: None,
            left_lower_arm: None,
            right_lower_arm: None,
            left_eye: None,
            right_eye: None,
            left_hand: None,
            right_hand: None,
            look_at_config: None,
        }
    }

    fn bone_ref(entity: Entity) -> BoneRef {
        BoneRef {
            entity,
            rest_translation: Vec3::ZERO,
            rest_rotation: Quat::IDENTITY,
        }
    }

    // what this catches: the RON/serde round-trip of the network-portable pose
    // drifting — a SkeletonPose that serializes but doesn't parse back identically
    // would corrupt a VLA pose in flight (Slice 3).
    #[test]
    fn pose_round_trips_through_serde() {
        let pose = SkeletonPose::new()
            .with_bone(bone::HEAD, BonePose::Absolute(QuatDesc::IDENTITY))
            .with_bone(
                bone::SPINE,
                BonePose::Full(TransformDesc::from_translation(Vec3Desc::new(
                    0.0, 1.0, 0.0,
                ))),
            )
            .with_morph(morph::HAPPY, 0.75);
        let text = serde_json::to_string(&pose).expect("serialize");
        let parsed: SkeletonPose = serde_json::from_str(&text).expect("parse");
        assert_eq!(pose, parsed, "pose serde round-trip must be identity");
    }

    // what this catches: the canonical bone/morph vocabulary drifting away from
    // the Skeleton/MorphTargets field map — every canonical name MUST resolve on
    // a fully-populated skeleton, or a pose addressing it would silently no-op.
    #[test]
    fn every_canonical_name_resolves() {
        let world = &mut World::new();
        let e = world.spawn_empty().id();
        let full = Skeleton {
            head: Some(bone_ref(e)),
            neck: Some(bone_ref(e)),
            spine: Some(bone_ref(e)),
            left_shoulder: Some(bone_ref(e)),
            right_shoulder: Some(bone_ref(e)),
            left_upper_arm: Some(bone_ref(e)),
            right_upper_arm: Some(bone_ref(e)),
            left_lower_arm: Some(bone_ref(e)),
            right_lower_arm: Some(bone_ref(e)),
            left_eye: Some(bone_ref(e)),
            right_eye: Some(bone_ref(e)),
            left_hand: Some(bone_ref(e)),
            right_hand: Some(bone_ref(e)),
            look_at_config: None,
        };
        for name in bone::ALL {
            assert!(full.bone(name).is_some(), "bone name `{name}` must resolve");
        }
        let targets = MorphTargets {
            mouth_open: Some(0),
            blink: Some(1),
            blink_left: Some(2),
            blink_right: Some(3),
            happy: Some(4),
            sad: Some(5),
            angry: Some(6),
            surprised: Some(7),
            relaxed: Some(8),
            look_up: Some(9),
            look_down: Some(10),
            look_left: Some(11),
            look_right: Some(12),
        };
        for name in morph::ALL {
            assert!(
                targets.resolve(name).is_some(),
                "morph name `{name}` must resolve"
            );
        }
    }

    // what this catches: the double-write race the whole seam exists to prevent —
    // an ExternalPose+External entity MUST be driven only by apply_external_pose
    // (breathing must NOT touch its spine), while a plain (procedural) entity MUST
    // still be driven by breathing. Proves the `Without<ExternalPose>` gate.
    #[test]
    fn external_pose_gates_out_the_builtins() {
        let mut app = App::new();
        app.init_resource::<Time>();

        // Two spine bone entities, one per avatar.
        let proc_spine = app.world_mut().spawn(Transform::default()).id();
        let ext_spine = app.world_mut().spawn(Transform::default()).id();

        // Procedural avatar: breathing should drive its spine.
        let mut proc_skel = empty_skeleton();
        proc_skel.spine = Some(bone_ref(proc_spine));
        app.world_mut().spawn((
            proc_skel,
            BreathingAnimation { phase_offset: 0.0 },
            PoseSource::Procedural,
        ));

        // External avatar: an absolute head/full-spine pose should drive it, and
        // breathing must NOT (it carries ExternalPose).
        let mut ext_skel = empty_skeleton();
        ext_skel.spine = Some(bone_ref(ext_spine));
        let commanded = TransformDesc {
            translation: Vec3Desc::new(1.0, 2.0, 3.0),
            rotation: QuatDesc::IDENTITY,
            scale: Vec3Desc::ONE,
        };
        app.world_mut().spawn((
            ext_skel,
            BreathingAnimation { phase_offset: 0.0 },
            PoseSource::External,
            ExternalPose {
                pose: SkeletonPose::new().with_bone(bone::SPINE, BonePose::Full(commanded)),
                generation: 1,
            },
        ));

        // Advance time so breathing produces a non-identity scale.
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_millis(500));
        app.add_systems(Update, (animate_breathing, apply_external_pose));
        app.update();

        let proc_tf = app.world().get::<Transform>(proc_spine).unwrap();
        let ext_tf = app.world().get::<Transform>(ext_spine).unwrap();

        // Breathing ran on the procedural spine (scale.y perturbed off 1.0).
        assert_ne!(
            proc_tf.scale.y, 1.0,
            "breathing must drive the procedural avatar's spine"
        );
        // The external spine is exactly the commanded pose — breathing did NOT touch it.
        assert_eq!(
            ext_tf.translation,
            Vec3::new(1.0, 2.0, 3.0),
            "external avatar's spine must be the applied pose, not breathing"
        );
        assert_eq!(
            ext_tf.scale.y, 1.0,
            "breathing must NOT touch an ExternalPose entity"
        );
    }
}
