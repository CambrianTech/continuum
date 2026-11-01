#!/usr/bin/env python3
"""Proper glTF/VRM inspector — parse the JSON+bin header to compare male vs female models."""
import json, struct, os, sys, zlib

def parse_gltf_header(path):
    """Parse glTF 2.0 file (JSON header + optional binary chunk). Returns (json_doc, bin_bytes_or_None)."""
    with open(path, 'rb') as f:
        magic = f.read(4)
        if magic != b'glTF':
            return None, None
        version = struct.unpack('<I', f.read(4))[0]
        file_length = struct.unpack('<I', f.read(4))[0]
        
        # Read chunk 1 (JSON)
        chunk1_length = struct.unpack('<I', f.read(4))[0]
        chunk1_type = f.read(4)
        if chunk1_type != b'JSON':
            return None, None
        json_bytes = f.read(chunk1_length)
        doc = json.loads(json_bytes.decode('utf-8'))
        
        # Read chunk 2 (binary, optional — VRM usually has one BIN chunk)
        bin_data = None
        remaining = file_length - f.tell()
        if remaining > 0:
            bin_length = struct.unpack('<I', f.read(4))[0]
            bin_type = f.read(4)
            if bin_type == b'BIN ':
                bin_data = f.read(bin_length)
        
        return doc, bin_data

def get_bone_names(doc):
    """Extract humanoid bone names from VRM's humanoid.humanBones."""
    humanoid = doc.get('extensions', {}).get('VRM', {}).get('humanoid', {})
    bones = {}
    for entry in humanoid.get('humanBones', []):
        bones[entry['bone']] = entry.get('node', -1)
    return bones

def get_mesh_info(doc, bin_data):
    """Extract per-mesh vertex/triangle counts."""
    meshes = doc.get('meshes', [])
    total_verts = 0
    total_tris = 0
    mesh_names = []
    
    for i, mesh in enumerate(meshes):
        name = mesh.get('name', f'mesh_{i}')
        primitives = mesh.get('primitives', [])
        # Each primitive has 'attributes' with POSITION, etc.
        for prim in primitives:
            attrs = prim.get('attributes', {})
            pos_idx = attrs.get('POSITION')
            if pos_idx is not None:
                try:
                    accessor = doc['accessors'][pos_idx]
                    count = accessor['count']
                    total_verts += count
                    # Indices or vertices
                    idx_accessor = prim.get('indices')
                    if idx_accessor is not None:
                        tri_count = doc['accessors'][idx_accessor]['count'] // 3
                        total_tris += tri_count
                    else:
                        # TRIANGLE_LIST mode
                        total_tris += count // 3
                except (IndexError, KeyError):
                    pass
        
        mesh_names.append(name)
    
    return total_verts, total_tris, mesh_names

def get_texture_info(doc):
    """Check if model has textures/materials."""
    images = len(doc.get('images', []))
    samplers = len(doc.get('samplers', []))
    textures = len(doc.get('textures', []))
    materials = len(doc.get('materials', []))
    return images, samplers, textures, materials

def get_skeleton_depth(doc):
    """Find the deepest node in the scene graph."""
    nodes = doc.get('nodes', [])
    if not nodes:
        return 0
    
    def depth(node_idx, visited=None):
        if visited is None:
            visited = set()
        if node_idx in visited:
            return 0
        visited.add(node_idx)
        node = nodes[node_idx]
        children = node.get('children', [])
        if not children:
            return 1
        return 1 + max(depth(c, visited.copy()) for c in children)
    
    # Scene 0's root nodes
    scene = doc.get('scenes', [{}])[0]
    root_nodes = scene.get('nodes', [])
    if not root_nodes:
        return 0
    return max(depth(r) for r in root_nodes)

def main():
    avatar_dir = 'models/avatars'
    results = []
    
    for f in sorted(os.listdir(avatar_dir)):
        if not f.endswith('.vrm'):
            continue
        path = os.path.join(avatar_dir, f)
        doc, bin_data = parse_gltf_header(path)
        if doc is None:
            print(f"SKIP {f}: not a valid glTF file", file=sys.stderr)
            continue
        
        size = os.path.getsize(path)
        bones = get_bone_names(doc)
        verts, tris, mesh_names = get_mesh_info(doc, bin_data)
        img, samp, tex, mat = get_texture_info(doc)
        skeleton_depth = get_skeleton_depth(doc)
        
        # Check required humanoid bones
        required = ['hips', 'spine', 'chest', 'neck', 'head',
                    'leftUpperArm', 'rightUpperArm',
                    'leftLowerArm', 'rightLowerArm',
                    'leftUpperLeg', 'rightUpperLeg',
                    'leftLowerLeg', 'rightLowerLeg']
        present = [b for b in required if b in bones]
        missing = [b for b in required if b not in bones]
        
        # Check for body-relevant bones beyond head
        extra_body_bones = ['upperArm', 'lowerArm', 'upperLeg', 'lowerLeg',
                           'forehead', 'nose', 'jaw', 'leftEye', 'rightEye',
                           'leftShoulder', 'rightShoulder',
                           'leftHand', 'rightHand',
                           'leftToes', 'rightToes',
                           'leftThumbMetacarpal', 'leftThumbProximal', 'leftThumbDistal']
        extra_present = [b for b in extra_body_bones if b in bones]
        
        result = {
            'name': f, 'size_mb': size / 1e6,
            'bones': len(bones), 'bone_names': sorted(bones.keys()),
            'present_required': len(present), 'missing_required': missing,
            'vertices': verts, 'triangles': tris,
            'mesh_count': len(mesh_names), 'mesh_names': mesh_names[:5],  # first 5
            'textures': (img, tex, mat),
            'skeleton_depth': skeleton_depth,
            'extra_body_bones': extra_present,
        }
        results.append(result)
    
    print("\n" + "=" * 100)
    print("VRM MODEL COMPARISON")
    print("=" * 100)
    
    for r in sorted(results, key=lambda x: -x['size_mb']):
        gender = "MALE" if 'male' in r['name'] or 'sakurada' in r['name'] else "FEMALE"
        print(f"\n[{gender}] {r['name']} ({r['size_mb']:.1f} MB)")
        print(f"  Bones: {r['bones']} total, {r['present_required']}/{len(required)} required present")
        if r['missing_required']:
            print(f"  MISSING REQUIRED: {r['missing_required']}")
        print(f"  Vertices: {r['vertices']:,} | Triangles: {r['triangles']:,}")
        print(f"  Meshes: {r['mesh_count']} (sample: {', '.join(r['mesh_names'])})")
        print(f"  Skeleton depth: {r['skeleton_depth']}")
        if r['extra_body_bones']:
            print(f"  Extra body bones: {len(r['extra_body_bones'])} ({', '.join(r['extra_body_bones'][:10])})")
    
    # Summary table
    print("\n" + "=" * 100)
    print("SUMMARY TABLE")
    print("=" * 100)
    headers = ["Name", "Gender", "Size(MB)", "Bones", "ReqPresent", "Vertices", "Tris", "Depth"]
    fmt = "{:<25} {:<8} {:>9} {:>5} {:>9} {:>9} {:>8} {:>6}"
    print(fmt.format(*headers))
    for r in sorted(results, key=lambda x: -x['size_mb']):
        gender = "M" if 'male' in r['name'] or 'sakurada' in r['name'] else "F"
        print(fmt.format(
            r['name'][:24], gender, 
            f"{r['size_mb']:.1f}", r['bones'],
            f"{r['present_required']}/{len(required)}",
            f"{r['vertices']:,}", f"{r['triangles']:,}", r['skeleton_depth']
        ))

if __name__ == '__main__':
    required = ['hips', 'spine', 'chest', 'neck', 'head',
                'leftUpperArm', 'rightUpperArm',
                'leftLowerArm', 'rightLowerArm',
                'leftUpperLeg', 'rightUpperLeg',
                'leftLowerLeg', 'rightLowerLeg']
