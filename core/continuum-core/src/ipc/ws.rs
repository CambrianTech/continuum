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

use continuum_airc_protocol::{WsClientMessage, WsServerMessage};
use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::{accept_async, tungstenite::Message};

use crate::identity::PeerId;
use crate::routing::{CallerIdentity, CommandRequestHandler};
use crate::runtime::CommandExecutor;

/// Bind the WS listener on `bind_addr` and accept thin-client connections
/// until the process exits. Spawned on the tokio runtime from `start_server`
/// when `CONTINUUM_CORE_WS` is set. Fails loud (logs + returns) if the bind
/// fails — a dead listener never silently pretends to serve.
pub async fn serve(bind_addr: String, executor: Arc<CommandExecutor>) {
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
            crate::log_error!("ipc", "ws", "WS listener failed to bind {}: {}", bind_addr, e);
            return;
        }
    };

    loop {
        match listener.accept().await {
            Ok((stream, peer)) => {
                let executor = Arc::clone(&executor);
                tokio::spawn(async move {
                    handle_ws_connection(stream, peer, executor).await;
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
/// Each inbound command is dispatched on its own task and replies funnel back
/// through an mpsc channel to a single sender task — this is what makes the
/// correlation `id` meaningful: command B can complete and reply before the
/// slower command A, and the client resolves each by its id. Serializing
/// dispatch inline would defeat the multiplexing the envelope exists for.
async fn handle_ws_connection(stream: TcpStream, peer: SocketAddr, executor: Arc<CommandExecutor>) {
    let ws_stream = match accept_async(stream).await {
        Ok(ws) => ws,
        Err(e) => {
            crate::log_debug!("ipc", "ws", "WS handshake failed for {}: {}", peer, e);
            return;
        }
    };
    crate::log_debug!("ipc", "ws", "WS client connected: {}", peer);

    let (mut ws_sink, mut ws_source) = ws_stream.split();

    // Sender task: owns the write half, serializes reply frames. Concurrent
    // dispatch tasks push completed WsServerMessages here.
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Message>(64);
    let sender_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if ws_sink.send(msg).await.is_err() {
                break;
            }
        }
    });

    while let Some(frame) = ws_source.next().await {
        match frame {
            Ok(Message::Text(text)) => {
                let executor = Arc::clone(&executor);
                let tx = tx.clone();
                tokio::spawn(async move {
                    if let Some(reply) = dispatch_ws_text(&text, &executor).await {
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
                    }
                });
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

    // Dropping the last `tx` closes the channel; the sender task drains and exits.
    drop(tx);
    let _ = sender_task.await;
    crate::log_debug!("ipc", "ws", "WS client disconnected: {}", peer);
}

/// Decode one text frame into a [`WsClientMessage`], dispatch it as a
/// Provisional WS caller, and produce the correlated [`WsServerMessage`].
///
/// Returns `None` only when the frame can't be parsed into an envelope — a
/// malformed frame carries no correlation `id`, so there is nothing to answer
/// to. That path logs (loud), never silently swallows a well-formed request.
async fn dispatch_ws_text(text: &str, executor: &CommandExecutor) -> Option<WsServerMessage> {
    let client_msg: WsClientMessage = match serde_json::from_str(text) {
        Ok(msg) => msg,
        Err(e) => {
            crate::log_warn!(
                "ipc",
                "ws",
                "dropping malformed WS frame (no correlation id to answer): {}",
                e
            );
            return None;
        }
    };

    match client_msg {
        WsClientMessage::Command { id, request } => {
            // Unauthenticated remote socket → nil peer_id, Ws source. Trust
            // comes from the source (Provisional ceiling), not the id.
            let caller = CallerIdentity::ws(PeerId::from_uuid(uuid::Uuid::nil()));
            let response =
                CommandRequestHandler::execute_command_request(executor, &request, caller).await;
            Some(WsServerMessage::response(id, response))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use continuum_airc_protocol::{AircCommandRequest, AircCommandResponse};

    // what this catches: a malformed frame must yield None (no id → nothing to
    // correlate a reply to), never a panic or a bogus id=0 response.
    #[tokio::test]
    async fn malformed_frame_yields_no_reply() {
        // No executor needed: parse fails before dispatch. Build a throwaway
        // executor only to satisfy the signature would be heavier than the
        // value; assert the parse-guard directly instead.
        let bad = "{ not json";
        let parsed: Result<WsClientMessage, _> = serde_json::from_str(bad);
        assert!(parsed.is_err(), "malformed frame must not parse to an envelope");
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
        }
        // And the reply pairs to the same id.
        let reply = WsServerMessage::response(42, AircCommandResponse::ok(serde_json::json!({})));
        match reply {
            WsServerMessage::Response { id, .. } => assert_eq!(id, 42),
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
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind ephemeral");
        let addr = listener.local_addr().expect("local addr");
        let exec = Arc::clone(&executor);
        tokio::spawn(async move {
            if let Ok((stream, peer)) = listener.accept().await {
                handle_ws_connection(stream, peer, exec).await;
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

        let reply = ws.next().await.expect("a reply arrives").expect("reply is ok");
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
        }
    }
}
