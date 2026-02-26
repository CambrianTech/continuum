//! Gender detection from TTS voice names and persona identities.
//!
//! Maps voice names from all TTS backends (Edge, Kokoro, Orpheus, Piper, Pocket)
//! to AvatarGender for model selection. Falls back to deterministic identity hashing.

use super::types::AvatarGender;
use super::hash::deterministic_pick;

/// Extract gender from a TTS voice name.
///
/// Covers all TTS backends used in the system:
/// - Edge TTS: names like "en-US-GuyNeural" (male), "en-US-JennyNeural" (female)
/// - Kokoro: prefixes "af_"/"bf_" (female), "am_"/"bm_" (male)
/// - Orpheus: gendered names (tara/leah/jess/mia/zoe = female, leo/dan/zac = male)
/// - Piper/Pocket: character names (alba/cosette/eponine = female, jean/marius = male)
///
/// Returns None for unrecognized voices (caller falls back to round-robin).
pub fn gender_from_voice_name(voice: &str) -> Option<AvatarGender> {
    let lower = voice.to_lowercase();

    // Kokoro prefix convention: af_ / bf_ = female, am_ / bm_ = male
    if lower.starts_with("af_") || lower.starts_with("bf_") {
        return Some(AvatarGender::Female);
    }
    if lower.starts_with("am_") || lower.starts_with("bm_") {
        return Some(AvatarGender::Male);
    }

    // Edge TTS: contains "Guy" or known male names → male, known female names → female
    if lower.contains("guyneural") || lower.contains("andrewneural")
        || lower.contains("brianneural") || lower.contains("ericneural")
        || lower.contains("rogerneural") || lower.contains("steffanneural")
        || lower.contains("christopherneural") || lower.contains("davisneural") {
        return Some(AvatarGender::Male);
    }
    if lower.contains("jennyneural") || lower.contains("arianeural")
        || lower.contains("emmaneural") || lower.contains("janeneural")
        || lower.contains("nancyneural") || lower.contains("saraneural")
        || lower.contains("michelleneural") || lower.contains("amberneural") {
        return Some(AvatarGender::Female);
    }

    // Orpheus: gendered character names
    let orpheus_female = ["tara", "leah", "jess", "mia", "zoe"];
    let orpheus_male = ["leo", "dan", "zac"];
    for name in &orpheus_female {
        if lower == *name || lower.starts_with(&format!("{}_", name))
            || lower.starts_with(&format!("{}.", name)) {
            return Some(AvatarGender::Female);
        }
    }
    for name in &orpheus_male {
        if lower == *name || lower.starts_with(&format!("{}_", name))
            || lower.starts_with(&format!("{}.", name)) {
            return Some(AvatarGender::Male);
        }
    }

    // Piper / Pocket: character names from literature
    let pocket_female = ["alba", "fantine", "cosette", "eponine", "azelma"];
    let pocket_male = ["marius", "javert", "jean"];
    for name in &pocket_female {
        if lower.contains(name) {
            return Some(AvatarGender::Female);
        }
    }
    for name in &pocket_male {
        if lower.contains(name) {
            return Some(AvatarGender::Male);
        }
    }

    None
}

/// Genders for deterministic identity-based selection.
/// All catalog models are Male or Female — no Neutral models in the catalog.
const IDENTITY_GENDERS: &[AvatarGender] = &[AvatarGender::Female, AvatarGender::Male];

/// Deterministically derive a gender from a persona identity.
/// Same persona always gets the same gender (Male or Female only).
pub fn gender_from_identity(identity: &str) -> AvatarGender {
    *deterministic_pick(identity, IDENTITY_GENDERS, "gender")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gender_from_voice_kokoro_female() {
        assert_eq!(gender_from_voice_name("af_bella"), Some(AvatarGender::Female));
        assert_eq!(gender_from_voice_name("bf_emma"), Some(AvatarGender::Female));
    }

    #[test]
    fn test_gender_from_voice_kokoro_male() {
        assert_eq!(gender_from_voice_name("am_adam"), Some(AvatarGender::Male));
        assert_eq!(gender_from_voice_name("bm_george"), Some(AvatarGender::Male));
    }

    #[test]
    fn test_gender_from_voice_edge_tts() {
        assert_eq!(gender_from_voice_name("en-US-GuyNeural"), Some(AvatarGender::Male));
        assert_eq!(gender_from_voice_name("en-US-JennyNeural"), Some(AvatarGender::Female));
        assert_eq!(gender_from_voice_name("en-US-BrianNeural"), Some(AvatarGender::Male));
    }

    #[test]
    fn test_gender_from_voice_orpheus() {
        assert_eq!(gender_from_voice_name("tara"), Some(AvatarGender::Female));
        assert_eq!(gender_from_voice_name("leo"), Some(AvatarGender::Male));
        assert_eq!(gender_from_voice_name("zoe"), Some(AvatarGender::Female));
    }

    #[test]
    fn test_gender_from_voice_pocket() {
        assert_eq!(gender_from_voice_name("alba"), Some(AvatarGender::Female));
        assert_eq!(gender_from_voice_name("marius"), Some(AvatarGender::Male));
    }

    #[test]
    fn test_gender_from_voice_unknown() {
        assert_eq!(gender_from_voice_name("some-random-voice"), None);
        assert_eq!(gender_from_voice_name(""), None);
    }

    #[test]
    fn test_gender_from_identity_stable() {
        let g1 = gender_from_identity("persona-abc-123");
        let g2 = gender_from_identity("persona-abc-123");
        assert_eq!(g1, g2);
    }

    #[test]
    fn test_gender_from_identity_covers_male_and_female() {
        let mut seen = std::collections::HashSet::new();
        for i in 0..100 {
            let g = gender_from_identity(&format!("persona-{}", i));
            assert!(g == AvatarGender::Male || g == AvatarGender::Female,
                "gender_from_identity should never return Neutral, got {:?}", g);
            seen.insert(g);
        }
        assert_eq!(seen.len(), 2, "Expected both Male and Female from 100 identities, got {:?}", seen);
    }
}
