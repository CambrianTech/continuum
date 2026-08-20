//! `cognition/genome-state` — snapshot the persona's genome paging state (typed,
//! dep-holding).
//!
//! Non-mutating read of the [`GenomePagingEngine`](crate::persona::genome_paging):
//! which adapters are resident, memory used/budget, and pressure. Reuses the engine's
//! [`GenomePagingState`] as the output. Captures the owning module's
//! [`CognitionState`](crate::modules::cognition::CognitionState).
//!
//! `access: Internal` — host-driven cognition IPC, not a persona toolbelt verb.

use std::sync::Arc;

use uuid::Uuid;

use crate::modules::cognition::CognitionState;
use crate::persona::genome_paging::GenomePagingState;
use crate::sdk_codegen::CommandError;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/GenomeStateParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct GenomeStateParams {
    /// Persona whose genome state is read.
    #[ts(type = "string")]
    pub persona_id: Uuid,
}

crate::action_command! {
    /// Snapshot the persona's genome paging state (resident adapters, memory,
    /// pressure). Non-mutating. Host-invoked.
    pub struct GenomeState { state: Arc<CognitionState> }
    name: "cognition/genome-state",
    access: Internal,
    params: GenomeStateParams,
    output: GenomePagingState,
    run(this, _ctx, p) => {
        let persona = this
            .state
            .personas
            .get(&p.persona_id)
            .ok_or_else(|| CommandError::NotFound(format!("No cognition for {}", p.persona_id)))?;
        Ok(persona.genome_engine.state())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand};

    // what this catches: the name + access contract. genome-state is host-driven
    // cognition IPC, so it is Internal — registered and grid-routable, never a
    // remote-callable persona toolbelt verb.
    #[test]
    fn name_and_access_are_the_contract() {
        assert_eq!(GenomeState::NAME, "cognition/genome-state");
        assert_eq!(GenomeState::ACCESS, AccessLevel::Internal);
    }
}
