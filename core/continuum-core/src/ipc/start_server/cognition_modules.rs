//! Boot phase 7: per-persona cognition, channel, models, memory and RAG modules.

use super::*;

/// The two handles the IPC server state shares with these modules.
pub(super) struct CognitionHandles {
    pub(super) rag_engine: Arc<RagEngine>,
    pub(super) channel_registries: Arc<DashMap<Uuid, (ChannelRegistry, PersonaState)>>,
}

pub(super) fn register_cognition_and_memory_modules(
    runtime: &Arc<Runtime>,
    gpu_manager: &Arc<GpuMemoryManager>,
    memory_manager: &Arc<crate::memory::PersonaMemoryManager>,
    model_catalog: Arc<crate::model_registry::live::ModelCatalog>,
    serving_daemon: &Arc<crate::modules::serving_daemon::ServingDaemonModule>,
) -> CognitionHandles {
    // Shared state for per-persona cognition (unified: engine + inbox + rate limiter + sleep + adapters + genome)
    let rag_engine = Arc::new(RagEngine::new());
    let cognition_state =
        Arc::new(CognitionState::new(rag_engine.clone()).with_gpu_manager(gpu_manager.clone()));
    let personas = cognition_state.personas.clone();
    runtime.register(Arc::new(CognitionModule::new(cognition_state)));

    // Channel module shares the unified personas map for fast-path decisions
    let channel_registries: Arc<DashMap<Uuid, (ChannelRegistry, PersonaState)>> =
        Arc::new(DashMap::new());
    let channel_state = Arc::new(ChannelState::from_existing(
        channel_registries.clone(),
        personas,
    ));
    runtime.register(Arc::new(ChannelModule::new(channel_state)));

    // Phase 3: ModelsModule holds the SAME live model universe constructed above
    // (shared with the serving daemon). The rich `models/*` commands capture this
    // one `Arc<ModelCatalog>`, so every caller reads/mutates — and the serving
    // daemon plans off — the SAME live universe.
    runtime.register(Arc::new(ModelsModule::new(
        model_catalog,
        crate::modules::ai_provider::global_registry(),
        // The serving daemon's live snapshot — so `models/remove` refuses to
        // delete weights out from under the currently-served lane.
        serving_daemon.subscribe_serving(),
    )));

    // Phase 3: MemoryModule (wraps PersonaMemoryManager)
    let memory_state = Arc::new(MemoryState::new(memory_manager.clone()));
    runtime.register(Arc::new(MemoryModule::new(memory_state)));

    // Phase 3: RagModule (batched RAG composition with parallel Rayon loading)
    let rag_state = Arc::new(RagState::new(memory_manager.clone()));
    runtime.register(Arc::new(RagModule::new(rag_state)));
    CognitionHandles {
        rag_engine,
        channel_registries,
    }
}
