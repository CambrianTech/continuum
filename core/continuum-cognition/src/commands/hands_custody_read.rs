//! `persona/hands/custody`: a persona's head custody as an operator reads it before asking an
//! adoption over it (card 9cf06ade): its digest, what it holds, and how far her accepted frontier
//! reaches. `persona/hands/adopt_capture` names the digest it is asked over.

use uuid::Uuid;

use crate::sdk_codegen::{Category, CommandError, NextMoves, NoMove, Refusal, RefusalCause};

/// Why a persona's custody cannot be read on this node.
#[derive(Debug, thiserror::Error)]
pub(crate) enum CustodyReadRefusal {
    #[error("no persona here answers to {reference}: {why}")]
    Unresolved { reference: String, why: String },
    #[error("persona {persona} has no mind assembled on this node")]
    NotHere { persona: Uuid },
    #[error("persona {persona} has no hands whose custody could be read")]
    NoHands { persona: Uuid },
    #[error("persona {persona}'s custody cannot be digested")]
    Undigestable {
        persona: Uuid,
        #[source]
        source: crate::cognition::capture_adoption::CaptureMismatch,
    },
}

impl RefusalCause for CustodyReadRefusal {
    fn category(&self) -> Category {
        match self {
            CustodyReadRefusal::Undigestable { .. } => Category::Invalid,
            CustodyReadRefusal::Unresolved { .. } | CustodyReadRefusal::NotHere { .. } | CustodyReadRefusal::NoHands { .. } => Category::NotFound,
        }
    }
}

fn refused(cause: CustodyReadRefusal) -> CommandError {
    Refusal::new(cause, NextMoves::None(NoMove::NothingMoreExists)).into()
}

/// Whose head custody to read.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/persona/PersonaHandsCustodyParams.ts")]
pub struct PersonaHandsCustodyParams {
    /// The persona: full UUID, 8-char short id, or name.
    pub persona: crate::identity::PersonaRef,
}

/// Her head custody now.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../../protocol/typescript/persona/PersonaHandsCustodyResult.ts")]
pub struct PersonaHandsCustodyResult {
    /// Her persona id.
    #[ts(type = "string")]
    pub persona_id: Uuid,
    /// Her custody's sha256: what persona/hands/adopt_capture is asked over.
    pub custody_sha256: String,
    /// What it holds: held generation N, corrupt, or unrecorded.
    pub holds: String,
    /// How many events of her view's epoch her last sent act carried; absent with no view.
    #[ts(optional, type = "number")]
    pub frontier_events: Option<u64>,
}

crate::action_command! {
    /// Read a persona's head custody: its digest (what an adoption from a capture is asked over),
    /// what it holds, and how far her accepted frontier reaches.
    pub struct PersonaHandsCustody;
    name: "persona/hands/custody",
    access: AiSafe,
    params: PersonaHandsCustodyParams,
    output: PersonaHandsCustodyResult,
    run(_this, _ctx, params) => {
        let registry = crate::cognition::persona_workspace::global();
        let persona_id = registry
            .resolve_persona(&params.persona)
            .map_err(|why| refused(CustodyReadRefusal::Unresolved { reference: params.persona.as_str().to_string(), why }))?
            .as_uuid();
        let cycle = registry.get(&persona_id).ok_or_else(|| refused(CustodyReadRefusal::NotHere { persona: persona_id }))?;
        let body = cycle.acting().ok_or_else(|| refused(CustodyReadRefusal::NoHands { persona: persona_id }))?;
        let receipt = body.working_memory.custody_receipt().map_err(|source| refused(CustodyReadRefusal::Undigestable { persona: persona_id, source }))?;
        Ok(PersonaHandsCustodyResult {
            persona_id,
            custody_sha256: receipt.sha256,
            holds: receipt.holds,
            frontier_events: receipt.frontier_events,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand};

    // what this catches: the name and access contract of the read an adoption is asked over.
    #[test]
    fn name_and_access_are_the_contract() {
        assert_eq!(PersonaHandsCustody::NAME, "persona/hands/custody");
        assert_eq!(PersonaHandsCustody::ACCESS, AccessLevel::AiSafe);
    }
}
