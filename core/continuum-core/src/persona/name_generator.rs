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
    // Widened pool (#200 follow-up): the name is a cosmetic projection of the unique
    // peer_id — collisions are harmless, but a bigger pool makes births feel varied
    // ([[persona-birth-is-a-first-class-handle-command]]). Kept disjoint from MALE_NAMES
    // (a dual-pool name breaks `gender_from_name`) — pinned by `pools_are_disjoint`.
    "Naima", "Freya", "Leila", "Priya", "Rania", "Suki", "Delia", "Marisol",
    "Chiara", "Noor", "Amara", "Sinead", "Talia", "Rosa", "Ingrid", "Fatima",
    "Elodie", "Kira", "Sana", "Yara", "Dalia", "Bruna", "Aiko", "Livia",
    "Neve", "Zuri", "Halima", "Ondine", "Mirela", "Saanvi", "Thea", "Lucia",
    "Esme", "Runa", "Cleo", "Aisha", "Nyla", "Isolde", "Ambika", "Soraya",
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
    // Widened pool (#200 follow-up) — see FEMALE_NAMES note. Disjoint from FEMALE_NAMES.
    "Ravi", "Bjorn", "Dmitri", "Hassan", "Omar", "Nikolai", "Tobias", "Emeka",
    "Rashid", "Lucas", "Mikael", "Arjun", "Cormac", "Dario", "Elias", "Finnian",
    "Gideon", "Hamza", "Isamu", "Joaquin", "Kwame", "Lorcan", "Marek", "Nestor",
    "Osman", "Pietro", "Quinlan", "Ronan", "Silas", "Taavi", "Ulf", "Viktor",
    "Xavier", "Yannick", "Zoltan", "Amadou", "Ciaran", "Desmond", "Ephraim", "Malik",
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
    match gender {
        AvatarGender::Female => *deterministic_pick(identity, FEMALE_NAMES, "agent_name"),
        AvatarGender::Male => *deterministic_pick(identity, MALE_NAMES, "agent_name"),
        // Neuter (they/them): no dedicated unisex pool yet — draw from BOTH so the
        // name isn't locked to a binary presentation (a they/them persona can carry
        // any name). Stable per identity via the same salt.
        AvatarGender::Neutral => {
            let combined: Vec<&'static str> =
                FEMALE_NAMES.iter().chain(MALE_NAMES.iter()).copied().collect();
            *deterministic_pick(identity, &combined, "agent_name")
        }
    }
}

/// Resolve a persona's gender from its NAME — which gendered pool the name belongs
/// to. This is the coherence ANCHOR ([[procedural-persona-genesis]]): a persona's
/// name is the stable, persisted, user-visible truth (and, unlike gender, is
/// self-editable), so the name — not the raw id — is what avatar/voice must cohere
/// WITH, or a feminine "Asha" ends up with a masculine face.
///
/// Post-#199 Slice 1b a FRESHLY-born persona's `peer_id` IS the seed her name was
/// derived from (continuum supplies `persona_id` to airc's mint as the peer_id via
/// `attach_as_with_peer_id`), so for her the name and id agree by construction. But
/// this name-anchor is still the right resolver: a persona may RENAME herself, and
/// pre-Slice-1b / custom / imported names never derived from the id — so the name,
/// not the id, remains authoritative. Returns `None` for a name in BOTH pools (a
/// genuinely unisex draw, e.g. a Neutral persona) or NEITHER (custom/legacy name),
/// letting the caller fall back to an id-hash gender for those.
pub fn gender_from_name(name: &str) -> Option<AvatarGender> {
    let in_female = FEMALE_NAMES.iter().any(|&n| n == name);
    let in_male = MALE_NAMES.iter().any(|&n| n == name);
    match (in_female, in_male) {
        (true, false) => Some(AvatarGender::Female),
        (false, true) => Some(AvatarGender::Male),
        // Both (unisex) or neither (custom) → ambiguous; caller falls back.
        _ => None,
    }
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
                // Neuter draws from BOTH pools — the name must be in one of them.
                AvatarGender::Neutral => assert!(
                    FEMALE_NAMES.contains(&name) || MALE_NAMES.contains(&name),
                    "{name} picked for neutral identity but not in either pool"
                ),
            }
        }
    }

    // what this catches: the two gender pools must be DEDUPED and DISJOINT. A name in
    // both pools makes `gender_from_name` return `None` (ambiguous) — silently breaking
    // the card's name↔gender coherence for that name. A dup within a pool skews the
    // deterministic draw. This guards every future pool widening.
    #[test]
    fn pools_are_disjoint_and_deduped() {
        let f: HashSet<&&str> = FEMALE_NAMES.iter().collect();
        let m: HashSet<&&str> = MALE_NAMES.iter().collect();
        assert_eq!(f.len(), FEMALE_NAMES.len(), "duplicate name within FEMALE_NAMES");
        assert_eq!(m.len(), MALE_NAMES.len(), "duplicate name within MALE_NAMES");
        let overlap: Vec<&&str> = FEMALE_NAMES.iter().filter(|n| m.contains(n)).collect();
        assert!(
            overlap.is_empty(),
            "names in BOTH pools break gender_from_name coherence: {overlap:?}"
        );
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
