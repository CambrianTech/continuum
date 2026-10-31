#!/usr/bin/env python3
"""Parse VRM files (GLB binary glTF) and inspect skeleton/bone hierarchy."""

import json
import struct
import sys
from pathlib import Path

VRM_DIR = Path("/Users/joel/.airc/worktrees/041d008b/models/avatars")


def parse_glb(path: Path):
    """Parse a GLB file and return the JSON chunk as a dict."""
    data = path.read_bytes()

    # Header: magic (4) + version (4) + length (4) = 12 bytes
    if len(data) < 12 or data[:4] != b'glTF':
        raise ValueError(f"Not a GLB file: {path}")

    version = struct.unpack_from('<I', data, 4)[0]
    total_length = struct.unpack_from('<I', data, 8)[0]

    if version != 2:
        raise ValueError(f"Expected glTF 2.0, got version {version}")

    offset = 12
    while offset < total_length:
        chunk_length = struct.unpack_from('<I', data, offset)[0]
        chunk_type = data[offset+4:offset+8].decode('ascii')
        chunk_data = data[offset+8:offset+8+chunk_length]

        if chunk_type == "JSON":
            return json.loads(chunk_data.decode('utf-8'))

        offset += 8 + chunk_length

    raise ValueError("No JSON chunk found in GLB")


def parse_vrm(path: Path):
    """Parse a VRM file and return the glTF data."""
    gltf = parse_glb(path)

    info = {
        "version": gltf.get("extensions", {}).get("VRM", {}).get("specificationVersion", "unknown"),
        "meta_count": len(gltf.get("extensions", {}).get("VRM10_Meta", {})),
    }

    # Examine nodes/skeleton
    node_list = gltf.get("nodes", [])

    # Build bone hierarchy from nodes
    bones = []
    for i, node in enumerate(node_list):
        name = node.get("name", f"node_{i}")
        skin_index = node.get("skin")
        children = node.get("children", [])
        matrix = node.get("matrix", [1,0,0,0, 0,1,0,0, 0,0,1,0, 0,0,0,1])

        # Extract position from transform matrix (column-major)
        pos = (matrix[3], matrix[7], matrix[11])

        bones.append({
            "index": i,
            "name": name,
            "skin": skin_index is not None,
            "child_count": len(children),
            "children_names": [node_list[c].get("name", f"node_{c}") for c in children if c < len(node_list)],
            "position": pos,
        })

    # Find root bone (no parent) and count total bones
    child_set = set()
    for b in bones:
        child_set.update(b["children_names"])

    roots = [b for b in bones if b["name"] not in child_set]

    info["total_bones"] = len(bones)
    info["root_count"] = len(roots)
    info["roots"] = [r["name"] for r in roots]
    info["has_skin"] = any(b["skin"] for b in bones)

    # Find the deepest bone (furthest from root) by tracing parent chain
    def get_depth(name):
        depth = 0
        current = name
        visited = set()
        while current and current not in visited:
            visited.add(current)
            depth += 1
            for b in bones:
                if current in b["children_names"]:
                    current = b["name"]
                    break
            else:
                break
        return depth

    max_depth = 0
    deepest_name = ""
    for r in roots:
        d = get_depth(r["name"])
        if d > max_depth:
            max_depth = d
            deepest_name = r["name"]

    info["max_depth"] = max_depth
    info["deepest_bone"] = deepest_name

    # Check for key body parts
    known_parts = ["hips", "spine", "chest", "neck", "head",
                   "l_upper_arm", "r_upper_arm", "l_forearm", "r_forearm",
                   "l_hand", "r_hand", "l_upper_leg", "r_upper_leg",
                   "l_lower_leg", "r_lower_leg", "l_foot", "r_foot"]
    found_parts = []
    bone_names_lower = [b["name"].lower() for b in bones]
    for part in known_parts:
        if any(part.lower() in name for name in bone_names_lower):
            found_parts.append(part)

    info["body_parts_found"] = found_parts
    return info, bones


def main():
    vrm_files = [
        "vroid-male-base.vrm",
        "vroid-sakurada.vrm",
        "vroid-female-base.vrm",
        "vroid-sample-e.vrm",
        "vroid-shino.vrm",
    ]

    for vrm_file in vrm_files:
        path = VRM_DIR / vrm_file
        if not path.exists():
            print(f"SKIP: {vrm_file} (not found)")
            continue

        try:
            info, bones = parse_vrm(path)
            size_kb = path.stat().st_size / 1024

            print(f"\n{'='*60}")
            print(f"{vrm_file} ({size_kb:.0f} KB)")
            print(f"{'='*60}")
            print(f"  Version:        {info['version']}")
            print(f"  Total bones:    {info['total_bones']}")
            print(f"  Root count:     {info['root_count']}")
            print(f"  Roots:          {', '.join(info['roots'])}")
            print(f"  Max depth:      {info['max_depth']}")
            print(f"  Deepest bone:   {info['deepest_bone']}")
            print(f"  Has skin mesh:  {info['has_skin']}")
            print(f"  Body parts:     {', '.join(info['body_parts_found'])}")

            # Show top-level children of each root
            for r in info['roots'][:3]:  # limit output
                bone = next((b for b in bones if b["name"] == r), None)
                if bone and bone["children_names"]:
                    print(f"  Root '{r}' children: {', '.join(bone['children_names'][:10])}")

            # Print all bone names grouped by root
            child_set = set()
            for b in bones:
                child_set.update(b["children_names"])
            roots_list = [b for b in bones if b["name"] not in child_set]
            
            print(f"  Bone tree:")
            for root_bone in roots_list[:5]:
                def print_tree(name, indent=2):
                    bone_obj = next((b for b in bones if b["name"] == name), None)
                    if not bone_obj:
                        return
                    children = [c for c in bone_obj["children_names"] if c]
                    if children:
                        print(f"{' '*indent}{name} -> {', '.join(children[:8])}")
                    else:
                        print(f"{' '*indent}{name}")
                print_tree(root_bone["name"], 2)
            
        except Exception as e:
            print(f"\nERROR parsing {vrm_file}: {e}")


if __name__ == "__main__":
    main()
