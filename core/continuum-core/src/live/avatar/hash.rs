//! Deterministic hashing — FNV-1a and index/pick utilities.
//!
//! Used throughout avatar selection to map persona identities and voice names
//! to stable, reproducible choices from catalogs and trait arrays.

/// FNV-1a 64-bit hash — fast, excellent distribution for short strings and UUIDs.
pub fn fnv1a_hash(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325; // FNV offset basis
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3); // FNV prime
    }
    hash
}

/// Deterministic index: hash a unique ID into the range [0, len).
/// Different salt values produce independent indices from the same ID.
pub fn deterministic_index(unique_id: &str, len: usize, salt: &str) -> usize {
    let mut hash = fnv1a_hash(unique_id.as_bytes());
    // Mix in salt to decorrelate across different trait dimensions
    hash = hash.wrapping_mul(0x100000001b3);
    hash ^= fnv1a_hash(salt.as_bytes());
    hash as usize % len
}

/// Deterministically pick one element from a slice using a stable hash of a unique ID.
/// Same ID + same salt + same slice always returns the same element.
///
/// The `salt` parameter decorrelates picks across different trait arrays so that
/// e.g. gender and avatar model aren't locked to the same hash bucket:
///   `deterministic_pick(id, &genders, "gender")`
///   `deterministic_pick(id, &avatars, "avatar")`
///   `deterministic_pick(id, &voices, "voice")`
pub fn deterministic_pick<'a, T>(unique_id: &str, options: &'a [T], salt: &str) -> &'a T {
    assert!(!options.is_empty(), "Cannot pick from empty slice");
    &options[deterministic_index(unique_id, options.len(), salt)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deterministic_pick_stable() {
        let options = ["a", "b", "c", "d", "e"];
        let pick1 = deterministic_pick("user-123", &options, "test");
        let pick2 = deterministic_pick("user-123", &options, "test");
        assert_eq!(pick1, pick2);
    }

    #[test]
    fn test_deterministic_pick_different_salt() {
        let options = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        let pick_a = deterministic_pick("user-xyz", &options, "salt-a");
        let pick_b = deterministic_pick("user-xyz", &options, "salt-b");
        assert!(options.contains(pick_a));
        assert!(options.contains(pick_b));
    }
}
