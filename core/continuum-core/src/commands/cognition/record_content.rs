//! `cognition/record-content` — commit content into a persona's recent-content memory
//! (typed, dep-holding).
//!
//! The mutating companion to [`super::check_content_dedup`]: records that the persona
//! said `content` in the room so future checks can detect the repeat. Captures the
//! owning module's [`CognitionState`](crate::modules::cognition::CognitionState) and
//! lazily creates the persona via `get_or_create_persona`.
//!
//! `access: Internal` — host-driven cognition IPC, not a persona toolbelt verb.

use std::sync::Arc;

use uuid::Uuid;

use crate::modules::cognition::CognitionState;
use crate::sdk_codegen::CommandError;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/RecordContentParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct RecordContentParams {
    /// Persona whose recent-content memory records the content.
    #[ts(type = "string")]
    pub persona_id: Uuid,
    /// Room the content belongs to.
    #[ts(type = "string")]
    pub room_id: Uuid,
    /// Content to record.
    pub content: String,
}

#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/RecordContentResult.ts"
)]
pub struct RecordContentResult {
    pub success: bool,
    pub recorded: bool,
}

crate::action_command! {
    /// Record that the persona said `content` in this room, so later
    /// `check-content-dedup` calls detect the repeat. Host-invoked.
    pub struct RecordContent { state: Arc<CognitionState> }
    name: "cognition/record-content",
    access: Internal,
    params: RecordContentParams,
    output: RecordContentResult,
    run(this, _ctx, p) => {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| CommandError::Internal(format!("system clock before UNIX epoch: {e}")))?
            .as_millis() as u64;
        this.state
            .get_or_create_persona(p.persona_id)
            .content_dedup
            .record(&p.content, p.room_id, now_ms);
        Ok(RecordContentResult { success: true, recorded: true })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand};

    // what this catches: the name + access contract. record-content is host-driven
    // cognition IPC, so it is Internal — registered and grid-routable, never a
    // remote-callable persona toolbelt verb.
    #[test]
    fn name_and_access_are_the_contract() {
        assert_eq!(RecordContent::NAME, "cognition/record-content");
        assert_eq!(RecordContent::ACCESS, AccessLevel::Internal);
    }
}
