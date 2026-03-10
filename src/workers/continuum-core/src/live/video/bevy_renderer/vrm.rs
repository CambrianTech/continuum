//! VRM extension parsing — blend shapes, humanoid bones, lookAt config.
//!
//! Parses VRM 0.x and VRM 1.0 extensions from .glb files to discover
//! morph target indices, humanoid bone mappings, and eye gaze parameters.

use std::collections::HashMap;

use super::types::VrmLookAtConfig;

// =============================================================================
// VRM Blend Shape Types
// =============================================================================

/// A VRM blend shape group — maps a named expression to morph target indices.
pub(super) struct VrmBlendShape {
    #[allow(dead_code)]
    pub name: String,
    pub preset_name: String,
    pub binds: Vec<VrmBlendShapeBind>,
}

/// A single morph target binding within a VRM blend shape group.
pub(super) struct VrmBlendShapeBind {
    #[allow(dead_code)]
    pub mesh: usize,
    pub index: usize,
    #[allow(dead_code)]
    pub weight: f32,
}

// =============================================================================
// GLB JSON Reader
// =============================================================================

/// Read the JSON chunk from a .glb file.
pub(super) fn read_glb_json(glb_path: &str) -> Option<serde_json::Value> {
    use std::io::Read;

    let mut file = std::fs::File::open(glb_path).ok()?;

    // GLB header: magic(4) + version(4) + length(4)
    let mut header = [0u8; 12];
    file.read_exact(&mut header).ok()?;
    let magic = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
    if magic != 0x46546C67 {
        return None;
    }

    // JSON chunk header: length(4) + type(4)
    let mut chunk_header = [0u8; 8];
    file.read_exact(&mut chunk_header).ok()?;
    let chunk_length = u32::from_le_bytes([
        chunk_header[0],
        chunk_header[1],
        chunk_header[2],
        chunk_header[3],
    ]) as usize;
    let chunk_type = u32::from_le_bytes([
        chunk_header[4],
        chunk_header[5],
        chunk_header[6],
        chunk_header[7],
    ]);
    if chunk_type != 0x4E4F534A {
        return None;
    }

    let mut json_data = vec![0u8; chunk_length];
    file.read_exact(&mut json_data).ok()?;
    let json_str = std::str::from_utf8(&json_data).ok()?;
    serde_json::from_str(json_str).ok()
}

// =============================================================================
// VRM Blend Shape Parsing
// =============================================================================

/// Parse VRM blend shape groups from a .glb file's JSON chunk.
/// Supports both VRM 0.x and VRM 1.0.
pub(super) fn parse_vrm_blend_shapes(glb_path: &str) -> Option<Vec<VrmBlendShape>> {
    let root = read_glb_json(glb_path)?;

    if let Some(shapes) = parse_vrm0x_blend_shapes(&root, glb_path) {
        return Some(shapes);
    }

    if let Some(shapes) = parse_vrmc_expressions(&root, glb_path) {
        return Some(shapes);
    }

    None
}

/// Parse VRM 0.x blend shape groups (extensions.VRM.blendShapeMaster.blendShapeGroups).
fn parse_vrm0x_blend_shapes(
    root: &serde_json::Value,
    _glb_path: &str,
) -> Option<Vec<VrmBlendShape>> {
    let groups = root
        .get("extensions")?
        .get("VRM")?
        .get("blendShapeMaster")?
        .get("blendShapeGroups")?
        .as_array()?;

    let mut shapes = Vec::new();
    for group in groups {
        let name = group
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let preset_name = group
            .get("presetName")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let binds_arr = group.get("binds").and_then(|v| v.as_array());
        let mut binds = Vec::new();
        if let Some(binds_arr) = binds_arr {
            for bind in binds_arr {
                let mesh = bind.get("mesh").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                let index = bind.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                let weight = bind.get("weight").and_then(|v| v.as_f64()).unwrap_or(100.0) as f32;
                binds.push(VrmBlendShapeBind {
                    mesh,
                    index,
                    weight,
                });
            }
        }
        shapes.push(VrmBlendShape {
            name,
            preset_name,
            binds,
        });
    }

    Some(shapes)
}

/// Parse VRM 1.0 expressions (extensions.VRMC_vrm.expressions.preset).
fn parse_vrmc_expressions(root: &serde_json::Value, _glb_path: &str) -> Option<Vec<VrmBlendShape>> {
    let preset = root
        .get("extensions")?
        .get("VRMC_vrm")?
        .get("expressions")?
        .get("preset")?
        .as_object()?;

    let mut shapes = Vec::new();
    for (preset_name, expr) in preset {
        let morph_binds = expr.get("morphTargetBinds").and_then(|v| v.as_array());
        let mut binds = Vec::new();
        if let Some(morph_binds) = morph_binds {
            for bind in morph_binds {
                let mesh = bind.get("node").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                let index = bind.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                let weight = bind.get("weight").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
                binds.push(VrmBlendShapeBind {
                    mesh,
                    index,
                    weight,
                });
            }
        }
        shapes.push(VrmBlendShape {
            name: preset_name.clone(),
            preset_name: preset_name.clone(),
            binds,
        });
    }

    Some(shapes)
}

// =============================================================================
// VRM Humanoid Bone Mapping
// =============================================================================

/// Parse VRM humanoid bone mapping from the .glb JSON extensions.
/// Returns a map of VRM bone name (e.g. "leftEye") -> glTF node name.
pub(super) fn parse_vrm_humanoid_bones(glb_path: &str) -> HashMap<String, String> {
    let root = match read_glb_json(glb_path) {
        Some(r) => r,
        None => return HashMap::new(),
    };

    let nodes = root.get("nodes").and_then(|v| v.as_array());

    let resolve_node_name = |node_index: u64| -> Option<String> {
        nodes
            .and_then(|n| n.get(node_index as usize))
            .and_then(|node| node.get("name"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    };

    let mut bone_map = HashMap::new();

    // Try VRM 0.x: extensions.VRM.humanoid.humanBones (array)
    if let Some(human_bones) = root
        .get("extensions")
        .and_then(|e| e.get("VRM"))
        .and_then(|v| v.get("humanoid"))
        .and_then(|h| h.get("humanBones"))
        .and_then(|b| b.as_array())
    {
        for bone_entry in human_bones {
            let bone_name = bone_entry.get("bone").and_then(|v| v.as_str());
            let node_idx = bone_entry.get("node").and_then(|v| v.as_u64());
            if let (Some(name), Some(idx)) = (bone_name, node_idx) {
                if let Some(node_name) = resolve_node_name(idx) {
                    bone_map.insert(name.to_string(), node_name);
                }
            }
        }
        if !bone_map.is_empty() {
            return bone_map;
        }
    }

    // Try VRM 1.0: extensions.VRMC_vrm.humanoid.humanBones (object)
    if let Some(human_bones) = root
        .get("extensions")
        .and_then(|e| e.get("VRMC_vrm"))
        .and_then(|v| v.get("humanoid"))
        .and_then(|h| h.get("humanBones"))
        .and_then(|b| b.as_object())
    {
        for (bone_name, bone_data) in human_bones {
            let node_idx = bone_data.get("node").and_then(|v| v.as_u64());
            if let Some(idx) = node_idx {
                if let Some(node_name) = resolve_node_name(idx) {
                    bone_map.insert(bone_name.clone(), node_name);
                }
            }
        }
    }

    bone_map
}

// =============================================================================
// VRM LookAt Config
// =============================================================================

/// Parse VRM lookAt configuration from the .glb JSON extensions.
pub(super) fn parse_vrm_look_at_config(glb_path: &str) -> Option<VrmLookAtConfig> {
    let root = read_glb_json(glb_path)?;

    // VRM 0.x
    if let Some(first_person) = root
        .get("extensions")
        .and_then(|e| e.get("VRM"))
        .and_then(|v| v.get("firstPerson"))
    {
        let look_at_type = first_person
            .get("lookAtTypeName")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if look_at_type != "Bone" {
            return None;
        }

        let get_output = |key: &str| -> f32 {
            first_person
                .get(key)
                .and_then(|v| v.get("yRange"))
                .and_then(|v| v.as_f64())
                .unwrap_or(8.0) as f32
        };

        let config = VrmLookAtConfig {
            horizontal_inner_deg: get_output("lookAtHorizontalInner"),
            horizontal_outer_deg: get_output("lookAtHorizontalOuter"),
            vertical_up_deg: get_output("lookAtVerticalUp"),
            vertical_down_deg: get_output("lookAtVerticalDown"),
        };
        return Some(config);
    }

    // VRM 1.0
    if let Some(look_at) = root
        .get("extensions")
        .and_then(|e| e.get("VRMC_vrm"))
        .and_then(|v| v.get("lookAt"))
    {
        let look_at_type = look_at.get("type").and_then(|v| v.as_str()).unwrap_or("");

        if look_at_type != "bone" {
            return None;
        }

        let get_output = |key: &str| -> f32 {
            look_at
                .get(key)
                .and_then(|v| v.get("outputScale"))
                .and_then(|v| v.as_f64())
                .unwrap_or(8.0) as f32
        };

        let config = VrmLookAtConfig {
            horizontal_inner_deg: get_output("rangeMapHorizontalInner"),
            horizontal_outer_deg: get_output("rangeMapHorizontalOuter"),
            vertical_up_deg: get_output("rangeMapVerticalUp"),
            vertical_down_deg: get_output("rangeMapVerticalDown"),
        };
        return Some(config);
    }

    None
}
