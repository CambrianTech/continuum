//! `InProcessTransport` — the local-optimization transport for citizens that
//! live INSIDE the substrate (personas, the foundry, any in-core node).
//!
//! ## Citizens use clients; clients are built around airc
//!
//! Everyone does work through a scoped, sessioned, authenticated
//! `continuum_client::Connection` — cli, desktop, iOS, a grid peer, AND a
//! persona. The persona is not a special endpoint; it is a citizen with a
//! `Connection` like any other ([[persona-is-a-client]]). What differs by
//! locality is only the `Transport` the `Connection` rides:
//!
//! - cli / desktop / iOS / grid peer → `AircIpcTransport` (out-of-process / over
//!   the grid, serialized airc frames).
//! - **a citizen in-core → `InProcessTransport`** — routes straight to this
//!   substrate's own [`CommandExecutor`], no serialization, no loopback.
//!
//! Same `Connection`, same `scoped(context)`, same session/identity, same
//! `AuthPolicy` gate. `InProcessTransport` is the LOCAL case of the airc-client
//! paradigm, not a separate mechanism: the moment the same citizen wants to call
//! across the grid, it is the identical `Connection` with an airc transport —
//! the persona's code does not change a line (the remote case is often the more
//! natural one on a grid: a light node leasing `ai/generate` from the GPU node).
//!
//! ## The gate comes for free
//!
//! `request` dispatches via [`CommandExecutor::execute_with_caller`], threading
//! the citizen's [`CallerIdentity`] into the SAME `AuthPolicy` chokepoint a
//! remote caller hits. A persona is gated exactly like anyone else — no
//! "internal/trusted" bypass, no forged identity. That is the no-holes guarantee
//! the uniform paradigm buys: one path to secure, everyone on it.
//!
//! ## Slice scope
//!
//! `request` (the Command CALL — the load-bearing path a persona's `ToolExecutor`
//! uses) is fully wired here. The Event verbs (`subscribe`/`emit`, the in-process
//! `MessageBus` bridge) and the serve side (`provide`/`revoke`, in-process
//! dynamic command registration) return a typed `NotImplemented` — they are the
//! next sub-slices, surfaced loudly rather than silently stubbed
//! ([[no-fallbacks-ever]]).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use continuum_client::event::EventStream;
use continuum_client::transport::{ServeHandler, Transport};
use continuum_client::ClientError;

use super::command_executor::CommandExecutor;
use super::service_module::CommandResult;
use crate::routing::CallerIdentity;

/// A `continuum_client::Transport` over this substrate's own [`CommandExecutor`]
/// — the in-core citizen's connection to the substrate it lives in. Carries the
/// citizen's [`CallerIdentity`] so every dispatch is gated as that citizen.
pub struct InProcessTransport {
    executor: Arc<CommandExecutor>,
    /// WHO this transport acts as — threaded into the `AuthPolicy` gate on every
    /// `request`. `None` = the substrate's own implicitly-trusted code; a persona
    /// passes `Some(CallerIdentity::airc(its_peer_id))` so it is gated as itself.
    caller: Option<CallerIdentity>,
    closed: AtomicBool,
}

impl InProcessTransport {
    /// Build an in-process transport that dispatches as `caller`. A persona
    /// passes its own `CallerIdentity`; substrate-internal use passes `None`.
    pub fn new(executor: Arc<CommandExecutor>, caller: Option<CallerIdentity>) -> Self {
        Self {
            executor,
            caller,
            closed: AtomicBool::new(false),
        }
    }
}

/// Map a `CommandResult` to the JSON `Value` the client boundary expects. JSON
/// and Handle results cross cleanly; Binary/Stream/Lambda do not fit a single
/// JSON value and surface a typed error rather than a silent lossy coercion.
fn command_result_to_value(command: &str, result: CommandResult) -> Result<Value, ClientError> {
    match result {
        CommandResult::Json(v) => Ok(v),
        CommandResult::Handle(handle) => {
            serde_json::to_value(&handle).map_err(|e| ClientError::Codec(e.to_string()))
        }
        // Mirror `CommandResult::to_json_value`: a Binary result surfaces its
        // metadata JSON (the raw bytes travel out-of-band via a handle, not on
        // this boundary). Returning the metadata — rather than erroring — keeps
        // the in-process client path behavior-equivalent to the direct executor
        // path it replaces, so a persona calling a Binary-returning command sees
        // the same success+metadata it always did.
        CommandResult::Binary { metadata, .. } => Ok(metadata),
        CommandResult::Stream(_) | CommandResult::Lambda(_) => {
            Err(ClientError::Transport(format!(
                "command `{command}` returned a Stream/Lambda result (reserved kinds, \
             not yet on the wire)"
            )))
        }
    }
}

#[async_trait]
impl Transport for InProcessTransport {
    async fn execute(&self, command: &str, params: Value) -> Result<Value, ClientError> {
        if self.closed.load(Ordering::Relaxed) {
            return Err(ClientError::Closed);
        }
        let result = self
            .executor
            .execute_with_caller(command, params, self.caller.clone())
            .await
            .map_err(|reason| ClientError::Refused {
                command: command.to_string(),
                reason,
            })?;
        command_result_to_value(command, result)
    }

    async fn subscribe(&self, _class: &str) -> Result<EventStream, ClientError> {
        Err(ClientError::NotImplemented(
            "InProcessTransport::subscribe — the in-process MessageBus event bridge is the next sub-slice",
        ))
    }

    async fn emit(&self, _class: &str, _payload: Value) -> Result<(), ClientError> {
        Err(ClientError::NotImplemented(
            "InProcessTransport::emit — the in-process MessageBus event bridge is the next sub-slice",
        ))
    }

    async fn provide(
        &self,
        _command: &str,
        _handler: Arc<dyn ServeHandler>,
    ) -> Result<(), ClientError> {
        Err(ClientError::NotImplemented(
            "InProcessTransport::provide — in-process dynamic command registration is a follow-up sub-slice",
        ))
    }

    async fn revoke(&self, _command: &str) -> Result<(), ClientError> {
        // Idempotent: nothing is provided over this transport yet, so revoking
        // is a no-op, not an error (matches the trait's idempotency contract).
        Ok(())
    }

    async fn close(&self) -> Result<(), ClientError> {
        if self.closed.swap(true, Ordering::Relaxed) {
            return Err(ClientError::Closed);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::{AuthPolicy, ClosurePolicy, RouteDecision, Verdict};
    use crate::runtime::{ModuleConfig, ModulePriority, ModuleRegistry, ServiceModule};
    use continuum_client::Connection;
    use serde_json::json;
    use std::sync::Mutex;

    // Echoes the params it received back as the result — so a test can read what
    // the handler actually saw (params + any stamped envelope siblings).
    struct EchoModule;

    impl EchoModule {
        const PREFIXES: &'static [&'static str] = &["echo/"];
    }

    #[async_trait]
    impl ServiceModule for EchoModule {
        fn config(&self) -> ModuleConfig {
            ModuleConfig {
                name: "echo",
                priority: ModulePriority::Normal,
                command_prefixes: Self::PREFIXES,
                event_subscriptions: &[],
                needs_dedicated_thread: false,
                max_concurrency: 0,
                tick_interval: None,
            }
        }
        async fn initialize(&self, _ctx: &crate::runtime::ModuleContext) -> Result<(), String> {
            Ok(())
        }
        async fn handle_command(
            &self,
            _command: &str,
            params: Value,
        ) -> Result<CommandResult, String> {
            Ok(CommandResult::Json(params))
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    // what this catches: the FULL persona-as-client request path end-to-end.
    // A scoped continuum_client::Connection over InProcessTransport must:
    //   1. stamp the room contextId into the envelope (sessions tier) — the
    //      handler sees it (proves scoped() flows through this transport), and
    //   2. thread the persona's identity into the SAME AuthPolicy gate (proves
    //      the no-holes guarantee — a persona is gated like any caller).
    // This composes the sessions work + the new transport + the core executor.
    #[tokio::test]
    async fn scoped_persona_client_dispatches_with_identity_and_context() {
        let captured: Arc<Mutex<Option<CallerIdentity>>> = Arc::new(Mutex::new(None));
        let cap = Arc::clone(&captured);
        let policy = ClosurePolicy::new(
            "capture-caller",
            move |_decision: &RouteDecision, caller: Option<&CallerIdentity>| {
                *cap.lock().unwrap() = caller.cloned();
                Verdict::Allowed
            },
        );

        let registry = Arc::new(ModuleRegistry::new());
        registry.register(Arc::new(EchoModule));
        let executor = Arc::new(
            CommandExecutor::new(registry).with_policy(Arc::new(policy) as Arc<dyn AuthPolicy>),
        );

        let persona = uuid::Uuid::new_v4();
        let room = uuid::Uuid::new_v4();
        let transport = InProcessTransport::new(
            executor,
            Some(CallerIdentity::airc(crate::identity::PeerId::from_uuid(
                persona,
            ))),
        );

        // The persona is a client: scope to the room, then act.
        let conn = Connection::new(transport).scoped(room);
        let echoed: Value = conn
            .commands()
            .execute("echo/run", json!({ "x": 1 }))
            .await
            .expect("dispatch");

        // (1) the handler saw the scoped contextId stamped into the envelope.
        assert_eq!(echoed["x"], 1, "command params survive intact");
        assert_eq!(
            echoed["contextId"],
            json!(room.to_string()),
            "scoped() stamped contextId; it flowed through InProcessTransport → executor → handler"
        );

        // (2) the persona's identity reached the AuthPolicy gate.
        let seen = captured.lock().unwrap().clone().expect("gate saw a caller");
        assert_eq!(
            seen.peer_id.as_uuid(),
            persona,
            "the persona is gated as ITSELF — no internal/trusted bypass, no forged identity"
        );
    }

    #[tokio::test]
    async fn execute_after_close_errors() {
        let registry = Arc::new(ModuleRegistry::new());
        registry.register(Arc::new(EchoModule));
        let executor = Arc::new(CommandExecutor::new(registry));
        let transport = InProcessTransport::new(executor, None);

        transport.close().await.expect("first close ok");
        let err = transport.execute("echo/run", json!({})).await.unwrap_err();
        assert!(matches!(err, ClientError::Closed));
    }

    // what this catches: the event/serve verbs are HONESTLY not-yet-implemented
    // (typed NotImplemented), never silently stubbed — so a caller learns the
    // sub-slice is pending rather than getting a silent no-op.
    #[tokio::test]
    async fn event_and_serve_verbs_surface_typed_not_implemented() {
        let registry = Arc::new(ModuleRegistry::new());
        let executor = Arc::new(CommandExecutor::new(registry));
        let transport = InProcessTransport::new(executor, None);

        // subscribe's Ok type (EventStream) isn't Debug, so match explicitly
        // rather than unwrap_err (which would require Debug on the Ok side).
        match transport.subscribe("x").await {
            Err(ClientError::NotImplemented(_)) => {}
            Err(e) => panic!("expected NotImplemented, got {e}"),
            Ok(_) => panic!("expected NotImplemented, got Ok(stream)"),
        }
        assert!(matches!(
            transport.emit("x", json!({})).await.unwrap_err(),
            ClientError::NotImplemented(_)
        ));
    }

    // what this catches: a Binary command result surfaces its METADATA as a
    // success (bytes travel out-of-band via a handle), NOT a transport error —
    // mirroring `CommandResult::to_json_value`. This keeps the in-process client
    // path behavior-equivalent to the direct executor path it replaces, so a
    // persona calling a Binary-returning command (screenshot, embedding) sees
    // the same success+metadata it always did. Regression-pins that arm.
    #[test]
    fn binary_result_surfaces_metadata_not_error() {
        let result = CommandResult::Binary {
            metadata: json!({ "mime": "image/png", "bytes": 3 }),
            data: vec![1, 2, 3],
        };
        let v = command_result_to_value("interface/screenshot", result)
            .expect("Binary → metadata, not a transport error");
        assert_eq!(v, json!({ "mime": "image/png", "bytes": 3 }));
    }
}
