//! Boot phase 12: the AI provider module and the genome / training modules.

use super::*;

pub(super) fn register_ai_provider_and_genome_modules(
    runtime: &Arc<Runtime>,
    gpu_manager: &Arc<GpuMemoryManager>,
    serving_daemon: &Arc<crate::modules::serving_daemon::ServingDaemonModule>,
    resource_daemon: &Arc<crate::resources::ResourceDaemon>,
    airc_interceptor_cell: &Arc<tokio::sync::OnceCell<Arc<airc_lib::Airc>>>,
) {
    // AIProviderModule: Unified AI provider for cloud and local inference
    // Provides ai/generate, ai/providers/list, ai/providers/health
    // Routes to DeepSeek, Anthropic, OpenAI, Together, Groq, Fireworks, XAI, Google, Mistral
    runtime.register(Arc::new(AIProviderModule::with_gpu_manager(
        gpu_manager.clone(),
    )));

    {
        // A batch /train run a previous core bound to a still-live engine has no owner since
        // learning moved in place: cancel it and release its residency record, so serving is
        // not held for the engine's lifetime (card 4114a2c6). Its examples are already pending.
        if let Ok(home) = crate::paths::continuum_home() {
            tokio::spawn(crate::genome::fine_tuning::batch_wind_down::wind_down(home, resource_daemon.clone()));
        }

        // genome/teach and genome/recall. Her learning runs in place in the training trigger.
        runtime.register(Arc::new(
            crate::modules::genome::GenomeModule::new()
                .with_teacher_serving(Arc::new(crate::modules::serving_daemon::academy_batch::ServingTeacher(serving_daemon.clone()))),
        ));

        // Port leases for citizen services (card 0c42c0bf): a dev server or database a citizen
        // runs gets a port clear of every other citizen's and of Continuum's own.
        let ports_node = airc_interceptor_cell.clone();
        runtime.register(Arc::new(crate::modules::ports::PortsModule::new(Arc::new(
            move || ports_node.get().map(|core| core.peer_id()),
        ))));

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

        // TrainingTriggerModule: the journal curriculum producers submit to (teacher synthesis,
        // the hippocampus's noteworthy drain, settled cards, operator submits), learned in place
        // by the engine that serves each base, under this core's resource authority, its serving
        // lifecycle gate and the process's lease custody.
        let authority = crate::genome::learner::seat::Authority {
            resources: resource_daemon.clone(),
            serving: serving_daemon.lifecycle_gate(),
            custody: crate::genome::learner::custody::Custody::global(),
        };
        runtime.register(Arc::new(
            crate::modules::training_trigger::TrainingTriggerModule::new(authority)
                .with_serving_changes(serving_daemon.subscribe_serving()),
        ));
        log_info!(
            "ipc",
            "server",
            "TrainingTriggerModule: registered (genome/training-trigger/*)"
        );
    }
}
