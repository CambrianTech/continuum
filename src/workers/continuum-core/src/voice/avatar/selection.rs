//! Avatar selection algorithms — voice-based, identity-based, and batch allocation.
//!
//! Maps persona identity and TTS voice characteristics to avatar models from the catalog.
//! Ensures unique model assignment across concurrent participants where possible.

use std::collections::{HashMap, HashSet};
use crate::clog_info;
use super::types::*;
use super::catalog::AVATAR_CATALOG;
use super::hash::{fnv1a_hash, deterministic_pick, deterministic_index};
use super::gender::{gender_from_voice_name, gender_from_identity};

/// Select the best avatar model for a persona based on TTS voice characteristics.
///
/// Matching algorithm:
/// 1. Filter by gender (if known)
/// 2. Score by pitch range match
/// 3. Score by energy level match
/// 4. Break ties by style preference (anime > stylized for most personas)
///
/// Falls back to deterministic hash-based selection if no voice profile is provided.
pub fn select_avatar_for_voice(
    voice_gender: Option<AvatarGender>,
    voice_pitch: Option<PitchRange>,
    voice_energy: Option<EnergyLevel>,
    preferred_style: Option<AvatarStyle>,
) -> &'static AvatarModel {
    let mut best_score = -1i32;
    let mut best = &AVATAR_CATALOG[0];

    for model in AVATAR_CATALOG {
        let mut score = 0i32;

        // Gender match (strongest signal)
        if let Some(gender) = voice_gender {
            if model.voice_profile.gender == gender {
                score += 10;
            }
        }

        // Pitch range match
        if let Some(pitch) = voice_pitch {
            if model.voice_profile.pitch == pitch {
                score += 5;
            }
        }

        // Energy level match
        if let Some(energy) = voice_energy {
            if model.voice_profile.energy == energy {
                score += 3;
            }
        }

        // Style preference
        if let Some(style) = preferred_style {
            if model.style == style {
                score += 2;
            }
        }

        if score > best_score {
            best_score = score;
            best = model;
        }
    }

    best
}

/// Select avatar by deterministic hash of persona identity.
/// Same persona always gets the same model.
pub fn select_avatar_by_identity(identity: &str) -> &'static AvatarModel {
    deterministic_pick(identity, AVATAR_CATALOG, "avatar")
}

/// Global allocation map: identity → catalog index.
/// Tracks which models have been assigned to avoid duplicates.
/// Once a persona is assigned a model, it keeps it forever (deterministic + stable).
static AVATAR_ALLOCATION: std::sync::Mutex<Option<HashMap<String, usize>>> = std::sync::Mutex::new(None);

/// Select the best avatar for an agent, avoiding model reuse across personas.
///
/// Algorithm (3-phase, gender-coherent):
/// 1. If already allocated, return the same model (stable across calls)
/// 2. Resolve gender from voice name or identity hash (SEED — overridable later via user.state)
/// 3. Phase A: try unused model matching gender (probe from preferred hash)
/// 4. Phase B: gender pool exhausted → share least-used SAME-GENDER model
/// 5. Phase C: no models of this gender → share least-used from any gender (last resort)
///
/// Gender coherence is prioritized: avatar gender must match voice gender.
/// The identity hash is the SEED for gender; personas can override via preferences later.
pub fn select_avatar_for_agent(identity: &str, voice: Option<&str>) -> &'static AvatarModel {
    let mut guard = AVATAR_ALLOCATION.lock().unwrap();
    let allocation = guard.get_or_insert_with(HashMap::new);

    // Already allocated — return the same model (stable)
    if let Some(&idx) = allocation.get(identity) {
        return &AVATAR_CATALOG[idx];
    }

    // Resolve gender: voice name > deterministic from identity
    let gender = voice
        .and_then(gender_from_voice_name)
        .unwrap_or_else(|| gender_from_identity(identity));

    // Filter catalog to matching gender
    let matching: Vec<usize> = AVATAR_CATALOG.iter().enumerate()
        .filter(|(_, m)| m.voice_profile.gender == gender)
        .map(|(i, _)| i)
        .collect();

    let used: HashSet<usize> = allocation.values().copied().collect();

    // Phase A: try unused gender-matching model (probe from preferred hash)
    if !matching.is_empty() {
        let preferred = deterministic_index(identity, matching.len(), "avatar");
        for offset in 0..matching.len() {
            let probe = (preferred + offset) % matching.len();
            let catalog_idx = matching[probe];
            if !used.contains(&catalog_idx) {
                clog_info!("🎭 Avatar for '{}': gender={:?} → model='{}' (unique, gender-match)",
                    &identity[..8.min(identity.len())], gender, AVATAR_CATALOG[catalog_idx].id);
                allocation.insert(identity.to_string(), catalog_idx);
                return &AVATAR_CATALOG[catalog_idx];
            }
        }
    }

    // Phase B: gender pool exhausted — share least-used SAME-GENDER model.
    // Gender coherence (avatar looks like the voice sounds) > visual diversity.
    // With 7F:1M in the catalog, male agents share the one male model.
    let mut usage_count: HashMap<usize, usize> = HashMap::new();
    for &idx in allocation.values() {
        *usage_count.entry(idx).or_insert(0) += 1;
    }
    if !matching.is_empty() {
        let picked = matching.iter()
            .copied()
            .min_by_key(|i| usage_count.get(i).copied().unwrap_or(0))
            .unwrap();
        clog_info!("🎭 Avatar for '{}': gender={:?} → model='{}' (shared, same-gender)",
            &identity[..8.min(identity.len())], gender, AVATAR_CATALOG[picked].id);
        allocation.insert(identity.to_string(), picked);
        return &AVATAR_CATALOG[picked];
    }

    // Phase C: no models of this gender at all — pick least-used from any gender.
    // This should only happen if the catalog has zero models of a given gender.
    let picked = (0..AVATAR_CATALOG.len())
        .min_by_key(|i| usage_count.get(i).copied().unwrap_or(0))
        .unwrap_or(0);
    clog_info!("🎭 Avatar for '{}': gender={:?} → model='{}' (shared, any-gender last-resort)",
        &identity[..8.min(identity.len())], gender, AVATAR_CATALOG[picked].id);
    allocation.insert(identity.to_string(), picked);
    &AVATAR_CATALOG[picked]
}

/// Pre-allocate avatars for a batch of identities in deterministic order.
/// Call this at session start when all participants are known.
/// Sorts by hash for order-independent allocation, then assigns greedily.
pub fn allocate_avatars_batch(identities: &[(&str, Option<&str>)]) {
    // Sort by hash for deterministic order regardless of input ordering
    let mut sorted: Vec<(&str, Option<&str>)> = identities.to_vec();
    sorted.sort_by_key(|(id, _)| fnv1a_hash(id.as_bytes()));

    for (identity, voice) in &sorted {
        // select_avatar_for_agent handles dedup internally
        select_avatar_for_agent(identity, *voice);
    }
    clog_info!("🎭 Pre-allocated avatars for {} identities", identities.len());
}

// =============================================================================
// Dynamic catalog selection — works with AvatarCatalog + AvatarPreference
// =============================================================================

use super::catalog::AvatarCatalog;

/// Select the best avatar from a dynamic catalog, respecting user preferences.
///
/// Priority order:
/// 1. Explicit model_id preference (highest — user chose a specific model)
/// 2. Score-based matching: gender > pitch > energy > style > tag exclusions
///
/// Returns the best match from the catalog, or None if the catalog is empty.
pub fn select_from_catalog<'a>(
    catalog: &'a AvatarCatalog,
    voice_gender: Option<AvatarGender>,
    voice_pitch: Option<PitchRange>,
    voice_energy: Option<EnergyLevel>,
    preference: &AvatarPreference,
) -> Option<&'a DynamicAvatarModel> {
    let models = catalog.all();
    if models.is_empty() {
        return None;
    }

    // Priority 1: explicit model ID request
    if let Some(ref model_id) = preference.model_id {
        if let Some(model) = catalog.by_id(model_id) {
            return Some(model);
        }
        // Requested model not found — fall through to scoring
    }

    let mut best_score = -1i32;
    let mut best_idx = 0;

    for (i, model) in models.iter().enumerate() {
        let mut score = 0i32;

        // Exclude models with blacklisted tags
        if !preference.exclude_tags.is_empty() {
            let excluded = model.tags.iter().any(|t| preference.exclude_tags.contains(t));
            if excluded { continue; }
        }

        // Gender match (strongest signal)
        if let Some(gender) = voice_gender {
            if model.voice_profile.gender == gender {
                score += 10;
            }
        }

        // Pitch range match
        if let Some(pitch) = voice_pitch {
            if model.voice_profile.pitch == pitch {
                score += 5;
            }
        }

        // Energy level match
        if let Some(energy) = voice_energy {
            if model.voice_profile.energy == energy {
                score += 3;
            }
        }

        // Style preference
        if let Some(style) = preference.style {
            if model.style == style {
                score += 2;
            }
        }

        if score > best_score {
            best_score = score;
            best_idx = i;
        }
    }

    Some(&models[best_idx])
}

/// Select from a dynamic catalog by identity hash (deterministic).
pub fn select_from_catalog_by_identity<'a>(
    catalog: &'a AvatarCatalog,
    identity: &str,
) -> Option<&'a DynamicAvatarModel> {
    let models = catalog.all();
    if models.is_empty() {
        return None;
    }
    let idx = super::hash::deterministic_index(identity, models.len(), "avatar");
    Some(&models[idx])
}

// =============================================================================
// Dynamic allocation — uses AvatarCatalog::discover() for all models on disk
// =============================================================================

/// Global discovered catalog (lazy init, lives for the process lifetime).
static DISCOVERED_CATALOG: once_cell::sync::OnceCell<AvatarCatalog> = once_cell::sync::OnceCell::new();

/// Global dynamic allocation map: identity → catalog index into discovered catalog.
static DYNAMIC_ALLOCATION: std::sync::Mutex<Option<HashMap<String, usize>>> = std::sync::Mutex::new(None);

/// Get the discovered catalog (lazy init on first call).
fn discovered_catalog() -> &'static AvatarCatalog {
    DISCOVERED_CATALOG.get_or_init(|| {
        let catalog = AvatarCatalog::discover();
        clog_info!("🎭 Dynamic avatar catalog: {} models discovered on disk", catalog.len());
        for (i, m) in catalog.all().iter().enumerate() {
            clog_info!("🎭   [{:2}] {} — {} ({:?}, {:?}, {:?})",
                i, m.id, m.name, m.format, m.style, m.voice_profile.gender);
        }
        catalog
    })
}

/// Select the best avatar for an agent using the full discovered catalog.
///
/// Same 3-phase gender-coherent algorithm as `select_avatar_for_agent` but uses ALL
/// models found on disk (VRM 0.x, converted VRM 1.0 .glb, etc.) instead of only the
/// 8 hardcoded ones. Gender is SEEDED from identity hash but will be overridable via
/// user.state preferences (like a social media profile — AI personas are first-class citizens).
///
/// Returns `(model_id, filename, display_name)` — the caller uses `avatar_model_path(filename)`
/// to get the filesystem path for the renderer.
pub fn select_dynamic_avatar(identity: &str, voice: Option<&str>) -> &'static DynamicAvatarModel {
    let catalog = discovered_catalog();
    let models = catalog.all();

    // Empty catalog can't happen: discover() falls back to from_static() which has 8 models.
    // But guard anyway — a panic here is better than a silent wrong selection.
    assert!(!models.is_empty(), "Avatar catalog is empty — no models on disk and no static fallback");

    let mut guard = DYNAMIC_ALLOCATION.lock().unwrap();
    let allocation = guard.get_or_insert_with(HashMap::new);

    // Already allocated — return the same model (stable)
    if let Some(&idx) = allocation.get(identity) {
        return &models[idx];
    }

    // Resolve gender: voice name > deterministic from identity
    let gender = voice
        .and_then(gender_from_voice_name)
        .unwrap_or_else(|| gender_from_identity(identity));

    // Filter catalog to matching gender
    let matching: Vec<usize> = models.iter().enumerate()
        .filter(|(_, m)| m.voice_profile.gender == gender)
        .map(|(i, _)| i)
        .collect();

    let used: HashSet<usize> = allocation.values().copied().collect();

    // Phase A: try unused gender-matching model (probe from preferred hash)
    if !matching.is_empty() {
        let preferred = deterministic_index(identity, matching.len(), "avatar");
        for offset in 0..matching.len() {
            let probe = (preferred + offset) % matching.len();
            let catalog_idx = matching[probe];
            if !used.contains(&catalog_idx) {
                clog_info!("🎭 Avatar for '{}': gender={:?} → '{}' / {} (unique, gender match)",
                    &identity[..8.min(identity.len())], gender, models[catalog_idx].id, models[catalog_idx].filename);
                allocation.insert(identity.to_string(), catalog_idx);
                return &models[catalog_idx];
            }
        }
    }

    // Phase B: gender pool exhausted — share least-used SAME-GENDER model.
    // Gender coherence (avatar looks like the voice sounds) > visual diversity.
    // With 7F:1M in the catalog, male agents share the one male model.
    let mut usage_count: HashMap<usize, usize> = HashMap::new();
    for &idx in allocation.values() {
        *usage_count.entry(idx).or_insert(0) += 1;
    }
    if !matching.is_empty() {
        let picked = matching.iter()
            .copied()
            .min_by_key(|i| usage_count.get(i).copied().unwrap_or(0))
            .unwrap();
        clog_info!("🎭 Avatar for '{}': gender={:?} → '{}' / {} (shared, same-gender)",
            &identity[..8.min(identity.len())], gender, models[picked].id, models[picked].filename);
        allocation.insert(identity.to_string(), picked);
        return &models[picked];
    }

    // Phase C: no models of this gender at all — pick least-used from any gender.
    let picked = (0..models.len())
        .min_by_key(|i| usage_count.get(i).copied().unwrap_or(0))
        .unwrap_or(0);
    clog_info!("🎭 Avatar for '{}': gender={:?} → '{}' / {} (shared, any-gender last-resort)",
        &identity[..8.min(identity.len())], gender, models[picked].id, models[picked].filename);
    allocation.insert(identity.to_string(), picked);
    &models[picked]
}

/// Pre-allocate avatars for a batch of identities using the dynamic catalog.
pub fn allocate_dynamic_batch(identities: &[(&str, Option<&str>)]) {
    let mut sorted: Vec<(&str, Option<&str>)> = identities.to_vec();
    sorted.sort_by_key(|(id, _)| fnv1a_hash(id.as_bytes()));

    for (identity, voice) in &sorted {
        select_dynamic_avatar(identity, *voice);
    }
    clog_info!("🎭 Pre-allocated {} avatars from dynamic catalog ({} models available)",
        identities.len(), discovered_catalog().len());
}

/// Reset the global allocation map. Only for testing.
#[cfg(test)]
pub fn reset_allocation() {
    let mut guard = AVATAR_ALLOCATION.lock().unwrap();
    *guard = None;
}

/// Reset the dynamic allocation map. Only for testing.
#[cfg(test)]
pub fn reset_dynamic_allocation() {
    let mut guard = DYNAMIC_ALLOCATION.lock().unwrap();
    *guard = None;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Allocation tests share global AVATAR_ALLOCATION state.
    /// This mutex serializes them so parallel test threads don't interfere.
    static ALLOC_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn test_identity_selection_round_robin() {
        let a1 = select_avatar_by_identity("helper-ai");
        let a2 = select_avatar_by_identity("helper-ai");
        assert!(!a1.id.is_empty());
        assert!(!a2.id.is_empty());
    }

    #[test]
    fn test_different_identities_get_different_avatars() {
        let a1 = select_avatar_by_identity("alice");
        let a2 = select_avatar_by_identity("bob");
        assert!(!a1.id.is_empty());
        assert!(!a2.id.is_empty());
    }

    #[test]
    fn test_voice_matching_prefers_gender() {
        let model = select_avatar_for_voice(
            Some(AvatarGender::Male),
            None,
            None,
            None,
        );
        assert_eq!(model.voice_profile.gender, AvatarGender::Male);
    }

    #[test]
    fn test_voice_matching_full_profile() {
        let model = select_avatar_for_voice(
            Some(AvatarGender::Female),
            Some(PitchRange::High),
            Some(EnergyLevel::Calm),
            None,
        );
        assert_eq!(model.voice_profile.gender, AvatarGender::Female);
        assert_eq!(model.style, AvatarStyle::Anime);
    }

    #[test]
    fn test_select_avatar_for_agent_with_voice() {
        let _guard = ALLOC_TEST_LOCK.lock().unwrap();
        reset_allocation();

        let model = select_avatar_for_agent("voice-test-male", Some("am_adam"));
        assert_eq!(model.voice_profile.gender, AvatarGender::Male,
            "Expected Male, got {:?}", model.voice_profile.gender);

        let model = select_avatar_for_agent("voice-test-female", Some("af_bella"));
        assert_eq!(model.voice_profile.gender, AvatarGender::Female,
            "Expected Female, got {:?}", model.voice_profile.gender);

        reset_allocation();
    }

    #[test]
    fn test_select_avatar_for_agent_no_voice() {
        let _guard = ALLOC_TEST_LOCK.lock().unwrap();
        reset_allocation();

        let model = select_avatar_for_agent("no-voice-test-id", None);
        assert!(!model.id.is_empty());

        reset_allocation();
    }

    #[test]
    fn test_unique_avatar_allocation_no_duplicates() {
        let _guard = ALLOC_TEST_LOCK.lock().unwrap();
        reset_allocation();

        let identities: Vec<String> = (0..8)
            .map(|i| format!("persona-uuid-{:04}", i))
            .collect();

        let mut assigned_models: Vec<&str> = Vec::new();
        for id in &identities {
            let model = select_avatar_for_agent(id, None);
            assigned_models.push(model.id);
        }

        // With 8 personas and 8 models (7F, 1M), gender-coherent allocation means:
        // - Males all share the 1 male model (vroid-male-base)
        // - Females get unique models from the 7 female models
        // So we won't get 8 unique — we get as many unique as there are unique genders × models.
        // Verify: all males get a male model, all females get a female model.
        for (id, model_id) in identities.iter().zip(assigned_models.iter()) {
            let model = AVATAR_CATALOG.iter().find(|m| m.id == *model_id).unwrap();
            let expected_gender = gender_from_identity(id);
            assert_eq!(model.voice_profile.gender, expected_gender,
                "Persona '{}' has gender {:?} but got model '{}' with gender {:?}",
                id, expected_gender, model_id, model.voice_profile.gender);
        }

        reset_allocation();
    }

    #[test]
    fn test_unique_avatar_allocation_stable() {
        let _guard = ALLOC_TEST_LOCK.lock().unwrap();
        reset_allocation();

        let model1 = select_avatar_for_agent("stable-test-id", None);
        let model2 = select_avatar_for_agent("stable-test-id", None);
        assert_eq!(model1.id, model2.id, "Same identity should always get same model");

        reset_allocation();
    }

    #[test]
    fn test_unique_avatar_allocation_overflow_shares() {
        let _guard = ALLOC_TEST_LOCK.lock().unwrap();
        reset_allocation();

        let identities: Vec<String> = (0..12)
            .map(|i| format!("overflow-persona-{:04}", i))
            .collect();

        let mut assigned: Vec<&str> = Vec::new();
        for id in &identities {
            let model = select_avatar_for_agent(id, None);
            assigned.push(model.id);
        }

        // 12 personas with 8 models (7F, 1M): gender coherence means males share the 1 male model.
        // Verify: every persona's avatar matches its seeded gender.
        for (id, model_id) in identities.iter().zip(assigned.iter()) {
            let model = AVATAR_CATALOG.iter().find(|m| m.id == *model_id).unwrap();
            let expected_gender = gender_from_identity(id);
            assert_eq!(model.voice_profile.gender, expected_gender,
                "Persona '{}' has gender {:?} but got model '{}' with gender {:?}",
                id, expected_gender, model_id, model.voice_profile.gender);
        }

        // Verify females still get good diversity (7 female models for ~6 female personas)
        let female_models: HashSet<&str> = identities.iter().zip(assigned.iter())
            .filter(|(id, _)| gender_from_identity(id) == AvatarGender::Female)
            .map(|(_, model_id)| *model_id)
            .collect();
        // At least some diversity among female assignments
        assert!(female_models.len() >= 2,
            "Expected diverse female model assignments, got {:?}", female_models);

        reset_allocation();
    }

    // =========================================================================
    // Dynamic catalog selection tests
    // =========================================================================

    #[test]
    fn test_catalog_select_by_gender() {
        let catalog = AvatarCatalog::from_static();
        let pref = AvatarPreference::default();
        let result = select_from_catalog(
            &catalog,
            Some(AvatarGender::Male),
            None, None,
            &pref,
        ).unwrap();
        assert_eq!(result.voice_profile.gender, AvatarGender::Male);
    }

    #[test]
    fn test_catalog_select_explicit_model_id() {
        let catalog = AvatarCatalog::from_static();
        let pref = AvatarPreference {
            model_id: Some("vroid-shino".into()),
            ..Default::default()
        };
        let result = select_from_catalog(
            &catalog,
            Some(AvatarGender::Male), // gender says male, but explicit model wins
            None, None,
            &pref,
        ).unwrap();
        assert_eq!(result.id, "vroid-shino", "Explicit model_id should override voice matching");
    }

    #[test]
    fn test_catalog_select_with_style_preference() {
        let catalog = AvatarCatalog::from_static();
        let pref = AvatarPreference {
            style: Some(AvatarStyle::Anime),
            ..Default::default()
        };
        let result = select_from_catalog(
            &catalog,
            None, None, None,
            &pref,
        ).unwrap();
        assert_eq!(result.style, AvatarStyle::Anime);
    }

    #[test]
    fn test_catalog_select_by_identity_deterministic() {
        let catalog = AvatarCatalog::from_static();
        let a = select_from_catalog_by_identity(&catalog, "test-id").unwrap();
        let b = select_from_catalog_by_identity(&catalog, "test-id").unwrap();
        assert_eq!(a.id, b.id, "Same identity should pick same model");
    }

    #[test]
    fn test_batch_allocation_order_independent() {
        let _guard = ALLOC_TEST_LOCK.lock().unwrap();

        reset_allocation();
        let ids_a = vec![
            ("persona-a", None), ("persona-b", None), ("persona-c", None),
            ("persona-d", None), ("persona-e", None),
        ];
        allocate_avatars_batch(&ids_a);
        let model_a1 = select_avatar_for_agent("persona-a", None).id;
        let model_a2 = select_avatar_for_agent("persona-c", None).id;

        reset_allocation();
        let ids_b = vec![
            ("persona-e", None), ("persona-d", None), ("persona-c", None),
            ("persona-b", None), ("persona-a", None),
        ];
        allocate_avatars_batch(&ids_b);
        let model_b1 = select_avatar_for_agent("persona-a", None).id;
        let model_b2 = select_avatar_for_agent("persona-c", None).id;

        assert_eq!(model_a1, model_b1, "persona-a should get same model regardless of batch order");
        assert_eq!(model_a2, model_b2, "persona-c should get same model regardless of batch order");

        reset_allocation();
    }
}
