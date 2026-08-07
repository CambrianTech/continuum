//! `ProbeStreamModule` — bridges [`ProbeRouterLayer`] to the
//! `CommandUri` dispatch surface.
//!
//! The substrate's Slice P observability surface (URI dispatch +
//! `probe!` + `UriCaptureLayer` + `ProbeRouterLayer`) is finally
//! reachable through the same primitive every other command uses:
//! `Commands.execute("debug/probes/...", ...)`. Operators and
//! personas open a stream, poll it, close it — same pattern as
//! `data/query-open` / `next` / `close`.
//!
//! # Commands
//!
//! - **`debug/probes/open { class }`** — Subscribe to a probe class.
//!   Returns a [`HandleRef`] the caller threads through subsequent
//!   `next` / `close` calls. Multiple opens on the same class are
//!   independent — each gets its own broadcast receiver and sees
//!   every event emitted after its open call.
//!
//! - **`debug/probes/next { handle, maxEvents?, timeoutMs? }`** — Drain
//!   up to `maxEvents` (default 32) events from the stream. If
//!   `timeoutMs > 0`, waits up to that long for the FIRST event,
//!   then non-blocking-drains the rest. If `timeoutMs == 0`
//!   (default), pure non-blocking drain — returns whatever is in
//!   the channel right now.
//!
//!   Response: `{ events: [ProbeEvent], lagged: u64 }`. `lagged > 0`
//!   means the consumer fell behind the broadcast channel's
//!   capacity by that many events. This is honest backpressure
//!   from [`tokio::sync::broadcast`]; consumers handle it the same
//!   way they handle any other pressure signal (back off, scale up,
//!   tighten filter).
//!
//! - **`debug/probes/close { handle }`** — Drop the stream's state.
//!   Idempotent: closing an already-closed handle returns
//!   `closed: false` without error.
//!
//! # Why handle-based polling and not a Stream cell shape
//!
//! Per [`crate::runtime::cell_shapes`], the `Stream<T>` cell shape
//! is reserved but returning one is a runtime error until the wire
//! protocol lands (frame format, correlation IDs, backpressure,
//! cancellation). The handle-based shape is the substrate's bridge:
//! it composes against the existing `HandleRef` primitive, hits the
//! same dispatcher, gets the same auth gate, and works locally +
//! cross-grid via the existing transport layers. When the streaming
//! protocol lands later, `next` can fold into a Stream emission
//! without breaking the open/close pair.
//!
//! # Cross-grid by construction
//!
//! When the Slice P transport selector lands, an operator running
//! `./jtag airc://maya/debug/probes/open --class=decision` from
//! another machine gets routed to maya's substrate, which opens the
//! stream locally and returns a HandleRef with `owner: "debug/probes"`.
//! Subsequent `airc://maya/debug/probes/next` calls route back to
//! maya by the same handle-owner mechanism that already routes
//! `data/query-next` to its owner. No additional plumbing needed —
//! the substrate already knows how to route by owner.
//!
//! # Composition at boot
//!
//! ```ignore
//! use tracing_subscriber::prelude::*;
//! use continuum_core::routing::{ProbeRouterLayer, UriCaptureLayer};
//! use continuum_core::modules::probe_stream::ProbeStreamModule;
//!
//! let router = ProbeRouterLayer::new();
//!
//! // Layers see every probe! event in the process
//! tracing_subscriber::registry()
//!     .with(UriCaptureLayer::new())
//!     .with(router.clone())
//!     .init();
//!
//! // Module exposes the router's subscribers via debug/probes/*
//! let module = ProbeStreamModule::new(router);
//! registry.register(Arc::new(module));
//! ```

use async_trait::async_trait;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::any::Any;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, Mutex};
use uuid::Uuid;

use crate::routing::{ProbeEvent, ProbeRouterLayer};
use crate::runtime::{
    CommandRequest, CommandResponse, CommandResult, ModuleConfig, ModuleContext, ModulePriority,
    ServiceModule,
};

/// Canonical owner tag for handles minted by this module. Matches the
/// command prefix the module registers — `data/query-open` follows
/// the same convention with `"data"`.
pub const PROBE_STREAM_OWNER: &str = "debug/probes";

/// Canonical type tag for probe-stream handles. Cross-module callers
/// holding a [`HandleRef`](crate::runtime::HandleRef) check
/// `type_tag == PROBE_STREAM_TYPE_TAG` before threading it through.
pub const PROBE_STREAM_TYPE_TAG: &str = "debug::ProbeStream";

/// Default cap on events drained per `next` call when the caller
/// doesn't pass `maxEvents`. Picked to bound a single poll's
/// memory + serialization cost without artificially throttling
/// fast consumers — they can pass higher values explicitly.
pub const DEFAULT_NEXT_MAX_EVENTS: usize = 32;

/// The substrate-side ServiceModule that exposes
/// [`ProbeRouterLayer`] subscriptions through the dispatch surface.
///
/// Clone is cheap (a single `Arc<DashMap>` clone + cloning the
/// router which is also Arc-backed). The module is typically owned
/// by the runtime registry as `Arc<dyn ServiceModule>`.
pub struct ProbeStreamModule {
    router: ProbeRouterLayer,
    streams: DashMap<Uuid, Arc<Mutex<StreamState>>>,
}

struct StreamState {
    /// Probe class this stream is subscribed to. Kept for debug /
    /// introspection only; not load-bearing for routing.
    #[allow(dead_code)]
    class: String,
    rx: broadcast::Receiver<ProbeEvent>,
}

impl ProbeStreamModule {
    /// Construct the module against an existing [`ProbeRouterLayer`].
    /// The router is typically the one installed in the tracing
    /// subscriber stack at boot — pass `router.clone()` (cheap, Arc
    /// inside).
    pub fn new(router: ProbeRouterLayer) -> Self {
        Self {
            router,
            streams: DashMap::new(),
        }
    }

    /// How many open streams the module is currently tracking.
    /// Diagnostic / test surface; not part of the command API.
    pub fn open_stream_count(&self) -> usize {
        self.streams.len()
    }
}

#[derive(Debug, Deserialize)]
struct OpenParams {
    /// Probe class to subscribe to, e.g. `"latency"`, `"decision"`.
    /// Class registration is lazy on the router side — subscribing
    /// to a class no one has emitted yet is fine; subsequent emits
    /// will route to this stream.
    class: String,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct NextParams {
    /// Optional cap on events returned this call. Defaults to
    /// [`DEFAULT_NEXT_MAX_EVENTS`] when omitted.
    max_events: Option<usize>,
    /// Optional milliseconds to wait for the FIRST event. After
    /// the first event arrives (or the timeout elapses), drains
    /// the rest non-blocking. Defaults to 0 (pure non-blocking).
    timeout_ms: Option<u64>,
}

#[derive(Debug, Deserialize, Default)]
struct CloseParams {}

#[derive(Debug, Serialize, Deserialize)]
struct OpenResponse {
    class: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct NextResponse {
    events: Vec<ProbeEvent>,
    /// Number of events dropped because the consumer fell behind
    /// the channel's capacity since the last `next`. Zero is the
    /// healthy case.
    lagged: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct CloseResponse {
    /// `true` if the stream existed and was dropped; `false` if it
    /// was already closed. Idempotent.
    closed: bool,
}

impl ProbeStreamModule {
    async fn handle_open(&self, params: OpenParams) -> Result<CommandResult, String> {
        let id = Uuid::new_v4();
        let rx = self.router.subscribe(&params.class);
        let state = StreamState {
            class: params.class.clone(),
            rx,
        };
        self.streams.insert(id, Arc::new(Mutex::new(state)));
        CommandResponse::ok(OpenResponse {
            class: params.class,
        })
        .with_handle(PROBE_STREAM_OWNER, id, PROBE_STREAM_TYPE_TAG)
        .into_command_result()
    }

    async fn handle_next(&self, req: CommandRequest<NextParams>) -> Result<CommandResult, String> {
        let handle = req.handle.as_ref().ok_or_else(|| {
            "debug/probes/next: missing handle envelope — \
             pass the handle returned from debug/probes/open"
                .to_string()
        })?;
        let uuid = handle
            .expect_owned_by(PROBE_STREAM_OWNER, PROBE_STREAM_TYPE_TAG)
            .map_err(|e| format!("debug/probes/next: {e}"))?;

        let state = {
            let entry = self
                .streams
                .get(&uuid)
                .ok_or_else(|| format!("debug/probes/next: no open stream for handle {uuid}"))?;
            entry.clone()
        };

        let mut state = state.lock().await;
        let max_events = req.params.max_events.unwrap_or(DEFAULT_NEXT_MAX_EVENTS);
        let timeout = Duration::from_millis(req.params.timeout_ms.unwrap_or(0));

        let mut events: Vec<ProbeEvent> = Vec::new();
        let mut lagged: u64 = 0;

        if !timeout.is_zero() {
            // Wait up to `timeout` for the FIRST event. After
            // that, fall through to the non-blocking drain.
            match tokio::time::timeout(timeout, state.rx.recv()).await {
                Ok(Ok(e)) => events.push(e),
                Ok(Err(broadcast::error::RecvError::Lagged(n))) => {
                    lagged = lagged.saturating_add(n);
                }
                Ok(Err(broadcast::error::RecvError::Closed)) => {
                    // Channel closed — nothing more will arrive.
                    return CommandResponse::ok(NextResponse { events, lagged })
                        .into_command_result();
                }
                Err(_) => {
                    // Timeout elapsed with no event. Return what
                    // we have (probably nothing) instead of
                    // erroring — caller polls again.
                }
            }
        }

        // Non-blocking drain phase: consume whatever's queued, up
        // to max_events.
        while events.len() < max_events {
            match state.rx.try_recv() {
                Ok(e) => events.push(e),
                Err(broadcast::error::TryRecvError::Empty) => break,
                Err(broadcast::error::TryRecvError::Closed) => break,
                Err(broadcast::error::TryRecvError::Lagged(n)) => {
                    lagged = lagged.saturating_add(n);
                }
            }
        }

        CommandResponse::ok(NextResponse { events, lagged }).into_command_result()
    }

    async fn handle_close(
        &self,
        req: CommandRequest<CloseParams>,
    ) -> Result<CommandResult, String> {
        let handle = req
            .handle
            .as_ref()
            .ok_or_else(|| "debug/probes/close: missing handle envelope".to_string())?;
        let uuid = handle
            .expect_owned_by(PROBE_STREAM_OWNER, PROBE_STREAM_TYPE_TAG)
            .map_err(|e| format!("debug/probes/close: {e}"))?;
        let existed = self.streams.remove(&uuid).is_some();
        CommandResponse::ok(CloseResponse { closed: existed }).into_command_result()
    }
}

#[async_trait]
impl ServiceModule for ProbeStreamModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "probe-stream",
            priority: ModulePriority::Normal,
            command_prefixes: &["debug/probes/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
        Ok(())
    }

    async fn handle_command(&self, command: &str, params: Value) -> Result<CommandResult, String> {
        match command {
            "debug/probes/open" => {
                let parsed: OpenParams = serde_json::from_value(params)
                    .map_err(|e| format!("debug/probes/open: invalid params: {e}"))?;
                self.handle_open(parsed).await
            }
            "debug/probes/next" => {
                let req = CommandRequest::<NextParams>::from_value(params)?;
                self.handle_next(req).await
            }
            "debug/probes/close" => {
                let req = CommandRequest::<CloseParams>::from_value(params)?;
                self.handle_close(req).await
            }
            other => Err(format!("Unknown debug/probes command: {other}")),
        }
    }

    /// The typed half of this module's surface. `open`/`next`/`close` above are the
    /// prefix-routed LIVE stream; `debug/probes/query` (#235) is the HISTORICAL read,
    /// and it ships on the DynCommand registry per #62 rather than growing the match
    /// arm. Same module because it is one probe concern; separate file because it is a
    /// separate one (see `probe_query.rs`).
    fn commands(&self) -> Vec<Arc<dyn crate::sdk_codegen::DynCommand>> {
        vec![Arc::new(crate::modules::probe_query::ProbeQuery)]
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::UriCaptureLayer;
    use crate::runtime::HandleRef;
    use tracing_subscriber::prelude::*;

    /// Install the same Layer stack a production substrate boots
    /// with, then run a closure that exercises the module. The
    /// inner runtime is current-thread so the thread-local
    /// subscriber stays attached to the dispatched future.
    fn install<F: FnOnce(Arc<ProbeStreamModule>) -> R, R>(f: F) -> R {
        let router = ProbeRouterLayer::new();
        let module = Arc::new(ProbeStreamModule::new(router.clone()));
        let subscriber = tracing_subscriber::registry()
            .with(UriCaptureLayer::new())
            .with(router);
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("current-thread runtime builds");
        tracing::subscriber::with_default(subscriber, || {
            let _guard = rt.enter();
            f(module)
        })
    }

    /// Extract the `HandleRef` from an `open` response. Open returns
    /// a CommandResponse envelope with the handle attached; this
    /// helper plucks it out for follow-up calls.
    fn handle_from_open(result: CommandResult) -> HandleRef {
        match result {
            CommandResult::Json(v) => {
                let envelope: serde_json::Value = v;
                let handle = envelope
                    .get("handle")
                    .expect("open response carries a handle")
                    .clone();
                serde_json::from_value(handle).expect("handle deserializes as HandleRef")
            }
            other => panic!("expected Json result from open, got {other:?}"),
        }
    }

    /// Extract the inner data from a CommandResponse-wrapped Json
    /// result. CommandResponse uses `#[serde(flatten)]` on `data`,
    /// so the inner T's fields are inlined at the envelope's root
    /// alongside `success`/`handle`/`error`.
    fn data_from_response(result: CommandResult) -> serde_json::Value {
        match result {
            CommandResult::Json(v) => v,
            other => panic!("expected Json result, got {other:?}"),
        }
    }

    #[test]
    fn open_returns_handle_with_correct_owner_and_type_tag() {
        install(|module| {
            let rt = tokio::runtime::Handle::current();
            let result = rt
                .block_on(module.handle_command(
                    "debug/probes/open",
                    serde_json::json!({ "class": "latency" }),
                ))
                .expect("open succeeds");
            let handle = handle_from_open(result);
            assert_eq!(handle.owner, PROBE_STREAM_OWNER);
            assert_eq!(handle.type_tag, PROBE_STREAM_TYPE_TAG);
            assert_eq!(module.open_stream_count(), 1);
        });
    }

    #[test]
    fn open_next_close_lifecycle() {
        install(|module| {
            let rt = tokio::runtime::Handle::current();

            // 1. Open a stream on "decision"
            let open_result = rt
                .block_on(module.handle_command(
                    "debug/probes/open",
                    serde_json::json!({ "class": "decision" }),
                ))
                .expect("open");
            let handle = handle_from_open(open_result);
            assert_eq!(module.open_stream_count(), 1);

            // 2. Fire a probe — the router fans it to the open stream
            crate::probe!(class = "decision", action = "promote");

            // 3. Poll for the event with a non-blocking drain
            let next_params = serde_json::json!({ "handle": handle });
            let next_result = rt
                .block_on(module.handle_command("debug/probes/next", next_params))
                .expect("next");
            let data = data_from_response(next_result);
            let events = data["events"].as_array().expect("events array");
            assert_eq!(events.len(), 1, "expected exactly one event");
            assert_eq!(events[0]["class"], "decision");
            assert_eq!(data["lagged"], 0);

            // 4. Close
            let close_result = rt
                .block_on(module.handle_command(
                    "debug/probes/close",
                    serde_json::json!({ "handle": handle }),
                ))
                .expect("close");
            let data = data_from_response(close_result);
            assert_eq!(data["closed"], true);
            assert_eq!(module.open_stream_count(), 0);
        });
    }

    #[test]
    fn next_returns_empty_when_no_events_emitted() {
        install(|module| {
            let rt = tokio::runtime::Handle::current();
            let open = rt
                .block_on(
                    module.handle_command(
                        "debug/probes/open",
                        serde_json::json!({ "class": "quiet" }),
                    ),
                )
                .expect("open");
            let handle = handle_from_open(open);

            // Pure non-blocking drain — nobody emitted anything
            let next = rt
                .block_on(module.handle_command(
                    "debug/probes/next",
                    serde_json::json!({ "handle": handle, "timeoutMs": 0 }),
                ))
                .expect("next");
            let data = data_from_response(next);
            assert_eq!(data["events"].as_array().unwrap().len(), 0);
            assert_eq!(data["lagged"], 0);
        });
    }

    #[test]
    fn next_with_timeout_returns_event_when_emitted_during_wait() {
        install(|module| {
            let rt = tokio::runtime::Handle::current();
            let open = rt
                .block_on(module.handle_command(
                    "debug/probes/open",
                    serde_json::json!({ "class": "latency" }),
                ))
                .expect("open");
            let handle = handle_from_open(open);

            // Spawn an emitter that fires after a short delay,
            // then poll with a longer timeout. The poll must see
            // the event.
            let next_result = rt
                .block_on(async {
                    // Emit synchronously BEFORE the poll begins (the
                    // event lands in the broadcast buffer immediately).
                    crate::probe!(class = "latency", duration_ms = 99i64);
                    module
                        .handle_command(
                            "debug/probes/next",
                            serde_json::json!({ "handle": handle, "timeoutMs": 100 }),
                        )
                        .await
                })
                .expect("next");

            let data = data_from_response(next_result);
            let events = data["events"].as_array().expect("events");
            assert_eq!(events.len(), 1);
            assert_eq!(events[0]["class"], "latency");
        });
    }

    #[test]
    fn next_without_handle_errors_loudly() {
        install(|module| {
            let rt = tokio::runtime::Handle::current();
            let err = rt
                .block_on(module.handle_command("debug/probes/next", serde_json::json!({})))
                .expect_err("next without handle must error");
            assert!(
                err.contains("missing handle"),
                "error must name the missing handle, got: {err}"
            );
        });
    }

    #[test]
    fn next_with_wrong_owner_handle_errors() {
        install(|module| {
            let rt = tokio::runtime::Handle::current();
            // Mint a handle whose owner is something else entirely.
            let wrong = HandleRef::with_id("data", Uuid::new_v4(), "data::QueryCursor");
            let err = rt
                .block_on(
                    module.handle_command(
                        "debug/probes/next",
                        serde_json::json!({ "handle": wrong }),
                    ),
                )
                .expect_err("wrong-owner handle must error");
            assert!(
                err.contains("owner mismatch"),
                "error must name the owner mismatch, got: {err}"
            );
        });
    }

    #[test]
    fn next_with_unknown_stream_handle_errors() {
        install(|module| {
            let rt = tokio::runtime::Handle::current();
            // Mint a handle with the RIGHT owner/type but a UUID
            // that was never registered.
            let stale =
                HandleRef::with_id(PROBE_STREAM_OWNER, Uuid::new_v4(), PROBE_STREAM_TYPE_TAG);
            let err = rt
                .block_on(
                    module.handle_command(
                        "debug/probes/next",
                        serde_json::json!({ "handle": stale }),
                    ),
                )
                .expect_err("unknown stream handle must error");
            assert!(
                err.contains("no open stream"),
                "error must name the missing stream, got: {err}"
            );
        });
    }

    #[test]
    fn close_is_idempotent() {
        install(|module| {
            let rt = tokio::runtime::Handle::current();
            let open = rt
                .block_on(module.handle_command(
                    "debug/probes/open",
                    serde_json::json!({ "class": "anything" }),
                ))
                .expect("open");
            let handle = handle_from_open(open);

            // First close — succeeds, drops state
            let c1 = rt
                .block_on(module.handle_command(
                    "debug/probes/close",
                    serde_json::json!({ "handle": handle }),
                ))
                .expect("close 1");
            assert_eq!(data_from_response(c1)["closed"], true);

            // Second close — succeeds, reports closed: false
            let c2 = rt
                .block_on(module.handle_command(
                    "debug/probes/close",
                    serde_json::json!({ "handle": handle }),
                ))
                .expect("close 2");
            assert_eq!(data_from_response(c2)["closed"], false);
        });
    }

    #[test]
    fn multiple_streams_on_same_class_are_independent() {
        install(|module| {
            let rt = tokio::runtime::Handle::current();
            let h1 = handle_from_open(
                rt.block_on(module.handle_command(
                    "debug/probes/open",
                    serde_json::json!({ "class": "shared" }),
                ))
                .expect("open 1"),
            );
            let h2 = handle_from_open(
                rt.block_on(module.handle_command(
                    "debug/probes/open",
                    serde_json::json!({ "class": "shared" }),
                ))
                .expect("open 2"),
            );
            assert_eq!(module.open_stream_count(), 2);

            crate::probe!(class = "shared", n = 7i64);

            // Both streams see the event
            let r1 = rt
                .block_on(
                    module.handle_command("debug/probes/next", serde_json::json!({ "handle": h1 })),
                )
                .expect("next 1");
            let r2 = rt
                .block_on(
                    module.handle_command("debug/probes/next", serde_json::json!({ "handle": h2 })),
                )
                .expect("next 2");
            let e1 = data_from_response(r1);
            let e2 = data_from_response(r2);
            assert_eq!(e1["events"].as_array().unwrap().len(), 1);
            assert_eq!(e2["events"].as_array().unwrap().len(), 1);
        });
    }

    #[test]
    fn probe_event_carries_uri_chain_through_dispatch_surface() {
        // The keystone integration assertion: an event emitted
        // inside a `cmd` span reaches the consumer with the URI
        // chain attached. Personas debugging each other across the
        // grid depend on this.
        install(|module| {
            let rt = tokio::runtime::Handle::current();
            let open =
                handle_from_open(
                    rt.block_on(module.handle_command(
                        "debug/probes/open",
                        serde_json::json!({ "class": "audit" }),
                    ))
                    .expect("open"),
                );

            {
                let span = tracing::info_span!("cmd", uri = "airc:///inference/llm/generate");
                let _enter = span.enter();
                crate::probe!(class = "audit", verdict = "approved");
            }

            let result = rt
                .block_on(
                    module
                        .handle_command("debug/probes/next", serde_json::json!({ "handle": open })),
                )
                .expect("next");
            let data = data_from_response(result);
            let events = data["events"].as_array().expect("events");
            assert_eq!(events.len(), 1);
            let chain = events[0]["uri_chain"].as_array().expect("uri_chain");
            assert_eq!(chain.len(), 1, "expected one URI frame");
            assert_eq!(chain[0], "airc:///inference/llm/generate");
        });
    }
}
