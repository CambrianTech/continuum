import hashlib

def deterministic_pick(identity, catalog, salt):
    """Replicate the Rust hash function from avatar/hash.rs"""
    key = f"{identity}{salt}"
    h = int(hashlib.sha256(key.encode()).hexdigest(), 16)
    return h % len(catalog)

identities = ["kira", "mathis", "solomon", "joaquin", "lorcan"]
scenes = ["office", "studio", "lounge", "home-office", "server-room"]
avatars_vrm = [
    ("vroid-sakurada-m-0", "Male VRID Sakurada 0"),
    ("vroid-sakurada-f-0", "Female VRID Sakurada 0"), 
    ("male-base-0", "Male Base 0"),
    ("female-base-0", "Female Base 0"),
]

print("=== Scene Assignment for Each Identity ===\n")
for identity in identities:
    pick = deterministic_pick(identity, scenes, "scene")
    print(f"{identity:>10} -> scene #{pick+1} ({scenes[pick]})")

print("\n=== Avatar Selection for Each Identity ===\n")
for identity in identities:
    pick = deterministic_pick(identity, avatars_vrm, "avatar")
    print(f"{identity:>10} -> {avatars_vrm[pick][0]:25s} ({avatars_vrm[pick][1]})")

print("\n=== Room Background Color for Each Identity ===\n")
for identity in identities:
    key = f"{identity}room"
    h = int(hashlib.sha256(key.encode()).hexdigest(), 16)
    hue = h % 360
    sat = 0.07 + (h % 10) * 0.009
    light = 0.12 + (h % 8) * 0.0075
    print(f"{identity:>10} -> HSL({hue:3d}, {sat:.3f}, {light:.3f})")

print("\n=== Key Insight ===\n")
# Check which identities share scenes and which get which avatar type
scene_map = {}
for identity in identities:
    pick = deterministic_pick(identity, scenes, "scene")
    scene_map.setdefault(scenes[pick], []).append(identity)

print("Shared scene assignments:")
for scene, ids in sorted(scene_map.items()):
    print(f"  {scene}: {', '.join(ids)}")
