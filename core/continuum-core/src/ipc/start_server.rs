//! The core's IPC boot path. `start_server` is the ordered list of boot
//! phases; each phase lives in the child module that owns it. The ORDER of the
//! calls below is the order of every spawn, registration and bind the boot
//! performs — reordering is a behaviour change.

use super::*;

mod accept_loop;
mod ai_genome_modules;
mod airc_boot;
mod closing_modules;
mod cognition_modules;
mod command_executor;
mod foundation_modules;
mod inference_modules;
mod listeners;
mod live_call;
mod memory_guard;
mod model_registry_boot;
mod persona_host_reconciler;
mod persona_hosting;
mod persona_registry_modules;
mod pressure_broker_wiring;
mod rehome_reconciler;
mod resource_authority;
mod runtime_start;
mod service_modules;
mod serving_modules;
#[cfg(unix)]
mod tcp_ingress;
mod ws_ingress;

/// What the kernel's extension slots hold in this process, installed before any executor
/// exists: every dispatch runs under the hooks (the hands' admission is a safety boundary), a
/// command name resolves through the persona path's vocabulary, and a stop lets admitted turns
/// finish. Cognition's slots (the verbs it calls that persona owns) are filled here too. One list, so boot and a test that dispatches through a real executor install the same.
pub(crate) fn install_kernel_extensions() {
    crate::runtime::dispatch_hook::install(Arc::new(crate::code::hands::HandsAdmission));
    crate::persona::cognition_extensions::install();
    crate::runtime::dispatch_hook::install(Arc::new(crate::persona::cognition_pulse::CardWorkPulse));
    crate::runtime::extensions::install_command_name_resolver(crate::ai::tool_dialect::resolve_wire_name);
    crate::runtime::extensions::install_settle_before_stop(Arc::new(crate::cognition::turn_ingress::TurnsSettle));
    // After the turns, so the page IO they started settles before the stop saves.
    crate::runtime::extensions::install_settle_before_stop(Arc::new(crate::inference::slots::PageIoSettle));
    crate::runtime::extensions::install_turns_in_flight(crate::cognition::turn_ingress::in_flight);
    crate::persona::command_inbound_pump::install_command_handler(|airc, executor, grants| {
        crate::ipc::command_ingress::CommandRequestHandler::with_grant_authorizer(airc, executor, grants)
    });
    crate::persona::card_checkout::install_card_stager(|home, peer, card| {
        Box::pin(async move {
            crate::modules::card_staging::stage_for_card(&home, peer, &card).await;
        })
    });
    crate::inference::governed_budget::install(|daemon| {
        crate::modules::serving_daemon::governed_host_budget(daemon).usable_bytes
    });
    crate::inference::resource_admission::install_directed_pending(crate::cognition::directed_pending::any_pending);
    crate::runtime::extensions::install_roster(|| {
        crate::persona::PersonaAircRuntimeRegistry::try_global()
            .map(|reg| reg.roster_snapshot().into_iter().map(|(_, id)| id).collect())
    });
}

pub fn start_server(
    socket_path: &str,
    livekit_manager: Arc<crate::live::transport::bridge_client::LiveKitAgentManager>,
    rt_handle: tokio::runtime::Handle,
    memory_manager: Arc<crate::memory::PersonaMemoryManager>,
    pressure_monitor: Arc<crate::system_resources::MemoryPressureMonitor>,
    disk_pressure_monitor: Arc<crate::system_resources::DiskPressureMonitor>,
    boot_mode: crate::runtime::BootMode,
) -> std::io::Result<()> {
    install_kernel_extensions();
    continuum_base::probe!(class = "boot.step", step = "kernel_extensions_installed", "kernel extensions installed; the socket path and the model registry are next");
    let phase_started =
        model_registry_boot::prepare_socket_and_load_model_registry(socket_path, boot_mode)?;

    let foundation_modules::Foundation {
        runtime,
        gpu_manager,
        system_monitor,
    } = foundation_modules::register_foundation_modules(&pressure_monitor);

    let (model_catalog, resource_daemon) =
        resource_authority::build_model_catalog_and_resource_daemon(&rt_handle, &system_monitor);
    let serving_daemon = serving_modules::register_serving_daemon(
        &runtime,
        &gpu_manager,
        &system_monitor,
        &resource_daemon,
        &model_catalog,
    )?;

    pressure_broker_wiring::register_pressure_broker(
        &runtime,
        &resource_daemon,
        &disk_pressure_monitor,
        &pressure_monitor,
        &rt_handle,
    );
    inference_modules::register_inference_modules(&runtime);

    let cognition_modules::CognitionHandles {
        rag_engine,
        channel_registries,
    } = cognition_modules::register_cognition_and_memory_modules(
        &runtime,
        &gpu_manager,
        &memory_manager,
        model_catalog,
        &serving_daemon,
    );

    let call_plane = live_call::start_live_call_plane(&rt_handle);
    let audio_pool = live_call::register_voice_module(
        &runtime,
        &livekit_manager,
        &resource_daemon,
        &gpu_manager,
        &call_plane.voice_service,
        &call_plane.call_manager,
    );

    let service_modules::CodeHandles {
        file_engines,
        shell_sessions,
        code_state,
    } = service_modules::register_code_data_and_service_modules(
        &runtime,
        &rt_handle,
        &memory_manager,
    );

    let airc_boot::AircBoot {
        discovery,
        persona_bootstrap_deps,
        operator_peer_socket,
        discovered_default_room,
        persona_bootstrap_room_name,
        node_presence_deps,
        airc_interceptor_cell,
    } = airc_boot::discover_and_register_airc(&runtime, &rt_handle, boot_mode)?;

    // PersonaInstanceManagerModule: owns the live PersonaAircRuntime
    // registry — the kernel's roster of citizens in The Grid. Exposes
    // `persona/instances/bootstrap`, `persona/instances/list`,
    // `persona/instances/get`. Only registered when AIRC discovery
    // produced both a daemon socket AND a default room — without
    // either, citizens have nowhere to attach. The degraded path
    // logs and skips registration so the rest of the server boots;
    // the operator's remedy is the same as for AIRC discovery
    // failures (install airc / run `airc room <name>`).
    // Set by the persona-supervisor block below when persona hosting
    // is wired. Fired AFTER `init_executor_with_interceptors` so the
    // spawned supervisor task can safely dereference `executor()` in
    // its eventual `bootstrap_one` calls. None in `--mode=inference-only`
    // where personas are not hosted at all.
    // Carries the WIRED executor (GridTrustAuthPolicy + interceptors) to the
    // persona supervisor task once it's built — both the readiness signal AND the
    // executor the personas' hands ride. One channel, two jobs (order + payload).
    let mut persona_supervisor_executor_ready_tx: Option<
        tokio::sync::oneshot::Sender<Arc<crate::runtime::CommandExecutor>>,
    > = None;

    // boot.stretch: the stretch between rag_inspect's registration and
    // ai_provider's carried 14 s on the M5 (boot.construct attributed it to
    // ai_provider because attribution is per REGISTER and nothing registers in
    // between). Step marks name which part of the persona-host setup owns it.
    let mut stretch_mark = {
        let mut last = std::time::Instant::now();
        move |step: &'static str, next: &'static str| {
            let now = std::time::Instant::now();
            continuum_base::probe!(
                class = "boot.stretch",
                stretch = "persona_host_setup",
                step = step,
                next = next,
                ms = now.duration_since(last).as_millis() as u64,
                "pre-bind stretch step done; the named next step starts now"
            );
            last = now;
        }
    };
    stretch_mark("enter", "resident_roles_and_overlay");
    if let Some(daemon_socket) = persona_bootstrap_deps {
        let instance_manager = persona_registry_modules::register_persona_registry_modules(
            &runtime,
            &resource_daemon,
            &memory_manager,
            &pressure_monitor,
            daemon_socket,
            discovered_default_room,
        );
        persona_supervisor_executor_ready_tx = Some(persona_hosting::start_persona_host(
            &runtime,
            &serving_daemon,
            &instance_manager,
            &airc_interceptor_cell,
            &rt_handle,
            &mut stretch_mark,
        ));
    } else {
        // A.2: by this point the mode-driven gate above has already
        // returned `Err` if persona hosting was required and discovery
        // failed. Reaching here means `--mode=inference-only` — the
        // operator explicitly opted into degraded operation. Single
        // info-level line tells them what's missing without scaring.
        tracing::info!(
            mode = boot_mode.label(),
            discovery_kind = discovery.kind(),
            "PersonaInstanceManagerModule not registered (--mode=inference-only, AIRC \
             discovery {}). Substrate continues for inference / embedding / forge / \
             cargo / code. Resolve AIRC and re-launch in --mode=full-citizen to host \
             personas.",
            discovery.kind()
        );
    }

    rehome_reconciler::spawn_rehome_reconciler(&rt_handle, &serving_daemon);
    stretch_mark("end_of_persona_host_block", "ai_provider_register");

    {
        // Trainers probe their own interpreter's imports when built (card 6f05b89d): the probe
        // is an owned task on this runtime, so registration runs inside it.
        let _rt_guard = rt_handle.enter();
        ai_genome_modules::register_ai_provider_and_genome_modules(
            &runtime,
            &gpu_manager,
            &serving_daemon,
            &airc_interceptor_cell,
        );
    }
    let grid_state = closing_modules::register_closing_modules(&runtime, &gpu_manager, &code_state);

    let runtime_start::RuntimeStarted {
        post_init_started,
        tick_handles: _tick_handles,
    } = runtime_start::initialize_and_start_runtime(
        &runtime,
        &rt_handle,
        &discovery,
        boot_mode,
        phase_started,
    )?;

    let (executor, provider_registry) = command_executor::build_and_install_executor(
        &runtime,
        grid_state,
        airc_interceptor_cell,
        &call_plane.live_registrar_executor,
    );
    command_executor::release_executor_dependents(
        &rt_handle,
        &executor,
        operator_peer_socket,
        persona_supervisor_executor_ready_tx,
    );

    #[cfg(unix)]
    let listener = listeners::bind_unix_listener(socket_path, post_init_started)?;
    listeners::announce_ipc_ready(socket_path);

    let state = Arc::new(ServerState::new_with_shared_state(
        rt_handle,
        memory_manager,
        runtime,
        channel_registries,
        rag_engine,
        call_plane.voice_service,
        audio_pool,
        file_engines,
        shell_sessions,
        gpu_manager,
        provider_registry,
        Arc::clone(&executor),
    ));

    log_info!("ipc", "server", "IPC server ready");

    #[cfg(unix)]
    tcp_ingress::spawn_optional_tcp_listener(&state);
    ws_ingress::spawn_ws_ingress(
        &state,
        &executor,
        &system_monitor,
        &call_plane.call_manager,
        &call_plane.voice_service_for_view,
        &node_presence_deps,
        &persona_bootstrap_room_name,
    );
    memory_guard::spawn_memory_guard(&state);

    #[cfg(unix)]
    let served = accept_loop::serve_unix_connections(state, listener);
    #[cfg(windows)]
    let served = accept_loop::serve_windows_tcp_connections(state, post_init_started);
    served
}
