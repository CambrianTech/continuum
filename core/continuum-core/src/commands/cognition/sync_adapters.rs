//! `cognition/sync-adapters` — replace a persona's adapter registry from a genome sync
//! (typed, dep-holding).
//!
//! Full (non-incremental) sync: clears the persona's
//! [`AdapterRegistry`](crate::persona::model_selection::AdapterRegistry) and rebuilds it
//! from the supplied [`AdapterInfo`] set, so the Rust-side model-selection view matches
//! the host's genome state exactly. Captures the owning module's
//! [`CognitionState`](crate::modules::cognition::CognitionState); `get_or_create_persona`
//! creates the entry on first sync.
//!
//! `access: Internal` — host-driven cognition IPC (the genome/skill layer pushes adapter
//! state down), not a persona toolbelt verb.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::modules::cognition::CognitionState;
use crate::persona::model_selection::AdapterInfo;

/// Full adapter-set sync for one persona: replace, don't merge.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/SyncAdaptersRequest.ts"
)]
pub struct SyncAdaptersRequest {
    /// Persona whose registry is being replaced.
    #[ts(type = "string")]
    pub persona_id: uuid::Uuid,
    /// The complete adapter set to install (the registry is cleared first).
    pub adapters: Vec<AdapterInfo>,
}

/// Outcome of a full adapter sync.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/SyncAdaptersResult.ts"
)]
pub struct SyncAdaptersResult {
    /// Always true on success (the command fails loud rather than returning false).
    pub synced: bool,
    /// Number of adapters now in the persona's registry.
    #[ts(type = "number")]
    pub adapter_count: usize,
}

crate::action_command! {
    /// Replace a persona's entire adapter registry with the supplied set (full genome
    /// sync, not incremental). Returns how many adapters are now installed. Host-invoked
    /// when the genome/skill layer pushes adapter state down; not a persona toolbelt verb.
    pub struct SyncAdapters { state: Arc<CognitionState> }
    name: "cognition/sync-adapters",
    access: Internal,
    params: SyncAdaptersRequest,
    output: SyncAdaptersResult,
    run(this, _ctx, p) => {
        let mut persona = this.state.get_or_create_persona(p.persona_id);

        // Replace entire adapter set (full sync, not incremental).
        persona.adapter_registry.adapters.clear();
        for adapter in p.adapters {
            persona
                .adapter_registry
                .adapters
                .insert(adapter.name.clone(), adapter);
        }
        let adapter_count = persona.adapter_registry.adapters.len();

        Ok(SyncAdaptersResult {
            synced: true,
            adapter_count,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand};

    // what this catches: the name + access contract. sync-adapters mutates a persona's
    // model-selection view from the genome layer, so it is Internal — registered and
    // grid-routable, never a remote-callable persona toolbelt verb.
    #[test]
    fn name_and_access_are_the_contract() {
        assert_eq!(SyncAdapters::NAME, "cognition/sync-adapters");
        assert_eq!(SyncAdapters::ACCESS, AccessLevel::Internal);
    }
}
