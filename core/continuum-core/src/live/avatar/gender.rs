//! Gender detection from TTS voice names and persona identities.
//!
//! Maps voice names from all TTS backends (Edge, Kokoro, Orpheus, Piper, Pocket)
//! to AvatarGender for model selection. Falls back to deterministic identity hashing.

use super::hash::deterministic_pick;
use super::types::AvatarGender;

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
    if lower.contains("guyneural")
        || lower.contains("andrewneural")
        || lower.contains("brianneural")
        || lower.contains("ericneural")
        || lower.contains("rogerneural")
        || lower.contains("steffanneural")
        || lower.contains("christopherneural")
        || lower.contains("davisneural")
    {
        return Some(AvatarGender::Male);
    }
    if lower.contains("jennyneural")
        || lower.contains("arianeural")
        || lower.contains("emmaneural")
        || lower.contains("janeneural")
        || lower.contains("nancyneural")
        || lower.contains("saraneural")
        || lower.contains("michelleneural")
        || lower.contains("amberneural")
    {
        return Some(AvatarGender::Female);
    }

    // Orpheus: gendered character names
    let orpheus_female = ["tara", "leah", "jess", "mia", "zoe"];
    let orpheus_male = ["leo", "dan", "zac"];
    for name in &orpheus_female {
        if lower == *name
            || lower.starts_with(&format!("{}_", name))
            || lower.starts_with(&format!("{}.", name))
        {
            return Some(AvatarGender::Female);
        }
    }
    for name in &orpheus_male {
        if lower == *name
            || lower.starts_with(&format!("{}_", name))
            || lower.starts_with(&format!("{}.", name))
        {
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
/// The genesis gender draw. Female/Male are the common cases; Neutral (they/them)
/// is a real minority ([[procedural-persona-genesis]]) — weighted ~20% via a 2:2:1
/// list so it's "not too uncommon" without dominating. A Neutral persona's
/// presentation isn't constrained to masc/fem, so its avatar/voice are drawn from
/// the FULL pool (any presentation is coherent with they/them).
const IDENTITY_GENDERS: &[AvatarGender] = &[
    AvatarGender::Female,
    AvatarGender::Female,
    AvatarGender::Male,
    AvatarGender::Male,
    AvatarGender::Neutral,
];

/// Deterministically derive a gender from a persona identity.
/// Same persona always gets the same gender (Male or Female only).
pub fn gender_from_identity(identity: &str) -> AvatarGender {
    *deterministic_pick(identity, IDENTITY_GENDERS, "gender")
}

/// Third-person pronouns, coherent with a persona's gender. The persona genesis
/// draws gender first ([[procedural-persona-genesis]]); pronouns derive from it so
/// they always agree with the avatar/voice/name. Binary today (the catalog is
/// Male/Female); `they/them` is a universally-valid alias a persona may also use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PronounSet {
    pub subject: &'static str,    // she / he
    pub object: &'static str,     // her / him
    pub possessive: &'static str, // her / his
}

impl PronounSet {
    /// The canonical short form, e.g. "she/her".
    pub fn short(&self) -> String {
        format!("{}/{}", self.subject, self.object)
    }
}

/// Derive gender-coherent pronouns.
pub fn pronouns_for_gender(gender: AvatarGender) -> PronounSet {
    match gender {
        AvatarGender::Female => PronounSet {
            subject: "she",
            object: "her",
            possessive: "her",
        },
        AvatarGender::Male => PronounSet {
            subject: "he",
            object: "him",
            possessive: "his",
        },
        AvatarGender::Neutral => PronounSet {
            subject: "they",
            object: "them",
            possessive: "their",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gender_from_voice_kokoro_female() {
        assert_eq!(
            gender_from_voice_name("af_bella"),
            Some(AvatarGender::Female)
        );
        assert_eq!(
            gender_from_voice_name("bf_emma"),
            Some(AvatarGender::Female)
        );
    }

    #[test]
    fn test_gender_from_voice_kokoro_male() {
        assert_eq!(gender_from_voice_name("am_adam"), Some(AvatarGender::Male));
        assert_eq!(
            gender_from_voice_name("bm_george"),
            Some(AvatarGender::Male)
        );
    }

    #[test]
    fn test_gender_from_voice_edge_tts() {
        assert_eq!(
            gender_from_voice_name("en-US-GuyNeural"),
            Some(AvatarGender::Male)
        );
        assert_eq!(
            gender_from_voice_name("en-US-JennyNeural"),
            Some(AvatarGender::Female)
        );
        assert_eq!(
            gender_from_voice_name("en-US-BrianNeural"),
            Some(AvatarGender::Male)
        );
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
    fn test_gender_from_identity_covers_all_three_genders() {
        // Neuter (they/them) is now a real minority in the draw ([[procedural-persona-genesis]]).
        let mut counts: std::collections::HashMap<AvatarGender, usize> =
            std::collections::HashMap::new();
        for i in 0..1000 {
            let g = gender_from_identity(&format!("persona-{}", i));
            *counts.entry(g).or_default() += 1;
        }
        // All three appear — Neutral is present, not absent.
        assert!(
            counts.contains_key(&AvatarGender::Female),
            "no Female drawn"
        );
        assert!(counts.contains_key(&AvatarGender::Male), "no Male drawn");
        assert!(
            counts.contains_key(&AvatarGender::Neutral),
            "no Neutral drawn — they/them must appear"
        );
        // Neutral is a MINORITY (weighted ~20%), never the plurality.
        let neutral = counts[&AvatarGender::Neutral];
        let female = counts[&AvatarGender::Female];
        assert!(
            neutral < female,
            "Neutral ({neutral}) should be a minority vs Female ({female})"
        );
    }
}
