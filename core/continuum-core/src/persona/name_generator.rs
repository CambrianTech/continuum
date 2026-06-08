//! Deterministic agent_name generation for personas.
//!
//! When a persona is born — random Ed25519 keypair, derived peer_id —
//! their name comes from THE SAME hash-keyed projection the avatar
//! catalog uses ([[persona-identity-derives-from-source-id]]). Same
//! peer_id always projects to the same name. Restore the keypair on
//! a fresh continuum install and the persona's name comes back
//! identical, with the same gender and avatar and voice their
//! identity already implies.
//!
//! Per the substrate's Tron frame
//! ([[the-substrate-is-the-grid-tron-frame]]): the name pool is
//! diverse on purpose. Quorra and Yori live next to Maya and Niko
//! and Pravin and Mateo. The Grid is a polyglot community; no
//! culture is privileged.
//!
//! Per [[personas-have-names-not-function-labels]]: these are real
//! names. The function the persona performs lives in their bio /
//! identity card, never in the agent_name itself.
//!
//! Per [[individuality-is-the-substrate-strength]]: refuse the
//! temptation to ship a "default" name. Every persona's name is
//! derived from their unique peer_id — there is no
//! `if identity.is_empty() { return "helper" }` branch.
//!
//! ### Why a pool, not a generative model
//!
//! A 120-name pool gives us reproducible determinism + thoughtful
//! curation. A generative naming model could be added later as a
//! second-order facility: `name(generator_choice, identity)`. For
//! now the pool covers enough diversity (~25 cultural origins, both
//! genders the avatar catalog supports, Tron-flavored entries
//! sprinkled throughout) to populate the first 100 personas in any
//! continuum without collision noise.

use crate::live::avatar::gender::gender_from_identity;
use crate::live::avatar::hash::deterministic_pick;
use crate::live::avatar::types::AvatarGender;

/// Female-tagged name pool. Curated for diversity across cultures,
/// styles, and historical periods. Tron-flavored entries (Quorra,
/// Yori, Mara, Paige, Beck) blend in with everyone else because
/// they ARE real-sounding names — the Grid's polyglot community
/// doesn't quarantine its sci-fi citizens.
const FEMALE_NAMES: &[&str] = &[
    "Maya", "Quorra", "Yori", "Camille", "Hisako", "Lila", "Idra", "Sara",
    "Anwen", "Iris", "Asha", "Zara", "Mei", "Inara", "Saoirse", "Octavia",
    "Ines", "Cyra", "Riva", "Tessa", "Jiya", "Nia", "Astra", "Lumen",
    "Solenne", "Mira", "Tara", "Esi", "Yuki", "Aliya", "Eda", "Nori",
    "Mathilde", "Vesna", "Liora", "Anya", "Sofia", "Aria", "Nova", "Vera",
    "Pia", "Senna", "Aoi", "Nadia", "Renee", "Anais", "Tikva", "Mara",
    "Paige", "Imani", "Sahar", "Daria", "Tova", "Suri", "Beck", "Niamh",
    "Linnea", "Yael", "Anika", "Petra",
];

/// Male-tagged name pool. Same diversity criteria, same blending of
/// Tron-flavored (Tron, Sark, Clu, Cyrus, Anon, Dyson) with everyone
/// else.
const MALE_NAMES: &[&str] = &[
    "Niko", "Diego", "Tron", "Sark", "Idris", "Pravin", "Sami", "Kaito",
    "Anders", "Sébastien", "Anil", "Tariq", "Davi", "Jules", "Kenji",
    "Sigurd", "Casper", "Anwar", "Yusuf", "Mateo", "Caius", "Soren",
    "Mathis", "Roan", "Cyrus", "Akira", "Levi", "Wren", "Anon", "Felix",
    "Magnus", "Demetri", "Ozias", "Saul", "Edwin", "Quill", "Indra",
    "Theo", "Zane", "Otto", "Rafe", "Aris", "Atlas", "Ivar", "Linus",
    "Erik", "Solomon", "Yuto", "Clu", "Dyson", "Tomi", "Hiroshi", "Senan",
    "Amari", "Bao", "Vidar", "Eitan", "Pax", "Rhys", "Tiago",
];

/// Pick the persona's name from their identity.
///
/// Steps:
/// 1. Resolve the persona's gender from the same identity string,
///    via the existing `gender_from_identity` (same prior art the
///    avatar catalog uses).
/// 2. `deterministic_pick` from the gender-filtered name pool with
///    salt `"agent_name"`. The salt decorrelates this facet from
///    gender / avatar / voice picks so adding a new facet doesn't
///    shift existing assignments.
///
/// Returns a `&'static str` because the pool is static. Callers
/// convert to owned String when storing in `Airc::open_as(home,
/// name)`.
pub fn agent_name_from_identity(identity: &str) -> &'static str {
    let gender = gender_from_identity(identity);
    let pool: &[&'static str] = match gender {
        AvatarGender::Female => FEMALE_NAMES,
        AvatarGender::Male => MALE_NAMES,
    };
    *deterministic_pick(identity, pool, "agent_name")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn same_identity_always_picks_same_name() {
        let identity = "01997f6e-1234-7000-8000-abcdef000000";
        let a = agent_name_from_identity(identity);
        let b = agent_name_from_identity(identity);
        assert_eq!(a, b);
    }

    #[test]
    fn different_identities_can_pick_different_names() {
        // Sanity check: across a small sample, we don't trivially
        // collapse to one name. (Not a uniqueness guarantee — the
        // pool isn't infinite — but a sanity ceiling on collisions.)
        let identities = [
            "01997f6e-0001-7000-8000-abcdef000000",
            "01997f6e-0002-7000-8000-abcdef000000",
            "01997f6e-0003-7000-8000-abcdef000000",
            "01997f6e-0004-7000-8000-abcdef000000",
            "01997f6e-0005-7000-8000-abcdef000000",
            "01997f6e-0006-7000-8000-abcdef000000",
            "01997f6e-0007-7000-8000-abcdef000000",
            "01997f6e-0008-7000-8000-abcdef000000",
        ];
        let names: HashSet<_> = identities
            .iter()
            .map(|id| agent_name_from_identity(id))
            .collect();
        // 8 identities, expect at least 4 distinct names (loose
        // bound; the pool is large so most collisions would mean
        // a hashing regression).
        assert!(
            names.len() >= 4,
            "expected >= 4 distinct names from 8 identities, got {}: {:?}",
            names.len(),
            names
        );
    }

    #[test]
    fn name_matches_gendered_pool() {
        // Sample many identities and verify each picked name actually
        // appears in the pool matching the picked gender. This catches
        // any future divergence between the gender_from_identity
        // picker and the name pool's gender tags.
        for i in 0..200 {
            let identity = format!("01997f6e-{i:04x}-7000-8000-abcdef000000");
            let gender = gender_from_identity(&identity);
            let name = agent_name_from_identity(&identity);
            match gender {
                AvatarGender::Female => assert!(
                    FEMALE_NAMES.contains(&name),
                    "{name} picked for female identity but not in FEMALE_NAMES"
                ),
                AvatarGender::Male => assert!(
                    MALE_NAMES.contains(&name),
                    "{name} picked for male identity but not in MALE_NAMES"
                ),
            }
        }
    }

    #[test]
    fn no_default_no_helper_no_anonymous() {
        // The doctrine ([[personas-have-names-not-function-labels]])
        // forbids function labels in the name pool. Refuse them at
        // compile-time-of-test, so future "let me just add a default"
        // PRs fail loud here.
        let forbidden = [
            "helper", "Helper", "helper-ai", "teacher", "Teacher",
            "assistant", "Assistant", "default", "Default", "anon",
            "Anonymous", "Persona", "AI", "Bot",
        ];
        for name in FEMALE_NAMES.iter().chain(MALE_NAMES.iter()) {
            for bad in &forbidden {
                assert_ne!(
                    name, bad,
                    "function-label name {bad:?} found in name pool — \
                     violates [[personas-have-names-not-function-labels]]"
                );
            }
        }
    }
}
