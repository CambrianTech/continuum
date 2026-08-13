//! `cognition/mark-evaluated` — commit a message into the persona's evaluation ledger
//! (typed, dep-holding).
//!
//! The mutating companion to [`super::has_evaluated`]: records that the persona has run
//! its response-decision pipeline against `message_id`, so future `has-evaluated` checks
//! short-circuit. Captures the owning module's
//! [`CognitionState`](crate::modules::cognition::CognitionState).
//!
//! `access: Internal` — host-driven cognition IPC, not a persona toolbelt verb.

use std::sync::Arc;

use uuid::Uuid;

use crate::modules::cognition::CognitionState;
use crate::sdk_codegen::CommandError;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/MarkEvaluatedParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct MarkEvaluatedParams {
    /// Persona whose evaluation ledger records the message.
    #[ts(type = "string")]
    pub persona_id: Uuid,
    /// Message to mark as evaluated.
    #[ts(type = "string")]
    pub message_id: Uuid,
}

#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/MarkEvaluatedResult.ts"
)]
pub struct MarkEvaluatedResult {
    pub marked: bool,
}

crate::action_command! {
    /// Record that the persona evaluated `message_id`, so later `has-evaluated` checks
    /// short-circuit. Host-invoked.
    pub struct MarkEvaluated { state: Arc<CognitionState> }
    name: "cognition/mark-evaluated",
    access: Internal,
    params: MarkEvaluatedParams,
    output: MarkEvaluatedResult,
    run(this, _ctx, p) => {
        let persona = this
            .state
            .personas
            .get(&p.persona_id)
            .ok_or_else(|| CommandError::NotFound(format!("No cognition for {}", p.persona_id)))?;
        persona.engine.mark_message_evaluated(p.message_id);
        Ok(MarkEvaluatedResult { marked: true })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand};

    // what this catches: the name + access contract. mark-evaluated is host-driven
    // cognition IPC, so it is Internal — registered and grid-routable, never a
    // remote-callable persona toolbelt verb.
    #[test]
    fn name_and_access_are_the_contract() {
        assert_eq!(MarkEvaluated::NAME, "cognition/mark-evaluated");
        assert_eq!(MarkEvaluated::ACCESS, AccessLevel::Internal);
    }
}
