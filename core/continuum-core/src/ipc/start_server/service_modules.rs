//! Boot phase 9: code, data, recipe, chat, logger, probe-stream, embedding,
//! runtime-control, MCP and agent modules.

use super::*;

/// The code-module state the IPC server state and the vision module share.
pub(super) struct CodeHandles {
    pub(super) file_engines: Arc<DashMap<String, FileEngine>>,
    pub(super) shell_sessions: Arc<DashMap<String, ShellSession>>,
    pub(super) code_state: Arc<CodeState>,
}

pub(super) fn register_code_data_and_service_modules(
    runtime: &Arc<Runtime>,
    rt_handle: &tokio::runtime::Handle,
    memory_manager: &Arc<crate::memory::PersonaMemoryManager>,
) -> CodeHandles {
    // Phase 3: CodeModule (wraps file engines and shell sessions per-persona)
    let file_engines: Arc<DashMap<String, FileEngine>> = Arc::new(DashMap::new());
    let shell_sessions: Arc<DashMap<String, ShellSession>> = Arc::new(DashMap::new());
    // Pushed shell completions (2026-08-24): the exit fold publishes
    // command:completed for handed-back executions; wire it the bus once.
    crate::code::shell_session::set_shell_completion_bus(runtime.bus_arc());
    // The retired benchmark harness's files move into one archive dir, once (card c4f4b8cd).
    crate::system_resources::harness_archive::spawn_archive(&rt_handle);
    let code_state = Arc::new(CodeState::new(
        file_engines.clone(),
        shell_sessions.clone(),
        rt_handle.clone(),
        Arc::new(crate::modules::acting_provider::CoreActing),
    ));
    runtime.register(Arc::new(CodeModule::new(code_state.clone())));

    // Phase 4: DataModule (database-agnostic storage via ORM adapters)
    // DB path is passed per-request from TypeScript - NO defaults
    runtime.register(Arc::new(crate::modules::data_wiring::data_module()));
    // RecipeModule: the node's durable recipe runs (`recipe_runs`) and the recipe/*
    // verbs. Its store opens through DataModule's adapter at initialize.
    runtime.register(Arc::new(crate::modules::recipe::RecipeService(crate::recipe::runs_owner::RecipeModule::new())));

    // ChatModule: the kernel `chat/send` + `chat/poll` surface (aliases of
    // `collaboration/chat/*`). Unlike `search/*` these DynCommands are NOT
    // inventory-self-registering — they carry a late-bound `CommandExecutor`
    // (via `command_objects(executor_slot)`) so `chat/send` can dual-write to
    // `data/*` + airc. That injected state is why the module is the required
    // carrier. It was defined and schema-registered but never wired into boot,
    // so `chat/send`/`chat/poll` LISTED via `commands/list` yet failed to route
    // ("No module registered for this command prefix") — a discoverability lie.
    // The slot is filled by `install_executor_on_all` after all registration,
    // so ordering here is irrelevant.
    // (#140: the durable-transcript writer is spawned by ChatModule::initialize
    // on the runtime handle — registration here runs on a non-tokio thread.)
    runtime.register(Arc::new(crate::modules::chat::ChatModule::new()));

    // Phase 4a: LoggerModule (absorbs standalone logger worker)
    // Provides log/write, log/ping via main socket
    runtime.register(Arc::new(LoggerModule::new()));

    // ProbeStreamModule: the LIVE glass-box stream — debug/probes/{open,next,close}
    // over the ProbeRouterLayer fanout (the historical on-disk ledger is
    // debug/probes/query, stateless, self-registered). #362: this module shipped
    // 2026-07 and was registered NOWHERE — and the second half of the trap is that
    // it MUST be built against the router installed in the global subscriber
    // stack. `ProbeRouterLayer::new()` here would compile, register, route, and
    // stream silence forever (per-instance Arc<RwLock<..>> state). The handle
    // comes from install_probe_tracing via installed_probe_router(). Fail LOUD,
    // not silent, when tracing was never installed — the stream API being absent
    // is then a stated fact, not a mystery "Unknown command".
    match crate::routing::installed_probe_router() {
        Some(router) => {
            runtime.register(Arc::new(
                crate::probe_stream::ProbeStreamModule::new(router),
            ));
        }
        None => {
            tracing::error!(
                "debug/probes live stream UNAVAILABLE: install_probe_tracing never ran \
                 in this process, so there is no ProbeRouterLayer to subscribe to — \
                 debug/probes/{{open,next,close}} will not route. Production boots \
                 install tracing in main.rs before start_server; only bare test \
                 harnesses should ever see this."
            );
        }
    }

    // search/* migrated to the DynCommand registry (commands/search/*) — the four
    // verbs self-register via inventory, no module registration needed here.

    // Phase 4c: EmbeddingModule (absorbs standalone embedding worker)
    // Provides embedding/generate, embedding/model/{load,list,info,unload}
    runtime.register(Arc::new(EmbeddingModule::new(
        memory_manager.embedding_provider(),
    )));

    // RuntimeModule: Exposes metrics and control for AI-driven system management (Ares)
    // Provides runtime/metrics/{all,module,slow}, runtime/list
    runtime.register(Arc::new(
        crate::modules::runtime_control::RuntimeModule::new(),
    ));

    // MCPModule: Dynamic tool discovery for MCP servers
    // Provides mcp/list-tools, mcp/search-tools, mcp/tool-help
    runtime.register(Arc::new(crate::modules::mcp::MCPModule::new()));

    CodeHandles {
        file_engines,
        shell_sessions,
        code_state,
    }
}
