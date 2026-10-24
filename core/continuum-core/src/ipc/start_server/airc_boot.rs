//! Boot phase 10: AIRC discovery, the AircModule, the AircInterceptor attach
//! task, and the mode gate that refuses to boot without a Healthy AIRC.

use super::*;

/// What discovery produced, for the persona-hosting, executor and WS phases.
pub(super) struct AircBoot {
    pub(super) discovery: crate::airc::AircDiscovery,
    pub(super) persona_bootstrap_deps: Option<std::path::PathBuf>,
    pub(super) operator_peer_socket: Option<std::path::PathBuf>,
    pub(super) discovered_default_room: Option<airc_core::RoomId>,
    pub(super) persona_bootstrap_room_name: Option<String>,
    pub(super) node_presence_deps: Option<(std::path::PathBuf, airc_core::RoomId)>,
    pub(super) airc_interceptor_cell: Arc<tokio::sync::OnceCell<Arc<airc_lib::Airc>>>,
}

pub(super) fn discover_and_register_airc(
    runtime: &Arc<Runtime>,
    rt_handle: &tokio::runtime::Handle,
    boot_mode: crate::runtime::BootMode,
) -> std::io::Result<AircBoot> {
    // AircModule: Rust-native AIRC queue/flywheel primitives.
    // Provides airc/queue-scan without routing through Node/TypeScript.
    // Discovery: `AircModule::discover_and_construct` asks `airc ipc-
    // endpoint` (airc#1095) for the canonical daemon socket and auto-
    // installs airc if missing — the previous derive-from-home scheme
    // drifted and broke headless boot. Uses rt_handle.block_on because
    // start_server is sync but discovery is async; we're on the main
    // bootstrap thread, not inside a tokio task, so blocking here is
    // safe and gates module registration on the discovery result.
    //
    // Outer 180s timeout caps total boot stall. Inner subprocess
    // waits have their own per-call deadlines (5s socket discovery,
    // 5s peer_id status, 120s auto-install) but the OUTER call has
    // no overall budget without this wrapper — a wedged daemon
    // could theoretically chain stalls beyond what individual
    // deadlines catch. 180s covers worst-case auto-install + the 45 s
    // patience for a restarting daemon (discover_with_patience). Reviewer-defect-driven (continuum #1507
    // finding 6); substrate-is-a-good-citizen "predictable startup"
    // non-negotiable.
    const AIRC_DISCOVERY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(180);
    let discovery = rt_handle.block_on(async {
        match tokio::time::timeout(
            AIRC_DISCOVERY_TIMEOUT,
            crate::airc::discover_with_patience(),
        )
        .await
        {
            Ok(d) => d,
            Err(_) => {
                tracing::error!(
                    timeout_secs = AIRC_DISCOVERY_TIMEOUT.as_secs(),
                    "AIRC discovery exceeded outer timeout — promoting to Unreachable."
                );
                crate::airc::AircDiscovery::Unreachable {
                    reason: crate::airc::DiscoveryFailure::EndpointCommandFailed(format!(
                        "discovery did not return within {}s — substrate is unresponsive",
                        AIRC_DISCOVERY_TIMEOUT.as_secs()
                    )),
                }
            }
        }
    });
    tracing::info!(
        kind = discovery.kind(),
        reason = ?discovery.reason(),
        "AIRC discovery complete"
    );
    // Honest boot-health line for the persona-comms-over-airc wire (card
    // e9f50a36 slice A3, reworked onto core/continuum-core). Operators +
    // sentinels see at boot whether airc is live / degraded / unreachable —
    // emitted via boot_status -> probe! (class "boot.status"), so it's both a
    // human stderr line AND subscribable by debug/probes/*. This is the
    // comms→airc lane making "is the persona comms wire actually up?" honest.
    {
        use crate::runtime::boot_status::{boot_status, BootStatusKind};
        let (kind, detail) = match &discovery {
            crate::airc::AircDiscovery::Healthy {
                socket,
                peer_id,
                room_name,
                ..
            } => {
                let peer_short: String = airc_core::shown_id::shown_form(*peer_id);
                (
                    BootStatusKind::Ok,
                    format!(
                        "socket={} peer={} room={}",
                        socket.display(),
                        peer_short,
                        room_name
                    ),
                )
            }
            crate::airc::AircDiscovery::Degraded { reason, .. } => {
                (BootStatusKind::Degraded, format!("degraded: {reason}"))
            }
            crate::airc::AircDiscovery::Unreachable { reason } => {
                (BootStatusKind::Failed, format!("unreachable: {reason}"))
            }
        };
        boot_status("airc", kind, &detail);
    }
    let airc_module = Arc::new(AircModule::from_discovery(&discovery));
    // #432 (was the vestigial gate): persona hosting needs the DAEMON SOCKET
    // only — #298 removed the operator room from birth deps, but the zip here
    // still required a discovered default room, so "no default room ⇒ no
    // citizens, for no reason". The room stays a dep ONLY for the consumers
    // that genuinely ride it: GridCapacityModule (gossip channel) and the
    // node-level presence emitter.
    let persona_bootstrap_deps = airc_module.daemon_socket().map(|p| p.to_path_buf());
    // #27: held for the operator self-peer boot after the executor is live.
    let operator_peer_socket = persona_bootstrap_deps.clone();
    let discovered_default_room = airc_module.default_room();
    let persona_bootstrap_room_name = airc_module.default_room_name().map(|s| s.to_string());
    // The node-level presence emitter (WS seam below) needs BOTH the daemon
    // socket and the default room. Build its pair here, BEFORE the persona
    // block moves the socket, so the emitter can attach a heartbeat-less node
    // reader against the same daemon + room the citizens attach to.
    let node_presence_deps = persona_bootstrap_deps.clone().zip(discovered_default_room);

    // AircInterceptor's late-bind Arc<Airc>: the interceptor is built ~800 lines
    // below (inside the executor), synchronously, but `Airc::attach_as` is async and
    // must not block the boot critical path. Create the cell HERE (where the daemon
    // socket is in scope), spawn a task that attaches the interceptor's own handle —
    // same daemon_socket + continuum_root the citizens use — and fills the cell, then
    // hand the SAME cell to the interceptor at construction. Until it's filled an
    // aircPeer-targeted command fails loud (never a silent fallthrough). Mirrors the
    // node-presence reader's own attach.
    let interceptor_airc_deps = persona_bootstrap_deps.clone();
    let airc_interceptor_cell: Arc<tokio::sync::OnceCell<Arc<airc_lib::Airc>>> =
        Arc::new(tokio::sync::OnceCell::new());
    {
        let cell = airc_interceptor_cell.clone();
        let root = crate::paths::continuum_root();
        // `rt_handle.spawn`, NOT bare `tokio::spawn` — this runs in the SYNCHRONOUS boot
        // region, AFTER the `rt_handle.enter()` guard (line ~1111) has dropped, so there is no
        // ambient Tokio runtime here. Bare `tokio::spawn` panics "there is no reactor running"
        // and kills the IPC listener thread → the socket never binds → the whole core is a
        // zombie (regression from #2051's AircInterceptor block; every sibling spawn in this fn
        // already uses `rt_handle.spawn`).
        //
        // UNCONDITIONAL since card e28a0340. This attach used to sit inside
        // `if let Some(daemon_socket)`, so a core that booted with no daemon never even
        // spawned the task — and the one attempt it did make on a found socket was a
        // one-shot whose own error text named "a boot with a reachable airc daemon" as
        // the remedy. Measured cost: BigMama failed 1,216 times over six hours against a
        // live pipe; Windows (2026-09-22) restored its daemon under a running core and
        // still refused every peer-addressed command. The boot socket is now a HINT for
        // the first attempt — `attach_until_live` re-resolves on every later one.
        rt_handle.spawn(crate::airc::reattach::attach_until_live(
            root,
            "continuum-airc-interceptor",
            interceptor_airc_deps,
            cell,
        ));
        // #2561: the ACTIVITY GATE's owning task — boredom as substrate. Spawned
        // here with every other boot task (rt_handle.spawn: synchronous boot
        // region, no ambient reactor).
        rt_handle.spawn(async move {
            crate::cognition::activity_gate::spawn_activity_gate();
        });
    }

    runtime.register(airc_module);

    // A.2 [[no-fallbacks-ever]]: in `FullCitizen` or `FailFast` mode,
    // the substrate REFUSES to boot if AIRC isn't `Healthy` — there's
    // no path where degraded discovery silently substitutes
    // inference-only mode (Slice A's R2#2 violation). Operator must
    // explicitly opt into `--mode=inference-only` to allow degraded
    // boot. The seed-presence heuristic Slice A used is gone.
    if boot_mode.requires_persona_hosting() && !discovery.can_host_personas() {
        let reason_msg = discovery
            .reason()
            .map(|r| format!("{}", r))
            .unwrap_or_else(|| "AIRC discovery did not produce Healthy state".to_string());
        tracing::error!(
            mode = boot_mode.label(),
            discovery_kind = discovery.kind(),
            reason = %reason_msg,
            "Refusing to boot: --mode={} requires AIRC Healthy, but discovery is {}. \
             Resolve the AIRC issue (see error above) OR re-launch with \
             --mode=inference-only to opt into degraded operation.",
            boot_mode.label(),
            discovery.kind()
        );
        return Err(std::io::Error::other(format!(
            "AIRC discovery {} but --mode={} requires Healthy. \
             Reason: {}. Resolve AIRC or use --mode=inference-only ([[no-fallbacks-ever]])",
            discovery.kind(),
            boot_mode.label(),
            reason_msg
        )));
    }
    Ok(AircBoot {
        discovery,
        persona_bootstrap_deps,
        operator_peer_socket,
        discovered_default_room,
        persona_bootstrap_room_name,
        node_presence_deps,
        airc_interceptor_cell,
    })
}
