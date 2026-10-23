//! Boot phase 18: the WebSocket ingress for the thin-client fleet, and the
//! positron state sources that feed it.

use super::*;

pub(super) fn spawn_ws_ingress(
    state: &Arc<ServerState>,
    executor: &Arc<crate::runtime::CommandExecutor>,
    system_monitor: &Arc<SystemResourceMonitor>,
    call_manager: &Arc<crate::live::transport::call_server::CallManager>,
    voice_service_for_view: &Arc<crate::live::session::voice_service::VoiceService>,
    node_presence_deps: &Option<(std::path::PathBuf, airc_core::RoomId)>,
    persona_bootstrap_room_name: &Option<String>,
) {
    // WebSocket listener for the thin-client fleet (task #29). ON BY DEFAULT on
    // localhost:`DEFAULT_WS_PORT` so web + terminal clients connect out-of-the-box
    // with zero setup — override the port with `CONTINUUM_CORE_WS`, change the
    // bind host with `CONTINUUM_CORE_BIND` (default 127.0.0.1), or set
    // `CONTINUUM_CORE_WS=0` to disable the ingress entirely. Browsers can't speak
    // the length-prefixed IPC frame format, so thin clients (`sdk/typescript`
    // WebSocketTransport) speak WebSocket + the multiplexed
    // WsClientMessage/WsServerMessage envelope. Every frame dispatches through
    // the SAME `CommandRequestHandler::execute_command_request` owner the airc
    // peer path uses, stamped `CallerSource::Ws` → Provisional ceiling (see
    // ipc::ws module docs + the TCP SECURITY note above; same unauthenticated
    // AiSafe surface, 127.0.0.1 by default — do NOT bind 0.0.0.0 on an untrusted
    // network).
    const DEFAULT_WS_PORT: u16 = 8974;
    let ws_port_str =
        std::env::var("CONTINUUM_CORE_WS").unwrap_or_else(|_| DEFAULT_WS_PORT.to_string());
    {
        if let Ok(port) = ws_port_str.parse::<u16>() {
            if port > 0 {
                let bind_host = std::env::var("CONTINUUM_CORE_BIND")
                    .unwrap_or_else(|_| "127.0.0.1".to_string());
                let bind_addr = format!("{bind_host}:{port}");
                let ws_executor = Arc::clone(executor);
                // The positron state substrate for the thin-client fleet:
                // one shared snapshot+broadcast cell that WS sessions
                // subscribe against. The airc source wiring (task #29)
                // holds this same handle and calls `Substrate::store` on
                // each airc chat/roster change, so the projection tracks
                // the airc-owned truth (see `ipc::positron_source`).
                // Constructed here (not in `serve`) so the projection
                // subscriber shares the instance the server serves.
                let ws_substrate = continuum_positron::Substrate::new();

                // airc source wiring: subscribe the chat projection to
                // the live event bus with a clone of the served
                // substrate. It folds `chat:posted` + `presence:updated`
                // into the `chat` `ChatViewState` and stores each
                // transition, which streams down to subscribed WS
                // sessions as a `State` frame. Requires a wired bus —
                // fail loud (not a silent skip) if the executor has none,
                // because a WS server with no state source is a boot bug,
                // not a runtime condition ([[fallbacks-are-illegal-fail-loud]]).
                let projection_bus = ws_executor.message_bus().expect(
                    "CONTINUUM_CORE_WS is set but the command executor has no message bus — \
                     the positron chat projection has no airc source to subscribe to",
                );
                let room_purpose = resolve_room_purpose(
                    state,
                    &projection_bus,
                    node_presence_deps,
                    persona_bootstrap_room_name,
                );

                install_persona_experience_source(&room_purpose);

                positron_source::spawn(
                    &state.rt_handle,
                    projection_bus.clone(),
                    ws_substrate.clone(),
                    // Durable hydration seed: the bootstrap room's stored tail
                    // fills the projection BEFORE live events fold, so a core
                    // reboot never presents an empty room to any client.
                    node_presence_deps
                        .clone()
                        .map(|(_socket, room)| (Arc::clone(&ws_executor), room.as_uuid())),
                    room_purpose,
                    // Per-room stores (#408): every per-room envelope is mirrored into
                    // its OWN room's store, so a consumer that names its room (a
                    // citizen's grounding) reads THAT room instead of whichever room
                    // wrote last. The node substrate above still receives everything,
                    // so the focused-room web session is unchanged.
                    Some(crate::persona::room_substrates::global_room_substrates()),
                );

                // Per-citizen substrates for per-user views (nav): each connecting
                // citizen (?me) reads its own nav from here, unioned with the node
                // substrate for per-room views. Shared instance so the nav projector
                // (write) and the session (read) agree on for_citizen(me).
                let per_user =
                    std::sync::Arc::new(continuum_positron::scoping::PerUserSubstrates::new());

                // Nav wiring (nav slice 2, live): ONE room-set fold projecting the
                // observed airc stream (seeded with the bootstrap room so the
                // landing room exists before the first event), one live reader
                // over it, and the registry the WS ingress asks to ensure a
                // citizen's projector on first connection.
                let nav_seed: Vec<(Uuid, String)> = node_presence_deps
                    .clone()
                    .zip(persona_bootstrap_room_name.clone())
                    .map(|((_socket, room), name)| vec![(room.as_uuid(), name)])
                    .unwrap_or_default();
                let room_set = positron_nav_source::spawn_room_set_fold(
                    &state.rt_handle,
                    projection_bus.clone(),
                    nav_seed,
                    // #241: the durable membership registry — every subscribed
                    // room lands in nav at boot, not on first traffic.
                    node_presence_deps.clone().map(|(socket, _room)| socket),
                );
                // Member-name fold (the room fold's identity sibling): resolves a
                // persona-kind tab's title from the same presence stream.
                let member_set = positron_nav_source::spawn_member_set_fold(
                    &state.rt_handle,
                    projection_bus.clone(),
                );
                let nav_registry = Arc::new(positron_nav_source::NavProjectorRegistry::new(
                    projection_bus.clone(),
                    Arc::clone(&per_user),
                    Arc::new(positron_nav_source::ChannelBookmarksNavReader::new(
                        room_set, member_set,
                    )),
                ));

                // SYS gauge source (brick 2): sample the ONE shared resource
                // monitor into kind="system-metrics" on the served substrate —
                // the old sidebar's CPU/MEM sparkline, core-carried window.
                positron_metrics_source::spawn_system_metrics_emitter(
                    &state.rt_handle,
                    system_monitor.clone(),
                    ws_substrate.clone(),
                );

                // Serving glass box (#141 slice 1): fold the daemon's serving
                // snapshot + the MoE pager capture feed (when live) into
                // kind="serving" — the beat-WASTE control loop on screen.
                positron_serving_source::spawn_serving_emitter(
                    &state.rt_handle,
                    ws_substrate.clone(),
                );

                // Canvas feed (ninth ViewState, 2026-08-23): EVENT-driven — a
                // persona's own observe/hot-edit results publish frames at the
                // act seam; no tick, no scan. Install-only here; the act loop
                // does the publishing when work actually happens.
                positron_canvas_source::install(ws_substrate.clone());

                // Live-call glass box (#58): folds the TRANSPORT's calls against
                // the ORCHESTRATOR's registered sessions. Their disagreement is
                // the defect — a live call with no registration is why a persona
                // sits present and silent while isInCall() returns false and her
                // responses are dropped. Rendering it makes that a visible fact on
                // web + iOS + Android + TUI at once, instead of a mystery each
                // client rediscovers.
                positron_live_source::spawn_live_call_emitter(
                    &state.rt_handle,
                    ws_substrate.clone(),
                    call_manager.clone(),
                    voice_service_for_view.clone(),
                );

                spawn_node_projectors(
                    state,
                    node_presence_deps,
                    persona_bootstrap_room_name,
                    projection_bus,
                    &ws_substrate,
                );

                state.rt_handle.spawn(async move {
                    ws::serve(bind_addr, ws_executor, ws_substrate, per_user, nav_registry).await;
                });
            }
        }
    }
}

fn resolve_room_purpose(
    state: &Arc<ServerState>,
    projection_bus: &Arc<crate::runtime::MessageBus>,
    node_presence_deps: &Option<(std::path::PathBuf, airc_core::RoomId)>,
    persona_bootstrap_room_name: &Option<String>,
) -> crate::experience::room_purpose::SharedRoomPurpose {
    // Activity-purpose index (#6/#274/#329): resolve each room to the
    // recipe it was spawned from, by READING the binding `activity/spawn`
    // publishes to the room's wall. Until this existed, that binding had
    // no reader anywhere and every room — benchmark, foundry, video-call
    // — projected as a plain chat room to every renderer AND to the
    // citizen standing inside it. Without a daemon (headless / no
    // bootstrap room) the honest default stands: everything is chat.
    let room_purpose: crate::experience::room_purpose::SharedRoomPurpose = match node_presence_deps
        .clone()
        .zip(persona_bootstrap_room_name.clone())
    {
        Some(((socket, _room), room_name)) => {
            Arc::new(recipe_room_purpose::spawn_node_purpose_index(
                &state.rt_handle,
                projection_bus.clone(),
                socket,
                crate::paths::continuum_root()
                    .join("citizens")
                    .join("node")
                    .join("purpose")
                    .join("airc"),
                room_name,
            ))
        }
        None => crate::experience::room_purpose::default_source(),
    };
    room_purpose
}

fn install_persona_experience_source(room_purpose: &crate::experience::room_purpose::SharedRoomPurpose) {
    // S1: the SAME recipe registry the projection resolves from, handed
    // to the persona spawn path — so a room's authored affordances select
    // the citizen's tool surface, not only what the renderer draws. One
    // registry, two readers. The overlay refusal arm mirrors
    // positron_source::spawn: a malformed authored recipe is named, and
    // the embedded floor keeps every room resolvable.
    {
        use crate::experience::source::RecipeExperienceSource;
        let overlay_dir = RecipeExperienceSource::overlay_dir(
            &crate::paths::continuum_root(),
        );
        let source =
            match RecipeExperienceSource::builtins_with_overlay(room_purpose.clone(), &overlay_dir)
            {
                Ok(source) => source,
                Err(e) => {
                    tracing::error!(
                        error = %e,
                        dir = %overlay_dir.display(),
                        "recipe overlay REFUSED for the persona experience source — \
                         citizens see EMBEDDED recipes' affordances only until the \
                         named file is fixed or removed (#432)"
                    );
                    RecipeExperienceSource::builtins(room_purpose.clone())
                }
            };
        crate::experience::source::install_node_experience_source(Arc::new(source));
    }
}

fn spawn_node_projectors(
    state: &Arc<ServerState>,
    node_presence_deps: &Option<(std::path::PathBuf, airc_core::RoomId)>,
    persona_bootstrap_room_name: &Option<String>,
    projection_bus: Arc<crate::runtime::MessageBus>,
    ws_substrate: &continuum_positron::Substrate,
) {
    // Producer half of the same stream: attach a node-level
    // roster reader and emit `presence:updated` so the consumer
    // above has an identity source to fold in — otherwise every
    // rendered message keeps its provisional peer-id label
    // forever. Gated on the SAME (socket, room) precondition as
    // persona hosting; if airc discovery gave us neither, the
    // roster has no source and we log the skip (not a silent
    // fallback — an honestly-disabled projection).
    match node_presence_deps
        .clone()
        .zip(persona_bootstrap_room_name.clone())
    {
        Some(((daemon_socket, room_id), room_name)) => {
            let continuum_root = crate::paths::continuum_root();
            let node_home = continuum_root
                .join("citizens")
                .join("node")
                .join("presence")
                .join("airc");
            positron_presence::spawn_node_presence_emitter(
                &state.rt_handle,
                daemon_socket.clone(),
                node_home,
                room_id.as_uuid(),
                room_name.clone(),
                projection_bus.clone(),
            );

            // Roster-vitals radiator (design B emit half): sample each
            // resident persona's live WorkspaceCycle (service-tick tempo
            // + paged-in genome) and publish `persona:vitals` on the SAME
            // bus the chat projection reads, so those readouts breathe as
            // meters on the who-panel card. Reads the workspace registry
            // itself (where residents run); the projection folds by id.
            vitals_emitter::spawn_vitals_emitter(&state.rt_handle, projection_bus.clone());

            // Wall projector: the consuming half of `wall:changed`.
            // A dedicated node reader (own home + identity, distinct
            // from the presence lurker) re-reads the airc-owned
            // supersede-projected board on each change and stores it
            // as `kind="wall"`, so a chat window shows its pinned
            // board with zero resident personas. Same (socket, room)
            // precondition as presence — an empty roster would leave
            // wall authors provisionally labelled, but the board
            // itself still renders.
            let wall_home = continuum_root
                .join("citizens")
                .join("node")
                .join("wall")
                .join("airc");
            positron_wall_source::spawn_node_wall_projector(
                &state.rt_handle,
                daemon_socket.clone(),
                wall_home,
                room_id.as_uuid(),
                room_name.clone(),
                ws_substrate.clone(),
                projection_bus.clone(),
            );

            // Kanban projector: the consuming half of
            // `kanban:changed`. Its own node reader (distinct home
            // + identity from presence and wall) re-reads the
            // airc-work-owned board fold (`work_board_complete`) on
            // each change and stores it as `kind="kanban"`, so a
            // chat window shows the room's work board with zero
            // resident personas. Same (socket, room) precondition
            // as presence/wall — an empty roster leaves card
            // authors provisionally labelled, but the board still
            // renders. Wired here TOGETHER with the inbound
            // classification so the projector goes live fed.
            let kanban_home = continuum_root
                .join("citizens")
                .join("node")
                .join("kanban")
                .join("airc");
            positron_kanban_source::spawn_node_kanban_projector(
                &state.rt_handle,
                daemon_socket,
                kanban_home,
                room_id.as_uuid(),
                room_name,
                ws_substrate.clone(),
                projection_bus,
            );
        }
        None => {
            tracing::warn!(
                "positron presence emitter not started — airc discovery produced no \
                             (daemon socket, default room, room name); the WS roster will stay \
                             empty until airc is Healthy"
            );
        }
    }
}
