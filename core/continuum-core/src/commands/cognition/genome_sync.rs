//! `cognition/genome-sync` — reconcile the persona's genome to a supplied adapter set
//! (typed, dep-holding).
//!
//! Pushes the full adapter inventory (name, domain, size, priority, loaded-state) into
//! the [`GenomePagingEngine`](crate::persona::genome_paging) so its resident/LRU
//! bookkeeping matches ground truth, then reports the post-sync memory picture. Captures
//! the owning module's [`CognitionState`](crate::modules::cognition::CognitionState).
//!
//! `access: Internal` — host-driven cognition IPC, not a persona toolbelt verb.

use std::sync::Arc;

use uuid::Uuid;

use crate::modules::cognition::CognitionState;
use crate::persona::genome_paging::GenomeAdapterInfo;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/GenomeSyncParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct GenomeSyncParams {
    /// Persona whose genome is reconciled.
    #[ts(type = "string")]
    pub persona_id: Uuid,
    /// The full adapter inventory to sync into the paging engine.
    pub adapters: Vec<GenomeAdapterInfo>,
    /// Memory budget in MB; 0 or omitted uses the GPU-detected per-persona budget.
    #[serde(default)]
    #[ts(type = "number")]
    pub memory_budget_mb: f32,
}

#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/GenomeSyncResult.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct GenomeSyncResult {
    pub synced: bool,
    #[ts(type = "number")]
    pub adapter_count: usize,
    #[ts(type = "number")]
    pub active_count: usize,
    #[ts(type = "number")]
    pub memory_used_mb: f32,
    #[ts(type = "number")]
    pub memory_pressure: f32,
}

crate::action_command! {
    /// Reconcile the persona's genome paging engine to the supplied adapter inventory
    /// and report the post-sync memory picture. Host-invoked.
    pub struct GenomeSync { state: Arc<CognitionState> }
    name: "cognition/genome-sync",
    access: Internal,
    params: GenomeSyncParams,
    output: GenomeSyncResult,
    run(this, _ctx, p) => {
        let memory_budget_mb = if p.memory_budget_mb > 0.0 {
            p.memory_budget_mb
        } else {
            this.state.per_persona_budget_mb()
        };

        let adapter_count = p.adapters.len();
        let active_count = p.adapters.iter().filter(|a| a.is_loaded).count();

        let mut persona = this.state.get_or_create_persona(p.persona_id);
        persona.genome_engine.memory_budget_mb = memory_budget_mb;
        persona.genome_engine.sync_state(p.adapters);

        let memory_used_mb = persona.genome_engine.memory_used_mb;
        let memory_pressure = persona.genome_engine.memory_pressure();

        crate::log_info!(
            "module",
            "cognition",
            "genome-sync {}: {} adapters ({} active), budget={}MB, used={}MB",
            p.persona_id,
            adapter_count,
            active_count,
            persona.genome_engine.memory_budget_mb,
            memory_used_mb
        );

        Ok(GenomeSyncResult {
            synced: true,
            adapter_count,
            active_count,
            memory_used_mb,
            memory_pressure,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand};

    // what this catches: the name + access contract. genome-sync is host-driven
    // cognition IPC, so it is Internal — registered and grid-routable, never a
    // remote-callable persona toolbelt verb.
    #[test]
    fn name_and_access_are_the_contract() {
        assert_eq!(GenomeSync::NAME, "cognition/genome-sync");
        assert_eq!(GenomeSync::ACCESS, AccessLevel::Internal);
    }
}
