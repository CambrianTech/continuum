//! continuum-client-ffi — the FFI-clean JSON-boundary facade over
//! `continuum-client`.
//!
//! The single binding source every per-platform SDK wraps. `Commands.execute`
//! is generic (`<P, R>`) and generics can't cross an FFI boundary, so this
//! facade reduces the two universal primitives to their JSON-at-the-boundary
//! form — exactly what they ARE on the wire (cross-grid via airc ==
//! cross-language via FFI, same shape):
//!
//! - `execute(command, params_json) -> result_json`
//! - `subscribe(class, callback)` streaming `event_json` to a foreign callback
//!
//! Each language SDK adds the typed/idiomatic layer on top (Swift async/await +
//! `AsyncStream`, Kotlin suspend + `Flow`, Dart `Stream`, TS Promise) from
//! GENERATED types — never hand-written. The JSON shape is the canonical
//! contract; this facade is the generic-free, tiny, stable surface the
//! generators bind.
//!
//! Distribution (see docs/architecture/CLIENT-SDK-PLATFORM-ARCHITECTURE.md):
//! uniffi reads this crate → one native binding → xcframework (Apple) + AAR
//! (Android); the native SDKs wrap those; Flutter bundles them; web takes a
//! separate wasm-bindgen path over the same facade.
//!
//! This module is the tool-agnostic Rust core. The uniffi `.udl` / annotations
//! and the wasm-bindgen layer are thin per-target shells over `ContinuumClient`
//! + the boundary helpers below.

use std::sync::Arc;

use continuum_client::{
    AircIpcTransport, ClientError, CommandClient, Connection, EventSubscriber, ServeHandler,
    SessionIdentity, Transport,
};
use futures::StreamExt;
use uuid::Uuid;

/// Re-export so every per-platform binding reads the one identity record:
/// `{ userId?, sessionId? }`. The FFI surface for `ContinuumClient::session`.
pub use continuum_client::SessionIdentity as Session;

/// FFI-clean error. `continuum-client`'s `ClientError` is rich and Rust-shaped;
/// this flattens it to the few variants a foreign caller needs, each carrying a
/// human message. A refusal keeps `command` + `reason` since callers branch on
/// "the substrate said no" vs "the transport broke".
#[derive(Debug, thiserror::Error)]
pub enum FfiError {
    /// Establishing the session failed.
    #[error("connect failed: {0}")]
    Connect(String),
    /// The session is closed; further calls won't succeed.
    #[error("connection closed")]
    Closed,
    /// The substrate received the command but refused it.
    #[error("command `{command}` refused: {reason}")]
    Refused { command: String, reason: String },
    /// Params/result JSON did not encode or decode.
    #[error("codec error: {0}")]
    Codec(String),
    /// The transport layer failed (IPC, wire, etc.).
    #[error("transport error: {0}")]
    Transport(String),
}

impl From<ClientError> for FfiError {
    fn from(e: ClientError) -> Self {
        match e {
            ClientError::Connect(m) => FfiError::Connect(m),
            ClientError::Closed => FfiError::Closed,
            ClientError::Refused { command, reason } => FfiError::Refused { command, reason },
            ClientError::Codec(m) => FfiError::Codec(m),
            ClientError::Transport(m) => FfiError::Transport(m),
            ClientError::NotImplemented(m) => FfiError::Transport(format!("not implemented: {m}")),
        }
    }
}

/// A foreign-implemented sink for an event subscription. uniffi exposes this as
/// a callback interface; each SDK adapts it into the platform's stream type
/// (`AsyncStream` / `Flow` / `Stream`). Plain trait here so the facade is
/// tool-agnostic and unit-testable.
pub trait EventCallback: Send + Sync {
    /// One event, already serialized to a JSON string.
    fn on_event(&self, event_json: String);
    /// A recoverable error on the stream (the stream stays open).
    fn on_error(&self, message: String);
    /// The stream ended (substrate closed it or the subscription was dropped).
    fn on_closed(&self);
}

/// A foreign-implemented handler for a command this client PROVIDES — the serve
/// side of the Command primitive. uniffi exposes this as a callback interface;
/// the platform SDK supplies the per-platform adapter (web = DOM/canvas
/// screenshot, desktop = OS, AR/VR = renderer capture — one command identity, N
/// adapters). `handle` is sync from Rust's view (a foreign call); the serve path
/// runs it on a blocking worker so a heavy handler doesn't stall the runtime.
pub trait CommandHandler: Send + Sync {
    /// Run the provided command — JSON params in, JSON result out. `Err`
    /// surfaces to the caller as a command error, never a silent drop.
    fn handle(&self, params_json: String) -> Result<String, FfiError>;
}

/// Adapts a foreign [`CommandHandler`] to the client's [`ServeHandler`]
/// (JSON-string boundary ↔ `Value`), running the sync foreign handler on a
/// blocking worker so the serve loop's task isn't blocked (off-main-thread).
struct CommandHandlerAdapter {
    command: String,
    cb: Arc<dyn CommandHandler>,
}

#[async_trait::async_trait]
impl ServeHandler for CommandHandlerAdapter {
    async fn handle(&self, params: serde_json::Value) -> Result<serde_json::Value, ClientError> {
        let params_json =
            serde_json::to_string(&params).map_err(|e| ClientError::Codec(e.to_string()))?;
        let cb = Arc::clone(&self.cb);
        let command = self.command.clone();
        let result_json = tokio::task::spawn_blocking(move || cb.handle(params_json))
            .await
            .map_err(|e| ClientError::Transport(format!("provided handler task join: {e}")))?
            .map_err(|fe| ClientError::Refused {
                command,
                reason: fe.to_string(),
            })?;
        serde_json::from_str(&result_json).map_err(|e| ClientError::Codec(e.to_string()))
    }
}

/// Run one command over a connection's command client, JSON in / JSON out.
/// Generic over `Transport` so it's tested against `MockTransport` without a
/// live daemon; the concrete [`ContinuumClient`] delegates here.
async fn execute_json<T: Transport>(
    commands: CommandClient<T>,
    command: &str,
    params_json: &str,
) -> Result<String, FfiError> {
    let params: serde_json::Value =
        serde_json::from_str(params_json).map_err(|e| FfiError::Codec(e.to_string()))?;
    // CommandClient::execute is generic; monomorphize to Value→Value — the
    // JSON-value transport boundary is exactly what it wraps.
    let result: serde_json::Value = commands.execute(command, params).await?;
    serde_json::to_string(&result).map_err(|e| FfiError::Codec(e.to_string()))
}

/// Drive an event subscription into a foreign callback until the stream ends.
/// Generic for testability; the concrete client spawns this on a task.
async fn pump_events<T: Transport>(
    events: EventSubscriber<T>,
    class: String,
    callback: Arc<dyn EventCallback>,
) {
    match events.subscribe(&class).await {
        Ok(mut stream) => {
            while let Some(item) = stream.next().await {
                match item {
                    Ok(value) => match serde_json::to_string(&value) {
                        Ok(json) => callback.on_event(json),
                        Err(e) => callback.on_error(format!("event encode failed: {e}")),
                    },
                    Err(e) => callback.on_error(e.to_string()),
                }
            }
            callback.on_closed();
        }
        Err(e) => callback.on_error(e.to_string()),
    }
}

/// An open subscription. Dropping it aborts the pump task — so a foreign caller
/// unsubscribes simply by releasing this handle (no explicit close call to
/// forget). Without this an event subscription would leak its task.
pub struct Subscription {
    handle: tokio::task::JoinHandle<()>,
}

impl Drop for Subscription {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

/// An active command provision — the serve twin of [`Subscription`]. Dropping it
/// DEREGISTERS the command (release = revoke): the serve loop stops matching it.
pub struct Registration {
    conn: Connection<AircIpcTransport>,
    command: String,
}

impl Drop for Registration {
    fn drop(&mut self) {
        // revoke is async; spawn it (we're under a tokio runtime). Removing the
        // handler is all it takes — the serve loop then ignores the command.
        let conn = self.conn.clone();
        let command = std::mem::take(&mut self.command);
        tokio::spawn(async move {
            let _ = conn.revoke(&command).await;
        });
    }
}

/// The concrete, generic-free client a foreign binding holds. Wraps a
/// `Connection` over the real airc IPC transport; the per-platform binding
/// (uniffi/wasm) exposes exactly `execute` + `subscribe`.
pub struct ContinuumClient {
    conn: Connection<AircIpcTransport>,
}

impl ContinuumClient {
    /// Build over an established airc handle + the substrate's peer id. The
    /// IN-PROCESS entry — the CLI / a persona in the same process calls this
    /// directly (it already holds the `Arc<Airc>`, no overhead). The FFI entry
    /// is [`ContinuumClient::connect`] (foreign callers can't construct an
    /// `Arc<airc_lib::Airc>` across the boundary).
    pub fn new(airc: Arc<airc_lib::Airc>, target_peer: Uuid) -> Self {
        Self {
            conn: Connection::connect(airc, target_peer),
        }
    }

    /// FOREIGN-friendly constructor — the FFI entry point. Builds the airc
    /// handle INTERNALLY (foreign callers can't make an `Arc<airc_lib::Airc>` —
    /// the outlier-B leak the uniffi pass surfaced): attaches to the running
    /// daemon at `socket` as `agent_name` under `home`, targeting the substrate
    /// `target_peer`. Identity defaults to `SessionIdentity::unknown()` (via
    /// `new` → `Connection::connect`) — the handshake populates it, never
    /// fabricated here. Returns `Arc<Self>` (the uniffi object shape); the
    /// in-process `new` stays for Rust consumers. Both entries coexist.
    pub async fn connect(
        home: String,
        agent_name: String,
        socket: String,
        target_peer: String,
    ) -> Result<Arc<ContinuumClient>, FfiError> {
        // Parse the peer id FIRST — a bad id fails cheaply before we touch the
        // daemon, and never fabricates a target.
        let peer = Uuid::parse_str(&target_peer)
            .map_err(|e| FfiError::Connect(format!("target_peer is not a valid UUID: {e}")))?;
        let airc = airc_lib::Airc::attach_as(std::path::PathBuf::from(home), &agent_name, socket)
            .await
            .map_err(|e| FfiError::Connect(format!("airc attach failed: {e}")))?;
        Ok(Arc::new(ContinuumClient::new(Arc::new(airc), peer)))
    }

    /// WHO this client acts as — citizen (`userId`) + session instance
    /// (`sessionId`). Readonly; surfaces the identity established at connect
    /// (airc pairing / handshake, or the persona's own id). Each platform SDK
    /// presents it idiomatically; the record shape is identical everywhere.
    pub fn session(&self) -> SessionIdentity {
        self.conn.session()
    }

    /// Return a client SCOPED to a conversation/room (`context_id`, the third
    /// ID tier). The scoped client's verbs auto-stamp `contextId` so callers
    /// never re-thread the scope — a persona services a room as a scoped client
    /// exactly the way a UI client does. Shares the same transport + identity.
    pub fn scoped(&self, context_id: Uuid) -> ContinuumClient {
        ContinuumClient {
            conn: self.conn.scoped(context_id),
        }
    }

    /// Execute a command: JSON params in, JSON result out. Stamps `contextId`
    /// when this client is `scoped`.
    pub async fn execute(&self, command: &str, params_json: &str) -> Result<String, FfiError> {
        execute_json(self.conn.commands(), command, params_json).await
    }

    /// Subscribe to an event class; events stream to `callback` as JSON strings
    /// until the returned [`Subscription`] is dropped.
    pub fn subscribe(&self, class: &str, callback: Arc<dyn EventCallback>) -> Subscription {
        let handle = tokio::spawn(pump_events(self.conn.events(), class.to_string(), callback));
        Subscription { handle }
    }

    /// EMIT (publish) an event to `class` — the publish side of the Event
    /// primitive (twin of `subscribe`). `payload_json` is the event body as a
    /// JSON string.
    pub async fn emit(&self, class: &str, payload_json: &str) -> Result<(), FfiError> {
        let payload: serde_json::Value =
            serde_json::from_str(payload_json).map_err(|e| FfiError::Codec(e.to_string()))?;
        self.conn.emit(class, payload).await?;
        Ok(())
    }

    /// PROVIDE (serve) a command: register `handler` to answer inbound requests
    /// the substrate routes to this client — the serve side of the Command
    /// primitive (client-provided commands like `interface/screenshot`). Returns
    /// a [`Registration`]; drop it to stop serving.
    pub async fn provide(
        &self,
        command: &str,
        handler: Arc<dyn CommandHandler>,
    ) -> Result<Registration, FfiError> {
        let adapter = Arc::new(CommandHandlerAdapter {
            command: command.to_string(),
            cb: handler,
        });
        self.conn.provide(command, adapter).await?;
        Ok(Registration {
            conn: self.conn.clone(),
            command: command.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use continuum_client::MockTransport;
    use serde_json::json;
    use std::sync::Mutex;
    use std::time::Duration;

    /// A test sink that records callback invocations.
    #[derive(Default)]
    struct Recorder {
        events: Mutex<Vec<String>>,
        errors: Mutex<Vec<String>>,
        closed: Mutex<bool>,
    }
    impl EventCallback for Recorder {
        fn on_event(&self, event_json: String) {
            self.events.lock().unwrap().push(event_json);
        }
        fn on_error(&self, message: String) {
            self.errors.lock().unwrap().push(message);
        }
        fn on_closed(&self) {
            *self.closed.lock().unwrap() = true;
        }
    }

    #[tokio::test]
    async fn execute_round_trips_json_at_the_boundary() {
        // what this catches: the facade must serialize params_json → Value,
        // dispatch, and serialize the result Value → result_json — the whole
        // point of the generic-free FFI surface.
        let mock = MockTransport::new();
        mock.respond_to("data/get", |params| {
            assert_eq!(params, json!({ "id": "abc" }));
            Ok(json!({ "id": "abc", "value": 42 }))
        });
        let conn = Connection::new(mock);
        let out = execute_json(conn.commands(), "data/get", r#"{"id":"abc"}"#)
            .await
            .expect("executes");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&out).unwrap(),
            json!({ "id": "abc", "value": 42 })
        );
    }

    #[tokio::test]
    async fn malformed_params_json_is_a_codec_error_not_a_dispatch() {
        // what this catches: bad JSON must fail at the boundary as Codec, never
        // reach the substrate as a garbage command.
        let conn = Connection::new(MockTransport::new());
        let err = execute_json(conn.commands(), "x", "{not json")
            .await
            .expect_err("rejects malformed params");
        assert!(matches!(err, FfiError::Codec(_)));
    }

    #[tokio::test]
    async fn substrate_refusal_maps_to_ffi_refused() {
        // what this catches: a substrate "no" must surface as FfiError::Refused
        // (command + reason), distinct from a transport break.
        let mock = MockTransport::new();
        mock.respond_to("danger", |_| {
            Err(ClientError::Refused {
                command: "danger".to_string(),
                reason: "not allowed".to_string(),
            })
        });
        let conn = Connection::new(mock);
        let err = execute_json(conn.commands(), "danger", "{}")
            .await
            .expect_err("refused");
        match err {
            FfiError::Refused { command, reason } => {
                assert_eq!(command, "danger");
                assert_eq!(reason, "not allowed");
            }
            other => panic!("expected Refused, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn subscribe_streams_events_as_json_to_the_callback() {
        // what this catches: emitted substrate events must reach the foreign
        // callback as JSON strings (the event half of the boundary), and a
        // stream close must surface as on_closed.
        let mock = MockTransport::new(); // Clone-shares Inner; keep a handle to emit/close
        let conn = Connection::new(mock.clone());
        let rec = Arc::new(Recorder::default());

        let cb: Arc<dyn EventCallback> = rec.clone();
        let pump = tokio::spawn(pump_events(
            conn.events(),
            "persona.response".to_string(),
            cb,
        ));

        // Wait deterministically until the pump has registered its subscription.
        for _ in 0..100 {
            if mock.subscriber_count("persona.response") == 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(
            mock.subscriber_count("persona.response"),
            1,
            "pump subscribed"
        );

        mock.emit("persona.response", json!({ "text": "hi" }));
        // close() drops the subscriber sender → the buffered event drains, then
        // the stream yields None and the pump calls on_closed.
        mock.close().await.expect("close");
        let _ = tokio::time::timeout(Duration::from_secs(2), pump).await;

        let events = rec.events.lock().unwrap();
        assert_eq!(events.len(), 1, "one event delivered");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&events[0]).unwrap(),
            json!({ "text": "hi" })
        );
        assert!(*rec.closed.lock().unwrap(), "stream close surfaced");
    }

    /// A foreign command handler that echoes a canned result.
    struct CannedHandler(serde_json::Value);
    impl CommandHandler for CannedHandler {
        fn handle(&self, _params_json: String) -> Result<String, FfiError> {
            Ok(serde_json::to_string(&self.0).unwrap())
        }
    }

    /// A handler that fails — exercises the error mapping.
    struct FailingHandler;
    impl CommandHandler for FailingHandler {
        fn handle(&self, _params_json: String) -> Result<String, FfiError> {
            Err(FfiError::Transport("device busy".into()))
        }
    }

    #[tokio::test]
    async fn provided_handler_answers_a_routed_command() {
        // what this catches: the serve side end-to-end — provide registers a
        // foreign CommandHandler (via the adapter), and a routed inbound command
        // dispatches to it and returns its JSON result. This is the client
        // serving interface/screenshot et al, the half #1663 lacked.
        let mock = MockTransport::new();
        let conn = Connection::new(mock.clone());
        let cb: Arc<dyn CommandHandler> = Arc::new(CannedHandler(json!({ "png_base64": "abc" })));
        let adapter = Arc::new(CommandHandlerAdapter {
            command: "interface/screenshot".into(),
            cb,
        });
        conn.provide("interface/screenshot", adapter).await.unwrap();

        let out = mock
            .dispatch_provided("interface/screenshot", json!({ "selector": "body" }))
            .await
            .expect("provided handler answers");
        assert_eq!(out, json!({ "png_base64": "abc" }));
    }

    #[tokio::test]
    async fn provided_handler_error_maps_to_refused() {
        // what this catches: a handler failure must surface as a typed command
        // error (Refused with the reason), never a silent empty result.
        let mock = MockTransport::new();
        let conn = Connection::new(mock.clone());
        let adapter = Arc::new(CommandHandlerAdapter {
            command: "x".into(),
            cb: Arc::new(FailingHandler),
        });
        conn.provide("x", adapter).await.unwrap();

        let err = mock.dispatch_provided("x", json!({})).await.unwrap_err();
        match err {
            ClientError::Refused { reason, .. } => assert!(reason.contains("device busy")),
            other => panic!("expected Refused, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn revoke_stops_serving_the_command() {
        // what this catches: dropping a Registration (→ revoke) deregisters the
        // command — the serve loop then has nothing to dispatch.
        let mock = MockTransport::new();
        let conn = Connection::new(mock.clone());
        let adapter = Arc::new(CommandHandlerAdapter {
            command: "x".into(),
            cb: Arc::new(CannedHandler(json!(1))),
        });
        conn.provide("x", adapter).await.unwrap();
        assert!(mock.provides("x"), "provided");

        conn.revoke("x").await.unwrap();
        assert!(!mock.provides("x"), "revoked");
        let err = mock.dispatch_provided("x", json!({})).await.unwrap_err();
        assert!(matches!(err, ClientError::NotImplemented(_)));
    }

    #[tokio::test]
    async fn emit_publishes_to_a_subscriber() {
        // what this catches: emit (publish) reaches a subscriber of the same
        // class — the publish twin of subscribe, end-to-end via the mock fan-out.
        // Completes the four-verb facade surface.
        let mock = MockTransport::new();
        let conn = Connection::new(mock.clone());
        let rec = Arc::new(Recorder::default());
        let cb: Arc<dyn EventCallback> = rec.clone();
        let pump = tokio::spawn(pump_events(conn.events(), "x".to_string(), cb));

        for _ in 0..100 {
            if mock.subscriber_count("x") == 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        conn.emit("x", json!({ "hello": "world" }))
            .await
            .expect("emit publishes");
        tokio::time::sleep(Duration::from_millis(50)).await;
        mock.close().await.ok();
        let _ = tokio::time::timeout(Duration::from_secs(2), pump).await;

        let events = rec.events.lock().unwrap();
        assert_eq!(events.len(), 1, "subscriber received the emitted event");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&events[0]).unwrap(),
            json!({ "hello": "world" })
        );
    }

    #[tokio::test]
    async fn connect_rejects_a_bad_target_peer_uuid() {
        // what this catches: the foreign constructor validates target_peer FIRST
        // (cheap, before touching the daemon) and surfaces FfiError::Connect —
        // never fabricates a target or proceeds with a garbage peer id.
        // match (not unwrap_err) — ContinuumClient isn't Debug, so unwrap_err
        // (which needs the Ok type to be Debug) wouldn't compile.
        match ContinuumClient::connect(
            "/tmp".into(),
            "tester".into(),
            "/tmp/nonexistent.sock".into(),
            "not-a-uuid".into(),
        )
        .await
        {
            Err(FfiError::Connect(_)) => {}
            Err(other) => panic!("expected FfiError::Connect, got {other:?}"),
            Ok(_) => panic!("expected an error for a bad target_peer uuid"),
        }
    }
}
