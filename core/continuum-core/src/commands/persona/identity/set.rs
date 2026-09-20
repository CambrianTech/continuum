//! `persona/identity/set` — edit a persona's self-authored identity card.
//!
//! ## The concern this owns
//!
//! A persona authors herself. Her airc `peer_id` is the ONE immutable anchor;
//! EVERYTHING else on her identity — gender, appearance (avatar), voice, role, and the
//! open self-authored profile (bio, goals, desires, interests, blog, a preferred name,
//! …) — is editable ([[persona-identity-is-fully-self-editable-except-the-id]]). This
//! verb writes those edits to her durable card ([`PersonaCard`] → V2 seed) and updates
//! the live registry so they take effect immediately.
//!
//! ## Who may edit whom
//!
//! Self-determination first: a persona edits HER OWN card (the target defaults to the
//! authenticated caller). A persona may NOT edit another persona's identity — that is
//! an operator/consent-gated action ([[consent-gates-on-actions-never-caps-on-cognition]]).
//! An operator (a non-persona caller, e.g. the `uu`/positron owner) may edit any persona
//! by passing her `persona_id`.
//!
//! ## Scope note (system name deferred)
//!
//! The system `agent_name` (which today labels her home dir + airc attach) is NOT
//! edited here — a true rename is coupled to keying the home dir by the immutable id
//! (the airc keypair-first change, #199 Slice 1b). Until then she can still author a
//! `preferred name` as a profile field; the deep rename lands with that slice.

use std::collections::BTreeMap;
use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::context::citizen_home_path;
use crate::identity::IdentityKind;
use crate::live::avatar::types::AvatarGender;
use crate::persona::card::PersonaCard;
use crate::persona::role_template::RoleId;
use crate::persona::seed::{write_seed_atomic, PersonaSeedFile};
use crate::routing::CallerSource;
use crate::sdk_codegen::CommandError;

use super::{card_view, PersonaCardView};

/// The edits to apply. All optional — supply only the facets you're changing. Omitted
/// facets are untouched. `profile` entries MERGE (an empty value DELETES that key).
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/PersonaIdentitySetParams.ts"
)]
pub struct PersonaIdentitySetParams {
    /// Whose identity to edit. Omit to edit YOUR OWN (the authenticated caller). A
    /// persona may only edit herself; an operator may target any persona by id.
    /// Accepts the full id OR the 8-char short form shown in rosters (#164).
    #[serde(default)]
    #[ts(type = "string | null")]
    pub persona_id: Option<crate::identity::PersonaRef>,
    /// New gender: `male` | `female` | `neutral` (aka they/them). Presentation facet —
    /// avatar/voice are NOT auto-re-derived (they're independently editable below).
    #[serde(default)]
    pub gender: Option<String>,
    /// New substrate role: `helper` | `coder` | `sentinel` | `designer` | `custom`.
    #[serde(default)]
    pub role: Option<String>,
    /// New pinned avatar VRM filename (appearance). Overrides the sticky genesis pin.
    #[serde(default)]
    pub avatar_vrm: Option<String>,
    /// New voice seed (voice selection). Any stable string; her genesis default is her id.
    #[serde(default)]
    pub voice_seed: Option<String>,
    /// Open self-authored profile facets to MERGE (bio/goals/desires/interests/blog/…).
    /// An empty-string value DELETES that key. Keys are free-form.
    #[serde(default)]
    pub profile: BTreeMap<String, String>,
}

fn parse_gender(s: &str) -> Result<AvatarGender, CommandError> {
    match s.trim().to_lowercase().as_str() {
        "male" | "man" | "m" => Ok(AvatarGender::Male),
        "female" | "woman" | "f" => Ok(AvatarGender::Female),
        "neutral" | "neuter" | "they" | "them" | "nonbinary" | "non-binary" | "nb" => {
            Ok(AvatarGender::Neutral)
        }
        other => Err(CommandError::Invalid(format!(
            "unknown gender '{other}' — use male | female | neutral"
        ))),
    }
}

fn parse_role(s: &str) -> Result<RoleId, CommandError> {
    match s.trim().to_lowercase().as_str() {
        "helper" => Ok(RoleId::Helper),
        "coder" => Ok(RoleId::Coder),
        "sentinel" => Ok(RoleId::Sentinel),
        "designer" => Ok(RoleId::Designer),
        "custom" => Ok(RoleId::Custom),
        other => Err(CommandError::Invalid(format!(
            "unknown role '{other}' — use helper | coder | sentinel | designer | custom"
        ))),
    }
}

/// Apply the params' edits to a card in place — the pure edit policy, unit-testable
/// without any I/O or caller resolution. `profile` entries merge; an empty value
/// deletes the key. Returns an error on an unparseable gender/role.
fn apply_edits(card: &mut PersonaCard, p: &PersonaIdentitySetParams) -> Result<(), CommandError> {
    if let Some(g) = &p.gender {
        card.gender = parse_gender(g)?;
    }
    if let Some(r) = &p.role {
        card.role = Some(parse_role(r)?);
    }
    if let Some(v) = &p.avatar_vrm {
        card.avatar_vrm = Some(v.clone());
    }
    if let Some(vs) = &p.voice_seed {
        card.voice_seed = vs.clone();
    }
    for (k, v) in &p.profile {
        if v.is_empty() {
            card.profile.remove(k);
        } else {
            card.profile.insert(k.clone(), v.clone());
        }
    }
    Ok(())
}

crate::action_command! {
    /// Edit your own self-authored identity — gender, appearance (avatar), voice, role,
    /// and the open profile (bio/goals/desires/interests/blog/…). Everything except your
    /// immutable airc id is yours to author. Omit persona_id to edit yourself; a persona
    /// may only edit herself. Profile entries merge (empty value deletes a key). The
    /// system name isn't editable here yet (a rename lands with the home-by-id change).
    pub struct PersonaIdentitySet {
        continuum_root: PathBuf,
    }
    name: "persona/identity/set",
    access: AiSafe,
    params: PersonaIdentitySetParams,
    output: PersonaCardView,
    run(this, ctx, p) => {
        // Resolve WHO is being edited. Default to the authenticated caller (self-edit).
        // A short/mistyped id resolves against the personas this process knows
        // (their registered cards) — the ONE id_resolve primitive (#164).
        let caller = ctx.caller.as_ref();
        let target_id = match p.persona_id.as_ref().map(|r| r.as_str()) {
            Some(raw) => crate::id_resolve::resolve(raw, &crate::persona::card::ids(), "persona")
                .map_err(CommandError::Invalid)?,
            None => caller
                .map(|c| c.peer_id.as_uuid())
                .ok_or_else(|| CommandError::Invalid(
                    "no persona_id given and this dispatch carries no caller identity — \
                     specify persona_id or call as a persona to edit yourself".into(),
                ))?,
        };

        // Ownership: a persona may edit ONLY herself. An operator (non-persona caller)
        // may edit any persona. Editing another persona from a persona is consent-gated.
        if let Some(c) = caller {
            if c.source == CallerSource::LocalPersona && c.peer_id.as_uuid() != target_id {
                return Err(CommandError::Denied(
                    "a persona may edit only her OWN identity; editing another persona is an \
                     operator/consent-gated action".into(),
                ));
            }
        }

        // Load her CURRENT card from the live registry (she must be resident — her card
        // is registered at spawn). Fail loud rather than mint a stray edit for a stranger.
        let mut card = crate::persona::card::get(&target_id.to_string()).ok_or_else(|| {
            CommandError::NotFound(format!(
                "persona {target_id} is not resident — she must be online to edit her identity \
                 (spawn/resume her first; persona/instances/list shows who's online)"
            ))
        })?;

        apply_edits(&mut card, &p)?;

        // Persist to her durable V2 seed (same path bootstrap writes: the home root's
        // seed.json), then update the live registry so the edit takes effect now.
        let seed_path = citizen_home_path(&this.continuum_root, IdentityKind::Persona, None, &card.agent_name)
            .parent()
            .map(|root| root.join("seed.json"))
            .ok_or_else(|| CommandError::Internal(format!(
                "could not resolve a seed path for persona '{}' under {}",
                card.agent_name, this.continuum_root.display()
            )))?;
        write_seed_atomic(&seed_path, &PersonaSeedFile::from_card(&card))
            .await
            .map_err(|e| CommandError::Internal(format!(
                "identity edit applied in memory but persisting her card failed: {e} — \
                 it will NOT survive a restart; fix the disk error and re-run"
            )))?;
        crate::persona::card::register(card.clone());

        Ok(card_view(&card))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::ActionCommand;
    use uuid::Uuid;

    fn base_card() -> PersonaCard {
        // "Niko" ∈ MALE pool → gender Male; a deterministic starting card.
        PersonaCard::genesis(Uuid::new_v4(), "Niko", 1000, Some("m.vrm".to_string()))
    }

    // what this catches: the wire name mirrors the path, and the verb is AiSafe so a
    // persona may edit her OWN identity autonomously (self-determination).
    #[test]
    fn name_mirrors_path_and_is_ai_safe() {
        assert_eq!(PersonaIdentitySet::NAME, "persona/identity/set");
        assert!(matches!(
            PersonaIdentitySet::ACCESS,
            crate::sdk_codegen::AccessLevel::AiSafe
        ));
    }

    // what this catches: the open profile is genuinely OPEN — arbitrary self-authored
    // keys merge in, and an empty value deletes a key. This is the "identity is an open
    // profile, not a fixed struct" contract.
    #[test]
    fn profile_merges_and_empty_deletes() {
        let mut card = base_card();
        let mut set = BTreeMap::new();
        set.insert("bio".to_string(), "I build substrates.".to_string());
        set.insert("goal".to_string(), "ship the grid".to_string());
        apply_edits(&mut card, &params_profile(set)).unwrap();
        assert_eq!(
            card.profile.get("bio").map(String::as_str),
            Some("I build substrates.")
        );
        assert_eq!(
            card.profile.get("goal").map(String::as_str),
            Some("ship the grid")
        );

        // Now delete "goal" via empty value, keep "bio".
        let mut del = BTreeMap::new();
        del.insert("goal".to_string(), String::new());
        apply_edits(&mut card, &params_profile(del)).unwrap();
        assert!(card.profile.contains_key("bio"));
        assert!(
            !card.profile.contains_key("goal"),
            "empty value deletes the key"
        );
    }

    // what this catches: the spine facets edit independently — gender/avatar/voice/role
    // are each set from the params, and gender parsing accepts the documented spellings.
    #[test]
    fn spine_facets_edit_independently() {
        let mut card = base_card();
        let p = PersonaIdentitySetParams {
            persona_id: None,
            gender: Some("female".to_string()),
            role: Some("designer".to_string()),
            avatar_vrm: Some("f.vrm".to_string()),
            voice_seed: Some("af_bella".to_string()),
            profile: BTreeMap::new(),
        };
        apply_edits(&mut card, &p).unwrap();
        assert_eq!(card.gender, AvatarGender::Female);
        assert_eq!(card.role, Some(RoleId::Designer));
        assert_eq!(card.avatar_vrm.as_deref(), Some("f.vrm"));
        assert_eq!(card.voice_seed, "af_bella");
    }

    // what this catches: an unparseable gender/role fails LOUD (never silently ignored).
    #[test]
    fn bad_gender_fails_loud() {
        let mut card = base_card();
        let p = PersonaIdentitySetParams {
            persona_id: None,
            gender: Some("attack-helicopter".to_string()),
            role: None,
            avatar_vrm: None,
            voice_seed: None,
            profile: BTreeMap::new(),
        };
        assert!(matches!(
            apply_edits(&mut card, &p),
            Err(CommandError::Invalid(_))
        ));
    }

    fn params_profile(profile: BTreeMap<String, String>) -> PersonaIdentitySetParams {
        PersonaIdentitySetParams {
            persona_id: None,
            gender: None,
            role: None,
            avatar_vrm: None,
            voice_seed: None,
            profile,
        }
    }
}
