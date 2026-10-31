//! Boot phase 6: resource-broker, inference-handle, inference-llm and VDD modules.

use super::*;

pub(super) fn register_inference_modules(runtime: &Arc<Runtime>) {
    // Runtime-owned lease ledger for CPU/GPU/memory/disk/network admission.
    // Subsystems ask this broker for capacity instead of keeping private caps.
    runtime.register(Arc::new(
        crate::resource_broker::ResourceBrokerModule::new(),
    ));

    // InferenceHandleModule — the `ai/inference/{open,generate,close,inspect}`
    // lane command surface, routed through the SAME `InferenceCoordinator`
    // the InferenceCoordinatorModule stood up + registered with the broker.
    // This makes the realistic-lane path LIVE: opens create lanes (consuming
    // the coordinator's admission budget + reporting footprint), and the
    // broker's tick evicts them under pressure. Disjoint from InferenceModule
    // above — `ai/inference/` vs `inference/`, "ai-inference-handle" vs
    // "inference" — so this is purely additive, not a replacement.
    let handle_coordinator = runtime
        .registry()
        .module_of_type::<crate::modules::inference_coordinator_module::InferenceCoordinatorModule>()
        .and_then(|m| {
            m.as_any()
                .downcast_ref::<crate::modules::inference_coordinator_module::InferenceCoordinatorModule>()
                .map(|cm| cm.coordinator())
        });
    if let Some(coordinator) = handle_coordinator {
        runtime.register(Arc::new(
            crate::inference::handle_module::InferenceHandleModule::with_coordinator(coordinator),
        ));
        log_info!(
            "ipc",
            "server",
            "InferenceHandleModule registered (ai/inference/* routed through the broker-managed coordinator)"
        );
    } else {
        log_error!(
            "ipc",
            "server",
            "InferenceCoordinatorModule missing — ai/inference/* lane surface not registered"
        );
    }

    // Phase 5: InferenceLlmModule (MODULE-CATALOG §II `inference-llm`)
    // — the substrate's local-LLM generation surface. Subscribes to
    // inference/llm/request commands, returns InferenceComplete +
    // FirstTokenEmitted bundles. Stub-backed in PR-2; adapter-routed
    // in PR-4 (#1395) when constructed via with_adapter. PR-5 (this
    // registration) wires the module into the runtime so it's
    // callable from the cognition path — no Runtime adapter wiring
    // yet (caller construction option lands when persona-cognition
    // composes via with_bus_and_adapter).
    //
    // Shipped via the .new() constructor (bus-less, stub-backed)
    // so this PR doesn't bind us to a specific LlamaCppAdapter
    // initialization story; downstream PRs swap construction when
    // the LlamaCppAdapter init lifecycle is integrated with the
    // Runtime startup phase.
    runtime.register(Arc::new(
        crate::inference::llm_module_service::InferenceLlmModule::new(),
    ));

    // Lane C PR-3: VddModule — `vdd/report` reads structured
    // VDD records from `~/.continuum/vdd/<sha>/<scenario>/record.jsonl`
    // (written by the harness via `ArtifactWriter`) and emits a
    // machine-readable report. Replaces "tail the log and grep
    // for first-token-ms" with a single command return. PR-body
    // VDD claims become `./jtag vdd/report --git_sha=<sha>`,
    // not pasted terminal text.
    runtime.register(Arc::new(continuum_mind::vdd::module::VddModule::new()));
}
