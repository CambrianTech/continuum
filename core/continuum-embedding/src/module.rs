//! EmbeddingModule — the embedding service module and `build_adapter_embedder`, the one
//! adapter-routed embedder every caller scores with.
//!
//! The `embedding/*` commands (similarity, similarity-matrix, top-k, cluster) are typed
//! stateless `ActionCommand`s under `commands/embedding/*`; the vector math they call (the
//! SIMD kernels, the pairwise matrix, top-k, `detect_clusters`) lives with the embedding
//! owner in `ai::embedding::vector_math`. This module's `handle_command` exists only
//! to fail loud if a typed registration is ever missing.
//!
//! Embedding *generation* is async and adapter-routed (llama-server `/v1/embeddings` via the
//! `AIProviderAdapter`, see [`crate::ai::embedding`]); nothing loads a model in-process.
//!
//! Priority: Normal — similarity math is CPU-bound but not time-critical.

use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use async_trait::async_trait;
use serde_json::Value;
use std::any::Any;

use std::sync::Arc;

// ─── Adapter-Routed Embedder Construction ─────────────────────────────────────

/// Build the adapter-routed embedder used by background/IPC embedding work
/// (Hippocampus boot, `vector/backfill`) — the SAME async, content-addressed,
/// neural-or-lexical embedder the live recall path uses
/// ([`crate::ai::embedding::resolve_recall_embedder`]).
///
/// Reads the served model from Contract A (the serving daemon's snapshot) and
/// builds the gateway adapter against it. llama-server serves embeddings from the
/// SAME process it serves chat from (launched with `--embeddings`), so the embed
/// model IS the served model — there is no separate embed gateway to probe. This
/// is the ONE place the gateway-adapter build is expressed for the memory side,
/// so callers don't each re-derive base-url / model-selection.
///
/// Fail-loud (no silent ONNX fallback, task #40): no ready served model within the
/// readiness bound, or a ready snapshot that names no model, is propagated with its
/// cause named — the caller decides whether that is fatal (it is for
/// `vector/backfill`). The readiness wait is bounded by Contract A's single
/// [`DEFAULT_SERVING_WAIT`](crate::inference::llama_server::DEFAULT_SERVING_WAIT);
/// reading the snapshot never hangs (a `watch` borrow, not an HTTP probe), so there
/// is no per-module timeout to maintain — the old 5s guard existed only because an
/// local llama-server could hold `/v1/models` open, a hazard the snapshot removes.
pub async fn build_adapter_embedder(
) -> Result<Arc<dyn crate::ai::embedding::EmbeddingProvider>, String> {
    // `initialize` is an `AIProviderAdapter` trait method — bring the trait into
    // scope so it resolves on the concrete adapter.
    use crate::ai::adapter::AIProviderAdapter as _;

    // Read the served model from the daemon's published snapshot instead of each
    // embedder re-probing `/v1/models` ("subscribers READ the snapshot, they do
    // NOT each HTTP-probe" — Contract A). Wait bounded for the daemon's first
    // ready reconcile (covers a cold GGUF load); no ready model → fail loud.
    let snap = crate::inference::llama_server::await_ready_serving(
        crate::inference::llama_server::DEFAULT_SERVING_WAIT,
    )
    .await
    .ok_or_else(|| {
        "serving daemon brought up NO ready model within the readiness bound — \
         cannot select an embed model (no local ONNX fallback)"
            .to_string()
    })?;
    let model = snap.active_model.ok_or_else(|| {
        "serving snapshot is ready but names no active model — cannot embed \
         (no local ONNX fallback)"
            .to_string()
    })?;
    let mut adapter = crate::ai::openai_adapter::OpenAICompatibleAdapter::from_registry(
        crate::inference::llama_server::PROVIDER_ID,
    )
    .with_runtime_base_url(snap.base_url)
    .with_default_model(model);
    adapter
        .initialize()
        .await
        .map_err(|e| format!("embed adapter initialize failed: {e}"))?;
    Ok(crate::ai::embedding::resolve_recall_embedder(Arc::new(adapter)).await)
}

pub struct EmbeddingModule {
    /// The ONE process-wide embedding space (the memory manager's lazy recall
    /// embedder), handed in at registration. The text-level commands
    /// (`embedding/similar`, `embedding/groups`) score in it; the vector-math
    /// commands stay stateless.
    embedder: Arc<dyn crate::ai::embedding::EmbeddingProvider>,
}

impl EmbeddingModule {
    pub fn new(embedder: Arc<dyn crate::ai::embedding::EmbeddingProvider>) -> Self {
        Self { embedder }
    }
}

#[async_trait]
impl ServiceModule for EmbeddingModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "embedding",
            priority: ModulePriority::Normal,
            command_prefixes: &["embedding/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            // Pure CPU similarity math (Rayon). Serialize at the runtime level
            // so concurrent matrix/cluster requests don't multiply Rayon
            // threadpools during persona bursts.
            max_concurrency: 1,
            tick_interval: None,
        }
    }

    async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
        Ok(())
    }

    fn commands(&self) -> Vec<Arc<dyn crate::sdk_codegen::DynCommand>> {
        vec![
            Arc::new(crate::commands::similar::EmbeddingSimilar {
                embedder: self.embedder.clone(),
            }),
            Arc::new(crate::commands::groups::EmbeddingGroups {
                embedder: self.embedder.clone(),
            }),
        ]
    }

    async fn handle_command(&self, command: &str, _params: Value) -> Result<CommandResult, String> {
        // The vector-math commands (similarity / matrix / top-k / cluster) are now
        // typed stateless ActionCommands under `commands/embedding/*` — the executor
        // routes them via the typed path (`route_object`) before it ever reaches a
        // module's legacy handler. If one lands here, the typed registration is
        // missing: fail loud rather than silently re-implement it.
        Err(format!(
            "'{command}' did not resolve on the typed command registry. Embedding \
             vector math is served by the stateless `commands/embedding/*` commands; \
             embedding *generation* is adapter-routed (ai/* via /v1/embeddings). This \
             module no longer carries a legacy handler — this error means the typed \
             command failed to register, not that the command is unknown."
        ))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
