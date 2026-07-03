//! `cognition/select-model` — pick the best adapter-backed model for a persona turn
//! (typed, dep-holding).
//!
//! Runs [`select_model`](crate::persona::model_selection::select_model) over the
//! persona's [`AdapterRegistry`](crate::persona::model_selection::AdapterRegistry),
//! choosing a trained adapter model by trait match → current adapter → any adapter, and
//! returns which model plus why (source tier, adapter name, matched trait, decision
//! time). Captures the owning module's
//! [`CognitionState`](crate::modules::cognition::CognitionState). Uses
//! `get_or_create_persona` (create-on-missing) to mirror the legacy arm — the registry
//! for a never-seen persona is empty, so selection fails loud with `NoCandidate` rather
//! than inventing a model.
//!
//! `access: Internal` — host-driven cognition IPC, not a persona toolbelt verb.

use std::sync::Arc;

use crate::modules::cognition::CognitionState;
use crate::persona::model_selection::{select_model, ModelSelectionRequest, ModelSelectionResult};
use crate::sdk_codegen::CommandError;

crate::action_command! {
    /// Select the best adapter-backed model for a persona given an optional task domain.
    /// Returns the chosen model plus provenance (source tier, adapter, trait, decision
    /// time). Host-invoked; fails loud when no trained candidate exists.
    pub struct SelectModel { state: Arc<CognitionState> }
    name: "cognition/select-model",
    access: Internal,
    params: ModelSelectionRequest,
    output: ModelSelectionResult,
    run(this, _ctx, p) => {
        let persona = this.state.get_or_create_persona(p.persona_id);
        select_model(&p, &persona.adapter_registry)
            .map_err(|e| CommandError::Internal(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand};

    // what this catches: the name + access contract. select-model is host-driven
    // cognition IPC, so it is Internal — registered and grid-routable, never a
    // remote-callable persona toolbelt verb.
    #[test]
    fn name_and_access_are_the_contract() {
        assert_eq!(SelectModel::NAME, "cognition/select-model");
        assert_eq!(SelectModel::ACCESS, AccessLevel::Internal);
    }
}
