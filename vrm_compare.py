#!/usr/bin/env python3
"""Parse GLB/VRM files and compare bone structure / mesh geometry."""
import struct
import os

AVATARS_DIR = "models/avatars"

def parse_glb(path):
    """Parse a glTF 2.0 binary file (GLB) and extract skeleton + mesh info."""
    result = {"path": path, "bones": {}, "meshes": []}
    
    with open(path, 'rb') as f:
        # GLB Header: magic(4) + version(4) + length(4) = 12 bytes
        header = f.read(12)
        if len(header) < 12:
            return None
        magic, version, length = struct.unpack('<III', header)
        if magic != 0x46546C67 or version != 2:
            print(f"  Not a valid GLB2 file (magic=0x{magic:x}, version={version})")
            return None
        
        while f.tell() < length - 12:
            chunk_len = struct.unpack('<I', f.read(4))[0]
            chunk_type_raw = f.read(4)
            data_start = f.tell()
            
            if chunk_type_raw == b'JSON':
                # JSON chunk contains accessors, animations, meshes, nodes, scene info
                json_data = f.read(chunk_len).decode('utf-8', errors='replace')
                result["json_size"] = chunk_len
                
                # Quick string-based extraction of bone names (node names)
                import re
                node_names = re.findall(r'"name"\s*:\s*"([^"]*)"', json_data)
                result["bone_names"] = list(set(node_names))
                
                # Check for skin/skeleton info
                if "skins" in json_data:
                    result["has_skins"] = True
                
                # Count mesh primitives (approximate vertex count via accessor refs)
                accessor_count = json_data.count('"accessor":')
                result["num_accessors"] = accessor_count
                
            elif chunk_type_raw == b'BIN ':
                # Binary data chunk - contains actual vertex data
                bin_data = f.read(chunk_len)
                result["bin_size"] = chunk_len
                
                # Parse bufferView references to find mesh geometry
                # Each primitive has attributes: POSITION, NORMAL, COLOR_0, etc.
                # We'll do a rough estimate by looking at accessor indices in the JSON
                pass
            else:
                f.seek(chunk_len, 1)  # Skip unknown chunk
        
        return result

def analyze_bones(bone_names):
    """Analyze bone hierarchy from extracted names."""
    key_bones = ['hips', 'spine', 'chest', 'upperChest', 'neck', 'head', 
                 'leftUpperArm', 'rightUpperArm', 
                 'leftHand', 'rightHand',
                 'leftUpLeg', 'rightUpLeg', 'leftLeg', 'rightLeg']
    
    found = {kb: kb in bone_names for kb in key_bones}
    all_present = sum(1 for v in found.values() if v)
    missing = [kb for kb, present in found.items() if not present]
    
    return found, all_present, missing

def main():
    # Only process actual .vrm files (not symlinks or toml files)
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
        
        bones_found, count_present, missing_bones = analyze_bones(data["bone_names"])
        
        results[model_name] = {
            "file_size": os.path.getsize(path),
            "num_bones": len(data["bone_names"]),
            "key_bones_present": count_present,
            "missing_bones": missing_bones,
            "has_skins": data.get("has_skins", False),
            "accessors": data.get("num_accessors", 0),
        }
        
        print(f"  Bones: {len(data['bone_names'])} unique names")
        print(f"  Key bones present: {count_present}/14")
        if missing_bones:
            print(f"  Missing key bones: {', '.join(missing_bones)}")
    
    # Print comparison summary
    print("\n\n" + "=" * 80)
    print("COMPARISON SUMMARY")
    print("=" * 80)
    print(f"\n{'Model':<25} {'Size':>10} {'Bones':>7} {'Key':>5} {'Skins':>6} {'Accessors':>10}")
    print("-" * 80)
    
    for model_name, data in sorted(results.items()):
        size_str = f"{data['file_size'] / 1e6:.1f}MB"
        skins = "yes" if data["has_skins"] else "no"
        print(f"{model_name:<25} {size_str:>10} {data['num_bones']:>7} "
              f"{data['key_bones_present']:>5}/14 {skins:>6} {data['accessors']:>10}")

if __name__ == "__main__":
    main()
