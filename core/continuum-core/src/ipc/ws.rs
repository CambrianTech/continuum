//! WebSocket ingress for the thin-client fleet (task #29).
//!
//! The Unix socket + TCP listeners speak the length-prefixed / newline IPC
//! frame format (`handle_client`), which is fine for the CLI and container
//! callers but not for a browser. Thin clients (web / mobile / TUI on the
//! `sdk/typescript` `WebSocketTransport`) speak **WebSocket** framing and the
//! multiplexed [`WsClientMessage`]/[`WsServerMessage`] envelope: a browser
//! fires N concurrent commands over one socket and matches replies by the
//! correlation `id`.
//!
//! This is additive — same binary, same server state, same dispatch owner.
//! Every frame funnels through
//! [`CommandRequestHandler::execute_command_request`], the single owner of
//! "wire request + caller → wire response" shared with the airc peer path
//! ([[the-compression-principle]]). No forked dispatch.
//!
//! ## Security
//!
//! A WS caller is stamped [`CallerSource::Ws`](crate::routing) → **Provisional
//! ceiling**, exactly like the TCP listener: the AiSafe surface (data reads,
//! `chat/send`, `ai/generate`) is reachable UNauthenticated, but Owner-gated
//! commands are refused at the dispatch boundary. Safe on the default loopback
//! bind; binding `0.0.0.0` exposes that surface to the LAN. A later GH-auth
//! handshake (task #29) raises the ceiling per authenticated user. Until then:
//! do NOT bind `0.0.0.0` on an untrusted network.

use std::net::SocketAddr;
use std::sync::Arc;

use continuum_airc_protocol::{AircCommandRequest, WsClientMessage, WsServerMessage};
use continuum_positron::scoping::{CompositeCache, PerUserSubstrates};
use continuum_positron::{run_session, ClientMessage, CommandDispatch, ServerMessage, Substrate};
use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_tungstenite::accept_hdr_async;
use tokio_tungstenite::tungstenite::handshake::server::{Request, Response};
use tokio_tungstenite::tungstenite::Message;

use crate::identity::PeerId;
use crate::ipc::positron_dispatch::ExecutorDispatch;
use crate::routing::{CallerIdentity, CommandRequestHandler};
use crate::runtime::CommandExecutor;

/// Extract the connecting citizen's id from the WS connect URL query — the
/// `me=<uuid>` param the client sends (`ws://…/?core=…&me=<uuid>`). `None` when
/// absent or unparseable: an anonymous session, honest — never a fabricated
/// identity ([[fallbacks-are-illegal-fail-loud]]). This is who the session's
/// per-user views (nav) belong to; a human and a persona pass it the same way.
fn parse_me(query: Option<&str>) -> Option<uuid::Uuid> {
    query?
        .split('&')
        .find_map(|pair| pair.strip_prefix("me="))
        .and_then(|v| uuid::Uuid::parse_str(v).ok())
}

/// Bind the WS listener on `bind_addr` and accept thin-client connections
/// until the process exits. Spawned on the tokio runtime from `start_server`
/// when `CONTINUUM_CORE_WS` is set. Fails loud (logs + returns) if the bind
/// fails — a dead listener never silently pretends to serve.
pub async fn serve(
    bind_addr: String,
    executor: Arc<CommandExecutor>,
    substrate: Substrate,
    per_user: Arc<PerUserSubstrates>,
    nav: Arc<crate::ipc::positron_nav_source::NavProjectorRegistry>,
) {
    let listener = match TcpListener::bind(&bind_addr).await {
        Ok(listener) => {
            crate::log_info!(
                "ipc",
                "ws",
                "WebSocket listener ready on {} (thin-client fleet — Provisional ceiling)",
                bind_addr
            );
            listener
        }
        Err(e) => {
            crate::log_error!(
                "ipc",
                "ws",
                "WS listener failed to bind {}: {}",
                bind_addr,
                e
            );
            return;
        }
    };

    // One production dispatcher shared across connections — the real
    // command surface behind the positron session's `CommandDispatch`
    // seam (see `positron_dispatch`). The WS transport routes commands
    // via the RPC path today, so this is not on the hot path here, but
    // `run_session` requires it and a session-routed command would
    // dispatch correctly.
    let dispatcher: Arc<dyn CommandDispatch> =
        Arc::new(ExecutorDispatch::new(Arc::clone(&executor)));

    loop {
        match listener.accept().await {
            Ok((stream, peer)) => {
                let executor = Arc::clone(&executor);
                let substrate = substrate.clone();
                let per_user = Arc::clone(&per_user);
                let dispatcher = Arc::clone(&dispatcher);
                let nav = Arc::clone(&nav);
                tokio::spawn(async move {
                    handle_ws_connection(
                        stream, peer, executor, substrate, per_user, dispatcher, nav,
                    )
                    .await;
                });
            }
            Err(e) => {
                crate::log_error!("ipc", "ws", "WS accept error: {}", e);
            }
        }
    }
}

/// Serve one upgraded WebSocket connection.
///
/// The one socket multiplexes two completion models (see the module
/// doc's ack-semantics note):
///
/// - **Commands** (`WsClientMessage::Command`) ride the **RPC path**:
///   each dispatches on its own task and its correlated
///   `WsServerMessage::Response` funnels back through the sender
///   channel. Per-command tasks are what make the correlation `id`
///   meaningful — command B can reply before the slower command A, and
///   the client resolves each by id. Serializing dispatch inline would
///   defeat the multiplexing the envelope exists for.
/// - **State subscriptions** (`Subscribe`/`Observe`) ride the
///   **positron path**: they feed one long-lived [`run_session`] task
///   (via [`WsClientMessage::to_session`]), whose `ServerMessage::State`
///   / `CommandFailed` output a drain task re-frames as
///   [`WsServerMessage`] onto the same sender channel. Subscriptions
///   have no per-frame reply — the live `State` stream IS the response.
async fn handle_ws_connection(
    stream: TcpStream,
    peer: SocketAddr,
    executor: Arc<CommandExecutor>,
    substrate: Substrate,
    per_user: Arc<PerUserSubstrates>,
    dispatcher: Arc<dyn CommandDispatch>,
    nav: Arc<crate::ipc::positron_nav_source::NavProjectorRegistry>,
) {
    // Capture the citizen id from the connect URL (`?me=<uuid>`) during the
    // handshake — this is WHO the session belongs to, so its per-user views (nav)
    // resolve to THIS citizen's substrate instead of an anonymous nil. A human
    // browser and a persona client pass it the identical way: the session is
    // citizen-scoped, and the code has no is-human branch to say otherwise.
    let captured_me: Arc<std::sync::Mutex<Option<uuid::Uuid>>> =
        Arc::new(std::sync::Mutex::new(None));
    let cap = captured_me.clone();
    let ws_stream = match accept_hdr_async(stream, move |req: &Request, resp: Response| {
        *cap.lock().unwrap_or_else(|e| e.into_inner()) = parse_me(req.uri().query());
        Ok(resp)
    })
    .await
    {
        Ok(ws) => ws,
        Err(e) => {
            crate::log_debug!("ipc", "ws", "WS handshake failed for {}: {}", peer, e);
            return;
        }
    };
    let citizen = *captured_me.lock().unwrap_or_else(|e| e.into_inner());
    crate::log_debug!(
        "ipc",
        "ws",
        "WS client connected: {} (citizen: {:?})",
        peer,
        citizen
    );

    // A citizen session's per-user views need their projector running — the
    // registry spawns it on first arrival, no-ops on every later connection.
    if let Some(me) = citizen {
        nav.ensure(me);
    }

    let (mut ws_sink, mut ws_source) = ws_stream.split();

    // Sender task: owns the write half, serializes reply frames. Both the
    // per-command RPC tasks and the positron drain task push here.
    let (tx, mut rx) = mpsc::channel::<Message>(64);
    let sender_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if ws_sink.send(msg).await.is_err() {
                break;
            }
        }
    });

    // Positron session: one task per connection. `run_session` reads
    // `ClientMessage`s from `session_in` and emits `ServerMessage`s on
    // `session_out`; the drain task re-frames those as `WsServerMessage`
    // onto the shared sender. Dropping `session_in_tx` at teardown closes
    // the inbound channel → `run_session` returns cleanly → its
    // forwarders abort → `session_out_tx` drops → the drain task ends.
    let (session_in_tx, session_in_rx) = mpsc::channel::<ClientMessage>(64);
    let (session_out_tx, mut session_out_rx) = mpsc::channel::<ServerMessage>(64);
    let session_task = tokio::spawn(async move {
        // A citizen session (`?me` present) reads its per-USER views (nav) from ITS
        // OWN substrate, unioned with the node substrate for per-ROOM views (chat/
        // wall/kanban) — the composite. An anonymous session reads the node substrate
        // alone. Identical path for a human and a persona: the only difference is
        // whether `?me` was on the URL, never who they are.
        let result = match citizen {
            Some(me) => {
                let composite = CompositeCache::new(substrate, per_user.for_citizen(me));
                run_session(session_in_rx, session_out_tx, composite, dispatcher).await
            }
            None => run_session(session_in_rx, session_out_tx, substrate, dispatcher).await,
        };
        if let Err(e) = result {
            crate::log_warn!(
                "ipc",
                "ws",
                "positron session for {} ended with error: {}",
                peer,
                e
            );
        }
    });
    let drain_tx = tx.clone();
    let drain_task = tokio::spawn(async move {
        while let Some(server_msg) = session_out_rx.recv().await {
            let Some(reply) = ws_reply_from_session(server_msg) else {
                continue;
            };
            match serde_json::to_string(&reply) {
                Ok(json) => {
                    if drain_tx.send(Message::Text(json.into())).await.is_err() {
                        break;
                    }
                }
                Err(e) => {
                    crate::log_error!("ipc", "ws", "failed to serialize positron frame: {}", e);
                }
            }
        }
    });

    // #170: live token rail. Subscribe this connection to the ephemeral persona-turn
    // token stream and push each token as a `StreamDelta` frame, alongside (never
    // replacing) the durable positron `State` path above — so a persona visibly types
    // in the browser. Loops on a process-static broadcast that never closes, so it is
    // ABORTED at teardown (below), not drained.
    let stream_tx = tx.clone();
    let stream_task = tokio::spawn(async move {
        let mut rail = crate::ipc::stream_rail::subscribe();
        loop {
            match rail.recv().await {
                Ok(d) => {
                    let frame = WsServerMessage::stream_delta(
                        d.room_id, d.sender_id, d.stream_id, d.seq, d.token, d.done,
                    );
                    match serde_json::to_string(&frame) {
                        Ok(json) => {
                            if stream_tx.send(Message::Text(json.into())).await.is_err() {
                                break;
                            }
                        }
                        Err(e) => {
                            crate::log_error!("ipc", "ws", "failed to serialize stream delta: {}", e)
                        }
                    }
                }
                // This client lagged past the buffer — it skipped some tokens. Cosmetic
                // (the durable say() row is authoritative); keep streaming the rest.
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                // The static rail sender never drops; treat a Closed as end-of-task.
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    while let Some(frame) = ws_source.next().await {
        match frame {
            Ok(Message::Text(text)) => {
                let client_msg: WsClientMessage = match serde_json::from_str(&text) {
                    Ok(msg) => msg,
                    Err(e) => {
                        // Malformed frame carries no correlation id to answer to.
                        // Log loud, drop; never fabricate a bogus id=0 reply.
                        crate::log_warn!(
                            "ipc",
                            "ws",
                            "dropping malformed WS frame (no correlation id to answer): {}",
                            e
                        );
                        continue;
                    }
                };
                match client_msg {
                    WsClientMessage::Command { id, request } => {
                        let executor = Arc::clone(&executor);
                        let tx = tx.clone();
                        tokio::spawn(async move {
                            let reply = dispatch_command(id, &request, &executor).await;
                            match serde_json::to_string(&reply) {
                                Ok(json) => {
                                    let _ = tx.send(Message::Text(json.into())).await;
                                }
                                Err(e) => {
                                    crate::log_error!(
                                        "ipc",
                                        "ws",
                                        "failed to serialize WS reply: {}",
                                        e
                                    );
                                }
                            }
                        });
                    }
                    // Subscribe / Observe → the positron session. `to_session`
                    // returns None only for Command (handled above), so a
                    // None here is a genuine protocol contradiction, not a
                    // dropped subscription.
                    other => match other.to_session() {
                        Some(session_msg) => {
                            if session_in_tx.send(session_msg).await.is_err() {
                                crate::log_debug!(
                                    "ipc",
                                    "ws",
                                    "positron session inbound closed; dropping frame from {}",
                                    peer
                                );
                            }
                        }
                        None => {
                            crate::log_warn!(
                                "ipc",
                                "ws",
                                "non-Command frame did not project to a session message (protocol contradiction)"
                            );
                        }
                    },
                }
            }
            // tokio-tungstenite auto-replies to Ping and handles Pong; Binary
            // isn't part of the thin-client envelope yet (all frames are JSON
            // text). Close / transport error ends the connection.
            Ok(Message::Close(_)) => break,
            Ok(_) => {}
            Err(e) => {
                crate::log_debug!("ipc", "ws", "WS receive error from {}: {}", peer, e);
                break;
            }
        }
    }

    // Teardown order: drop the session inbound so `run_session` exits and
    // its forwarders abort; drop the last sender `tx` so the sender task
    // drains and exits. Then await all three tasks so no forwarder or
    // channel outlives the connection.
    drop(session_in_tx);
    // #170: the stream task loops on the process-static rail (never closes) and holds
    // a `tx` clone — abort + await it FIRST so that clone drops, else the sender task
    // never sees all senders gone and teardown hangs.
    stream_task.abort();
    let _ = stream_task.await;
    drop(tx);
    let _ = session_task.await;
    let _ = drain_task.await;
    let _ = sender_task.await;
    crate::log_debug!("ipc", "ws", "WS client disconnected: {}", peer);
}

/// Dispatch one RPC `Command` as a Provisional WS caller and pair the
/// result to its correlation `id`. Unauthenticated remote socket → nil
/// peer_id, `Ws` source; trust comes from the source (Provisional
/// ceiling), not the id.
async fn dispatch_command(
    id: u64,
    request: &AircCommandRequest,
    executor: &CommandExecutor,
) -> WsServerMessage {
    let caller = CallerIdentity::ws(PeerId::from_uuid(uuid::Uuid::nil()));
    let response = CommandRequestHandler::execute_command_request(executor, request, caller).await;
    WsServerMessage::response(id, response)
}

/// Re-frame a positron [`ServerMessage`] as the thin-client
/// [`WsServerMessage`] that rides the WS transport.
///
/// `State` is the live snapshot/update — the whole reason the positron
/// path exists. `CommandFailed` returns `None`: this transport routes
/// commands via the RPC path (`WsServerMessage::Response{status:error}`
/// carries a failure), so `run_session` here is never fed a `Command`
/// and cannot legitimately emit `CommandFailed`. If one arrives it's a
/// wiring contradiction — log it loud (never fabricate a wire frame the
/// envelope has no variant for), and let the caller skip it.
fn ws_reply_from_session(msg: ServerMessage) -> Option<WsServerMessage> {
    match msg {
        ServerMessage::State(envelope) => Some(WsServerMessage::state(envelope)),
        ServerMessage::CommandFailed { correlation_id, .. } => {
            crate::log_warn!(
                "ipc",
                "ws",
                "positron session emitted CommandFailed (correlation {}) on the WS transport, \
                 which routes commands via RPC — dropping (command failures surface as \
                 Response{{status:error}})",
                correlation_id
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use continuum_airc_protocol::{AircCommandRequest, AircCommandResponse};

    /// An inert nav registry for connection-handler tests: real type, fixed
    /// empty room set, fresh bus. Anonymous test connections never call
    /// `ensure`, and a citizen-scoped test that does gets a projector writing
    /// into the same `per_user` the session reads — the real wiring, no mock.
    fn test_nav_registry(
        per_user: Arc<PerUserSubstrates>,
    ) -> Arc<crate::ipc::positron_nav_source::NavProjectorRegistry> {
        use crate::ipc::positron_nav_source::{ChannelBookmarksNavReader, NavProjectorRegistry};
        Arc::new(NavProjectorRegistry::new(
            Arc::new(crate::runtime::MessageBus::new()),
            per_user,
            Arc::new(ChannelBookmarksNavReader::fixed(Vec::new())),
        ))
    }

    // what this catches: the citizen id is extracted from the `?me=<uuid>` connect
    // query the client already sends — so a WS session is citizen-scoped, not the
    // nil-caller anonymous it was. Absent/garbage → None (honest anonymous), not a
    // fabricated identity.
    #[test]
    fn parse_me_extracts_the_citizen_from_the_connect_query() {
        let me = uuid::Uuid::from_u128(0xa54a);
        let q = format!("core=ws%3A%2F%2Fx&me={me}&other=1");
        assert_eq!(parse_me(Some(&q)), Some(me), "extracts me from a real query");
        assert_eq!(parse_me(Some("me=not-a-uuid")), None, "garbage uuid → None");
        assert_eq!(parse_me(Some("core=x&room=general")), None, "no me param → None");
        assert_eq!(parse_me(None), None, "no query → None");
    }

    // what this catches: a malformed frame must yield None (no id → nothing to
    // correlate a reply to), never a panic or a bogus id=0 response.
    #[tokio::test]
    async fn malformed_frame_yields_no_reply() {
        // No executor needed: parse fails before dispatch. Build a throwaway
        // executor only to satisfy the signature would be heavier than the
        // value; assert the parse-guard directly instead.
        let bad = "{ not json";
        let parsed: Result<WsClientMessage, _> = serde_json::from_str(bad);
        assert!(
            parsed.is_err(),
            "malformed frame must not parse to an envelope"
        );
    }

    // what this catches: a well-formed Command frame round-trips into the
    // envelope with its correlation id intact, so a reply can be paired to it.
    #[test]
    fn command_frame_carries_correlation_id() {
        let msg = WsClientMessage::Command {
            id: 42,
            request: AircCommandRequest::new(
                "chat/send".into(),
                continuum_airc_protocol::KIND_PEER.into(),
                None,
                serde_json::json!({"room": "general", "message": "hi"}),
            ),
        };
        let wire = serde_json::to_string(&msg).expect("serialize");
        let back: WsClientMessage = serde_json::from_str(&wire).expect("deserialize");
        match back {
            WsClientMessage::Command { id, request } => {
                assert_eq!(id, 42);
                assert_eq!(request.path, "chat/send");
            }
            other => panic!("expected a Command frame, got {other:?}"),
        }
        // And the reply pairs to the same id.
        let reply = WsServerMessage::response(42, AircCommandResponse::ok(serde_json::json!({})));
        match reply {
            WsServerMessage::Response { id, .. } => assert_eq!(id, 42),
            other => panic!("expected a Response reply, got {other:?}"),
        }
    }

    // what this catches: the FULL server ingress over a REAL socket — the piece
    // no unit test reaches: `accept_async` upgrade, the mpsc sender task, and the
    // per-command dispatch spawn in `handle_ws_connection`. A real client frames a
    // `WsClientMessage::Command`, it flows through `execute_command_request` into a
    // module, and the correlated `WsServerMessage::Response` unwraps to the result.
    // This is the wire-level twin of the TS `WebSocketTransport` spec.
    #[tokio::test]
    async fn ws_command_roundtrips_through_a_real_socket() {
        use crate::runtime::service_module::CommandResult;
        use crate::runtime::{
            ModuleConfig, ModuleContext, ModulePriority, ModuleRegistry, ServiceModule,
        };
        use tokio_tungstenite::connect_async;

        // A module that echoes its params back — so the reply proves the command
        // reached a handler with its params intact, not just that a frame bounced.
        struct EchoModule;
        #[async_trait::async_trait]
        impl ServiceModule for EchoModule {
            fn config(&self) -> ModuleConfig {
                ModuleConfig {
                    name: "echo",
                    priority: ModulePriority::Normal,
                    command_prefixes: &["echo/"],
                    event_subscriptions: &[],
                    needs_dedicated_thread: false,
                    max_concurrency: 0,
                    tick_interval: None,
                }
            }
            async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
                Ok(())
            }
            async fn handle_command(
                &self,
                _command: &str,
                params: serde_json::Value,
            ) -> Result<CommandResult, String> {
                Ok(CommandResult::Json(params))
            }
            fn as_any(&self) -> &dyn std::any::Any {
                self
            }
        }

        let registry = Arc::new(ModuleRegistry::new());
        registry.register(Arc::new(EchoModule));
        let executor = Arc::new(CommandExecutor::new(registry));

        // Bind an ephemeral loopback port and drive the REAL connection handler.
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind ephemeral");
        let addr = listener.local_addr().expect("local addr");
        let exec = Arc::clone(&executor);
        let substrate = Substrate::new();
        let dispatcher: Arc<dyn CommandDispatch> =
            Arc::new(ExecutorDispatch::new(Arc::clone(&exec)));
        let per_user = Arc::new(PerUserSubstrates::new());
        let nav = test_nav_registry(Arc::clone(&per_user));
        tokio::spawn(async move {
            if let Ok((stream, peer)) = listener.accept().await {
                handle_ws_connection(stream, peer, exec, substrate, per_user, dispatcher, nav)
                    .await;
            }
        });

        let (mut ws, _resp) = connect_async(format!("ws://{addr}"))
            .await
            .expect("client connects to WS ingress");

        let frame = WsClientMessage::Command {
            id: 7,
            request: AircCommandRequest::new(
                "echo/run".into(),
                continuum_airc_protocol::KIND_PEER.into(),
                None,
                serde_json::json!({"hello": "world"}),
            ),
        };
        ws.send(Message::Text(serde_json::to_string(&frame).unwrap().into()))
            .await
            .expect("send command frame");

        let reply = ws
            .next()
            .await
            .expect("a reply arrives")
            .expect("reply is ok");
        let text = match reply {
            Message::Text(t) => t,
            other => panic!("expected a text reply, got {other:?}"),
        };
        let msg: WsServerMessage = serde_json::from_str(&text).expect("reply parses");
        match msg {
            WsServerMessage::Response { id, response } => {
                assert_eq!(id, 7, "correlation id survives the real socket round-trip");
                match response {
                    AircCommandResponse::Ok { result } => assert_eq!(
                        result["hello"], "world",
                        "the echo module saw the params; the reply unwraps to its result"
                    ),
                    AircCommandResponse::Error { message } => {
                        panic!("expected ok, got error: {message}")
                    }
                }
            }
            other => panic!("expected a Response, got {other:?}"),
        }
    }

    // what this catches: the OTHER half of the WS ingress no unit test reaches —
    // the positron state path over a REAL socket. A client `Subscribe` frame must
    // be decoded, routed into the per-connection `run_session` (NOT the RPC path),
    // deliver the snapshot as a `WsServerMessage::State`, and then fan a later
    // `Substrate::store` out live as another `State` frame — all without a second
    // request. This is the wire-level twin of `run_session`'s in-memory tests: it
    // proves `handle_ws_connection` wires Subscribe→session, the drain task, and
    // `ws_reply_from_session` together. A regression that dropped the session task
    // (or only served the snapshot) leaves the thin-client fleet un-live (#794).
    #[tokio::test]
    async fn ws_subscribe_streams_a_live_state_frame_over_a_real_socket() {
        use crate::runtime::{CommandExecutor as Exec, ModuleRegistry};
        use continuum_positron::{StateEnvelope, StateLayer};
        use tokio_tungstenite::connect_async;

        fn state_env(kind: &str, revision: u64) -> StateEnvelope {
            StateEnvelope {
                kind: kind.to_string(),
                revision: Some(revision),
                layer: StateLayer::Session,
                payload: serde_json::json!({ "rev": revision }),
            }
        }

        // The state path never dispatches a command, so an empty executor is
        // enough to satisfy the dispatcher the session task requires.
        let executor = Arc::new(Exec::new(Arc::new(ModuleRegistry::new())));

        // Shared substrate — the same handle the (future) airc source writes to.
        // Seed the "chat" kind so the Subscribe snapshot has a revision to serve.
        let substrate = Substrate::new();
        substrate.store(state_env("chat", 1));
        let dispatcher: Arc<dyn CommandDispatch> =
            Arc::new(ExecutorDispatch::new(Arc::clone(&executor)));

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind ephemeral");
        let addr = listener.local_addr().expect("local addr");
        let conn_substrate = substrate.clone();
        let per_user = Arc::new(PerUserSubstrates::new());
        let nav = test_nav_registry(Arc::clone(&per_user));
        tokio::spawn(async move {
            if let Ok((stream, peer)) = listener.accept().await {
                handle_ws_connection(
                    stream,
                    peer,
                    executor,
                    conn_substrate,
                    per_user,
                    dispatcher,
                    nav,
                )
                .await;
            }
        });

        let (mut ws, _resp) = connect_async(format!("ws://{addr}"))
            .await
            .expect("client connects to WS ingress");

        // Subscribe to "chat" — rides the positron path (to_session), not RPC.
        let subscribe = WsClientMessage::Subscribe {
            kinds: vec!["chat".into()],
            layers: vec![StateLayer::Session],
            last_seen: vec![],
        };
        ws.send(Message::Text(
            serde_json::to_string(&subscribe).unwrap().into(),
        ))
        .await
        .expect("send subscribe frame");

        // First frame: the snapshot at revision 1, re-framed as WsServerMessage::State.
        let snapshot = decode_state(ws.next().await);
        assert_eq!(snapshot.kind, "chat");
        assert_eq!(
            snapshot.revision,
            Some(1),
            "snapshot rides the wire as a State frame"
        );

        // A later store must arrive live over the SAME socket, no second request.
        substrate.store(state_env("chat", 2));
        let live = decode_state(ws.next().await);
        assert_eq!(
            live.revision,
            Some(2),
            "the store fanned out over the real socket as a live State frame"
        );
    }

    /// Decode the next WS frame as a `State` envelope, or panic loud with what
    /// arrived instead — a `Response` here would mean Subscribe was mis-routed
    /// onto the RPC path.
    #[cfg(test)]
    fn decode_state(
        frame: Option<Result<Message, tokio_tungstenite::tungstenite::Error>>,
    ) -> continuum_positron::StateEnvelope {
        let text = match frame.expect("a frame arrives").expect("frame is ok") {
            Message::Text(t) => t,
            other => panic!("expected a text frame, got {other:?}"),
        };
        match serde_json::from_str::<WsServerMessage>(&text).expect("frame parses") {
            WsServerMessage::State(env) => env,
            other => panic!("expected a State frame, got {other:?}"),
        }
    }
}
