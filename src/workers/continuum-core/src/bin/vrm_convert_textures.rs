//! VRM Model Converter — Normalizes VRM files for Bevy compatibility
//!
//! Performs two idempotent fixes on VRM files:
//!
//! 1. **Texture fix** (VRM 1.0 with KHR_texture_basisu):
//!    Move texture source from KHR_texture_basisu extension → standard texture.source.
//!    Remove KHR_texture_basisu from extensionsUsed/Required.
//!    Keep KTX2 binary data untouched — Bevy's KTX2 decoder handles it.
//!
//! 2. **Orientation fix** (VRM 1.0):
//!    VRM 0.x faces -Z (Unity coordinate export), VRM 1.0 faces +Z (GLTF convention).
//!    Apply 180° Y rotation to the skeleton root node so all models consistently
//!    face -Z. This way all models share the same coordinate convention and can
//!    coexist in a scene without per-model orientation logic.
//!
//! Both fixes are idempotent: running the converter on an already-converted file
//! detects no work needed and returns without rewriting.
//!
//! Usage: vrm-convert-textures <input.vrm> [output.vrm]
//!        If output is omitted, overwrites the input file.
//!
//! This is 100% safe Rust — no C++ FFI, no pixel decoding, just JSON surgery.

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 || args.len() > 3 {
        eprintln!("Usage: {} <input.vrm> [output.vrm]", args[0]);
        eprintln!("  If output is omitted, overwrites the input file.");
        std::process::exit(1);
    }

    let input_path = &args[1];
    let output_path = if args.len() == 3 { &args[2] } else { &args[1] };

    eprintln!("Reading {}", input_path);
    let data = std::fs::read(input_path).expect("Failed to read input file");

    // Parse GLB header
    assert!(data.len() >= 20, "File too small for GLB header");
    assert_eq!(&data[0..4], b"glTF", "Not a GLB file");
    let _version = u32::from_le_bytes(data[4..8].try_into().unwrap());
    let json_len = u32::from_le_bytes(data[12..16].try_into().unwrap()) as usize;
    let json_type = u32::from_le_bytes(data[16..20].try_into().unwrap());
    assert_eq!(json_type, 0x4E4F534A, "Expected JSON chunk type");
    let json_bytes = &data[20..20 + json_len];

    let bin_chunk_start = 20 + json_len;
    let bin_len = u32::from_le_bytes(data[bin_chunk_start..bin_chunk_start + 4].try_into().unwrap()) as usize;
    let bin_type = u32::from_le_bytes(data[bin_chunk_start + 4..bin_chunk_start + 8].try_into().unwrap());
    assert_eq!(bin_type, 0x004E4942, "Expected BIN chunk type");
    let bin_data = &data[bin_chunk_start + 8..bin_chunk_start + 8 + bin_len];

    let mut json: serde_json::Value = serde_json::from_slice(json_bytes).expect("Invalid JSON");

    let mut modified = false;

    // =========================================================================
    // Fix 1: KHR_texture_basisu — move texture sources to standard field
    // =========================================================================
    let needs_basisu_fix = json["extensionsRequired"]
        .as_array()
        .map(|arr| arr.iter().any(|v| v.as_str() == Some("KHR_texture_basisu")))
        .unwrap_or(false);

    if needs_basisu_fix {
        let ktx2_count = json["images"]
            .as_array()
            .map(|imgs| imgs.iter().filter(|img| img["mimeType"].as_str() == Some("image/ktx2")).count())
            .unwrap_or(0);
        eprintln!("  [Texture] Found {} KTX2 textures", ktx2_count);

        let mut moved = 0;
        if let Some(textures) = json["textures"].as_array_mut() {
            for tex in textures.iter_mut() {
                let basisu_source = tex
                    .get("extensions")
                    .and_then(|e| e.get("KHR_texture_basisu"))
                    .and_then(|b| b.get("source"))
                    .cloned();
                if let Some(source) = basisu_source {
                    tex["source"] = source;
                    if let Some(exts) = tex.get_mut("extensions") {
                        if let Some(obj) = exts.as_object_mut() {
                            obj.remove("KHR_texture_basisu");
                        }
                    }
                    moved += 1;
                }
            }
            // Clean up empty extension objects
            for tex in textures.iter_mut() {
                if tex.get("extensions").and_then(|e| e.as_object()).map(|o| o.is_empty()).unwrap_or(false) {
                    if let Some(obj) = tex.as_object_mut() {
                        obj.remove("extensions");
                    }
                }
            }
        }
        eprintln!("  [Texture] Moved {} sources from KHR_texture_basisu → standard field", moved);

        for key in &["extensionsUsed", "extensionsRequired"] {
            if let Some(arr) = json[key].as_array_mut() {
                arr.retain(|v| v.as_str() != Some("KHR_texture_basisu"));
            }
        }
        modified = true;
    }

    // =========================================================================
    // Fix 2: VRM 1.0 orientation — rotate skeleton root 180° Y
    // =========================================================================
    let is_vrm_1_0 = json.get("extensions")
        .and_then(|e| e.get("VRMC_vrm"))
        .is_some();

    if is_vrm_1_0 {
        // Find the skeleton root node. In VRoid Hub VRM 1.0 models, all skins
        // share the same skeleton root (typically node 0 "Root").
        let skeleton_root_idx = json["skins"]
            .as_array()
            .and_then(|skins| skins.first())
            .and_then(|skin| skin.get("skeleton"))
            .and_then(|v| v.as_u64())
            .map(|v| v as usize);

        if let Some(root_idx) = skeleton_root_idx {
            let already_rotated = json["nodes"]
                .as_array()
                .and_then(|nodes| nodes.get(root_idx))
                .and_then(|node| node.get("rotation"))
                .and_then(|r| r.as_array())
                .map(|arr| {
                    let x = arr.get(0).and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let y = arr.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let z = arr.get(2).and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let w = arr.get(3).and_then(|v| v.as_f64()).unwrap_or(1.0);
                    // 180° Y quaternion = [0, 1, 0, 0]
                    x.abs() < 0.01 && (y - 1.0).abs() < 0.01 && z.abs() < 0.01 && w.abs() < 0.01
                })
                .unwrap_or(false);

            if already_rotated {
                eprintln!("  [Orient] Skeleton root already has 180° Y rotation — no fix needed");
            } else {
                let node_name = json["nodes"]
                    .as_array()
                    .and_then(|n| n.get(root_idx))
                    .and_then(|n| n.get("name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                eprintln!("  [Orient] VRM 1.0 faces +Z → rotating skeleton root node {} '{}' by 180° Y",
                    root_idx, node_name);

                // Apply 180° Y rotation to skeleton root.
                // Quaternion for 180° around Y: (x=0, y=sin(90°), z=0, w=cos(90°)) = (0, 1, 0, 0)
                //
                // This rotates ALL joint global transforms through the skinning pipeline,
                // causing skinned vertices to face -Z (matching VRM 0.x convention).
                // Binary data (vertex positions, IBMs) stays unchanged — the rotation
                // is applied at render time through the joint matrix computation:
                //   jointMatrix = (Rot * originalGlobal) * inverseBindMatrix = Rot
                // So every vertex is simply rotated 180° Y at the GPU level.
                if let Some(nodes) = json["nodes"].as_array_mut() {
                    if let Some(node) = nodes.get_mut(root_idx) {
                        node["rotation"] = serde_json::json!([0.0, 1.0, 0.0, 0.0]);
                        modified = true;
                    }
                }
            }
        } else {
            eprintln!("  [Orient] VRM 1.0 but no skeleton root found in skins — skipping orientation fix");
        }
    }

    // =========================================================================
    // Write output if anything changed
    // =========================================================================
    if !modified {
        eprintln!("No fixes needed.");
        if input_path != output_path {
            std::fs::copy(input_path, output_path).unwrap();
        }
        return;
    }

    // Serialize updated JSON, pad to 4-byte boundary (GLB requirement)
    let mut new_json = serde_json::to_vec(&json).expect("JSON serialize failed");
    while new_json.len() % 4 != 0 {
        new_json.push(b' ');
    }

    // Rebuild GLB: header (12) + JSON chunk (8 + json) + BIN chunk (8 + bin)
    // Binary data is UNCHANGED — vertex positions, IBMs, KTX2 textures all stay as-is
    let total = 12 + 8 + new_json.len() + 8 + bin_data.len();
    let mut out = Vec::with_capacity(total);

    // GLB header
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2u32.to_le_bytes()); // version
    out.extend_from_slice(&(total as u32).to_le_bytes());

    // JSON chunk
    out.extend_from_slice(&(new_json.len() as u32).to_le_bytes());
    out.extend_from_slice(&0x4E4F534Au32.to_le_bytes()); // "JSON"
    out.extend_from_slice(&new_json);

    // BIN chunk (unchanged)
    out.extend_from_slice(&(bin_data.len() as u32).to_le_bytes());
    out.extend_from_slice(&0x004E4942u32.to_le_bytes()); // "BIN\0"
    out.extend_from_slice(bin_data);

    std::fs::write(output_path, &out).expect("Failed to write output");
    eprintln!(
        "Done: {} -> {} ({:.1} MB -> {:.1} MB, json {} -> {} bytes)",
        input_path,
        output_path,
        data.len() as f64 / 1_048_576.0,
        out.len() as f64 / 1_048_576.0,
        json_len,
        new_json.len(),
    );
}
