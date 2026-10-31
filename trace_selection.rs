// Trace avatar/scene/gender selection for key identities

fn fnv1a_64(data: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &byte in data {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn deterministic_index(identity: &str, salt: &str, n: usize) -> usize {
    let mut buf = identity.as_bytes().to_vec();
    buf.extend_from_slice(salt.as_bytes());
    (fnv1a_64(&buf) % n as u64) as usize
}

const IDENTITY_GENDERS: &[&str] = &["Female", "Female", "Male", "Male", "Neutral"];

const FEMALE_AVATARS: &[&str] = &[
    "vroid-female-base",
    "vroid-shino", 
    "vroid-darkness",
    "vroid-sample-d",
    "vroid-sample-e",
    "vroid-sample-f",
];

const MALE_AVATARS: &[&str] = &[
    "vroid-male-base",
    "vroid-sakurada",
];

const SCENE_CATALOG: &[&str] = &[
    "office.glb",
    "studio.glb",
    "lounge.glb",
    "home-office.glb",
    "server-room.glb",
];

fn main() {
    let identities = ["Kira", "Mathis", "Solomon", "Joaquin", "Lorcan"];
    
    for identity in &identities {
        // Gender selection
        let gender_idx = deterministic_index(identity, "gender", IDENTITY_GENDERS.len());
        let gender = IDENTITY_GENDERS[gender_idx];
        
        // Avatar selection (from correct gender pool)
        let avatar_pool: Vec<&str> = if gender == "Male" {
            MALE_AVATARS.to_vec()
        } else {
            FEMALE_AVATARS.to_vec()
        };
        let avatar_idx = deterministic_index(identity, "avatar", avatar_pool.len());
        let avatar = avatar_pool[avatar_idx];
        
        // Scene selection
        let scene_idx = deterministic_index(identity, "scene", SCENE_CATALOG.len());
        let scene = SCENE_CATALOG[scene_idx];
        
        println!("{:12} → gender={:8} avatar={:20} scene={}", identity, gender, avatar, scene);
    }
}
