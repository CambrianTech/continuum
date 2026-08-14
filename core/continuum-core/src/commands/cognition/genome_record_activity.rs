//! `cognition/genome-record-activity` — log a domain interaction into the genome's
//! coverage ledger (typed, dep-holding).
//!
//! Records that the persona acted in `domain` (with success/failure) so the
//! [`GenomePagingEngine`](crate::persona::genome_paging) can track which skill domains
//! are exercised — feeding the gap detection that
//! [`super::genome_coverage_report`] reads. Captures the owning module's
//! [`CognitionState`](crate::modules::cognition::CognitionState).
//!
//! `access: Internal` — host-driven cognition IPC, not a persona toolbelt verb.

use std::sync::Arc;

use uuid::Uuid;

use crate::modules::cognition::CognitionState;

fn default_success() -> bool {
    true
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/GenomeRecordActivityParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct GenomeRecordActivityParams {
    /// Persona whose coverage ledger records the activity.
    #[ts(type = "string")]
    pub persona_id: Uuid,
    /// Skill domain the persona acted in (e.g. "code", "chat").
    pub domain: String,
    /// Whether the interaction succeeded (default true).
    #[serde(default = "default_success")]
    pub success: bool,
}

#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/GenomeRecordActivityResult.ts"
)]
pub struct GenomeRecordActivityResult {
    pub recorded: bool,
    pub domain: String,
    pub success: bool,
}

crate::action_command! {
    /// Record that the persona acted in `domain` (success/failure) into its genome
    /// coverage ledger, feeding gap detection. Host-invoked.
    pub struct GenomeRecordActivity { state: Arc<CognitionState> }
    name: "cognition/genome-record-activity",
    access: Internal,
    params: GenomeRecordActivityParams,
    output: GenomeRecordActivityResult,
    run(this, _ctx, p) => {
        let mut persona = this.state.get_or_create_persona(p.persona_id);
        persona.genome_engine.record_activity(&p.domain, p.success);
        Ok(GenomeRecordActivityResult {
            recorded: true,
            domain: p.domain,
            success: p.success,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand};

    // what this catches: the name + access contract. genome-record-activity is
    // host-driven cognition IPC, so it is Internal — registered and grid-routable,
    // never a remote-callable persona toolbelt verb.
    #[test]
    fn name_and_access_are_the_contract() {
        assert_eq!(
            GenomeRecordActivity::NAME,
            "cognition/genome-record-activity"
        );
        assert_eq!(GenomeRecordActivity::ACCESS, AccessLevel::Internal);
    }
}
