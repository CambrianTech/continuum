#!/usr/bin/env python3
"""Deep VRM/ glTF 2.0 inspection: compare male vs female model bone hierarchy."""
import os, struct, json

AVATAR_DIR = '/Users/joel/.airc/worktrees/041d008b/models/avatars'

def parse_gltf_binary(path):
    """Parse glTF 2.0 binary format (VRM 2.0 uses this)."""
    size = os.path.getsize(path)
    
    with open(path, 'rb') as f:
        magic = f.read(4)
        if magic != b'glTF':
            return {'error': f'Not glTF file (magic={magic})', 'path': path}
        
        version = struct.unpack('<I', f.read(4))[0]
        total_length = struct.unpack('<I', f.read(4))[0]
        
        json_data = None
        bin_data = None
        
        offset = 12  # After magic + version + total_length
        
        while offset < total_length:
            chunk_length = struct.unpack('<I', f.read(4))[0]
            chunk_type = f.read(4).decode('ascii')
            chunk_data = f.read(chunk_length)
            
            if chunk_type == 'JSON':
                json_data = chunk_data
            elif chunk_type == 'BIN ':
                bin_data = chunk_data
            
            offset += 4 + 4 + chunk_length  # length + type + data
    
    if not json_data:
        return {'error': 'No JSON chunk found', 'path': path}
    
    try:
        gltf = json.loads(json_data.decode('utf-8'))
    except Exception as e:
        return {'error': f'JSON parse error: {e}', 'path': path}
    
    return {
        'size': size,
        'version': version,
        'gltf': gltf,
        'bin_data': bin_data,
        'path': path
    }

def extract_bones(gltf):
    """Extract humanoid bone mappings from VRM extension or glTF nodes."""
    bones = {}
    
    # Check for VRM 2.0 secondary animation / humanoid structure
    extensions = gltf.get('extensions', {})
    vrm_ext = extensions.get('VRM', {}) or extensions.get('THIRD_PARTY_LICENSE', {})
    
    # VRM 2.0 uses `secondaryAnimation` and bone mapping via nodes
    # In VRM 2.0, humanoid bones are defined in the extension
    if 'humanoid' in vrm_ext:
        for hb in vrm_ext['humanoid'].get('humanBones', []):
            bones[hb.get('bone', '?')] = hb.get('node')
    
    # Also check glTF nodes directly - VRM 2.0 may use node names as bone identifiers
    nodes = gltf.get('nodes', [])
    for i, node in enumerate(nodes):
        name = node.get('name', f'node_{i}')
        skin = node.get('skin')
        if skin is not None:
            # This node has a skin (mesh deformation) — it's part of the character
            bones[f'{name}_mesh'] = i
    
    return bones

def extract_mesh_info(gltf):
    """Extract mesh vertex/triangle counts."""
    meshes = gltf.get('meshes', [])
    total_verts = 0
    total_tris = 0
    mesh_details = []
    
    for i, mesh in enumerate(meshes):
        prim = mesh.get('primitives', [{}])[0]
        positions = prim.get('attributes', {}).get('POSITION')
        indices = prim.get('indices')
        
        if positions is not None:
            # Accessor index - we'd need the accessors array to get actual count
            # For now just note it exists
            pass
        
        if indices is not None:
            tri_count = 0  # Would need accessor data
        else:
            tri_count = 0  # No indexed primitives
        
        name = mesh.get('name', f'mesh_{i}')
        mesh_details.append({'name': name, 'index': i})
        total_verts += 1  # Placeholder
    
    return {
        'mesh_count': len(meshes),
        'total_verts': total_verts,
        'total_tris': total_tris,
        'details': mesh_details
    }

def analyze_path(path):
    """Analyze a single VRM/glTF file."""
    result = parse_gltf_binary(path)
    
    if 'error' in result:
        return {**result, 'bone_count': 0, 'vertex_count': 0}
    
    gltf = result['gltf']
    bones = extract_bones(gltf)
    mesh_info = extract_mesh_info(gltf)
    
    # Count all nodes (including non-mesh ones that might be bones)
    nodes = gltf.get('nodes', [])
    
    return {
        'path': result['path'],
        'size': result['size'],
        'version': result.get('version', 0),
        'bone_count': len(bones),
        'vertex_count': mesh_info['total_verts'],
        'triangle_count': mesh_info['total_tris'],
        'mesh_count': mesh_info['mesh_count'],
        'node_count': len(nodes),
        'bones': bones,
        'error': None
    }

def main():
    results = []
    
    for fname in sorted(os.listdir(AVATAR_DIR)):
        if not fname.endswith('.vrm'):
            continue
        
        path = os.path.join(AVATAR_DIR, fname)
        print(f"\n{'='*60}")
        print(f"Analyzing: {fname}")
        print('='*60)
        
        result = analyze_path(path)
        
        if result.get('error'):
            print(f"  ERROR: {result['error']}")
            continue
        
        results.append(result)
        
        # Print bone info
        bones = result.get('bones', {})
        print(f"\n  Nodes: {result['node_count']}, Meshes: {result['mesh_count']}")
        print(f"  Bones found: {len(bones)}")
        
        if bones:
            print("\n  Bone mappings:")
            for bone_name, node_idx in sorted(bones.items()):
                print(f"    {bone_name:30s} -> node {node_idx}")
        
        # Print mesh details
        nodes = []
        gltf = parse_gltf_binary(path).get('gltf', {})
        if gltf:
            for i, node in enumerate(gltf.get('nodes', [])):
                name = node.get('name', f'node_{i}')
                children = node.get('children', [])
                has_skin = 'skin' in node
                print(f"  Node {i}: {name:40s} skin={has_skin} children={len(children)}")

    # Summary comparison
    print("\n\n" + "="*80)
    print("COMPARISON SUMMARY")
    print("="*80)
    print(f"{'Model':<35s} {'Size':>8s} {'Nodes':>6s} {'Meshes':>7s} {'Bones':>6s}")
    print("-"*80)
    
    for r in sorted(results, key=lambda x: -x['size']):
        fname = os.path.basename(r['path'])
        print(f"{fname:<35s} {r['size']/1e6:>7.1f}MB {r['node_count']:>6d} {r['mesh_count']:>7d} {r['bone_count']:>6d}")

if __name__ == '__main__':
    main()
