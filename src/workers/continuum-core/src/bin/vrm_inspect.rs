//! VRM Model Inspector
//!
//! Diagnostic tool that loads VRM/GLB files and prints geometry/skeleton data.
//! Used to debug rendering issues (e.g., VRM 1.0 "monster" geometry).
//!
//! Usage: vrm-inspect <model1.vrm> [model2.vrm ...]
//!
//! Outputs per model:
//!   - VRM version (0.x vs 1.0)
//!   - Node hierarchy with transforms
//!   - Mesh bounding boxes (min/max vertex positions)
//!   - Skin/joint data
//!   - Bone naming conventions

use std::collections::HashMap;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <model1.vrm> [model2.vrm ...]", args[0]);
        std::process::exit(1);
    }

    for path in &args[1..] {
        inspect_model(path);
        eprintln!("\n{}", "=".repeat(80));
    }
}

fn inspect_model(path: &str) {
    eprintln!("\n📦 Inspecting: {}", path);

    // Read GLB and parse JSON
    let data = match std::fs::read(path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("  ❌ Failed to read: {}", e);
            return;
        }
    };

    if data.len() < 20 || &data[0..4] != b"glTF" {
        eprintln!("  ❌ Not a valid GLB file");
        return;
    }

    let json_len = u32::from_le_bytes(data[12..16].try_into().unwrap()) as usize;
    let json_bytes = &data[20..20 + json_len];
    let root: serde_json::Value = match serde_json::from_slice(json_bytes) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("  ❌ Failed to parse JSON: {}", e);
            return;
        }
    };

    // BIN chunk
    let bin_chunk_start = 20 + json_len;
    let bin_len = if bin_chunk_start + 8 <= data.len() {
        u32::from_le_bytes(data[bin_chunk_start..bin_chunk_start + 4].try_into().unwrap()) as usize
    } else {
        0
    };
    let bin_data = if bin_len > 0 && bin_chunk_start + 8 + bin_len <= data.len() {
        &data[bin_chunk_start + 8..bin_chunk_start + 8 + bin_len]
    } else {
        &[] as &[u8]
    };

    // VRM version
    let vrm_version = if root.get("extensions").and_then(|e| e.get("VRMC_vrm")).is_some() {
        "1.0"
    } else if root.get("extensions").and_then(|e| e.get("VRM")).is_some() {
        "0.x"
    } else {
        "unknown"
    };
    eprintln!("  VRM version: {}", vrm_version);
    eprintln!("  File size: {:.1} MB (JSON: {} bytes, BIN: {} bytes)",
        data.len() as f64 / 1_048_576.0, json_len, bin_len);

    // Extensions
    if let Some(req) = root.get("extensionsRequired").and_then(|v| v.as_array()) {
        let names: Vec<&str> = req.iter().filter_map(|v| v.as_str()).collect();
        eprintln!("  extensionsRequired: {:?}", names);
    }
    if let Some(used) = root.get("extensionsUsed").and_then(|v| v.as_array()) {
        let names: Vec<&str> = used.iter().filter_map(|v| v.as_str()).collect();
        eprintln!("  extensionsUsed: {:?}", names);
    }

    // Scene info
    let scene_idx = root.get("scene").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let scenes = root.get("scenes").and_then(|v| v.as_array());
    if let Some(scenes) = scenes {
        eprintln!("  Scenes: {} (default: {})", scenes.len(), scene_idx);
        if let Some(scene) = scenes.get(scene_idx) {
            if let Some(nodes) = scene.get("nodes").and_then(|v| v.as_array()) {
                let root_nodes: Vec<u64> = nodes.iter().filter_map(|v| v.as_u64()).collect();
                eprintln!("  Scene root nodes: {:?}", root_nodes);
            }
        }
    }

    // Nodes
    let nodes = root.get("nodes").and_then(|v| v.as_array());
    let node_count = nodes.map(|n| n.len()).unwrap_or(0);
    eprintln!("  Total nodes: {}", node_count);

    // Collect node data for hierarchy printing
    if let Some(nodes) = nodes {
        // Find root nodes (scene root children)
        let root_node_indices: Vec<usize> = if let Some(scenes) = root.get("scenes").and_then(|v| v.as_array()) {
            scenes.get(scene_idx)
                .and_then(|s| s.get("nodes"))
                .and_then(|n| n.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_u64().map(|i| i as usize)).collect())
                .unwrap_or_default()
        } else {
            vec![]
        };

        // Print root node transforms
        eprintln!("\n  🦴 Root node transforms:");
        for &idx in &root_node_indices {
            print_node_info(nodes, idx, 2);
        }

        // Print full bone hierarchy (first 3 levels)
        eprintln!("\n  🦴 Skeleton hierarchy (3 levels):");
        for &idx in &root_node_indices {
            print_hierarchy(nodes, idx, 0, 3);
        }

        // Collect ALL bone names
        let mut bone_names: Vec<(usize, String)> = Vec::new();
        for (i, node) in nodes.iter().enumerate() {
            if let Some(name) = node.get("name").and_then(|v| v.as_str()) {
                bone_names.push((i, name.to_string()));
            }
        }

        // Check specific bones the renderer uses
        let target_bones = [
            "J_Bip_C_Head", "Head",
            "J_Bip_C_Neck", "Neck",
            "J_Bip_L_UpperArm", "LeftUpperArm",
            "J_Bip_R_UpperArm", "RightUpperArm",
            "J_Bip_C_Hips", "Hips",
            "J_Bip_C_Spine", "Spine",
        ];
        eprintln!("\n  🎯 Key bone lookup:");
        for target in &target_bones {
            let found: Vec<_> = bone_names.iter()
                .filter(|(_, name)| name.contains(target))
                .collect();
            if !found.is_empty() {
                for (idx, name) in &found {
                    let node = &nodes[*idx];
                    let t = get_translation(node);
                    let r = get_rotation(node);
                    let s = get_scale(node);
                    eprintln!("    [{}] '{}': pos=({:.3},{:.3},{:.3}) rot=({:.3},{:.3},{:.3},{:.3}) scale=({:.3},{:.3},{:.3})",
                        idx, name, t[0], t[1], t[2], r[0], r[1], r[2], r[3], s[0], s[1], s[2]);
                }
            }
        }

        // Check for non-identity rotations on bones
        let mut identity_count = 0;
        let mut non_identity_count = 0;
        for node in nodes.iter() {
            let r = get_rotation(node);
            if is_identity_rotation(&r) {
                identity_count += 1;
            } else {
                non_identity_count += 1;
            }
        }
        eprintln!("\n  Rotation stats: {} identity, {} non-identity ({:.0}% have real rotations)",
            identity_count, non_identity_count,
            non_identity_count as f64 / (identity_count + non_identity_count) as f64 * 100.0);
    }

    // Meshes
    if let Some(meshes) = root.get("meshes").and_then(|v| v.as_array()) {
        let mut total_primitives = 0;
        for mesh in meshes {
            if let Some(prims) = mesh.get("primitives").and_then(|v| v.as_array()) {
                total_primitives += prims.len();
            }
        }
        eprintln!("\n  Meshes: {} ({} primitives)", meshes.len(), total_primitives);

        // Check mesh morph targets
        let mut morph_count = 0;
        for mesh in meshes {
            if let Some(prims) = mesh.get("primitives").and_then(|v| v.as_array()) {
                for prim in prims {
                    if let Some(targets) = prim.get("targets").and_then(|v| v.as_array()) {
                        morph_count += targets.len();
                    }
                }
            }
        }
        eprintln!("  Morph targets: {}", morph_count);
    }

    // Skins
    if let Some(skins) = root.get("skins").and_then(|v| v.as_array()) {
        eprintln!("\n  Skins: {}", skins.len());
        for (i, skin) in skins.iter().enumerate() {
            let joint_count = skin.get("joints").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0);
            let ibm_accessor = skin.get("inverseBindMatrices").and_then(|v| v.as_u64());
            let skeleton = skin.get("skeleton").and_then(|v| v.as_u64());
            eprintln!("    Skin {}: {} joints, IBM accessor={:?}, skeleton root={:?}",
                i, joint_count, ibm_accessor, skeleton);

            // Check IBM data: are rotations present?
            if let Some(ibm_idx) = ibm_accessor {
                check_inverse_bind_matrices(&root, ibm_idx as usize, bin_data);
            }
        }
    }

    // Accessors summary
    if let Some(accessors) = root.get("accessors").and_then(|v| v.as_array()) {
        eprintln!("\n  Accessors: {}", accessors.len());

        // Find POSITION accessors and compute bounding box
        let meshes = root.get("meshes").and_then(|v| v.as_array());
        if let Some(meshes) = meshes {
            for (mi, mesh) in meshes.iter().enumerate() {
                if let Some(prims) = mesh.get("primitives").and_then(|v| v.as_array()) {
                    for (pi, prim) in prims.iter().enumerate() {
                        if let Some(pos_idx) = prim.get("attributes")
                            .and_then(|a| a.get("POSITION"))
                            .and_then(|v| v.as_u64())
                        {
                            let accessor = &accessors[pos_idx as usize];
                            let min_val = accessor.get("min");
                            let max_val = accessor.get("max");
                            let count = accessor.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
                            let comp_type = accessor.get("componentType").and_then(|v| v.as_u64()).unwrap_or(0);
                            let acc_type = accessor.get("type").and_then(|v| v.as_str()).unwrap_or("?");

                            if mi < 3 || pi == 0 {
                                eprintln!("    Mesh[{}].prim[{}] POSITION: {} verts, type={}/{}, min={}, max={}",
                                    mi, pi, count, acc_type, comp_type,
                                    format_vec(min_val), format_vec(max_val));
                            }
                        }
                    }
                }
            }
        }
    }

    // Materials
    if let Some(materials) = root.get("materials").and_then(|v| v.as_array()) {
        let mut mtoon_count = 0;
        let mut unlit_count = 0;
        let mut pbr_count = 0;
        for mat in materials {
            if mat.get("extensions").and_then(|e| e.get("VRMC_materials_mtoon")).is_some() {
                mtoon_count += 1;
            } else if mat.get("extensions").and_then(|e| e.get("KHR_materials_unlit")).is_some() {
                unlit_count += 1;
            } else {
                pbr_count += 1;
            }
        }
        eprintln!("\n  Materials: {} total (MToon: {}, Unlit: {}, PBR: {})",
            materials.len(), mtoon_count, unlit_count, pbr_count);
    }

    // Images
    if let Some(images) = root.get("images").and_then(|v| v.as_array()) {
        let mut type_counts: HashMap<String, usize> = HashMap::new();
        for img in images {
            let mime = img.get("mimeType").and_then(|v| v.as_str()).unwrap_or("unknown");
            *type_counts.entry(mime.to_string()).or_insert(0) += 1;
        }
        eprintln!("  Images: {} total ({:?})", images.len(), type_counts);
    }

    // VRM-specific: humanoid bone mapping
    if vrm_version == "1.0" {
        if let Some(humanoid) = root.get("extensions")
            .and_then(|e| e.get("VRMC_vrm"))
            .and_then(|v| v.get("humanoid"))
        {
            if let Some(bones) = humanoid.get("humanBones").and_then(|v| v.as_object()) {
                eprintln!("\n  VRMC humanoid bones ({}):", bones.len());
                for (bone_name, bone_data) in bones {
                    let node_idx = bone_data.get("node").and_then(|v| v.as_u64());
                    if let Some(idx) = node_idx {
                        let nodes_arr = root.get("nodes").and_then(|v| v.as_array());
                        let node_name = nodes_arr
                            .and_then(|n| n.get(idx as usize))
                            .and_then(|n| n.get("name"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("?");
                        if bone_name == "head" || bone_name == "neck" || bone_name == "hips"
                            || bone_name == "spine" || bone_name.contains("UpperArm")
                            || bone_name.contains("Shoulder")
                        {
                            eprintln!("    {} → node {} '{}'", bone_name, idx, node_name);
                        }
                    }
                }
            }
        }
    } else if vrm_version == "0.x" {
        if let Some(humanoid) = root.get("extensions")
            .and_then(|e| e.get("VRM"))
            .and_then(|v| v.get("humanoid"))
        {
            if let Some(bones) = humanoid.get("humanBones").and_then(|v| v.as_array()) {
                eprintln!("\n  VRM 0.x humanoid bones ({}):", bones.len());
                for bone in bones {
                    let bone_name = bone.get("bone").and_then(|v| v.as_str()).unwrap_or("?");
                    let node_idx = bone.get("node").and_then(|v| v.as_u64());
                    if let Some(idx) = node_idx {
                        let nodes_arr = root.get("nodes").and_then(|v| v.as_array());
                        let node_name = nodes_arr
                            .and_then(|n| n.get(idx as usize))
                            .and_then(|n| n.get("name"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("?");
                        if bone_name == "head" || bone_name == "neck" || bone_name == "hips"
                            || bone_name == "spine" || bone_name.contains("UpperArm")
                            || bone_name.contains("Shoulder")
                        {
                            eprintln!("    {} → node {} '{}'", bone_name, idx, node_name);
                        }
                    }
                }
            }
        }
    }
}

fn get_translation(node: &serde_json::Value) -> [f64; 3] {
    if let Some(t) = node.get("translation").and_then(|v| v.as_array()) {
        [
            t.get(0).and_then(|v| v.as_f64()).unwrap_or(0.0),
            t.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0),
            t.get(2).and_then(|v| v.as_f64()).unwrap_or(0.0),
        ]
    } else {
        [0.0, 0.0, 0.0]
    }
}

fn get_rotation(node: &serde_json::Value) -> [f64; 4] {
    if let Some(r) = node.get("rotation").and_then(|v| v.as_array()) {
        [
            r.get(0).and_then(|v| v.as_f64()).unwrap_or(0.0),
            r.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0),
            r.get(2).and_then(|v| v.as_f64()).unwrap_or(0.0),
            r.get(3).and_then(|v| v.as_f64()).unwrap_or(1.0),
        ]
    } else {
        [0.0, 0.0, 0.0, 1.0]
    }
}

fn get_scale(node: &serde_json::Value) -> [f64; 3] {
    if let Some(s) = node.get("scale").and_then(|v| v.as_array()) {
        [
            s.get(0).and_then(|v| v.as_f64()).unwrap_or(1.0),
            s.get(1).and_then(|v| v.as_f64()).unwrap_or(1.0),
            s.get(2).and_then(|v| v.as_f64()).unwrap_or(1.0),
        ]
    } else {
        [1.0, 1.0, 1.0]
    }
}

fn is_identity_rotation(r: &[f64; 4]) -> bool {
    r[0].abs() < 0.001 && r[1].abs() < 0.001 && r[2].abs() < 0.001 && (r[3] - 1.0).abs() < 0.001
}

fn print_node_info(nodes: &[serde_json::Value], idx: usize, indent: usize) {
    if let Some(node) = nodes.get(idx) {
        let name = node.get("name").and_then(|v| v.as_str()).unwrap_or("(unnamed)");
        let t = get_translation(node);
        let r = get_rotation(node);
        let s = get_scale(node);
        let has_mesh = node.get("mesh").is_some();
        let has_skin = node.get("skin").is_some();
        let mesh_tag = if has_mesh { " [MESH]" } else { "" };
        let skin_tag = if has_skin { " [SKINNED]" } else { "" };

        eprintln!("{:indent$}[{}] '{}'{}{}", "", idx, name, mesh_tag, skin_tag, indent = indent);
        if !is_identity_rotation(&r) || t[0].abs() > 0.001 || t[1].abs() > 0.001 || t[2].abs() > 0.001 {
            eprintln!("{:indent$}  pos=({:.4},{:.4},{:.4}) rot=({:.4},{:.4},{:.4},{:.4}) scale=({:.2},{:.2},{:.2})",
                "", t[0], t[1], t[2], r[0], r[1], r[2], r[3], s[0], s[1], s[2], indent = indent);
        }
    }
}

fn print_hierarchy(nodes: &[serde_json::Value], idx: usize, depth: usize, max_depth: usize) {
    if depth >= max_depth { return; }
    let indent = 4 + depth * 2;
    print_node_info(nodes, idx, indent);

    if let Some(children) = nodes.get(idx)
        .and_then(|n| n.get("children"))
        .and_then(|c| c.as_array())
    {
        for child in children {
            if let Some(child_idx) = child.as_u64() {
                print_hierarchy(nodes, child_idx as usize, depth + 1, max_depth);
            }
        }
    }
}

fn format_vec(val: Option<&serde_json::Value>) -> String {
    match val {
        Some(v) => {
            if let Some(arr) = v.as_array() {
                let nums: Vec<String> = arr.iter()
                    .map(|n| format!("{:.3}", n.as_f64().unwrap_or(0.0)))
                    .collect();
                format!("[{}]", nums.join(","))
            } else {
                format!("{}", v)
            }
        }
        None => "N/A".to_string(),
    }
}

fn check_inverse_bind_matrices(root: &serde_json::Value, accessor_idx: usize, bin_data: &[u8]) {
    let accessors = match root.get("accessors").and_then(|v| v.as_array()) {
        Some(a) => a,
        None => return,
    };
    let accessor = match accessors.get(accessor_idx) {
        Some(a) => a,
        None => return,
    };

    let buffer_view_idx = accessor.get("bufferView").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let acc_offset = accessor.get("byteOffset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let count = accessor.get("count").and_then(|v| v.as_u64()).unwrap_or(0) as usize;

    let buffer_views = match root.get("bufferViews").and_then(|v| v.as_array()) {
        Some(bv) => bv,
        None => return,
    };
    let bv = match buffer_views.get(buffer_view_idx) {
        Some(bv) => bv,
        None => return,
    };

    let bv_offset = bv.get("byteOffset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let start = bv_offset + acc_offset;

    // Each IBM is a 4x4 float matrix = 64 bytes
    let mat_size = 64;

    // Print first few IBMs for inspection
    for i in 0..count.min(3) {
        let offset = start + i * mat_size;
        if offset + mat_size > bin_data.len() { break; }

        let mut mat = [[0.0f32; 4]; 4];
        for col in 0..4 {
            for row in 0..4 {
                let byte_off = offset + (col * 4 + row) * 4;
                mat[col][row] = f32::from_le_bytes(
                    bin_data[byte_off..byte_off + 4].try_into().unwrap()
                );
            }
        }

        let is_identity_rot =
            (mat[0][0] - 1.0).abs() < 0.01 && mat[0][1].abs() < 0.01 && mat[0][2].abs() < 0.01 &&
            mat[1][0].abs() < 0.01 && (mat[1][1] - 1.0).abs() < 0.01 && mat[1][2].abs() < 0.01 &&
            mat[2][0].abs() < 0.01 && mat[2][1].abs() < 0.01 && (mat[2][2] - 1.0).abs() < 0.01;

        eprintln!("      IBM[{}]: trans=({:.3},{:.3},{:.3}) rot_identity={} diag=({:.3},{:.3},{:.3})",
            i, mat[3][0], mat[3][1], mat[3][2], is_identity_rot,
            mat[0][0], mat[1][1], mat[2][2]);
    }

    // Count all IBMs for rotation stats
    let mut total_rot = 0;
    for i in 0..count {
        let offset = start + i * mat_size;
        if offset + mat_size > bin_data.len() { break; }
        let mut has_rot = false;
        for col in 0..3 {
            for row in 0..3 {
                let byte_off = offset + (col * 4 + row) * 4;
                let val = f32::from_le_bytes(bin_data[byte_off..byte_off + 4].try_into().unwrap());
                let expected = if col == row { 1.0 } else { 0.0 };
                if (val - expected).abs() > 0.01 {
                    has_rot = true;
                }
            }
        }
        if has_rot { total_rot += 1; }
    }
    eprintln!("      IBM rotation stats: {}/{} have non-identity rotation ({:.0}%)",
        total_rot, count, total_rot as f64 / count.max(1) as f64 * 100.0);
}
