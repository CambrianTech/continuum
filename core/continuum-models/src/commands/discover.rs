//! `models/discover` — query provider listing APIs for available models.
//!
//! Migrated from the legacy `ModelsModule::handle_command` arm. Each provider's
//! `/v1/models` (or native listing API) is queried concurrently off the runtime
//! worker; a provider that fails contributes nothing rather than failing the
//! whole sweep (the listing is best-effort reconnaissance, not a precondition).
//!
//! Slice 2 returns the raw discovered rows. FOLDING them into `Model`s and
//! `register`ing them into the live [`ModelCatalog`](crate::model_registry::live::ModelCatalog)
//! — the "query for more models and they appear live, no reboot" payoff — is the
//! hydration concern (#74): a `DiscoveredModel` → `Model` conversion that earns
//! its own slice + tests rather than being rushed in here.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model_registry::discovery::{discover_all, DiscoveredModel, ProviderConfig};

/// The providers to query. Each carries its base URL + key + any static models
/// for providers without a listing endpoint (e.g. Anthropic).
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/model_registry/ModelsDiscoverParams.ts"
)]
pub struct ModelsDiscoverParams {
    /// Provider configs to query. `ProviderConfig` is an internal IPC shape, so
    /// this command accepts it as opaque JSON on the wire and deserializes it in
    /// the body — the typed surface is the discovered result, not the request.
    #[serde(default)]
    #[ts(type = "Array<unknown>")]
    #[schemars(with = "Vec<serde_json::Value>")]
    pub providers: Vec<ProviderConfig>,
}

/// The discovered model listing across all queried providers.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/model_registry/ModelsDiscoverResult.ts"
)]
pub struct ModelsDiscoverResult {
    pub models: Vec<DiscoveredModel>,
    #[ts(type = "number")]
    pub count: usize,
    #[ts(type = "number")]
    pub providers: usize,
}

crate::action_command! {
    /// Query the configured providers' listing APIs to discover which models they
    /// currently offer. Best-effort: a provider that is unreachable simply
    /// contributes no models. Returns the raw listing for review.
    pub struct ModelsDiscover;
    name: "models/discover",
    access: AiSafe,
    params: ModelsDiscoverParams,
    output: ModelsDiscoverResult,
    run(_this, _ctx, p) => {
        let provider_count = p.providers.len();
        let models = discover_all(p.providers).await;
        let count = models.len();
        crate::log_info!(
            "module",
            "models",
            "Discovered {} models from {} providers",
            count,
            provider_count
        );
        Ok(ModelsDiscoverResult {
            models,
            count,
            providers: provider_count,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{ActionCommand, Ctx};

    // what this catches: discovery with no providers is a clean empty result, not
    // a panic or error — the best-effort contract holds at the zero boundary, and
    // the typed result shape (models/count/providers) stays wired.
    #[tokio::test]
    async fn empty_providers_yields_empty_listing() {
        let cmd = ModelsDiscover;
        let ctx = Ctx::default();
        let r = cmd
            .run(&ctx, ModelsDiscoverParams { providers: vec![] })
            .await
            .expect("empty discovery must succeed");
        assert_eq!(r.count, 0);
        assert_eq!(r.providers, 0);
        assert!(r.models.is_empty());
    }

    #[test]
    fn name_mirrors_path() {
        assert_eq!(ModelsDiscover::NAME, "models/discover");
    }
}
