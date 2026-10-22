//! Boot phase 2: the runtime, the GPU memory manager, the system monitor and
//! the first stateless modules registered against them.

use super::*;

/// The three handles every later boot phase is built against.
pub(super) struct Foundation {
    pub(super) runtime: Arc<Runtime>,
    pub(super) gpu_manager: Arc<GpuMemoryManager>,
    pub(super) system_monitor: Arc<SystemResourceMonitor>,
}

pub(super) fn register_foundation_modules(
    pressure_monitor: &Arc<crate::system_resources::MemoryPressureMonitor>,
) -> Foundation {
    // Create modular runtime
    log_info!("ipc", "server", "Initializing modular runtime...");
    let runtime = Arc::new(Runtime::new());

    // Phase 0: GPU Memory Manager (detect VRAM, create budgets)
    let gpu_manager = Arc::new(GpuMemoryManager::detect());

    // Provide GPU manager to TTS and renderer subsystems for VRAM tracking.
    // (Embedding no longer loads an in-process ONNX model — it is adapter-routed,
    // task #40 — so it has no GPU allocation to track here.)
    crate::live::audio::tts::set_gpu_manager(gpu_manager.clone());
    // The Bevy renderer exists only under `avatar-3d`.
    #[cfg(feature = "avatar-3d")]
    crate::live::video::bevy_renderer::set_gpu_manager(gpu_manager.clone());

    // Phase 1: HealthModule (stateless)
    runtime.register(Arc::new(HealthModule::new()));

    // NavModule — nav/mark-read: advance the shared read cursor + publish NAV_CHANGED
    // on the airc bus (nav slice 3). Command in, Event out; the write half of the
    // dual-consumer atom — one (user, room) cursor, read by the human unread badge AND
    // the persona's RAG grounding. Captures the bus in initialize (like vision).
    // Phase C (S7): authored commands — a verb is a manifest in <continuum_root>/commands.
    // Loaded FIRST so the live catalogue carries them before any reader memoizes
    // (the ACL's command sets, the tool dialect index). A refused manifest is probed by
    // file and reason; the boot never refuses over one bad file.
    runtime.register(Arc::new(
        crate::modules::ext_commands::ExtCommandsModule::new(
            &crate::modules::persona_instance_manager::resolve_continuum_root(),
        ),
    ));
    runtime.register(Arc::new(crate::modules::nav::NavModule::new()));

    // ai/should-respond — the kernel command that runs a persona's WorkspaceCycle
    // (the brain) and returns a Decision. Resolves the per-persona cycle from the
    // process-global PersonaWorkspaceRegistry, which is populated at persona spawn
    // (supervisor::materialize_adapters). One command, N lane-routed handlers;
    // this is the continuum-native handler. Additive — the recipe walker + the
    // service-loop cutover consume it; existing heuristics stay live until then.
    runtime.register(Arc::new(
        crate::cognition::should_respond_module::ShouldRespondModule::new(
            crate::cognition::persona_workspace::global(),
        ),
    ));

    // LaunchModeModule — system/launch-mode/{get,set} as typed self-routing commands
    // (`get` stateless, `set` dep-holding over the module bus). Headless-native
    // runtime lever for the headless-vs-UI launch preference; persists
    // CONTINUUM_LAUNCH_MODE to config.env (same key bin/continuum reads) and emits
    // system:launch-mode:changed so a running UI can attach/tear down its overlay.
    runtime.register(Arc::new(LaunchModeModule::new()));

    // The cadence-deploy DECISION, in Rust (replacing the Unix-only track-canary.sh shell
    // jack): watches the tracked branch tip and records a DeployRequest for the supervisor
    // when a green tip differs from the running build. Never reboots. Degrades safely with
    // no checkout/gh. (card ee76c0df)
    runtime.register(Arc::new(
        crate::modules::deploy_tracker::DeployTrackerModule::new(),
    ));

    // AircBridgeDirectiveModule — recognizes inbound `!continuum` directives on
    // the airc bus (chat:posted) OFF the transport hot path, emitting an
    // observable airc:bridge:directive event. Passive subscriber, NO execution
    // (slice 3a). Header+kind gated so media/WebRTC never reach it.
    runtime.register(Arc::new(AircBridgeDirectiveModule::new()));

    // AircBridgeDispatchModule — consumes airc:bridge:directive (from 3a) and
    // emits airc:bridge:reply. ping/status reply locally; command-executing
    // directives are recognized but NOT executed (peer-command authorization is
    // slice 3b-2). Off-loop consumer, no kernel execution from peer content.
    runtime.register(Arc::new(AircBridgeDispatchModule::new()));

    // ExternalWebviewAuthModule — OAuth 2.0 + PKCE via system browser.
    // Landed in 26ab8c0ad; re-enabling after merge from feat/mac-docker-model-runner
    // briefly restored m5's stub from before 26ab8c0ad landed.
    runtime.register(Arc::new(ExternalWebviewAuthModule::new()));

    // Phase 1: GpuModule (GPU stats + pressure IPC)
    runtime.register(Arc::new(GpuModule::new(gpu_manager.clone())));

    // ForgeModule (continuum#1164 Phase 4 stub — forge/run IPC).
    // v1 returns a stub ForgeArtifact from a recipe; Phase 5+ wires the
    // real foundry executor.
    runtime.register(Arc::new(ForgeModule::new()));

    // GeneratorModule — hosts `generate/module` (scaffold a fresh ServiceModule).
    //
    // This was the ONE REAL ORPHAN the dispatch-parity audit had been reporting all
    // along, invisible under two false positives until `dispatch_orphans` stopped
    // counting adapter-served `Provided` commands (#325). Nothing was wrong with the
    // command: `GenerateModule` is a correct dep-holding typed command and IS in
    // `GeneratorModule::commands()`. The module itself was simply never registered —
    // it appeared in the codebase only in doc comments — so `generate/module` was
    // advertised in `commands/help`, did-you-mean suggestions, and the persona tool
    // offer while nothing on the runtime could route it. Exactly the #309 class, still
    // live, and the reason the audit exists.
    //
    // Background priority, no dedicated thread, `generate/` prefix — see its
    // `ModuleConfig`. Privileged: it writes Rust source into the workspace tree, so it
    // is not on the persona toolbelt.
    runtime.register(Arc::new(crate::modules::generator::GeneratorModule::new()));

    // EventsModule (L1-1 — event-class declaration registry).
    // Spec: GRID-BUS-ARCHITECTURE §2.2 (continuum#1439).
    // Exposes events/declare-class, events/get-class, events/list-classes,
    // events/resolve-channel. The TS thin shim at src/system/events/shared/
    // EventClass.ts reads through this; the L1-2 AircEventTransport will
    // consult resolve-channel at emit time.
    runtime.register(Arc::new(EventsModule::new()));

    // Phase 1: PersonaAllocatorModule (hardware-aware persona allocation)
    runtime.register(Arc::new(PersonaAllocatorModule::new(gpu_manager.clone())));

    // Phase 1: SystemResourceModule (CPU + memory + process monitoring IPC)
    let system_monitor = Arc::new(SystemResourceMonitor::new());
    let system_resource_module = Arc::new(SystemResourceModule::new(system_monitor.clone()));
    system_resource_module.set_pressure_monitor(pressure_monitor.clone());
    runtime.register(system_resource_module);
    Foundation {
        runtime,
        gpu_manager,
        system_monitor,
    }
}
