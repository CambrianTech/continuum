//! The node's ONE engine, in this process: one model, one persona scheduler, one decoding mind at a
//! time. Serving drives it through the same [`LlamaServerControl`] seam it drove llama-server
//! through, so planning, the snapshot and grants are unchanged; what changes is that nothing is
//! launched, nothing is reached over HTTP, and the weights load once.
//!
//! Serving owns it. It is published once ([`install`]) so the residents' adapter factory and the
//! re-home reconciler bind to the SAME handle ([`InProcessEngine::adapter`]); neither may load an
//! engine of its own.
//!
//! What it does not do yet, by name, so no caller mistakes a missing capability for a working one:
//! it switches no base while residents hold minds on it (S2 suspends them first), it has no host
//! prompt cache to resize, and its native paging and learning arrive with S2/S3.

use std::num::NonZeroU32;
use std::sync::{Arc, OnceLock};

use async_trait::async_trait;

use crate::ai::adapter::AIProviderAdapter;
use crate::inference::backends::persona_scheduler::{EngineGeometry, EngineState};
use crate::inference::llama_server::{
    LanePlacement, LlamaServerControl, LlamaServerError, PromptCacheLimit, PromptCacheRefusal, ServingTarget,
};
use crate::inference::llamacpp_adapter::{EngineKv, EngineLaunch, LlamaCppAdapter, Offload};
use crate::inference::weight_residency::ContextGeometry;

/// One loaded engine: which model, the adapter residents share, and its live lifecycle.
struct Loaded {
    model_id: String,
    adapter: Arc<LlamaCppAdapter>,
    state: tokio::sync::watch::Receiver<EngineState>,
}

/// The in-process engine serving controls. `None` inside = nothing loaded (idle).
#[derive(Default)]
pub struct InProcessEngine {
    loaded: tokio::sync::Mutex<Option<Loaded>>,
    /// The adapter residents bind to, readable without awaiting the load lock.
    published: parking_lot::RwLock<Option<Arc<dyn AIProviderAdapter>>>,
}

static NODE_ENGINE: OnceLock<Arc<InProcessEngine>> = OnceLock::new();

/// Publish the node's engine once, at boot, from serving. A second install is refused: one node,
/// one engine.
pub fn install(engine: Arc<InProcessEngine>) -> Result<(), Arc<InProcessEngine>> {
    NODE_ENGINE.set(engine)
}

/// The node's engine, if serving installed one.
pub fn node_engine() -> Option<Arc<InProcessEngine>> {
    NODE_ENGINE.get().cloned()
}

impl InProcessEngine {
    pub fn new() -> Self {
        Self::default()
    }

    /// The adapter every resident shares, once the engine is ready. `None` before serving served.
    pub fn adapter(&self) -> Option<Arc<dyn AIProviderAdapter>> {
        self.published.read().clone()
    }

    /// The geometry the engine's context measured, while it serves.
    async fn geometry(&self) -> Result<EngineGeometry, LlamaServerError> {
        let guard = self.loaded.lock().await;
        let loaded = guard
            .as_ref()
            .ok_or_else(|| LlamaServerError::Unreachable("the in-process engine is not loaded".into()))?;
        let state = loaded.state.borrow().clone();
        match state {
            EngineState::Ready(geometry) => Ok(geometry),
            EngineState::Starting(_) => Err(LlamaServerError::Unreachable("the in-process engine is still starting".into())),
            EngineState::Refused { reason, .. } => Err(LlamaServerError::Spawn(format!("the in-process engine was refused: {reason}"))),
            EngineState::Stopped(_) => Err(LlamaServerError::Unreachable("the in-process engine stopped".into())),
        }
    }

    /// The launch serving's own owners decide: the window and placement from the target, the
    /// micro-batch from the measured prefill batch and the model's ceiling, the KV types from the KV
    /// plan. Anything serving cannot name is refused here, never filled with a default.
    fn launch_for(target: &ServingTarget) -> Result<EngineLaunch, LlamaServerError> {
        let refuse = |why: String| LlamaServerError::Spawn(format!("the in-process engine for {}: {why}", target.model.id));
        let gguf_path = target
            .model
            .gguf_local_path
            .clone()
            .ok_or_else(|| LlamaServerError::ModelNotFound(target.model.id.clone()))?;
        let context = NonZeroU32::new(target.context_window).ok_or_else(|| refuse("serving planned a zero window".into()))?;
        let n_ubatch = crate::inference::lane_args::prefill_ubatch(target.model.serving.max_ubatch)
            .ok_or_else(|| refuse("the model's catalog ceiling allows no micro-batch".into()))?;
        let kv_plan = crate::inference::kv_cache_plan::resolve();
        let kv = kv_plan
            .engine_cache_type()
            .ok_or_else(|| refuse(format!("serving's KV plan names `{}`, which this engine cannot hold", kv_plan.cache_type)))?;
        let offload = match target.placement {
            LanePlacement::Gpu => Offload::AllLayers,
            LanePlacement::Cpu => Offload::CpuOnly,
        };
        EngineLaunch {
            model_id: target.model.id.clone(),
            gguf_path,
            context,
            // One decode call per micro-batch: always a whole number of them, as the learner needs.
            n_batch: n_ubatch,
            n_ubatch,
            offload,
            kv: EngineKv { k: kv, v: kv },
        }
        .validated()
        .map_err(|refusal| refuse(refusal.to_string()))
    }

    async fn load(target: &ServingTarget) -> Result<Loaded, LlamaServerError> {
        let launch = Self::launch_for(target)?;
        let model_id = launch.model_id.clone();
        let gguf = launch.gguf_path.clone();
        let adapter = Arc::new(LlamaCppAdapter::for_engine(launch));
        let starting = adapter.clone();
        // Loading the weights and making the context is blocking native work: never on the runtime.
        let mut state = tokio::task::spawn_blocking(move || starting.start_engine())
            .await
            .map_err(|join| LlamaServerError::Spawn(format!("the engine load task failed: {join}")))?
            .map_err(LlamaServerError::Spawn)?;
        state
            .wait_for(|s| !matches!(s, EngineState::Starting(_)))
            .await
            .map_err(|_| LlamaServerError::Spawn("the engine's driver exited while starting".into()))?;
        continuum_base::probe!(
            class = "serving.engine.in_process.loaded",
            model = %model_id,
            gguf = %gguf.display(),
            state = ?*state.borrow(),
            "the node's one engine loaded in process"
        );
        Ok(Loaded { model_id, adapter, state })
    }
}

#[async_trait]
impl LlamaServerControl for InProcessEngine {
    async fn active_model(&self) -> Result<Option<String>, LlamaServerError> {
        Ok(self.loaded.lock().await.as_ref().map(|loaded| loaded.model_id.clone()))
    }

    /// Genes are paged per turn by the scheduler (a request names its genes and their scales), never
    /// a load-time set: the engine serves whatever set the target names without a relaunch.
    async fn active_adapters(&self, target: &ServingTarget) -> Result<Vec<String>, LlamaServerError> {
        Ok(target.adapter_paths())
    }

    async fn serve(&self, target: &ServingTarget) -> Result<(), LlamaServerError> {
        let mut guard = self.loaded.lock().await;
        if let Some(loaded) = guard.as_ref() {
            if loaded.model_id == target.model.id {
                return Ok(());
            }
            return Err(LlamaServerError::Spawn(format!(
                "the in-process engine holds {} and was asked for {}: one engine holds one base, and \
                 switching it while residents hold minds on it waits for their suspension (S2)",
                loaded.model_id, target.model.id
            )));
        }
        let loaded = Self::load(target).await?;
        if loaded.state.borrow().ready().is_none() {
            return Err(LlamaServerError::Spawn(format!(
                "the in-process engine for {} did not become ready: {:?}",
                loaded.model_id,
                *loaded.state.borrow()
            )));
        }
        *self.published.write() = Some(loaded.adapter.clone() as Arc<dyn AIProviderAdapter>);
        *guard = Some(loaded);
        Ok(())
    }

    /// Dropping the context would drop every resident's live KV. Until residents can be suspended
    /// through the lifecycle tree (S2), going idle is refused while the engine holds them.
    async fn idle(&self) -> Result<(), LlamaServerError> {
        if self.loaded.lock().await.is_none() {
            return Ok(());
        }
        Err(LlamaServerError::Spawn(
            "the in-process engine is serving residents; it goes idle only after their minds are \
             suspended (S2), never by dropping their state"
                .into(),
        ))
    }

    async fn served_context_window(&self) -> Result<u32, LlamaServerError> {
        Ok(self.geometry().await?.n_ctx_seq)
    }

    async fn served_geometry(&self) -> Result<ContextGeometry, LlamaServerError> {
        let geometry = self.geometry().await?;
        Ok(ContextGeometry {
            allocated_tokens: u64::from(geometry.n_ctx),
            max_tokens_per_sequence: u64::from(geometry.n_ctx_seq),
            sequence_capacity: u64::from(geometry.n_seq_max),
        })
    }

    /// One mind decodes at a time.
    async fn served_lanes(&self) -> Result<u32, LlamaServerError> {
        Ok(self.geometry().await?.decoding)
    }

    async fn set_prompt_cache_limit(&self, _limit_mib: std::num::NonZeroU64) -> Result<PromptCacheLimit, LlamaServerError> {
        Err(LlamaServerError::PromptCacheRefused(PromptCacheRefusal::Disabled))
    }

    /// The decode path proven by a one-token warm-up on the shared engine.
    async fn decode_smoke_ok(&self) -> bool {
        let adapter = self.loaded.lock().await.as_ref().map(|loaded| loaded.adapter.clone());
        match adapter {
            Some(adapter) => adapter.warmup().await.is_ok(),
            None => false,
        }
    }

    /// This process owns the engine outright: there is no orphan to probe before adopting.
    fn owns_child(&self) -> bool {
        true
    }
}
