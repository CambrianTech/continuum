//! `cognition/fast-path-decision`: a persona's fast-path decision on one inbox message (typed,
//! dep-holding). The message is the domain [`InboxMessage`] itself: decoded by serde at the
//! boundary, so no field can be missing past it.

use std::sync::Arc;

use uuid::Uuid;

use crate::cognition::conversation::types::InboxMessage;
use crate::modules::cognition::CognitionState;
use crate::persona::cognition::CognitionDecision;
use crate::sdk_codegen::CommandError;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/FastPathDecisionParams.ts"
)]
pub struct FastPathDecisionParams {
    #[ts(type = "string")]
    pub persona_id: Uuid,
    pub message: InboxMessage,
}

crate::action_command! {
    /// Decide a message on the persona's fast path. Host-invoked.
    pub struct FastPathDecision { state: Arc<CognitionState> }
    name: "cognition/fast-path-decision",
    access: Internal,
    params: FastPathDecisionParams,
    output: CognitionDecision,
    run(this, _ctx, p) => {
        let persona = this
            .state
            .personas
            .get(&p.persona_id)
            .ok_or_else(|| CommandError::NotFound(format!("no cognition for persona {}", p.persona_id)))?;
        Ok(persona.engine.fast_path_decision(&p.message))
    }
}
