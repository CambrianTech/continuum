//! `persona/hands/adopt` — the explicit repair of a head that could not resume (card 9cf06ade).
//!
//! Her head's epoch keeps the exact tool schemas her engine holds across restarts
//! (`hands_custody`). When it cannot be resumed (its record does not decode, or her view was
//! checkpointed with no head beside it), her acts wait rather than silently taking the live tools
//! in its place (Codex's review: no compatibility fallback). This verb is the explicit step that
//! takes the live surface as a new epoch of her head at her next act; her view is kept and the
//! engine prefills what differs from her head on. She may call it on herself; an operator may too.

use uuid::Uuid;

use crate::sdk_codegen::{Category, CommandError, NextMoves, NoMove, Refusal, RefusalCause};

/// Why a persona's head cannot be adopted on this node.
#[derive(Debug, thiserror::Error)]
pub enum AdoptRefusal {
    #[error("no persona here answers to {reference}: {why}")]
    Unresolved { reference: String, why: String },
    #[error("persona {persona} has no mind assembled on this node")]
    NotHere { persona: Uuid },
    #[error("persona {persona} has no hands whose head could be adopted")]
    NoHands { persona: Uuid },
}

impl RefusalCause for AdoptRefusal {
    fn category(&self) -> Category {
        Category::NotFound
    }
}

/// A refusal she can read: nothing of hers moves an adoption for a persona that is not here.
fn refused(cause: AdoptRefusal) -> CommandError {
    Refusal::new(cause, NextMoves::None(NoMove::NothingMoreExists)).into()
}

/// Whose head to adopt from the live tool surface.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/persona/PersonaHandsAdoptParams.ts")]
pub struct PersonaHandsAdoptParams {
    /// The persona: full UUID, 8-char short id, or name.
    pub persona: crate::identity::PersonaRef,
}

/// The adoption her next act makes.
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../../protocol/typescript/persona/PersonaHandsAdoptResult.ts")]
pub struct PersonaHandsAdoptResult {
    /// Her persona id.
    #[ts(type = "string")]
    pub persona_id: Uuid,
}

crate::action_command! {
    /// Adopt a persona's head from the live tool surface at her next act: the explicit repair of a
    /// head that could not resume (a record that does not decode, or a view checkpointed with no head
    /// beside it). Her view is kept; the engine prefills what differs from her head on.
    pub struct PersonaHandsAdopt;
    name: "persona/hands/adopt",
    access: AiSafe,
    params: PersonaHandsAdoptParams,
    output: PersonaHandsAdoptResult,
    run(_this, ctx, params) => {
        let registry = crate::cognition::persona_workspace::global();
        let persona_id = registry
            .resolve_persona(&params.persona)
            .map_err(|why| refused(AdoptRefusal::Unresolved { reference: params.persona.as_str().to_string(), why }))?
            .as_uuid();
        let cycle = registry.get(&persona_id).ok_or_else(|| refused(AdoptRefusal::NotHere { persona: persona_id }))?;
        let body = cycle.acting().ok_or_else(|| refused(AdoptRefusal::NoHands { persona: persona_id }))?;
        let by = ctx.caller.as_ref().map_or_else(|| "the substrate".to_string(), |caller| format!("{caller:?}")); // map_or_else: no caller identity is the substrate's own local code
        body.working_memory.request_hands_adoption(&by);
        Ok(PersonaHandsAdoptResult { persona_id })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand};

    // what this catches: the name and access contract. Adoption is a verb she may call on herself
    // (AiSafe), never a substrate-only one she could not reach when her head waits for it.
    #[test]
    fn name_and_access_are_the_contract() {
        assert_eq!(PersonaHandsAdopt::NAME, "persona/hands/adopt");
        assert_eq!(PersonaHandsAdopt::ACCESS, AccessLevel::AiSafe);
    }
}
