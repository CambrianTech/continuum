//! `cognition/create-engine` — instantiate a persona's cognition engine (typed,
//! dep-holding).
//!
//! Creates a fresh [`PersonaCognition`](crate::persona::PersonaCognition) for the
//! persona and inserts it into the module's shared persona map, sharing the module's
//! RAG engine. Captures the owning module's
//! [`CognitionState`](crate::modules::cognition::CognitionState).
//!
//! `access: Internal` — host-driven persona lifecycle IPC, not a persona toolbelt verb.

use std::sync::Arc;

use uuid::Uuid;

use crate::modules::cognition::CognitionState;
use crate::persona::PersonaCognition;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/CreateEngineParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct CreateEngineParams {
    /// Persona to create a cognition engine for.
    #[ts(type = "string")]
    pub persona_id: Uuid,
    /// Human-readable persona name (drives the `@mention` marker).
    pub persona_name: String,
}

#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/CreateEngineResult.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct CreateEngineResult {
    pub created: bool,
}

crate::action_command! {
    /// Instantiate a persona's cognition engine, sharing the module's RAG engine.
    /// Host-invoked persona lifecycle.
    pub struct CreateEngine { state: Arc<CognitionState> }
    name: "cognition/create-engine",
    access: Internal,
    params: CreateEngineParams,
    output: CreateEngineResult,
    run(this, _ctx, p) => {
        let cognition = PersonaCognition::new(
            p.persona_id,
            p.persona_name.clone(),
            this.state.rag_engine.clone(),
        );
        this.state.personas.insert(p.persona_id, cognition);

        crate::log_info!(
            "module",
            "cognition",
            "Created cognition for {}",
            p.persona_id
        );

        Ok(CreateEngineResult { created: true })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand};

    // what this catches: the name + access contract. create-engine is host-driven
    // persona lifecycle IPC, so it is Internal — registered and grid-routable,
    // never a remote-callable persona toolbelt verb.
    #[test]
    fn name_and_access_are_the_contract() {
        assert_eq!(CreateEngine::NAME, "cognition/create-engine");
        assert_eq!(CreateEngine::ACCESS, AccessLevel::Internal);
    }
}
