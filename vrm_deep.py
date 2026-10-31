#!/usr/bin/env python3
"""Deep-dive VRM comparison: meshes, bone hierarchy depth, vertex counts."""
import struct
import json
import os

AVATARS_DIR = "models/avatars"

def parse_glb(path):
    """Parse a glTF 2.0 binary file (GLB)."""
    result = {"path": path}
    
    with open(path, 'rb') as f:
        header = f.read(12)
        if len(header) < 12:
            return None
        magic, version, length = struct.unpack('<III', header)
        if magic != 0x46546C67 or version != 2:
            return None
        
        json_data_str = None
        
        while f.tell() < length - 12:
            chunk_len = struct.unpack('<I', f.read(4))[0]
            chunk_type_raw = f.read(4)
            
            if chunk_type_raw == b'JSON':
                json_data_str = f.read(chunk_len).decode('utf-8', errors='replace')
                result["json_size"] = chunk_len
            elif chunk_type_raw == b'BIN ':
                bin_start = f.tell()
                bin_end = bin_start + chunk_len
                result["bin_size"] = chunk_len
                f.seek(bin_end)  # skip binary data for now
            else:
                f.seek(chunk_len, 1)
        
        if json_data_str:
            try:
                result["json"] = json.loads(json_data_str)
            except json.JSONDecodeError as e:
                print(f"  JSON parse error: {e}")
                result["json"] = {}
    
    return result

def analyze_meshes(glb):
    """Extract mesh info from parsed GLB."""
    gltf = glb.get("json", {})
    meshes = gltf.get("meshes", [])
    
    mesh_info = []
    for i, mesh in enumerate(meshes):
        primitives = mesh.get("primitives", [])
        for prim_idx, prim in enumerate(primitives):
            # Count vertices via POSITION accessor
            pos_acc = prim.get("attributes", {}).get("POSITION")
            if pos_acc is not None:
                accs = gltf.get("accessors", [])
                verts = 0
                if isinstance(pos_acc, int) and pos_acc < len(accs):
                    verts = accs[pos_acc].get("count", 0)
                
                # Check for COLOR_0 (degenerate vertex colors)
                has_color0 = "COLOR_0" in prim.get("attributes", {})
                
                mesh_info.append({
                    "mesh_idx": i,
                    "prim_idx": prim_idx,
                    "vertices": verts,
                    "has_COLOR_0": has_color0,
                    "material": prim.get("material"),
                })
    
    return mesh_info

def analyze_bone_hierarchy(glb):
    """Analyze bone/skeleton hierarchy depth and structure."""
    gltf = glb.get("json", {})
    nodes = gltf.get("nodes", [])
    
    # Build parent->children map
    children_map = {}  # parent_idx -> list of (child_gltf_index)
    node_names = {}
    for idx, node in enumerate(nodes):
        nid = node.get("name", f"unnamed-{idx}")
        node_index = node.get("index", idx)  # glTF index field or fallback to position
        node_names[node_index] = nid
        parent_idx = node.get("parent")
        if parent_idx is not None:
            children_map.setdefault(parent_idx, []).append(node_index)
    
    # Find root nodes (no parent)
    roots = [n for n in nodes if "parent" not in n]
    
    # Calculate max depth of hierarchy
    def get_depth(node_idx, visited=None):
        if visited is None:
            visited = set()
        if node_idx in visited:
            return 0
        visited.add(node_idx)
        kids = children_map.get(node_idx, [])
        if not kids:
            return 1
        # kids are now integers (gltf indices), pass directly
        return 1 + max(get_depth(k, visited) for k in kids)
    max_depths = {}
    for idx, root in enumerate(roots):
        name = root.get("name", f"root-{idx}")
        depth = get_depth(root.get("index", idx))
        max_depths[name] = depth
    
    # Count bones with "J_" prefix (VRM joint names) vs standard skeleton
    j_bones = [n for n in node_names.values() if n.startswith("J_")]
    bip_bones = [n for n in node_names.values() if n.startswith("J_Bip")]
    
    return {
        "num_nodes": len(nodes),
        "roots": [(r.get("name", f"root"), r.get("parent")) for r in roots],
        "max_depths": max_depths,
        "j_bone_count": len(j_bones),
        "bip_bone_count": len(bip_bones),
    }

def get_body_bounds(glbf):
    """Try to extract bounding box from mesh vertex data (from BIN chunk)."""
    gltf = glbf.get("json", {})
    
    # We need the binary buffer to compute actual bounds.
    # For now, just report what we can from the JSON: 
    # VRM extension has firstPerson bone info and blendShape groups
    
    vrm_ext = gltf.get("extensions", {}).get("VRM", {})
    if not vrm_ext:
        return {}
    
    info = {"has_vrm_ext": True}
    
    # First person: head bone reference
    fp = vrm_ext.get("firstPerson", {})
    head_bone = fp.get("headBone", "")
    offset_vec = fp.get("headBoneOffsetVector")
    if offset_vec:
        info["head_bone"] = head_bone
        info["head_offset"] = tuple(offset_vec)
    
    # Look at blendShape groups for face expression data (indicates facial mesh complexity)
    bs_groups = vrm_ext.get("blendFormMaster", {})
    info["blendshape_count"] = len(bs_groups) if isinstance(bs_groups, dict) else 0
    
    return info

def main():
    vrm_files = []
    for f in sorted(os.listdir(AVATARS_DIR)):
        path = os.path.join(AVATARS_DIR, f)
        if f.endswith('.vrm') and os.path.isfile(path):
            vrm_files.append((f, path))
    
    print(f"Found {len(vrm_files)} VRM files\n")
    
    results = {}
    for model_name, path in vrm_files:
        size_mb = os.path.getsize(path) / 1e6
        print(f"Parsing {model_name} ({size_mb:.1f}MB)...")
        
        data = parse_glb(path)
        if not data or "json" not in data:
            print("  SKIPPED (no JSON)")
            continue
        
        mesh_info = analyze_meshes(data)
        bone_info = analyze_bone_hierarchy(data)
        vrm_info = get_body_bounds(data)
        
        total_verts = sum(m["vertices"] for m in mesh_info)
        has_color0_any = any(m["has_COLOR_0"] for m in mesh_info)
        
        results[model_name] = {
            "size_mb": size_mb,
            "num_nodes": bone_info["num_nodes"],
            "num_meshes": len(mesh_info),
            "total_vertices": total_verts,
            "mesh_details": mesh_info,
            "bone_depths": bone_info["max_depths"],
            "j_bones": bone_info["j_bone_count"],
            "bip_bones": bone_info["bip_bone_count"],
            "roots": bone_info["roots"],
            "vrm_ext": vrm_info,
        }
        
        print(f"  Nodes: {bone_info['num_nodes']}, Meshes: {len(mesh_info)}, Vertices: ~{total_verts}")
        for mi in mesh_info:
            print(f"    Mesh {mi['mesh_idx']} prim {mi['prim_idx']}: {mi['vertices']} verts, COLOR_0={mi['has_COLOR_0']}, mat={mi['material']}")
        print(f"  Bone depths: {bone_info['max_depths']}")
        print(f"  J_bones: {bone_info['j_bone_count']}, Bip_bones: {bone_info['bip_bone_count']}")
        if vrm_info.get("head_bone"):
            print(f"  VRM head_bone: {vrm_info['head_bone']}, offset: {vrm_info['head_offset']}")
    
    # Summary table
    print("\n\n" + "=" * 100)
    print("COMPARISON SUMMARY")
    print("=" * 100)
    header = f"{'Model':<25} {'Size':>8} {'Nodes':>6} {'Meshes':>7} {'Verts':>8} {'J_bones':>9} {'Bip':>4} {'Depth':>6}"
    print(header)
    print("-" * 100)
    
    for model_name in sorted(results.keys()):
        r = results[model_name]
        max_d = max(r["bone_depths"].values()) if r["bone_depths"] else 0
        print(f"{model_name:<25} {r['size_mb']:>7.1f}MB {r['num_nodes']:>6} {r['num_meshes']:>7} "
              f"{r['total_vertices']:>8} {r['j_bones']:>9} {r['bip_bones']:>4} {max_d:>6}")

if __name__ == "__main__":
    main()
