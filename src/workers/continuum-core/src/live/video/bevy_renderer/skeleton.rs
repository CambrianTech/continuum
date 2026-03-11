//! Skeleton — bone discovery, T-pose fix, and scene tree helpers.
//!
//! Discovers upper-body bones from VRM/Mixamo/generic naming conventions,
//! applies T-pose arm correction, and provides scene tree traversal utilities.

use bevy::prelude::*;

use super::scene::{AvatarBones, BoneInfo};
use super::types::BoneRegistry;
use super::vrm;
use crate::clog_info;
use crate::clog_warn;

// =============================================================================
// Scene Tree Helpers
// =============================================================================

/// Count all descendant entities recursively (for debug logging).
pub(super) fn count_descendants(entity: Entity, children: &Query<&Children>) -> usize {
    let mut count = 0;
    if let Ok(child_list) = children.get(entity) {
        for child in child_list.iter() {
            count += 1 + count_descendants(child, children);
        }
    }
    count
}

/// Recursively propagate RenderLayers to all descendant entities of a scene root.
pub(super) fn propagate_render_layers(
    entity: Entity,
    layer: &bevy::camera::visibility::RenderLayers,
    children: &Query<&Children>,
    commands: &mut Commands,
) {
    if let Ok(child_list) = children.get(entity) {
        for child in child_list.iter() {
            commands.entity(child).insert(layer.clone());
            propagate_render_layers(child, layer, children, commands);
        }
    }
}

/// Dump all named entities in a scene hierarchy (for debugging bone names).
pub(super) fn dump_bone_names(_entity: Entity, _children: &Query<&Children>, _names: &Query<&Name>) {
    // No-op: bevy_debug calls removed. Function retained for call-site compatibility.
}

/// Reference head Y for VRM standard models (~1.50m).
pub(super) const REFERENCE_HEAD_Y: f32 = 1.50;

/// Baseline camera Z distance for the reference head height.
pub(super) const REFERENCE_CAMERA_Z: f32 = -0.55;

/// Compute camera Z distance from head world-Y position.
pub(super) fn camera_z_for_head(head_y: f32) -> f32 {
    let scale = (head_y / REFERENCE_HEAD_Y).clamp(0.5, 2.0);
    REFERENCE_CAMERA_Z * scale
}

/// Find a bone entity by matching against a list of known names.
pub(super) fn find_bone_by_name(
    root: Entity,
    children: &Query<&Children>,
    names: &Query<&Name>,
    target_names: &[&str],
) -> Option<Entity> {
    if let Ok(name) = names.get(root) {
        let name_str = name.as_str();
        for target in target_names {
            if name_str.contains(target) {
                return Some(root);
            }
        }
    }
    if let Ok(child_list) = children.get(root) {
        for child in child_list.iter() {
            if let Some(found) = find_bone_by_name(child, children, names, target_names) {
                return Some(found);
            }
        }
    }
    None
}

// =============================================================================
// T-Pose Fix
// =============================================================================

/// Fix T-pose by rotating arm bones to a natural resting position.
pub(super) fn fix_tpose_arms(
    entity: Entity,
    children: &Query<&Children>,
    names: &Query<&Name>,
    transforms: &mut Query<&mut Transform>,
) {
    let mut adjustments = Vec::new();
    collect_arm_adjustments(entity, children, names, &mut adjustments);

    if adjustments.is_empty() {
        clog_warn!("🎨 T-pose fix: no arm bones found — model may use unknown naming convention");
    } else {
        clog_info!(
            "🎨 T-pose fix: {} arm bone adjustments applied",
            adjustments.len()
        );
    }

    for (bone_entity, rotation) in adjustments {
        if let Ok(mut transform) = transforms.get_mut(bone_entity) {
            transform.rotation *= rotation;
        }
    }
}

/// Recursively search for arm bones and collect transform adjustments.
fn collect_arm_adjustments(
    entity: Entity,
    children: &Query<&Children>,
    names: &Query<&Name>,
    adjustments: &mut Vec<(Entity, Quat)>,
) {
    if let Ok(name) = names.get(entity) {
        let name_str = name.as_str();
        let name_lower = name_str.to_lowercase();

        let is_left_upper = name_str.contains("J_Bip_L_UpperArm")
            || name_str.contains("J_Sec_L_UpperArm")
            || (name_str.contains("mixamorig:LeftArm") && !name_str.contains("ForeArm"))
            || name_str == "LeftUpperArm"
            || name_str == "Left_UpperArm"
            || name_str == "Left arm"
            || name_str == "Upperarm_L"
            || name_str == "upper_arm.L"
            || (name_lower.ends_with("upperarm")
                && (name_lower.contains("left") || name_lower.contains("_l_")))
            || (name_lower.ends_with("upper_arm")
                && (name_lower.contains("left") || name_lower.contains("_l_")));
        let is_right_upper = name_str.contains("J_Bip_R_UpperArm")
            || name_str.contains("J_Sec_R_UpperArm")
            || (name_str.contains("mixamorig:RightArm") && !name_str.contains("ForeArm"))
            || name_str == "RightUpperArm"
            || name_str == "Right_UpperArm"
            || name_str == "Right arm"
            || name_str == "Upperarm_R"
            || name_str == "upper_arm.R"
            || (name_lower.ends_with("upperarm")
                && (name_lower.contains("right") || name_lower.contains("_r_")))
            || (name_lower.ends_with("upper_arm")
                && (name_lower.contains("right") || name_lower.contains("_r_")));

        let is_left_lower = name_str.contains("J_Bip_L_LowerArm")
            || name_str.contains("J_Sec_L_LowerArm")
            || name_str.contains("mixamorig:LeftForeArm")
            || name_str == "LeftLowerArm"
            || name_str == "Left_LowerArm"
            || name_str == "LeftForeArm"
            || name_str == "Left elbow"
            || name_str == "Lowerarm_L"
            || name_str == "lower_arm.L"
            || (name_lower.ends_with("lowerarm")
                && (name_lower.contains("left") || name_lower.contains("_l_")))
            || (name_lower.ends_with("forearm")
                && (name_lower.contains("left") || name_lower.contains("_l_")));
        let is_right_lower = name_str.contains("J_Bip_R_LowerArm")
            || name_str.contains("J_Sec_R_LowerArm")
            || name_str.contains("mixamorig:RightForeArm")
            || name_str == "RightLowerArm"
            || name_str == "Right_LowerArm"
            || name_str == "RightForeArm"
            || name_str == "Right elbow"
            || name_str == "Lowerarm_R"
            || name_str == "lower_arm.R"
            || (name_lower.ends_with("lowerarm")
                && (name_lower.contains("right") || name_lower.contains("_r_")))
            || (name_lower.ends_with("forearm")
                && (name_lower.contains("right") || name_lower.contains("_r_")));

        if is_left_upper {
            adjustments.push((entity, Quat::from_rotation_z(1.13)));
        } else if is_right_upper {
            adjustments.push((entity, Quat::from_rotation_z(-1.13)));
        } else if is_left_lower {
            adjustments.push((entity, Quat::from_rotation_z(0.26)));
        } else if is_right_lower {
            adjustments.push((entity, Quat::from_rotation_z(-0.26)));
        }
    }

    if let Ok(child_list) = children.get(entity) {
        for child in child_list.iter() {
            collect_arm_adjustments(child, children, names, adjustments);
        }
    }
}

// =============================================================================
// Upper Body Bone Discovery
// =============================================================================

/// Discover upper-body bone entities from scene hierarchy for animation systems.
pub(super) fn discover_upper_body_bones(
    root: Entity,
    slot: u8,
    model_path: &str,
    children: &Query<&Children>,
    names: &Query<&Name>,
    transforms: &Query<&mut Transform>,
    bone_registry: &mut ResMut<BoneRegistry>,
) {
    let head_names = ["J_Bip_C_Head", "mixamorig:Head", "Head"];
    let neck_names = ["J_Bip_C_Neck", "mixamorig:Neck", "Neck"];
    let spine_names = ["J_Bip_C_Spine", "mixamorig:Spine", "Spine"];
    let left_shoulder_names = [
        "J_Bip_L_Shoulder",
        "mixamorig:LeftShoulder",
        "LeftShoulder",
        "Left shoulder",
        "Shoulder_L",
        "Shoulder.L",
    ];
    let right_shoulder_names = [
        "J_Bip_R_Shoulder",
        "mixamorig:RightShoulder",
        "RightShoulder",
        "Right shoulder",
        "Shoulder_R",
        "Shoulder.R",
    ];
    let left_upper_arm_names = [
        "J_Bip_L_UpperArm",
        "mixamorig:LeftArm",
        "LeftUpperArm",
        "Left_UpperArm",
        "Left arm",
        "Upperarm_L",
        "upper_arm.L",
    ];
    let right_upper_arm_names = [
        "J_Bip_R_UpperArm",
        "mixamorig:RightArm",
        "RightUpperArm",
        "Right_UpperArm",
        "Right arm",
        "Upperarm_R",
        "upper_arm.R",
    ];
    let left_lower_arm_names = [
        "J_Bip_L_LowerArm",
        "mixamorig:LeftForeArm",
        "LeftLowerArm",
        "Left_LowerArm",
        "LeftForeArm",
        "Left elbow",
        "Lowerarm_L",
        "lower_arm.L",
    ];
    let right_lower_arm_names = [
        "J_Bip_R_LowerArm",
        "mixamorig:RightForeArm",
        "RightLowerArm",
        "Right_LowerArm",
        "RightForeArm",
        "Right elbow",
        "Lowerarm_R",
        "lower_arm.R",
    ];

    let discover = |target_names: &[&str], _label: &str| -> Option<BoneInfo> {
        find_bone_by_name(root, children, names, target_names).and_then(|entity| {
            if let Ok(t) = transforms.get(entity) {
                Some(BoneInfo {
                    entity,
                    rest_translation: t.translation,
                    rest_rotation: t.rotation,
                })
            } else {
                None
            }
        })
    };

    let head = discover(&head_names, "Head");
    let neck = discover(&neck_names, "Neck");
    let spine = discover(&spine_names, "Spine");
    let left_shoulder = discover(&left_shoulder_names, "L.Shoulder");
    let right_shoulder = discover(&right_shoulder_names, "R.Shoulder");
    let left_upper_arm = discover(&left_upper_arm_names, "L.UpperArm");
    let right_upper_arm = discover(&right_upper_arm_names, "R.UpperArm");
    let left_lower_arm = discover(&left_lower_arm_names, "L.LowerArm");
    let right_lower_arm = discover(&right_lower_arm_names, "R.LowerArm");

    let left_eye_names = [
        "J_Adj_L_FaceEye",
        "mixamorig:LeftEye",
        "LeftEye",
        "Eye_L",
        "eye.L",
    ];
    let right_eye_names = [
        "J_Adj_R_FaceEye",
        "mixamorig:RightEye",
        "RightEye",
        "Eye_R",
        "eye.R",
    ];
    let left_hand_names = [
        "J_Bip_L_Hand",
        "mixamorig:LeftHand",
        "LeftHand",
        "Hand_L",
        "hand.L",
    ];
    let right_hand_names = [
        "J_Bip_R_Hand",
        "mixamorig:RightHand",
        "RightHand",
        "Hand_R",
        "hand.R",
    ];

    let mut left_eye = discover(&left_eye_names, "L.Eye");
    let mut right_eye = discover(&right_eye_names, "R.Eye");
    let mut left_hand = discover(&left_hand_names, "L.Hand");
    let mut right_hand = discover(&right_hand_names, "R.Hand");

    // Parse VRM humanoid bone mapping for any bones that name-based discovery missed.
    let vrm_bones = vrm::parse_vrm_humanoid_bones(model_path);
    if !vrm_bones.is_empty() {
        let vrm_discover = |vrm_name: &str, _label: &str| -> Option<BoneInfo> {
            vrm_bones.get(vrm_name).and_then(|node_name| {
                find_bone_by_name(root, children, names, &[node_name.as_str()]).and_then(
                    |entity| {
                        if let Ok(t) = transforms.get(entity) {
                            Some(BoneInfo {
                                entity,
                                rest_translation: t.translation,
                                rest_rotation: t.rotation,
                            })
                        } else {
                            None
                        }
                    },
                )
            })
        };

        if left_eye.is_none() {
            left_eye = vrm_discover("leftEye", "L.Eye");
        }
        if right_eye.is_none() {
            right_eye = vrm_discover("rightEye", "R.Eye");
        }
        if left_hand.is_none() {
            left_hand = vrm_discover("leftHand", "L.Hand");
        }
        if right_hand.is_none() {
            right_hand = vrm_discover("rightHand", "R.Hand");
        }
    }

    let look_at_config = vrm::parse_vrm_look_at_config(model_path);

    let upper_body_count = [&head, &neck, &spine, &left_shoulder, &right_shoulder]
        .iter()
        .filter(|b| b.is_some())
        .count();
    let arm_count = [
        &left_upper_arm,
        &right_upper_arm,
        &left_lower_arm,
        &right_lower_arm,
    ]
    .iter()
    .filter(|b| b.is_some())
    .count();
    let eye_count = [&left_eye, &right_eye]
        .iter()
        .filter(|b| b.is_some())
        .count();
    let hand_count = [&left_hand, &right_hand]
        .iter()
        .filter(|b| b.is_some())
        .count();
    clog_info!("🎨 Bone discovery slot {}: {}/5 upper body (head={} neck={} spine={} lsh={} rsh={}), {}/4 arms (lua={} rua={} lla={} rla={}), eyes={}/2, hands={}/2, lookAt={}",
        slot, upper_body_count,
        head.is_some(), neck.is_some(), spine.is_some(),
        left_shoulder.is_some(), right_shoulder.is_some(),
        arm_count,
        left_upper_arm.is_some(), right_upper_arm.is_some(),
        left_lower_arm.is_some(), right_lower_arm.is_some(),
        eye_count, hand_count, look_at_config.is_some());

    bone_registry.slots.insert(
        slot,
        AvatarBones {
            head,
            neck,
            spine,
            left_shoulder,
            right_shoulder,
            left_upper_arm,
            right_upper_arm,
            left_lower_arm,
            right_lower_arm,
            left_eye,
            right_eye,
            left_hand,
            right_hand,
            look_at_config,
        },
    );
}
