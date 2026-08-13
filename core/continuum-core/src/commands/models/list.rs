//! `models/list` — the live model universe as the rich API's read surface.
//!
//! A pure projection of the current [`CatalogSnapshot`](crate::model_registry::live::CatalogSnapshot):
//! every model with its static facts (arch, context window, capabilities, cost)
//! AND its runtime status (is it on disk? has `models/try` verified it? what
//! tok/s did we actually measure?). The `generation` rides along so a caller can
//! tell whether the universe changed since it last looked. Lock-free: the body
//! `borrow()`s an `Arc` and projects — it never blocks a concurrent mutation.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model_registry::live::{Availability, LiveModel, ModelCatalog, VerifyReport};
use crate::model_registry::types::{Arch, Capability};

/// `models/list` takes no input — it reports the whole live universe.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/model_registry/ModelsListParams.ts"
)]
pub struct ModelsListParams {}

/// One model's card in the live universe: the static seed facts a caller picks a
/// model on, flattened with the runtime status it acts on. A deliberate
/// widget/persona-facing DTO — the projection of [`LiveModel`], not the internal
/// struct (which embeds the full `Model`).
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/model_registry/ModelSummary.ts"
)]
pub struct ModelSummary {
    pub id: String,
    #[ts(optional)]
    pub name: Option<String>,
    pub provider: String,
    pub arch: Arch,
    #[ts(type = "number")]
    pub context_window: u32,
    #[ts(type = "number")]
    pub max_output_tokens: u32,
    pub tokens_per_second: f32,
    pub capabilities: Vec<Capability>,
    pub cost_input_per_1k: f32,
    pub cost_output_per_1k: f32,
    /// Usable right now, or awaiting a `models/pull`.
    pub availability: Availability,
    /// What `models/try` last learned by actually running the model. `None` until
    /// it has been verified on this host.
    pub verified: Option<VerifyReport>,
}

impl ModelSummary {
    /// Project a live entry into its widget card. The single place the live
    /// universe becomes the read DTO.
    pub fn from_live(live: &LiveModel) -> Self {
        let m = &live.model;
        Self {
            id: m.id.clone(),
            name: m.name.clone(),
            provider: m.provider.clone(),
            arch: m.arch,
            context_window: m.context_window,
            max_output_tokens: m.max_output_tokens,
            tokens_per_second: m.tokens_per_second,
            capabilities: m.capabilities.iter().copied().collect(),
            cost_input_per_1k: m.cost_input_per_1k,
            cost_output_per_1k: m.cost_output_per_1k,
            availability: live.status.availability,
            verified: live.status.verified.clone(),
        }
    }
}

/// The whole live universe at one instant, with the generation that produced it.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/model_registry/ModelCatalogView.ts"
)]
pub struct ModelCatalogView {
    /// Snapshot generation — bumped on every mutation. A subscriber compares this
    /// to its last seen value to know the universe changed without diffing.
    #[ts(type = "number")]
    pub generation: u64,
    #[ts(type = "number")]
    pub count: usize,
    /// Deterministic order (the snapshot is a `BTreeMap` keyed by id).
    pub models: Vec<ModelSummary>,
}

crate::action_command! {
    /// List every model in the live universe with its capabilities, context
    /// window, cost, and runtime status (downloaded? verified? measured speed?).
    /// Use this to see what you can run right now and to pick a model for a task.
    pub struct ModelsList { catalog: Arc<ModelCatalog> }
    name: "models/list",
    access: AiSafe,
    params: ModelsListParams,
    output: ModelCatalogView,
    run(this, _ctx, _p) => {
        let snap = this.catalog.snapshot();
        let models = snap.models.values().map(ModelSummary::from_live).collect();
        Ok(ModelCatalogView {
            generation: snap.generation,
            count: snap.len(),
            models,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_registry::catalog;
    use crate::sdk_codegen::ActionCommand;

    // what this catches: models/list must project the live snapshot faithfully —
    // same count as the seed, and the generation rides along so a subscriber can
    // detect change. A regression in the projection (dropped models, wrong
    // generation) silently hands the widget/persona a stale or partial universe.
    #[tokio::test]
    async fn lists_the_seeded_universe_with_its_generation() {
        let reg = catalog::registry().expect("Rust catalog must validate");
        let catalog = Arc::new(ModelCatalog::from_registry(&reg));
        let cmd = ModelsList {
            catalog: catalog.clone(),
        };
        let ctx = crate::sdk_codegen::Ctx::default();
        let view = cmd
            .run(&ctx, ModelsListParams {})
            .await
            .expect("list must succeed");
        assert_eq!(view.generation, 0, "fresh seed is generation 0");
        assert_eq!(view.count, reg.models().count());
        assert_eq!(view.models.len(), view.count);
        assert!(
            view.models.iter().any(|m| m.provider == "anthropic"),
            "the seeded universe includes the cloud anchor models"
        );
    }

    // what this catches: the wire name mirrors the file path — the routing key
    // every caller (cu, persona tools, the grid ACL) binds to. Drift here breaks
    // the "file tree IS the namespace" contract.
    #[test]
    fn name_mirrors_path() {
        assert_eq!(ModelsList::NAME, "models/list");
    }
}
