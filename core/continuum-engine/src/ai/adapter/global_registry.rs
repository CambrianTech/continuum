//! The process's one adapter registry (the engine's table of inference adapters) and the plain
//! "generate with what is registered" verb over it. The registry is what the
//! layers above populate (the AI provider module registers its adapters at boot) and every
//! inference seam reads. The registry is an engine fact, so it lives with the adapter types;
//! the populating module calls down to it rather than the engine reaching up.

use std::sync::{Arc, LazyLock};

use tokio::sync::RwLock;

use super::{AdapterRegistry, InferenceDevice};
use crate::ai::{RoutingInfo, TextGenerationRequest, TextGenerationResponse};

/// Survives the AI provider module being recreated on a server restart.
static GLOBAL_REGISTRY: LazyLock<Arc<RwLock<AdapterRegistry>>> =
    LazyLock::new(|| Arc::new(RwLock::new(AdapterRegistry::new())));

/// The shared adapter registry.
pub fn global_registry() -> Arc<RwLock<AdapterRegistry>> {
    Arc::clone(&GLOBAL_REGISTRY)
}

/// Provider ID for the Docker Model Runner adapter — single source of truth
/// shared between init-time registration and the watchdog tick.
pub const DMR_PROVIDER_ID: &str = "docker-model-runner";

/// Build the user-visible error message when `select()` returns None.
/// Distinguishes:
///   - "no providers at all" (config issue — surfaces config.env hint)
///   - "asked for local but DMR is down" (Docker Desktop needs to be running)
///   - "asked for a specific provider/model that isn't here" (existing message)
///
/// Hoisted out of both `ai/generate` and the convenience `generate_text` so
/// the two paths report the same diagnosis.
pub fn select_failure_message(
    registry: &AdapterRegistry,
    requested_provider: Option<&str>,
    requested_model: Option<&str>,
) -> String {
    let available = registry.available();
    if available.is_empty() {
        return "No AI providers configured. Add API keys to ~/.continuum/config.env, \
                or start Docker Desktop for local AI."
            .to_string();
    }
    // The "local" sentinel means "give me whatever the best local adapter is."
    // If the user asked for that and DMR isn't in the registry, the watchdog
    // either (a) hasn't seen DMR come up yet or (b) saw it crash and dropped
    // it. Either way, the actionable message is "start Docker Desktop."
    let asked_local = requested_provider == Some("local");
    let dmr_registered = registry.is_registered(DMR_PROVIDER_ID);
    if asked_local && !dmr_registered {
        return format!(
            "Local AI is unavailable — Docker Desktop is not running or Docker Model \
             Runner isn't enabled. To enable: docker desktop enable model-runner --tcp=12434. \
             Other available providers: {:?}",
            available
        );
    }
    // A refusal the caller can ACT on. `available` is provider ids; a caller
    // who named a model (or nothing) needs the model ids the lanes actually
    // serve, or they are stuck guessing — the weak-node consumer case, where
    // the caller has no local registry at all (card a466fdd4).
    let served: Vec<String> = registry
        .served_models()
        .into_iter()
        .map(|(provider, model)| format!("{provider}={model}"))
        .collect();
    match (requested_provider, requested_model) {
        // "local" is the best-local-GPU sentinel, not an adapter id: DMR IS
        // registered here (the branch above handled the down case), so the
        // honest diagnosis is "no local adapter serves that model".
        (Some("local"), model) => format!(
            "No local adapter can serve model {model:?}. \
             Served models (provider=model): {served:?}. \
             Pass `model` as one of those, or `provider` as one of {available:?}."
        ),
        (Some(provider), _) => format!(
            "Provider {provider:?} is not available (model={requested_model:?}). \
             Available providers: {available:?}; served models (provider=model): {served:?}."
        ),
        (None, Some(model)) => format!(
            "Model {model:?} is not served by any available provider. \
             Served models (provider=model): {served:?}. \
             Pass `model` as one of those, or `provider` as one of {available:?}."
        ),
        (None, None) => format!(
            "No provider or model specified — the substrate never picks one for you. \
             Pass `provider` as one of {available:?}, or `model` as one of the served \
             models (provider=model): {served:?}."
        ),
    }
}

/// Generate text using the best available provider
/// This is a convenience function for internal use (e.g., the recipe generator)
pub async fn generate_text(
    registry: &AdapterRegistry,
    request: TextGenerationRequest,
) -> Result<TextGenerationResponse, String> {
    // Device = `Auto` — this convenience helper trusts what the
    // caller specified via `request.provider` + `request.model`.
    // The registered adapter is the authority on its own device
    // class; layering a `Gpu` filter on top of an already-named
    // (provider, model) pair wrongly excludes CPU-only adapters
    // even when they are the only ones claiming the requested
    // model. Used by cognition::analyze and other internal
    // callers that don't have a device opinion.
    let (provider_id, adapter) = registry
        .select(
            request.provider.as_deref(),
            request.model.as_deref(),
            InferenceDevice::Auto,
        )
        .ok_or_else(|| {
            select_failure_message(
                registry,
                request.provider.as_deref(),
                request.model.as_deref(),
            )
        })?;

    let mut response = adapter.generate_text(request).await?;

    RoutingInfo::stamp(
        &mut response.routing,
        provider_id,
        adapter.capabilities().is_local,
        "generate_text_call",
    );
    Ok(response)
}

#[cfg(test)]
mod tests;
