//! Boot phase 15: build the substrate-wide CommandExecutor, install it on
//! every module, and release everything that was waiting on it.

use super::*;

/// Builds the wired executor and installs it on every registered module.
/// Returns it with the provider registry the connection layer shares.
pub(super) fn build_and_install_executor(
    runtime: &Arc<Runtime>,
    grid_state: Arc<crate::modules::grid::GridState>,
    airc_interceptor_cell: Arc<tokio::sync::OnceCell<Arc<airc_lib::Airc>>>,
    live_registrar_executor: &Arc<crate::runtime::LateBound<crate::runtime::CommandExecutor>>,
) -> (
    Arc<crate::runtime::CommandExecutor>,
    Arc<crate::runtime::ProviderRegistry>,
) {
    // Build the substrate-wide `CommandExecutor` and install it on every
    // registered module via the typed `ServiceModule::install_executor`
    // path (task #224 — replaces the deleted `GLOBAL_EXECUTOR` +
    // `executor()` panic accessor).
    //
    // Interceptor chain order (per MODULE-ARCHITECTURE.md §5): airc
    // sits at the head so explicit aircPeer/aircRoom targeting beats
    // grid's capability-based remote routing. grid sits next so
    // routingHint / nodeId / capability-based commands hop to a peer
    // before the kernel tries local Rust dispatch. Both interceptors
    // decline cleanly when their routing decision is "local," so
    // existing commands see zero behavior change.
    //
    // The ONE provider registry, OWNED by the Runtime so all three readers share
    // it: the ProvidedCommandInterceptor (in-process/persona route), the
    // connection layer (writer, via ServerState — binds an eye-node on connect),
    // AND `Runtime::route_command` (the socket route: uu / IPC / MCP). An
    // eye-node's `provider/register` on any connection binds here; every dispatch
    // path routes perception/observe to it, or fails loud when none is connected.
    let provider_registry = runtime.provider_registry();
    let executor = Arc::new(
        crate::runtime::CommandExecutor::new(runtime.registry_arc())
            .with_interceptor_chain(runtime.interceptor_chain())
            // Share the ONE runtime bus (the same Arc every ModuleContext
            // gets — `chat:posted` from the airc daemon-attach projector
            // and `presence:updated` from the node presence emitter both
            // land here). Without this the WS positron projection has no
            // event source and `message_bus()` is None — the boot-loud
            // panic the `CONTINUUM_CORE_WS` block asserts against.
            .with_message_bus(runtime.bus_arc())
            // INSTALL THE GENERIC CROSS-GRID TRANSPORT.
            //
            // Every piece of command-over-airc was already built and green —
            // the wire envelope carries an arbitrary path + params, the
            // receiver (`CommandRequestHandler::execute_command_request`)
            // does `CommandUri::local(path)` then `execute_with_caller` for
            // ANY path with AuthPolicy seeing the remote caller, and
            // `AircTransport` packages / sends / awaits / decodes any
            // non-Local `RouteDecision`. The ONLY call to
            // `with_remote_transport` in the whole crate lived inside
            // `#[cfg(test)]`.
            //
            // So the general mechanism was dark in production while the
            // PARALLEL one below it — `AircInterceptor`, which hard-refuses
            // everything except `ai/generate` — was wired. That is why the
            // substrate could only hop inference to a peer: not a missing
            // capability, an uninstalled one. The interceptor's own error
            // string calls generic command-over-airc "the follow-up", and
            // the follow-up already existed one module over.
            //
            // Shares the interceptor's late-bind cell rather than attaching a
            // second handle: `with_remote_transport` runs in the synchronous
            // boot region while `Airc::attach_as` is async, which is the exact
            // problem that cell already solves. Until it fills, dispatch fails
            // loud — never declines, because declining would run a
            // PEER-addressed command on this node.
            .with_remote_transport(Arc::new(crate::routing::LateBoundAircTransport::new(
                airc_interceptor_cell.clone(),
            )))
            .with_interceptor(Arc::new(crate::inference::airc_remote::interceptor::AircInterceptor::with_airc_cell(
                airc_interceptor_cell,
            )))
            .with_interceptor(Arc::new(crate::modules::grid::interceptor::GridInterceptor::new(grid_state)))
            // `provided` sits at the TAIL of the chain: airc/grid get first look
            // so an explicitly remote-targeted perception/observe still hops to a
            // peer's eye, but an ordinary (untargeted) Provided call — a persona
            // asking to SEE — routes here to a connected eye-node adapter, or
            // fails loud naming the missing eye-node. Empty registry today (the
            // eye-node client rides task #29): perception/observe + interface/
            // screenshot fail loud honestly instead of "no Rust module handles".
            // The connection layer will register providers via the interceptor's
            // shared `ProviderRegistry` when an eye-node connects.
            .with_interceptor(Arc::new(crate::runtime::ProvidedCommandInterceptor::new(
                Arc::clone(&provider_registry),
            )))
            // Hard ACL gate: authenticated AIRC callers use recorded access policy;
            // unknown peers keep the existing restricted tier. This is separate from
            // unauthenticated socket ceilings and verified per-command capability grants.
            //
            // Authenticated AIRC PeerId decisions already come from access_decision.
            // No registry bridge: discovery's default Blocked is not an explicit
            // authenticated-peer deny, and address trust must not promote AIRC callers.
            // grid/nodes exposes both facts separately. Unknown peers keep the existing
            // Provisional tier; enrollment alone grants no additional permissions.
            .with_policy(Arc::new(crate::modules::grid::GridTrustAuthPolicy::new())),
    );
    runtime
        .registry()
        .install_executor_on_all(Arc::clone(&executor));
    // The live session-registrar's executor (citizen population on human join).
    live_registrar_executor.install(Arc::clone(&executor));

    // L2 continuous-learning producer: hand the same wired executor to the
    // turn-completion training producer so a live persona reply can be scored,
    // classified, and submitted to `genome/training-trigger` AS the persona
    // (LocalPersona → Trusted). Late-bound because the service loop has no
    // executor in scope; this is the one install site.
    crate::persona::training_producer::install_executor(Arc::clone(&executor));
    // #249: the durable-transcript reader behind persona wake hydration shares
    // the same substrate executor (one dispatch chain, no parallel query stack).
    crate::persona::durable_history::install_executor(Arc::clone(&executor));
    (executor, provider_registry)
}

/// Everything that waits on the wired executor: the desktop server, the
/// operator self-peer, the dream consolidator and the persona host task.
pub(super) fn release_executor_dependents(
    rt_handle: &tokio::runtime::Handle,
    executor: &Arc<crate::runtime::CommandExecutor>,
    operator_peer_socket: Option<std::path::PathBuf>,
    mut persona_supervisor_executor_ready_tx: Option<
        tokio::sync::oneshot::Sender<Arc<crate::runtime::CommandExecutor>>,
    >,
) {
    // The desktop display manager (Joel: 'should work like a Display Manager'):
    // the core serves its own built UI, always current — browsers attach and
    // detach freely; continuum desktop opens the door.
    crate::http::desktop::spawn_if_configured(rt_handle);
    // #27 CLOSED: the operator self-peer boots beside the citizens — the
    // human's in-core airc identity, so room-scoped verbs invoked from the
    // operator seat READ AND SPEAK instead of denying. Spawned (not awaited):
    // identity ceremony does disk + daemon I/O, and boot must not block on it.
    if let Some(sock) = operator_peer_socket {
        let exec = Arc::clone(executor);
        let root = crate::paths::continuum_root();
        rt_handle.spawn(async move {
            crate::persona::operator_peer::ensure_operator_peer(&root, sock.clone(), exec.clone())
                .await;
            crate::persona::operator_peer::ensure_agent_peer(&root, sock.clone(), exec.clone()).await;
            // THE NODE ANSWERS (2026-09-17): a command pump under the scope's own
            // identity — the one the capacity beacon is stamped with and placement
            // targets — so a remote-bound mind's `ai/generate` reaches a handler
            // instead of every citizen saying "not for me" while it dies at 600 s.
            let state_home = crate::paths::continuum_home()
                .map(|h| h.join("state").join("node-responder"));
            match state_home {
                Ok(state_home) => {
                    if let Err(e) = crate::persona::command_inbound_pump::spawn_node_pump(
                        &root, sock, exec, &state_home,
                    )
                    .await
                    {
                        continuum_base::probe!(
                            class = "node.command_pump.boot_failed",
                            error = %e.to_string(),
                            "the node cannot answer cross-grid commands this boot — minds seated here by its beacon will time out until it can"
                        );
                    }
                }
                Err(e) => continuum_base::probe!(
                    class = "node.command_pump.boot_failed",
                    error = %e.to_string(),
                    "no continuum home for the node pump's state — the node cannot answer cross-grid commands this boot"
                ),
            }
        });
    }
    // Autonomic dream consolidation: the dream region dispatches `memory/consolidate`
    // through the SAME wired executor, so a persona consolidates the lessons other agents
    // taught her into her genome WHILE IDLE — the being-loop's received axis going
    // autonomic. Same install site + same executor as the L2 producer above.
    crate::cognition::dream_consolidation::install_consolidate_executor(Arc::clone(executor));

    // Round-2 verifier fix on PR #1568: now that the executor is
    // installed on every module, release the persona-supervisor task
    // (if wired). The spawned task at line ~1239 has been awaiting
    // this signal; it can now safely dispatch through `bootstrap_one`,
    // whose PIM has just been populated with the executor. Send-failure
    // means the receiver was dropped — substrate shutdown mid-boot —
    // and the supervisor already exited; ignoring is correct.
    if let Some(tx) = persona_supervisor_executor_ready_tx.take() {
        // Deliver the WIRED executor (GridTrustAuthPolicy + interceptors) — the
        // personas' hands ride exactly this, so the ACL gates what they touch.
        let _ = tx.send(Arc::clone(executor));
    }
}
