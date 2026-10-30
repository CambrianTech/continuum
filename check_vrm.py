import struct, json

def read_vrm_json(path):
    with open(path, 'rb') as f:
        header = f.read(4)
        if header == b'vrm\x00':
            # VRM 0.x - plain text format
            ver_bytes = bytearray()
            while True:
                b = f.read(1)
                if b == b'\x00' or len(b) == 0:
                    break
                ver_bytes.extend(b)
            
            rest = f.read()
            
            # Find the end of JSON by brace counting
            text = rest.decode('utf-8', errors='ignore')
            brace_count = 0
            json_end = -1
            in_string = False
            escape_next = False
            for i, c in enumerate(text):
                if escape_next:
                    escape_next = False
                    continue
                if c == '\\':
                    escape_next = True
                    continue
                if c == '"':
                    in_string = not in_string
                    continue
                if in_string:
                    continue
                if c == '{':
                    brace_count += 1
                elif c == '}':
                    brace_count -= 1
                    if brace_count == 0:
                        json_end = i + 1
                        break
            
            if json_end > 0:
                try:
                    data = json.loads(text[:json_end])
                    return data, "VRM 0.x (with trailing binary)"
                except Exception as e:
                    pass
            
            # Try parsing the whole thing as JSON
            try:
                data = json.loads(text)
                return data, "VRM 0.x (pure JSON)"
            except:
                pass
        
        elif header == b'glTF':
            # VRM 1.0 - GLB container
            f.seek(0)
            magic, version, total_len = struct.unpack('<III', f.read(12))
            
            chunk_offset = 12
            while chunk_offset < total_len - 8:
                chunk_data = f.read(8)
                if len(chunk_data) < 8:
                    break
                chunk_len, chunk_type = struct.unpack('<II', chunk_data)
                
                if chunk_type == 0x6E6F534A:  # JSON
                    json_bytes = f.read(chunk_len)
                    return json.loads(json_bytes), "VRM 1.0 (GLB)"
                
                elif chunk_type == 0x00474C42:  # BIN
                    pass
                
                f.seek(chunk_offset + 8 + chunk_len, 0)
                chunk_offset += 8 + chunk_len
            
            return None, "Not a GLB or no JSON"
        
        else:
            return None, f"Unknown format (header={header!r})"

for name in ['vroid-sakurada', 'vroid-shino', 'vroid-male-base']:
    path = f'models/avatars/{name}.vrm'
    data, fmt = read_vrm_json(path)
    
    print(f"=== {name} (format: {fmt}) ===")
    if not data:
        print("  Could not parse!")
        continue
    
    vrm = (data.get('extensions') or {}).get('VRM', {}) or {}
    
    bone_names = []
    for b in (vrm.get('bones') or [])[:50]:
        if isinstance(b, dict):
            bn = b.get('bone')
            if isinstance(bn, dict) and 'name' in bn:
                bone_names.append(bn['name'])
            elif isinstance(bn, str):
                bone_names.append(bn)
    
    human_bones = {}
    for hb in (vrm.get('humanBone') or []):
        if isinstance(hb, dict):
            bone_type = hb.get('name', '?')
            bone_ref = hb.get('bone')
            idx = None
            if isinstance(bone_ref, int):
                idx = bone_ref
            elif isinstance(bone_ref, dict) and 'index' in bone_ref:
                idx = bone_ref['index']
            human_bones[bone_type] = bone_names[idx] if idx is not None else '?'
    
    print(f"  Total bones: {len(bone_names)}")
    print(f"  Bone names (first 40): {bone_names[:40]}")
    print()
    print(f"  humanBone assignments:")
    for k, v in sorted(human_bones.items()):
        print(f"    {k}: {v}")
    
    # Check if body bones exist
    body_bones = ['hips', 'spine', 'chest', 'upperChest', 
                  'leftUpperArm', 'rightUpperArm',
                  'leftUpperLeg', 'rightUpperLeg']
    has_body = any(b in bone_names for b in body_bones)
    print(f"  Has body bones: {has_body}")
    
    # Check mesh info from the glTF root
    meshes = data.get('meshes', [])
    total_verts = 0
    accessors = data.get('accessors', [])
    for mi, mesh in enumerate(meshes):
        if isinstance(mesh, dict):
            prims = mesh.get('primitives', [])
            if isinstance(prims, list) and len(prims) > 0:
                accs = prims[0].get('attributes', {})
                pos_acc = accs.get('POSITION')
                if isinstance(pos_acc, int):
                    try:
                        count = accessors[pos_acc]['count']
                        total_verts += count
                    except (KeyError, IndexError):
                        pass
    
    print(f"  Total meshes: {len(meshes)}")
    print(f"  Estimated total vertices: {total_verts}")
    print()
