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
use crate::persona::model_selection::{
    select_model_with_signatures, ModelSelectionRequest, ModelSelectionResult, NeedEmbedding,
};
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
        // Rung 0's inputs, computed at THIS async seam so the selector stays
        // pure + sync (memory/recall's query_embedding pattern): embed the need
        // text through the one recall lane, and join the persona's adapters to
        // their minted signatures by NAME via the manifest + sidecar.
        let need_text = p.need.clone().or_else(|| p.task_domain.clone());
        let (need, signatures) = match need_text {
            Some(text) => {
                let embedder = crate::cognition::embedding::resolve_recall_embedder_local().await;
                let need = NeedEmbedding {
                    embedder_id: embedder.id().to_string(),
                    vector: embedder.embed(&text).await,
                    unrelated_null: embedder.unrelated_null(),
                };
                let mut by_name = std::collections::HashMap::new();
                if let (Ok(manifest), Ok(store_path)) = (
                    crate::forge::adapter_manifest::load(),
                    crate::genome::signature::signature_store_path(),
                ) {
                    if let Ok(store) = crate::genome::signature::SignatureStore::load_at(&store_path) {
                        for a in &manifest {
                            if let Some(sig) = store.by_path.get(&a.path.display().to_string()) {
                                by_name.insert(a.alias.clone(), sig.clone());
                            }
                        }
                    }
                }
                (Some(need), by_name)
            }
            None => (None, std::collections::HashMap::new()),
        };
        let persona = this.state.get_or_create_persona(p.persona_id);
        let result = select_model_with_signatures(
            &p,
            &persona.adapter_registry,
            need.as_ref(),
            &signatures,
        )
        .map_err(|e| CommandError::Internal(e.to_string()))?;
        crate::probe!(
            class = "cognition.model_selection",
            persona = %p.persona_id,
            source = %result.source,
            adapter = %result.adapter_name.as_deref().unwrap_or("-"), // probe display only: "-" reads as no-adapter
            similarity = %result.similarity.unwrap_or(0.0), // probe display only: 0.0 beside a non-distance source reads as not-applicable
            "model selected"
        );
        Ok(result)
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
