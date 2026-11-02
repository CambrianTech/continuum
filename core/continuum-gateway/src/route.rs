//! One route from a decoded call to the engine: select the adapter the residents use, then
//! generate. No fallback: an unserved model is refused with what is served.

use std::sync::Arc;

use continuum_engine::ai::adapter::{GenerationChunk, InferenceDevice};
use continuum_engine::ai::{AIProviderAdapter, TextGenerationRequest, TextGenerationResponse};

/// Why a call could not be served, as the status an agent understands.
#[derive(Debug)]
pub(crate) enum RouteError {
    /// The model named is not served here; carries what is.
    UnknownModel { model: String, served: Vec<String> },
    /// The engine refused or failed mid-generation.
    Engine(String),
}

/// The adapter that serves `model` on this node, held without the registry lock.
pub(crate) async fn select(model: &str) -> Result<Arc<dyn AIProviderAdapter>, RouteError> {
    let registry = continuum_engine::ai::adapter::global_registry();
    let registry = registry.read().await;
    match registry.select_arc(Some("local"), Some(model), InferenceDevice::Auto) {
        Some((_, adapter)) => Ok(adapter),
        None => Err(RouteError::UnknownModel { model: model.to_owned(), served: served_models_of(&registry) }),
    }
}

/// Every model this node serves, for `GET /v1/models` and for a refusal.
pub(crate) async fn served_models() -> Vec<String> {
    let registry = continuum_engine::ai::adapter::global_registry();
    let registry = registry.read().await;
    served_models_of(&registry)
}

fn served_models_of(registry: &continuum_engine::ai::AdapterRegistry) -> Vec<String> {
    let mut models: Vec<String> = registry
        .available()
        .into_iter()
        .filter_map(|id| registry.get(id))
        .flat_map(|adapter| adapter.served_model_ids())
        .collect();
    models.sort();
    models.dedup();
    models
}

/// Generate the whole answer.
pub(crate) async fn generate(adapter: &Arc<dyn AIProviderAdapter>, request: TextGenerationRequest) -> Result<TextGenerationResponse, RouteError> {
    adapter.generate_text(request).await.map_err(RouteError::Engine)
}

/// Generate, sending each chunk to `sink` as the engine decodes it.
pub(crate) async fn stream(
    adapter: &Arc<dyn AIProviderAdapter>,
    request: TextGenerationRequest,
    sink: tokio::sync::mpsc::UnboundedSender<GenerationChunk>,
) -> Result<TextGenerationResponse, RouteError> {
    adapter.generate_stream(request, sink).await.map_err(RouteError::Engine)
}
