//! `cognition/calculate-priority`: a persona's priority score for one message (typed,
//! dep-holding). Every field is required by its type; a request without one never decodes, so
//! there is no "missing" path to write.

use std::sync::Arc;

use uuid::Uuid;

use crate::cognition::conversation::types::SenderType;
use crate::modules::cognition::CognitionState;
use crate::persona::cognition::PriorityScore;
use crate::sdk_codegen::CommandError;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/CalculatePriorityParams.ts"
)]
pub struct CalculatePriorityParams {
    #[ts(type = "string")]
    pub persona_id: Uuid,
    pub content: String,
    pub sender_type: SenderType,
    #[serde(default)]
    pub is_voice: bool,
    #[ts(type = "string")]
    pub room_id: Uuid,
    #[ts(type = "number")]
    pub timestamp: u64,
}

crate::action_command! {
    /// Score one message's priority for a persona. Host-invoked.
    pub struct CalculatePriority { state: Arc<CognitionState> }
    name: "cognition/calculate-priority",
    access: Internal,
    params: CalculatePriorityParams,
    output: PriorityScore,
    run(this, _ctx, p) => {
        let persona = this
            .state
            .personas
            .get(&p.persona_id)
            .ok_or_else(|| CommandError::NotFound(format!("no cognition for persona {}", p.persona_id)))?;
        Ok(persona.engine.calculate_priority(&p.content, p.sender_type, p.is_voice, p.room_id, p.timestamp))
    }
}
