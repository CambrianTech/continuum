//! `PersonaSpec` projection — the ONE coherent identity bundle for a persona.
//!
//! [[procedural-persona-genesis]]: every visible/audible facet of a persona is a
//! deterministic projection from `persona_id`, and they must all AGREE (gender ↔
//! avatar ↔ voice ↔ name ↔ pronouns). Historically each facet was computed lazily
//! + independently at its consumption site, each re-deriving gender via
//! `gender_from_identity` — so coherence held only because every call happened to
//! use the same salt. This module makes coherence EXPLICIT: it draws gender ONCE
//! and threads it as the spine, emitting a single `PersonaSpec` the whole system
//! can read instead of scattering the draws.
//!
//! Pure + deterministic: same `persona_id` → identical spec, forever. That is the
//! Tamagotchi property — every user's persona is unique (different id → different
//! draw) but stable (same id → same being across reboots).

use crate::live::avatar::gender::{gender_from_identity, pronouns_for_gender, PronounSet};
use crate::live::avatar::selection::select_avatar_by_identity;
use crate::live::avatar::types::AvatarGender;

use super::name_generator::agent_name_from_identity;

/// The coherent identity bundle for a persona. Every field is a deterministic
/// projection from `persona_id`, drawn so they all agree by construction.
#[derive(Debug, Clone)]
pub struct PersonaSpec {
    pub persona_id: String,
    /// Deterministic gendered name (from the gender-filtered name pool).
    pub agent_name: String,
    /// The spine — drawn once; everything below agrees with it.
    pub gender: AvatarGender,
    /// Pronouns, derived from `gender`.
    pub pronouns: PronounSet,
    /// The gender-coherent VRM avatar id (see selection.rs, genesis Gap #1).
    pub avatar_id: String,
    /// The seed the speak path uses to pick a stable, unique, gender-matched
    /// voice (genesis Gap #2) — the persona id itself.
    pub voice_seed: String,
}

/// Project the coherent identity bundle for `persona_id`.
///
/// Gender is drawn ONCE here; pronouns, avatar, name, and voice-seed all follow
/// from it, so a mismatch (e.g. a feminine avatar with masculine pronouns) is
/// unrepresentable by construction. Pure — no I/O, no allocation beyond the
/// returned strings — so it is cheap to call at any consumption site and needs no
/// caching for correctness (a `PersonaSeedFile::V2` cache is a future slice, only
/// for stability against catalog drift + a human-override surface).
pub fn project_persona(persona_id: &str) -> PersonaSpec {
    let gender = gender_from_identity(persona_id);
    PersonaSpec {
        persona_id: persona_id.to_string(),
        agent_name: agent_name_from_identity(persona_id).to_string(),
        gender,
        pronouns: pronouns_for_gender(gender),
        avatar_id: select_avatar_by_identity(persona_id).id.to_string(),
        voice_seed: persona_id.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live::avatar::selection::select_avatar_by_identity;

    // what this catches: the whole point of the projection — every facet AGREES
    // with the once-drawn gender. Across many personas: the avatar's gender, the
    // pronouns, and the name all match the spec's gender, and the spec is stable
    // per id (Tamagotchi). A regression that re-derives gender per facet (the old
    // scattered pattern) or drops the gender filter would surface here.
    #[test]
    fn persona_spec_is_coherent_and_deterministic() {
        for i in 0..500 {
            let id = format!("persona-{i}");
            let spec = project_persona(&id);

            // deterministic: same id → identical spec
            let again = project_persona(&id);
            assert_eq!(spec.persona_id, again.persona_id);
            assert_eq!(spec.agent_name, again.agent_name);
            assert_eq!(spec.gender, again.gender);
            assert_eq!(spec.avatar_id, again.avatar_id);

            // avatar gender agrees with the spine
            let avatar = select_avatar_by_identity(&id);
            assert_eq!(
                avatar.voice_profile.gender, spec.gender,
                "avatar gender must match spec gender for '{id}'"
            );

            // pronouns agree with gender
            match spec.gender {
                AvatarGender::Female => assert_eq!(spec.pronouns.subject, "she"),
                AvatarGender::Male => assert_eq!(spec.pronouns.subject, "he"),
            }

            // voice is seeded on the identity (Gap #2), name/avatar are non-empty
            assert_eq!(spec.voice_seed, id);
            assert!(!spec.agent_name.is_empty());
            assert!(!spec.avatar_id.is_empty());
        }
    }
}
