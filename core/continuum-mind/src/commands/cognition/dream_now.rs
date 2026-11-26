//! `cognition/dream-now`, the factory flywheel button: ask a persona's dream for ONE pass now instead
//! of waiting for her next rest or experience. Her dream part takes it on her bus like any other ask,
//! one pass of hers at a time, so a pass already running finishes first and this one follows it.
//!
//! `access: Internal` — operator/proctor-driven schedule control, not a persona
//! toolbelt verb (personas do not force each other's sleep).

use std::sync::Arc;

use uuid::Uuid;

use crate::modules::cognition::CognitionState;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/DreamNowParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct DreamNowParams {
    /// Persona whose dream pass is being forced.
    #[ts(type = "string")]
    pub persona_id: Uuid,
}

/// Whether her dream was asked. The pass runs on her dream part (results land through
/// `admit_reflection` and the `hippocampus.supersede` / `persona.dream.pass_complete` probes).
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/DreamNowResult.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct DreamNowResult {
    /// True when she is resident on this core and her dream was asked.
    pub asked: bool,
}

crate::action_command! {
    /// Force one dream-consolidation pass for a persona now (factory flywheel).
    pub struct DreamNow { state: Arc<CognitionState> }
    name: "cognition/dream-now",
    access: Internal,
    params: DreamNowParams,
    output: DreamNowResult,
    run(this, _ctx, p) => {
        let _ = &this.state; // command host plumbing; her dream part is the authority
        let Some(her) = crate::persona::senses::of(p.persona_id) else {
            return Ok(DreamNowResult { asked: false });
        };
        her.ask_dream(crate::cognition::dream_consolidation::Ask::Now);
        Ok(DreamNowResult { asked: true })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand};

    // what this catches: the name + access contract. dream-now is operator
    // schedule-control (factory flywheel) — Internal, never a persona toolbelt
    // verb; and the wire name must keep routing to this command.
    #[test]
    fn name_and_access_are_the_contract() {
        assert_eq!(DreamNow::NAME, "cognition/dream-now");
        assert_eq!(DreamNow::ACCESS, AccessLevel::Internal);
    }
}
