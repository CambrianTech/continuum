//! `persona/identity/<verb>` — the persona's self-authored, editable identity card.
//!
//! Her airc `peer_id` is the one immutable anchor; everything else (gender, appearance,
//! voice, role, and the open profile — bio/goals/desires/interests/…) is hers to author
//! ([[persona-identity-is-fully-self-editable-except-the-id]]). [`set`] is the edit
//! verb; [`get`] reads a persona's card (her first-class, public "social card").
//!
//! The read-friendly [`PersonaCardView`] + its projection [`card_view`] live here so
//! both verbs share one shape.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::live::avatar::types::AvatarGender;
use crate::persona::card::PersonaCard;
use crate::sdk_codegen::DynCommand;

pub mod get;
pub mod set;

use set::PersonaIdentitySet;

/// A read-friendly view of a persona's identity card — her name, presentation spine,
/// and open self-authored profile. Echoed by `set` and returned by `get`.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/persona/PersonaCardView.ts")]
pub struct PersonaCardView {
    #[ts(type = "string")]
    pub persona_id: Uuid,
    pub agent_name: String,
    pub gender: String,
    pub pronouns: String,
    pub avatar_vrm: Option<String>,
    pub voice_seed: String,
    pub role: Option<String>,
    pub profile: BTreeMap<String, String>,
}

/// The wire spelling of a gender (matches the `set` param spellings).
pub(crate) fn gender_str(g: AvatarGender) -> &'static str {
    match g {
        AvatarGender::Male => "male",
        AvatarGender::Female => "female",
        AvatarGender::Neutral => "neutral",
    }
}

/// Project a durable [`PersonaCard`] into its read-friendly view.
pub(crate) fn card_view(card: &PersonaCard) -> PersonaCardView {
    PersonaCardView {
        persona_id: card.persona_id,
        agent_name: card.agent_name.clone(),
        gender: gender_str(card.gender).to_string(),
        pronouns: card.pronouns().short(),
        avatar_vrm: card.avatar_vrm.clone(),
        voice_seed: card.voice_seed.clone(),
        role: card.role.map(|r| r.as_str().to_string()),
        profile: card.profile.clone(),
    }
}

/// The dep-holding `persona/identity/*` command objects. Only `set` is dep-holding
/// (it resolves persona homes under `continuum_root` to persist edits); `get` is
/// STATELESS (reads the live card registry) and auto-registers via the macro, so it is
/// deliberately NOT built here.
pub fn command_objects(continuum_root: PathBuf) -> Vec<Arc<dyn DynCommand>> {
    vec![Arc::new(PersonaIdentitySet { continuum_root })]
}
