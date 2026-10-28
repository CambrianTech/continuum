//! Boot phase 12: the AI provider module and the genome / training modules.

use super::*;

pub(super) fn register_ai_provider_and_genome_modules(
    runtime: &Arc<Runtime>,
    gpu_manager: &Arc<GpuMemoryManager>,
    serving_daemon: &Arc<crate::modules::serving_daemon::ServingDaemonModule>,
    airc_interceptor_cell: &Arc<tokio::sync::OnceCell<Arc<airc_lib::Airc>>>,
) {
    // AIProviderModule: Unified AI provider for cloud and local inference
    // Provides ai/generate, ai/providers/list, ai/providers/health
    // Routes to DeepSeek, Anthropic, OpenAI, Together, Groq, Fireworks, XAI, Google, Mistral
    runtime.register(Arc::new(AIProviderModule::with_gpu_manager(
        gpu_manager.clone(),
    )));

    // GenomeModule: substrate-side dispatch for `genome/*` commands.
    // genome/job-create, genome/job-status, genome/job-cancel route
    // through the FineTuningCoordinator + a registry seeded with
    // whichever cloud LoRA-trainer adapters have credentials, plus
    // the LocalCandleFineTuner skeleton (always registered — slot
    // is visible to the coordinator even before tasks #231-#233
    // implement the optimizer loop). Per
    // [[commands-are-dumb-daemons-are-smart]] the module is narrow;
    // selection logic lives in the coordinator.
    {
        use crate::genome::fine_tuning::{
            CudaLoraFineTuner, EngineLoraFineTuner, FineTuningRegistry, LocalCandleFineTuner,
            MlxLoraFineTuner, OpenAIFineTuningAdapter,
        };
        let ft_registry = std::sync::Arc::new(FineTuningRegistry::new());
        ft_registry.register(std::sync::Arc::new(CudaLoraFineTuner::new()));
        // In-engine training on the resident weights (the dream's trainer): selected by
        // preference ("engine-local") until its measured 27B run makes it the default
        // over the process trainers (charter S6); needs a live lane serving the base.
        // SHARED-RESIDENT-LIFECYCLE.md step 3: a run a previous core bound to a still-live engine
        // gets its lease back HERE, before any module ticks and so before anything can admit
        // into the memory it is using; the trigger's first tick then re-attaches it.
        let engine_tuner = std::sync::Arc::new(EngineLoraFineTuner::new());
        engine_tuner.reclaim_resident_leases();
        ft_registry.register(engine_tuner);
        // In-engine training on a lane stood up for the run, for a base no live lane here serves
        // (card d67fd801): ranked after the resident lane and before any separate trainer process
        // (TrainingPlace::EngineLaneHere). Its runs' lanes die with the core, so it has no leases
        // to reclaim at boot.
        ft_registry.register(std::sync::Arc::new(EngineLoraFineTuner::on_engine_lanes()));

        // OpenAI when credentials present. Other cloud LoRA-trainer
        // adapters (Mistral, Anthropic, Fireworks, DeepSeek,
        // Together) plug in here as their impls land — same pattern,
        // gate on the matching `*_API_KEY` secret.
        if continuum_base::secrets::get_secret("OPENAI_API_KEY").is_some() {
            match OpenAIFineTuningAdapter::new() {
                Ok(adapter) => {
                    ft_registry.register(std::sync::Arc::new(adapter));
                    log_info!(
                        "ipc",
                        "server",
                        "GenomeModule: registered OpenAIFineTuningAdapter"
                    );
                }
                Err(e) => log_error!(
                    "ipc",
                    "server",
                    "GenomeModule: OpenAIFineTuningAdapter not registered: {}",
                    e
                ),
            }
        }

        // LocalCandleFineTuner always registered. The skeleton
        // returns LocalTrainerFailed with a pointer to follow-up
        // tasks until the math lands; making the slot present means
        // the coordinator + operator + telemetry can see the
        // architectural seam.
        ft_registry.register(std::sync::Arc::new(LocalCandleFineTuner::new()));
        log_info!(
            "ipc",
            "server",
            "GenomeModule: registered LocalCandleFineTuner (skeleton — tasks #231-#233 \
             track the optimizer-loop landing)"
        );

        // MlxLoraFineTuner — the REAL owned trainer (#32): Apple's
        // mlx_lm.lora on the Metal GPU → forge-custodian converts to a
        // GGUF-lora gene → llama-server serves it. Always registered; its
        // capability declares `requires: TrainerHardware::AppleSilicon`, so the
        // coordinator routes here only on Apple Silicon, and create_job
        // fails loud (never silently) when Metal / the mlx python env /
        // a base model / examples are missing. Without this slot the live
        // registry held only the synthetic Candle skeleton + conditional
        // cloud OpenAI, so genome/job-create could never produce a
        // real, loadable LoRA — the closed L1→L3 loop was broken here.
        ft_registry.register(std::sync::Arc::new(MlxLoraFineTuner::new()));
        log_info!(
            "ipc",
            "server",
            "GenomeModule: registered MlxLoraFineTuner (mlx-local — real Apple-Silicon \
             LoRA trainer; coordinator gates on a probed Metal device)"
        );

        // L3 completion sentinel shares the SAME registry — it polls the handles the
        // trigger registers and looks the owning adapter back up here. Clone the Arc
        // before the registry is moved into GenomeModule.
        let completion_sentinel = Arc::new(
            crate::modules::training_completion_sentinel::TrainingCompletionSentinel::new(
                ft_registry.clone(),
            ),
        );

        let genome = crate::modules::genome::GenomeModule::new(ft_registry.clone())
            .with_teacher_serving(Arc::new(crate::modules::serving_daemon::academy_batch::ServingTeacher(serving_daemon.clone())));
        // The grid-peer trainer (card e53bb821): leases a LoRA job to one fresh peer that
        // advertises the base, training_lease and training_lease_return, and pulls its adapter
        // back into this node's job directory for adoption (card 92cd2251).
        ft_registry.register(Arc::new(
            crate::commands::genome::grid_peer_adapter::GridPeerAdapter::on_grid(genome.lease_executor()),
        ));
        runtime.register(Arc::new(genome));

        // Port leases for citizen services (card 0c42c0bf): a dev server or database a citizen
        // runs gets a port clear of every other citizen's and of Continuum's own.
        let ports_node = airc_interceptor_cell.clone();
        runtime.register(Arc::new(crate::modules::ports::PortsModule::new(Arc::new(
            move || ports_node.get().map(|core| core.peer_id()),
        ))));

        // TrainingCompletionSentinel: L3 of the dev-task continuous-learning loop.
        // Polls in-flight training jobs (the TrainingJobBoard the trigger writes to);
        // on completion converts, registers and opens the gene's trial, which wakes the
        // examiner below. Its executor is installed by `install_executor_on_all`.
        runtime.register(completion_sentinel);
        log_info!(
            "ipc",
            "server",
            "TrainingCompletionSentinel: registered (L3 train-done → register → trial opened)"
        );

        // The gene EXAMINER: every trial that opens is sat on the continual-learning
        // acceptance test (unseen related tasks, unrelated retention, with vs without the gene
        // on her live lane) and its verdict promotes or retires the gene; the first pass of
        // each core process is the retention re-run after a restart.
        runtime.register(Arc::new(crate::modules::gene_examiner::GeneExaminerModule));

        // GenomeFitnessSentinel: the self-evolving genome's fitness daemon. On a slow
        // (5-min) Background tick it measures each resident layer's value-density
        // (lift/GB, from the manifest + eval ledger) and GLASS-BOXES the ranking +
        // retire-candidates — OBSERVE-ONLY, no eviction (earn the emergent version:
        // validate the fitness signal against ground truth before it ever evicts,
        // SELF-EVOLVING-GENOME.md §5). Stateless; reads fresh each tick.
        runtime.register(Arc::new(
            crate::modules::genome_fitness_sentinel::GenomeFitnessSentinel::new(),
        ));
        log_info!(
            "ipc",
            "server",
            "GenomeFitnessSentinel: registered (observe-only value-density fitness landscape, 5-min tick)"
        );

        // TrainingTriggerModule: substrate-native batching coordinator
        // sitting between curriculum producers (teacher persona's
        // synthesis, hippocampus's noteworthy drain, operator submits)
        // and `genome/job-create`. Accumulates per-(persona, trait)
        // and auto-dispatches when the threshold is reached. Per
        // `[[no-fallbacks-ever]]`, dispatch failure preserves bucket
        // contents — curated examples never silently disappear.
        runtime.register(Arc::new(
            crate::modules::training_trigger::TrainingTriggerModule::new()
                .with_serving_changes(serving_daemon.subscribe_serving()),
        ));
        log_info!(
            "ipc",
            "server",
            "TrainingTriggerModule: registered (genome/training-trigger/*)"
        );
    }
}
