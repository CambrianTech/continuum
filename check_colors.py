import struct, json, sys

def parse_glb(path):
    with open(path, 'rb') as f:
        header = f.read(12)
        magic, version, total_len = struct.unpack('<III', header)
        assert magic == 0x46546C67 and version == 2, f"Not GLB: {magic:x}"
        
        offset = 12
        json_data = None
        
        while offset < total_len - 8:
            chunk_header = f.read(8)
            if len(chunk_header) < 8:
                break
            chunk_len, chunk_type = struct.unpack('<II', chunk_header)
            
            chunk_data = f.read(chunk_len)
            if len(chunk_data) < chunk_len:
                print(f"  WARN: truncated chunk at offset {offset}", file=sys.stderr)
                break
            
            if chunk_type == 0x4E6f534a:  # JSON
                try:
                    json_data = json.loads(chunk_data.decode('utf-8'))
                except Exception as e:
                    print(f"  WARN: JSON parse error: {e}", file=sys.stderr)
            
            offset += 8 + chunk_len
        
        return json_data

def analyze_glb(path):
    data = parse_glb(path)
    
    if not data:
        print(f"  FAILED to parse JSON from {path}")
        return
    
    accessors = data.get('accessors', [])
    meshes = data.get('meshes', [])
    buffer_views = data.get('bufferViews', [])
    color_found = False
    
    for mi, mesh in enumerate(meshes):
        if not isinstance(mesh, dict):
            continue
        prims = mesh.get('primitives', [])
        for pi, prim in enumerate(prims):
            attrs = prim.get('attributes', {})
            
            if 'COLOR_0' not in attrs:
                continue
            
            color_found = True
            color_idx = attrs['COLOR_0']
            
            acc = accessors[color_idx] if isinstance(color_idx, int) and color_idx < len(accessors) else None
            if not acc:
                print(f"  Mesh {mi} Prim {pi}: COLOR_0 accessor index {color_idx} out of range")
                continue
            
            count = acc.get('count', 0)
            comp_type = acc.get('componentType', 0)
            typ = acc.get('type', '?')
            
            comp_bytes_map = {5120: 1, 5121: 1, 5122: 2, 5123: 2, 5124: 4, 5125: 4, 5126: 4}
            bpc = comp_bytes_map.get(comp_type, 4)
            ncomp = len(typ) if typ in ('VEC2', 'VEC3', 'VEC4') else 1
            stride = bpc * ncomp
            
            print(f"    Mesh {mi} Prim {pi}: COLOR_0 count={count}, compType={comp_type}, type={typ}")
            print(f"      stride={stride}b")
    
    if not color_found:
        print(f"  No COLOR_0 attribute found in any primitive")

for name in ['vroid-sakurada', 'vroid-male-base', 'vroid-female-base', 'vroid-darkness']:
    path = f'models/avatars/{name}.vrm'
    print(f"=== {name} ===")
    try:
        analyze_glb(path)
    except Exception as e:
        import traceback
        print(f"  ERROR: {e}")
        traceback.print_exc()
    print()
