//! Morph target discovery — finds blend shape indices from glTF meshes AND VRM extensions.

use bevy::mesh::morph::MorphWeights;
use bevy::prelude::*;

use super::components::*;
use super::super::vrm;
use crate::clog_info;

/// Discover morph targets on avatar entities that have SlotId + ModelPath but no MorphTargets yet.
/// Reads from both mesh target names AND VRM extension blend shapes — they're complementary sources.
pub(in crate::live::video::bevy_renderer) fn discover_morph_targets(
    time: Res<Time>,
    query: Query<(Entity, &SlotId, &ModelPath), Without<MorphTargets>>,
    morph_query: Query<(Entity, &MorphWeights)>,
    children_query: Query<&Children>,
    meshes: Res<Assets<Mesh>>,
    mut commands: Commands,
) {
    let elapsed = time.elapsed_secs();
    for (avatar_entity, slot_id, model_path) in &query {
        let morph_entity = match find_morph_entity(avatar_entity, &children_query, &morph_query) {
            Some(e) => e,
            None => continue,
        };

        let mut targets = MorphTargets {
            mouth_open: None,
            blink: None,
            blink_left: None,
            blink_right: None,
            happy: None,
            sad: None,
            angry: None,
            surprised: None,
            relaxed: None,
            look_up: None,
            look_down: None,
            look_left: None,
            look_right: None,
        };

        // Source 1: glTF mesh target names
        let mesh_names: Vec<String> = morph_query
            .get(morph_entity)
            .ok()
            .and_then(|(_, weights)| weights.first_mesh())
            .and_then(|mesh_handle| meshes.get(mesh_handle))
            .and_then(|mesh| mesh.morph_target_names())
            .map(|names| names.to_vec())
            .unwrap_or_default();

        if !mesh_names.is_empty() {
            discover_from_mesh_names(&mesh_names, &mut targets);
        }

        // Source 2: VRM extension blend shape bindings
        discover_from_vrm_extension(&model_path.0, slot_id.0, &mut targets);

        let weight_count = morph_query
            .get(morph_entity)
            .ok()
            .map(|(_, w)| w.weights().len())
            .unwrap_or(0);

        let emotion_count = [
            targets.happy, targets.sad, targets.angry,
            targets.surprised, targets.relaxed,
        ].iter().filter(|i| i.is_some()).count();
        let gaze_count = [
            targets.look_up, targets.look_down, targets.look_left, targets.look_right,
        ].iter().filter(|i| i.is_some()).count();
        clog_info!(
            "🎨 Morph discovery slot {}: {} weights, {} mesh names, mouth={:?}, blink={:?}, blink_l={:?}, blink_r={:?}, emotions={}/5, gaze={}/4",
            slot_id.0, weight_count, mesh_names.len(), targets.mouth_open, targets.blink,
            targets.blink_left, targets.blink_right, emotion_count, gaze_count,
        );

        // Insert Components on the avatar entity
        commands.entity(avatar_entity).insert(targets);
        commands.entity(avatar_entity).insert(MorphMeshLink(morph_entity));
        commands.entity(avatar_entity).insert(BlinkAnimation::new(elapsed, slot_id.0));
        commands.entity(avatar_entity).insert(EyeGaze::new(slot_id.0));
        commands.entity(avatar_entity).insert(EmotionAnimation::default());
    }
}

/// Discover morph target indices from VRM extension blend shape bindings.
/// Sets any indices not already discovered by mesh names.
fn discover_from_vrm_extension(model_path: &str, slot: u8, targets: &mut MorphTargets) {
    let vrm_shapes = match vrm::parse_vrm_blend_shapes(model_path) {
        Some(s) => s,
        None => return,
    };

    for shape in &vrm_shapes {
        let preset = shape.preset_name.to_lowercase();
        let first_index = shape.binds.first().map(|b| b.index);

        macro_rules! map_preset {
            ($field:ident, $($name:literal)|+) => {
                if targets.$field.is_none() && matches!(preset.as_str(), $($name)|+) {
                    targets.$field = first_index;
                }
            };
        }

        map_preset!(mouth_open, "a" | "aa");
        map_preset!(happy, "joy" | "happy");
        map_preset!(sad, "sorrow" | "sad");
        map_preset!(angry, "angry");
        map_preset!(surprised, "fun" | "surprised");
        map_preset!(relaxed, "relaxed");
        map_preset!(blink_left, "blink_l" | "blinkleft");
        map_preset!(blink_right, "blink_r" | "blinkright");
        map_preset!(look_up, "lookup");
        map_preset!(look_down, "lookdown");
        map_preset!(look_left, "lookleft");
        map_preset!(look_right, "lookright");

        // "blink" preset with 2 binds → split into left/right
        if targets.blink.is_none() && preset == "blink" {
            targets.blink = first_index;
            if shape.binds.len() >= 2 {
                targets.blink_left = Some(shape.binds[0].index);
                targets.blink_right = Some(shape.binds[1].index);
            }
        }
    }
    clog_info!("🎨 VRM blend shapes slot {}: {} groups parsed", slot, vrm_shapes.len());
}

/// Discover morph target indices from standard glTF mesh target names.
fn discover_from_mesh_names(mesh_names: &[String], layout: &mut MorphTargets) {
    for (i, name) in mesh_names.iter().enumerate() {
        let lower = name.to_lowercase();

        macro_rules! set_first {
            ($field:ident, $cond:expr) => {
                if layout.$field.is_none() && $cond {
                    layout.$field = Some(i);
                }
            };
        }

        set_first!(mouth_open,
            lower == "aa" || lower == "a"
            || lower.ends_with("_mth_a") || lower.ends_with("mth_a")
            || lower.ends_with("_v_aa") || lower == "v_aa"
            || lower.ends_with("mouth_open") || lower.ends_with("jawopen")
            || lower == "fcl_mth_a"
        );
        set_first!(blink,
            lower == "blink" || lower == "fcl_eye_close" || lower == "vrc.blink"
            || (lower.contains("eye_close")
                && !lower.contains("_l") && !lower.contains("_r")
                && !lower.contains("left") && !lower.contains("right"))
        );
        set_first!(blink_left,
            lower == "blinkleft" || lower == "blink_l" || lower == "fcl_eye_close_l"
            || lower.contains("eye_close_l") || lower.contains("eye_close_left")
        );
        set_first!(blink_right,
            lower == "blinkright" || lower == "blink_r" || lower == "fcl_eye_close_r"
            || lower.contains("eye_close_r") || lower.contains("eye_close_right")
        );
        set_first!(happy,
            lower == "happy" || lower == "joy"
            || lower.ends_with("_joy") || lower.ends_with("_happy")
            || lower == "fcl_all_joy" || lower == "fcl_eye_joy"
        );
        set_first!(sad,
            lower == "sad" || lower == "sorrow"
            || lower.ends_with("_sad") || lower.ends_with("_sorrow")
            || lower == "fcl_all_sorrow" || lower == "fcl_eye_sorrow"
        );
        set_first!(angry,
            lower == "angry" || lower.ends_with("_angry")
            || lower == "fcl_all_angry" || lower == "fcl_mth_angry"
        );
        set_first!(surprised,
            lower == "surprised" || lower == "fun"
            || lower.ends_with("_surprised") || lower.ends_with("_fun")
            || lower == "fcl_all_fun" || lower == "fcl_brw_surprised"
        );
        set_first!(relaxed,
            lower == "relaxed" || lower.ends_with("_relaxed") || lower == "fcl_all_relaxed"
        );
        set_first!(look_up,
            lower == "lookup" || lower == "look_up"
            || lower.ends_with("lookup") || lower == "fcl_eye_lookup"
        );
        set_first!(look_down,
            lower == "lookdown" || lower == "look_down"
            || lower.ends_with("lookdown") || lower == "fcl_eye_lookdown"
        );
        set_first!(look_left,
            lower == "lookleft" || lower == "look_left"
            || lower.ends_with("lookleft") || lower == "fcl_eye_lookleft"
        );
        set_first!(look_right,
            lower == "lookright" || lower == "look_right"
            || lower.ends_with("lookright") || lower == "fcl_eye_lookright"
        );
    }
}

/// Find the first entity with MorphWeights in a scene hierarchy.
fn find_morph_entity(
    root: Entity,
    children: &Query<&Children>,
    morph_query: &Query<(Entity, &MorphWeights)>,
) -> Option<Entity> {
    if morph_query.get(root).is_ok() {
        return Some(root);
    }
    if let Ok(child_list) = children.get(root) {
        for child in child_list.iter() {
            if let Some(found) = find_morph_entity(child, children, morph_query) {
                return Some(found);
            }
        }
    }
    None
}
