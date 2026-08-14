//! `persona/identity/get` — read a persona's identity card.
//!
//! A persona's card is her first-class, PUBLIC presence (her "social card"), so anyone
//! may read any persona's card — unlike [`set`](super::set), which is self-scoped.
//! Stateless: reads the live card registry, no injected deps.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::sdk_codegen::CommandError;

use super::{card_view, PersonaCardView};

/// Whose card to read. Omit to read YOUR OWN (the authenticated caller). Accepts
/// the full id OR the 8-char short form a persona is shown in rosters (#164).
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/PersonaIdentityGetParams.ts"
)]
pub struct PersonaIdentityGetParams {
    #[serde(default)]
    #[ts(type = "string | null")]
    pub persona_id: Option<crate::identity::PersonaRef>,
}

crate::action_command! {
    /// Read a persona's identity card — her name, gender, pronouns, appearance, voice,
    /// role, and open self-authored profile (bio/goals/desires/interests/…). A card is a
    /// public presence, so anyone may read any persona's. Omit personaId to read your own.
    pub struct PersonaIdentityGet;
    name: "persona/identity/get",
    access: AiSafe,
    params: PersonaIdentityGetParams,
    output: PersonaCardView,
    run(this, ctx, p) => {
        let _ = this;
        // A short/mistyped id a caller quotes back resolves against the personas
        // this process knows (their registered cards) — the ONE id_resolve
        // primitive (#164). Omitted → your own card (the authenticated caller).
        let target_id = match p.persona_id.as_ref().map(|r| r.as_str()) {
            Some(raw) => crate::id_resolve::resolve(raw, &crate::persona::card::ids(), "persona")
                .map_err(CommandError::Invalid)?,
            None => ctx
                .caller
                .as_ref()
                .map(|c| c.peer_id.as_uuid())
                .ok_or_else(|| CommandError::Invalid(
                    "no personaId given and this dispatch carries no caller identity — \
                     specify personaId".into(),
                ))?,
        };
        let card = crate::persona::card::get(&target_id.to_string()).ok_or_else(|| {
            CommandError::NotFound(format!(
                "persona {target_id} is not resident — spawn/resume her to read her card \
                 (persona/instances/list shows who's online)"
            ))
        })?;
        Ok(card_view(&card))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::ActionCommand;

    // what this catches: the read verb's routing + access contract — it mirrors the
    // path and is AiSafe (a card is public; anyone may read any persona's).
    #[test]
    fn name_mirrors_path_and_is_ai_safe() {
        assert_eq!(PersonaIdentityGet::NAME, "persona/identity/get");
        assert!(matches!(
            PersonaIdentityGet::ACCESS,
            crate::sdk_codegen::AccessLevel::AiSafe
        ));
    }
}
