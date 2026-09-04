//! `models/capabilities` — the canonical capability vocabulary for one model id.
//!
//! Migrated from the legacy `ModelsModule::handle_command` arm. It now reads the
//! LIVE catalog (so a freshly discovered or forged model answers too), not just
//! the immutable seed. Intentionally strict: a caller that only knows desired
//! capabilities must go through the allocator/resolver, not send a raw provider
//! or HuggingFace string here — an unknown id fails loud, naming the boundary.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model_registry::live::ModelCatalog;
use crate::model_registry::types::Capability;
use crate::sdk_codegen::CommandError;

/// Look up one model by its catalog id.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/model_registry/ModelsCapabilitiesParams.ts"
)]
pub struct ModelsCapabilitiesParams {
    /// The catalog model id (e.g. `qwen2.5-omni-7b-instruct`), not a raw provider
    /// artifact name.
    pub model_id: String,
}

/// The model's closed capability set.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/model_registry/ModelCapabilities.ts"
)]
pub struct ModelCapabilities {
    pub model_id: String,
    pub capabilities: Vec<Capability>,
}

crate::action_command! {
    /// Report the capability set (text, vision, audio-in/out, tool-use, …) for a
    /// known model id. Use this to check whether a model can do what a task needs
    /// before you select it.
    pub struct ModelsCapabilities { catalog: Arc<ModelCatalog> }
    name: "models/capabilities",
    access: AiSafe,
    params: ModelsCapabilitiesParams,
    output: ModelCapabilities,
    run(this, _ctx, p) => {
        let snap = this.catalog.snapshot();
        let live = snap.get(&p.model_id).ok_or_else(|| {
            CommandError::NotFound(format!(
                "unknown model id '{}' — call the model allocator/resolver instead of naming a provider artifact",
                p.model_id
            ))
        })?;
        let capabilities = live.model.capabilities.iter().copied().collect();
        Ok(ModelCapabilities {
            model_id: p.model_id,
            capabilities,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_registry::catalog;
    use crate::sdk_codegen::{ActionCommand, Ctx};

    // what this catches: a known model returns its capability set from the LIVE
    // catalog, and an unknown id fails loud as NotFound (not an empty success).
    // The Omni anchor carries Vision + AudioInput — a catalog edit that drops a
    // sensory cap, or a migration that silently swallows unknown ids, gets caught.
    #[tokio::test]
    async fn known_model_reports_caps_unknown_fails_loud() {
        let reg = catalog::registry().expect("Rust catalog must validate");
        let catalog = Arc::new(ModelCatalog::from_registry(&reg));
        let cmd = ModelsCapabilities {
            catalog: catalog.clone(),
        };
        let ctx = Ctx::default();

        let omni = cmd
            .run(
                &ctx,
                ModelsCapabilitiesParams {
                    model_id: "qwen2.5-omni-7b-instruct".into(),
                },
            )
            .await
            .expect("known model must resolve");
        assert!(omni.capabilities.contains(&Capability::Vision));
        assert!(omni.capabilities.contains(&Capability::AudioInput));

        let err = cmd
            .run(
                &ctx,
                ModelsCapabilitiesParams {
                    model_id: "definitely-not-a-model".into(),
                },
            )
            .await
            .expect_err("unknown id must fail loud");
        assert!(matches!(err, CommandError::NotFound(_)));
    }

    #[test]
    fn name_mirrors_path() {
        assert_eq!(ModelsCapabilities::NAME, "models/capabilities");
    }
}
