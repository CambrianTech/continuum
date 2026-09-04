//! `cognition/check-content-dedup` — "have I already said this here?" check (typed,
//! dep-holding).
//!
//! Reads the persona's [`ContentDeduplicator`](crate::persona::message_cache) without
//! mutating it: returns whether `content` duplicates something recently seen in the
//! room, plus the check latency. Companion to [`super::record_content`] (which commits
//! the content). Captures the owning module's
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
    export_to = "../../../protocol/typescript/cognition/CheckContentDedupParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct CheckContentDedupParams {
    /// Persona whose recent-content memory is consulted.
    #[ts(type = "string")]
    pub persona_id: Uuid,
    /// Room the check is scoped to.
    #[ts(type = "string")]
    pub room_id: Uuid,
    /// Candidate content to test for redundancy.
    pub content: String,
}

#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/CheckContentDedupResult.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct CheckContentDedupResult {
    pub success: bool,
    pub is_duplicate: bool,
    #[ts(type = "number")]
    pub check_time_us: u64,
}

crate::action_command! {
    /// Check whether `content` duplicates something the persona recently said in this
    /// room (non-mutating). Returns is_duplicate + check latency. Host-invoked.
    pub struct CheckContentDedup { state: Arc<CognitionState> }
    name: "cognition/check-content-dedup",
    access: Internal,
    params: CheckContentDedupParams,
    output: CheckContentDedupResult,
    run(this, _ctx, p) => {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| CommandError::Internal(format!("system clock before UNIX epoch: {e}")))?
            .as_millis() as u64;
        let persona = this
            .state
            .personas
            .get(&p.persona_id)
            .ok_or_else(|| CommandError::NotFound(format!("No cognition for {}", p.persona_id)))?;
        let result = persona.content_dedup.is_duplicate(&p.content, p.room_id, now_ms);
        Ok(CheckContentDedupResult {
            success: true,
            is_duplicate: result.is_duplicate,
            check_time_us: result.check_time_us,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand};

    // what this catches: the name + access contract. check-content-dedup is host-driven
    // cognition IPC, so it is Internal — registered and grid-routable, never a
    // remote-callable persona toolbelt verb.
    #[test]
    fn name_and_access_are_the_contract() {
        assert_eq!(CheckContentDedup::NAME, "cognition/check-content-dedup");
        assert_eq!(CheckContentDedup::ACCESS, AccessLevel::Internal);
    }
}
