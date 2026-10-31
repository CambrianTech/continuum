#!/usr/bin/env python3
"""Parse GLB/VRM files and compare node hierarchy + root bone positions."""
import struct
import json
import os

AVATARS_DIR = "models/avatars"

def parse_glb(path):
    """Parse a glTF 2.0 binary file (GLB) and extract nodes, meshes, accessors."""
    result = {"path": path}
    
    with open(path, 'rb') as f:
        header = f.read(12)
        if len(header) < 12:
            return None
        magic, version, length = struct.unpack('<III', header)
        if magic != 0x46546C67 or version != 2:
            print(f"  Not a valid GLB2 file (magic=0x{magic:x}, version={version})")
            return None
        
        json_data = {}
        
        while f.tell() < length - 12:
            chunk_len = struct.unpack('<I', f.read(4))[0]
            chunk_type_raw = f.read(4)
            
            if chunk_type_raw == b'JSON':
                json_bytes = f.read(chunk_len)
                json_str = json_bytes.decode('utf-8', errors='replace')
                try:
                    json_data = json.loads(json_str)
                except json.JSONDecodeError:
                    print(f"  WARNING: JSON parse failed, trying partial")
                    pass
                
            elif chunk_type_raw == b'BIN ':
                result["bin_size"] = chunk_len
                f.read(chunk_len)  # skip binary data, we only need JSON for now
            else:
                f.seek(chunk_len, 1)
        
        # Extract node hierarchy from parsed JSON
        nodes = json_data.get("nodes", [])
        meshes = json_data.get("meshes", [])
        scenes = json_data.get("scenes")
        scene_nodes = []
        if scenes and len(scenes) > 0:
            scene_nodes = scenes[0].get("nodes", [])
        
        # Build full node tree from root nodes
        result["all_nodes"] = {}  # name -> {parent, translation, rotation, scale}
        for node in nodes:
            nid = node.get("name", f"node_{node.get('index', '?')}")
            parent_idx = node.get("parent")
            trans = node.get("translation", [0.0, 0.0, 0.0])
            rot = node.get("rotation", [0.0, 0.0, 0.0, 1.0])
            scale = node.get("scale", [1.0, 1.0, 1.0])
            
            # Find parent name
            parent_name = None
            if parent_idx is not None:
                for n in nodes:
                    if n.get("index") == parent_idx and "name" in n:
                        parent_name = n["name"]
                        break
            
            result["all_nodes"][nid] = {
                "parent": parent_name,
                "translation": trans,
                "rotation": rot,
                "scale": scale,
                "children": []
            }
        
        # Build children lists
        for nid, info in result["all_nodes"].items():
            if info["parent"] and info["parent"] in result["all_nodes"]:
                result["all_nodes"][info["parent"]]["children"].append(nid)
        
        result["num_nodes"] = len(nodes)
        result["num_meshes"] = len(meshes)
        result["scene_root_names"] = []
        for sni in scene_nodes:
            for n in nodes:
                if n.get("index") == sni and "name" in n:
                    result["scene_root_names"].append(n["name"])
        
        # Count accessors/primitives for vertex estimate
        accessors = json_data.get("accessors", [])
        result["num_accessors"] = len(accessors)
        
        # Count positions accessor to estimate vertices
        pos_count = sum(1 for a in accessors if a.get("componentType") == 5126 and a.get("type") == "VEC3")
        result["pos_accessor_count"] = pos_count
        
        return result

def get_subtree(node_name, all_nodes, depth=0):
    """Get subtree under a node as string."""
    if node_name not in all_nodes:
        return ""
    info = all_nodes[node_name]
    trans = info["translation"]
    line = "  " * depth + f"{node_name} (t={trans[0]:+.2f},{trans[1]:+.2f},{trans[2]:+.2f})"
    children = sorted(info.get("children", []))
    return [line] + [get_subtree(c, all_nodes, depth+1) for c in children[:5]]  # cap at 5 children

def main():
    vrm_files = []
    for f in sorted(os.listdir(AVATARS_DIR)):
        path = os.path.join(AVATARS_DIR, f)
        if f.endswith('.vrm') and os.path.isfile(path):
            vrm_files.append((f, path))
    
    print(f"Found {len(vrm_files)} VRM files\n")
    
    results = {}
    for model_name, path in vrm_files:
        print(f"Parsing {model_name} ({os.path.getsize(path) / 1e6:.1f}MB)...")
        data = parse_glb(path)
        if data is None:
            continue
        
        results[model_name] = {
            "file_size": os.path.getsize(path),
            "num_nodes": data["num_nodes"],
            "num_meshes": data["num_meshes"],
            "scene_roots": data["scene_root_names"],
            "all_nodes": data["all_nodes"],
        }
        
        # Print first 10 node names
        all_names = sorted(data["all_nodes"].keys())
        print(f"  Nodes: {data['num_nodes']}, Meshes: {data['num_meshes']}")
        print(f"  Scene roots: {data['scene_root_names']}")
        if len(all_names) > 0:
            sample = all_names[:15]
            print(f"  Sample node names: {', '.join(sample)}")
        
        # Find root nodes (no parent or parent is None)
        roots = [n for n, info in data["all_nodes"].items() if not info.get("parent")]
        if roots:
            root_info = data["all_nodes"][roots[0]]
            t = root_info["translation"]
            print(f"  Root node '{roots[0]}': translation=({t[0]:+.3f}, {t[1]:+.3f}, {t[2]:+.3f})")
            
            # Also check the "hips" or "root" bone if present
            for candidate in ['hips', 'Root', 'root', 'Hips', 'Body', 'body']:
                if candidate in data["all_nodes"]:
                    ci = data["all_nodes"][candidate]
                    ct = ci["translation"]
                    print(f"  '{candidate}': parent={ci['parent']}, translation=({ct[0]:+.3f}, {ct[1]:+.3f}, {ct[2]:+.3f})")
        print()

    # COMPARISON
    print("\n" + "=" * 80)
    print("COMPARISON SUMMARY")
    print("=" * 80)
    print(f"\n{'Model':<25} {'Size':>10} {'Nodes':>7} {'Meshes':>7} {'SceneRoots':>12}")
    print("-" * 80)
    
    for model_name in sorted(results.keys()):
        d = results[model_name]
        size_str = f"{d['file_size'] / 1e6:.1f}MB"
        roots_str = ', '.join(d['scene_roots']) if d['scene_roots'] else '(none)'
        print(f"{model_name:<25} {size_str:>10} {d['num_nodes']:>7} {d['num_meshes']:>7} {roots_str:>12}")

if __name__ == "__main__":
    main()
