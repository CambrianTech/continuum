//! `cognition/has-evaluated` — "have I already judged this message?" check (typed,
//! dep-holding).
//!
//! Non-mutating read of the persona's evaluation ledger: returns whether the persona
//! already ran its response-decision pipeline against `message_id`. Companion to
//! [`super::mark_evaluated`] (which commits the ledger entry). Captures the owning
//! module's [`CognitionState`](crate::modules::cognition::CognitionState).
//!
//! `access: Internal` — host-driven cognition IPC, not a persona toolbelt verb.

use std::sync::Arc;

use uuid::Uuid;

use crate::modules::cognition::CognitionState;
use crate::sdk_codegen::CommandError;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/HasEvaluatedParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct HasEvaluatedParams {
    /// Persona whose evaluation ledger is consulted.
    #[ts(type = "string")]
    pub persona_id: Uuid,
    /// Message to test for prior evaluation.
    #[ts(type = "string")]
    pub message_id: Uuid,
}

#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/HasEvaluatedResult.ts"
)]
pub struct HasEvaluatedResult {
    pub evaluated: bool,
}

crate::action_command! {
    /// Check whether the persona already evaluated `message_id` (non-mutating), so the
    /// cognitive loop doesn't re-judge the same message. Host-invoked.
    pub struct HasEvaluated { state: Arc<CognitionState> }
    name: "cognition/has-evaluated",
    access: Internal,
    params: HasEvaluatedParams,
    output: HasEvaluatedResult,
    run(this, _ctx, p) => {
        let persona = this
            .state
            .personas
            .get(&p.persona_id)
            .ok_or_else(|| CommandError::NotFound(format!("No cognition for {}", p.persona_id)))?;
        let evaluated = persona.engine.has_evaluated_message(p.message_id);
        Ok(HasEvaluatedResult { evaluated })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand};

    // what this catches: the name + access contract. has-evaluated is host-driven
    // cognition IPC, so it is Internal — registered and grid-routable, never a
    // remote-callable persona toolbelt verb.
    #[test]
    fn name_and_access_are_the_contract() {
        assert_eq!(HasEvaluated::NAME, "cognition/has-evaluated");
        assert_eq!(HasEvaluated::ACCESS, AccessLevel::Internal);
    }
}
