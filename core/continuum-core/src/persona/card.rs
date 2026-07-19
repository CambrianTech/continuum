//! `PersonaCard` — the durable, coherent identity a persona presents.
//!
//! A persona IS its airc user; this card is the application-layer projection of
//! that one identity — the "card" / social presence Joel names in
//! [[persona-is-the-airc-user-one-identity-one-card]]: name, gender, pronouns,
//! avatar, voice, role. Every facet COHERES because they all derive from (and are
//! persisted alongside) the one `persona_id` (== the airc peer_id post-collapse).
//!
//! ### Genesis vs. persistence vs. the live registry — one card, three lives
//!
//! - **Genesis** ([`PersonaCard::genesis`], and the pure [`super::projection::project_persona`]):
//!   derive the coherent card from an identity. Pure + deterministic.
//! - **Persistence** ([`super::seed::PersonaSeedFile::V2`]): the card written to
//!   `seed.json`, so it is STABLE across reboots + catalog drift and becomes an
//!   editable surface (the airc identity card) rather than a per-boot re-derivation.
//! - **Live registry** (this module's [`register`] / [`get`] / [`gender_of`]): the
//!   in-memory lookup the hot avatar/voice seams read — they can't touch disk. A
//!   LOCAL persona registers her persisted card at spawn; the seams resolve her
//!   gender/presentation from it.
//!
//! ### Why this replaces the name-anchor hack for LOCAL personas
//!
//! Historically a persona's gender was RE-DERIVED from her NAME string every spawn
//! (`selection::register_persona_gender` → `gender_from_name`), held in a process-
//! global map. That worked, but the truth lived in a transient re-derivation, not a
//! durable record — so it couldn't be edited or trusted as identity. Now the durable
//! CARD is the truth: a local persona registers her card here, and
//! `selection::registered_gender` consults this registry FIRST. The name-anchored
//! map survives only for REMOTE/live participants (a peer in a call, resolved from
//! their display name — no local card exists for them), which is a genuinely
//! different function, not the hack.
//!
//! ### Coherence during the throwaway-name era (Slice 1)
//!
//! [`genesis`](PersonaCard::genesis) draws gender from the NAME first
//! (`gender_from_name`), falling back to the id-hash — which is EXACTLY the effective
//! gender the live seams computed before this slice, so migrating the existing
//! personas shifts nobody. When the fresh-mint name later derives from the peer_id
//! (the airc keypair-first change, Slice 1b), `gender_from_name(name)` will equal
//! `gender_from_identity(id)` and genesis collapses to pure `project_persona(id)`.

use std::collections::HashMap;
use std::sync::Mutex;

use std::collections::BTreeMap;

use uuid::Uuid;

use crate::live::avatar::gender::{gender_from_identity, pronouns_for_gender, PronounSet};
use crate::live::avatar::types::AvatarGender;
use crate::persona::name_generator::gender_from_name;
use crate::persona::role_template::RoleId;

/// The coherent identity card. Every field agrees with `gender`, the presentation
/// spine (avatar, voice, and pronouns all follow from it). Owned + serializable via
/// [`super::seed::PersonaSeedFile::V2`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersonaCard {
    /// The one durable identity (== the airc peer_id post-collapse).
    pub persona_id: Uuid,
    /// The persona's name — user-visible, the airc display name, and the home-dir label.
    pub agent_name: String,
    /// Birth time (ms since epoch). Stable across reboots.
    pub created_at_ms: u64,
    /// The presentation spine. Avatar, voice, and pronouns all cohere with this.
    pub gender: AvatarGender,
    /// The PINNED avatar VRM filename (sticky, resolve-once — #174). `None` until her
    /// face is pinned at first spawn.
    pub avatar_vrm: Option<String>,
    /// The seed the speak path uses to pick a stable, gender-matched voice. Today it
    /// is the identity string; a distinct field so a persona can later choose a voice
    /// independent of her id without a schema change (the editable-card model).
    pub voice_seed: String,
    /// The substrate role, when known. `None` on a card minted before role threading
    /// (that lands in a later #199 slice); the persona still functions.
    pub role: Option<RoleId>,
    /// The OPEN, self-authored part of the identity — arbitrary key→value facets the
    /// persona (or a human with permission) writes about herself: `bio`, `goals`,
    /// `desires`, `interests`, `blog`, `pronouns` (an explicit override), … The typed
    /// fields above are the small coherent SPINE (the facets that must agree —
    /// gender↔avatar↔voice); this map is everything else, extensible without a schema
    /// change so the identity is an open profile, not a fixed struct
    /// ([[persona-identity-is-fully-self-editable-except-the-id]]). Empty at genesis;
    /// grows as she authors herself. Deterministic on-disk order (`BTreeMap`).
    pub profile: BTreeMap<String, String>,
}

impl PersonaCard {
    /// Pronouns, derived from `gender`. NOT a stored field: pronouns are a pure
    /// function of the spine today (compression — one logical decision, one place).
    /// They EARN a stored slot the moment they become independently overridable
    /// (`airc identity set --pronouns`, a later slice); until then, deriving keeps
    /// them from drifting away from the gender they must agree with.
    pub fn pronouns(&self) -> PronounSet {
        pronouns_for_gender(self.gender)
    }

    /// Derive the coherent card from an identity + its visible name.
    ///
    /// Gender coheres with the NAME first (`gender_from_name`), falling back to the
    /// id-hash for a unisex/custom name — which is exactly the effective gender the
    /// live seams computed pre-Slice-1, so persisting it shifts nobody. `voice_seed`
    /// is the identity string (matching [`super::projection::project_persona`]);
    /// `role` is unknown at genesis. `avatar_vrm` is threaded in (a resumed persona's
    /// pinned face is preserved; a fresh mint passes `None` and pins later).
    pub fn genesis(
        persona_id: Uuid,
        agent_name: impl Into<String>,
        created_at_ms: u64,
        avatar_vrm: Option<String>,
    ) -> Self {
        let agent_name = agent_name.into();
        let id_str = persona_id.to_string();
        let gender =
            gender_from_name(&agent_name).unwrap_or_else(|| gender_from_identity(&id_str));
        Self {
            persona_id,
            agent_name,
            created_at_ms,
            gender,
            avatar_vrm,
            voice_seed: id_str,
            role: None,
            profile: BTreeMap::new(),
        }
    }
}

/// The live card registry — identity string (`persona_id.to_string()`) → the LOCAL
/// persona's durable card. Populated at spawn from the persisted seed; read by the
/// hot avatar/voice seams via [`crate::live::avatar::selection::registered_gender`],
/// which consults it FIRST. Same process-global shape as the substrate's other
/// stateless lookups (`perception_registry`, `shared_compute::global`): a lookup
/// table, not a manager.
static CARD_REGISTRY: Mutex<Option<HashMap<String, PersonaCard>>> = Mutex::new(None);

/// Register (or refresh) a local persona's durable card, keyed by her identity. The
/// authoritative gender/presentation source for every avatar/voice selection — this
/// is what replaces the name-re-derivation for our OWN personas.
pub fn register(card: PersonaCard) {
    let key = card.persona_id.to_string();
    CARD_REGISTRY
        .lock()
        .unwrap()
        .get_or_insert_with(HashMap::new)
        .insert(key, card);
}

/// The full card for an identity, if a local persona registered one.
pub fn get(identity: &str) -> Option<PersonaCard> {
    CARD_REGISTRY
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|m| m.get(identity).cloned())
}

/// Every registered card's persona_id — the candidate set for resolving a
/// short/mistyped persona id a caller quotes back (`persona/identity/{get,set}`
/// via [`crate::id_resolve::resolve`], #164). Cards register at spawn and on
/// every identity edit, so this is "the personas this process knows about" — the
/// same role `PersonaAircRuntimeRegistry::ids()` plays for the runtime verbs.
pub fn ids() -> Vec<Uuid> {
    CARD_REGISTRY
        .lock()
        .unwrap()
        .as_ref()
        .map(|m| m.values().map(|c| c.persona_id).collect())
        .unwrap_or_default()
}

/// The registered card's gender for an identity, if present. The seam
/// `registered_gender` calls before falling back to the remote-participant
/// name-anchor and finally the id-hash.
pub fn gender_of(identity: &str) -> Option<AvatarGender> {
    CARD_REGISTRY
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|m| m.get(identity).map(|c| c.gender))
}

/// Drop a persona's card from the live registry (test isolation + despawn).
#[cfg(test)]
pub fn remove(identity: &str) {
    if let Some(m) = CARD_REGISTRY.lock().unwrap().as_mut() {
        m.remove(identity);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the coherence contract of the card — gender is the spine,
    // pronouns derive from it, voice_seed is the identity, and a female/male NAME
    // drives the gender (so the card agrees with the visible name). This is the
    // invariant the whole "one identity, one coherent card" model rests on.
    #[test]
    fn genesis_card_is_coherent_with_the_name() {
        // A name unambiguously in the FEMALE pool.
        let female_name = "Asha";
        assert_eq!(gender_from_name(female_name), Some(AvatarGender::Female));
        let id = Uuid::new_v4();
        let card = PersonaCard::genesis(id, female_name, 1000, None);
        assert_eq!(card.gender, AvatarGender::Female, "gender agrees with the name");
        assert_eq!(card.pronouns().subject, "she", "pronouns cohere with gender");
        assert_eq!(card.voice_seed, id.to_string(), "voice seeds on the identity");
        assert_eq!(card.persona_id, id);
        assert_eq!(card.created_at_ms, 1000);
        assert!(card.role.is_none(), "role unknown at genesis");
    }

    // what this catches: ids() reports every REGISTERED card's persona_id — the
    // candidate set that lets a short/mistyped persona id a caller quotes back
    // resolve via id_resolve (#164). Membership, not equality: the registry is a
    // process-global shared with other tests, so we only assert OUR two ids appear
    // (and remove them after) rather than pinning the whole set.
    #[test]
    fn ids_reports_registered_cards() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        register(PersonaCard::genesis(a, "Asha", 1, None));
        register(PersonaCard::genesis(b, "Niko", 2, None));
        let registered = ids();
        assert!(
            registered.contains(&a) && registered.contains(&b),
            "both registered ids are candidates"
        );
        remove(&a.to_string());
        remove(&b.to_string());
        let after = ids();
        assert!(!after.contains(&a) && !after.contains(&b), "removed ids drop out");
    }

    // what this catches: a unisex/custom name (not in either gendered pool) falls
    // back to the id-hash gender — never panics, always coheres with SOMETHING. This
    // is the fallback the pre-Slice-1 seams used, so it must be preserved to avoid a
    // migration shift.
    #[test]
    fn genesis_falls_back_to_id_hash_for_a_custom_name() {
        let custom = "Zzyzx-not-in-any-pool";
        assert_eq!(gender_from_name(custom), None);
        let id = Uuid::new_v4();
        let card = PersonaCard::genesis(id, custom, 0, None);
        assert_eq!(
            card.gender,
            gender_from_identity(&id.to_string()),
            "custom name → id-hash gender (the effective pre-Slice-1 behavior)"
        );
    }

    // what this catches: the live registry round-trips a card and answers gender_of
    // — the exact lookup the avatar/voice seams depend on. A missing id returns None
    // (so the seam falls through to the name-anchor / id-hash), never a wrong gender.
    #[test]
    fn registry_round_trips_and_gender_of_resolves() {
        let id = Uuid::new_v4();
        let card = PersonaCard::genesis(id, "Niko", 0, None); // Niko ∈ MALE_NAMES
        let key = id.to_string();
        register(card.clone());
        assert_eq!(get(&key), Some(card));
        assert_eq!(gender_of(&key), Some(AvatarGender::Male));
        assert_eq!(gender_of(&Uuid::new_v4().to_string()), None, "unknown id → None");
        remove(&key);
        assert_eq!(get(&key), None, "removed card is gone");
    }
}
