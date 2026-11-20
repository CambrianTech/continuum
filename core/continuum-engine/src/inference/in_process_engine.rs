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

/// Why the in-process engine refused a launch, a load or a lifecycle change.
#[derive(Debug, thiserror::Error)]
pub enum InProcessRefusal {
    #[error("the in-process engine for {model}: serving planned a zero window")]
    ZeroWindow { model: String },
    #[error("the in-process engine for {model}: the model's catalog ceiling allows no micro-batch")]
    NoMicroBatch { model: String },
    #[error("the in-process engine for {model}: serving's KV plan names `{cache_type}`, which this engine cannot hold")]
    KvType { model: String, cache_type: String },
    #[error("the in-process engine for {model}: {refusal}")]
    Launch { model: String, refusal: crate::inference::llamacpp_adapter::EngineLaunchRefusal },
    #[error("the engine load task failed: {0}")]
    LoadTask(#[from] tokio::task::JoinError),
    #[error("the engine did not start: {0}")]
    Start(String), // start_engine's own refusal, which is text
    #[error("the engine's driver exited while starting")]
    DriverExited,
    #[error("the in-process engine was refused: {0}")]
    Refused(Arc<str>),
    #[error("the in-process engine holds {held} and was asked for {asked}: one engine holds one base, and switching it while residents hold minds on it waits for their suspension (S2)")]
    HoldsAnotherBase { held: String, asked: String },
    #[error("the in-process engine for {model} did not become ready: {state:?}")]
    NotReady { model: String, state: EngineState },
    #[error("the in-process engine is serving residents; it goes idle only after their minds are suspended (S2), never by dropping their state")]
    ServingResidents,
}

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
            EngineState::Refused { reason, .. } => Err(InProcessRefusal::Refused(reason).into()),
            EngineState::Stopped(_) => Err(LlamaServerError::Unreachable("the in-process engine stopped".into())),
        }
    }

    /// The launch serving's own owners decide: the window and placement from the target, the
    /// micro-batch from the measured prefill batch and the model's ceiling, the KV types from the KV
    /// plan. Anything serving cannot name is refused here, never filled with a default.
    fn launch_for(target: &ServingTarget) -> Result<EngineLaunch, LlamaServerError> {
        let model = || target.model.id.clone();
        let gguf_path = target
            .model
            .gguf_local_path
            .clone()
            .ok_or_else(|| LlamaServerError::ModelNotFound(model()))?;
        let context = NonZeroU32::new(target.context_window).ok_or_else(|| InProcessRefusal::ZeroWindow { model: model() })?;
        // The micro-batch the launched server's `--ubatch-size` applies, lowered (never raised) by the
        // model's own catalog ceiling; a ceiling of zero allows no batch, and is refused.
        let ceiling = target.model.serving.max_ubatch.map_or(crate::inference::lane_args::UBATCH_TOKENS, |cap| cap.min(crate::inference::lane_args::UBATCH_TOKENS));
        let n_ubatch = NonZeroU32::new(ceiling).ok_or_else(|| InProcessRefusal::NoMicroBatch { model: model() })?;
        let kv_plan = crate::inference::kv_cache_plan::resolve();
        let kv = kv_plan
            .engine_cache_type()
            .ok_or_else(|| InProcessRefusal::KvType { model: model(), cache_type: kv_plan.cache_type.to_string() })?;
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
        .map_err(|refusal| InProcessRefusal::Launch { model: model(), refusal }.into())
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
            .map_err(InProcessRefusal::LoadTask)?
            .map_err(InProcessRefusal::Start)?;
        if state.wait_for(|s| !matches!(s, EngineState::Starting(_))).await.is_err() {
            return Err(InProcessRefusal::DriverExited.into());
        }
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
        Ok(self.loaded.lock().await.as_ref().map(|loaded| &loaded.model_id).cloned())
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
            return Err(InProcessRefusal::HoldsAnotherBase { held: loaded.model_id.clone(), asked: target.model.id.clone() }.into());
        }
        let loaded = Self::load(target).await?;
        let state = loaded.state.borrow().clone();
        if state.ready().is_none() {
            return Err(InProcessRefusal::NotReady { model: loaded.model_id, state }.into());
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
        Err(InProcessRefusal::ServingResidents.into())
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
        let adapter = self.loaded.lock().await.as_ref().map(|loaded| Arc::clone(&loaded.adapter));
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
