//! Every resident on a node reasons through ONE in-process engine: one model, one persona
//! scheduler, one decode Core. Minds take turns on it (one decodes at a time, the next is staged as
//! data, the previous retires to RAM); nobody gets an engine of her own and nobody reaches a local
//! HTTP server. A mind leased from another node goes over the grid instead (the
//! `RemoteLaneAdapterFactory` wrapping this one routes her before she reaches here).
//!
//! Serving owns the engine and publishes it ([`crate::inference::in_process_engine::node_engine`]).
//! This factory only binds a resident to that handle; it never loads one, so the weights load once.

use super::*;

/// Binds each resident to the node's one engine.
#[derive(Default)]
pub struct SharedEnginePersonaAdapterFactory;

impl SharedEnginePersonaAdapterFactory {
    pub fn new() -> Self {
        Self
    }
}

/// The adapter serving published, waiting (bounded) for the first serve so an upstart that races
/// boot still binds. A resident is never given a stand-in.
pub async fn node_engine_adapter() -> Result<Arc<dyn AIProviderAdapter>, String> {
    let engine = crate::inference::in_process_engine::node_engine()
        .ok_or("serving installed no in-process engine on this node")?;
    if let Some(adapter) = engine.adapter() {
        return Ok(adapter);
    }
    crate::inference::llama_server::await_ready_serving(crate::inference::llama_server::DEFAULT_SERVING_WAIT)
        .await
        .ok_or("serving brought up no ready model: a persona cannot upstart without one")?;
    engine
        .adapter()
        .ok_or_else(|| "serving reports ready but published no engine (serving invariant violated)".to_string())
}

#[async_trait]
impl PersonaAdapterFactory for SharedEnginePersonaAdapterFactory {
    async fn build_adapter(
        &self,
        profile: &PersonaInferenceProfile,
    ) -> Result<Arc<dyn AIProviderAdapter>, String> {
        let adapter = node_engine_adapter().await?;
        continuum_base::probe!(
            class = "persona.upstart.bind",
            persona = %profile.persona_name,
            persona_id = %profile.persona_id,
            model = %adapter.default_model(),
            "persona bound to the node's one engine"
        );
        Ok(adapter)
    }
}
