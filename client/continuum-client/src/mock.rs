//! `MockTransport` — programmable in-memory `Transport` impl for
//! testing downstream consumers without spinning a real airc daemon.
//!
//! Gated behind the `test-fixtures` feature so production binaries
//! cannot link it. The cargo feature is the contract: any consumer
//! that wants `MockTransport` in its tests opts in explicitly.
//!
//! ## Use cases
//!
//! - **`ctm` CLI tests** — assert a subcommand dispatches the right
//!   substrate command without booting `continuum-core-server`.
//! - **Per-language SDK tests** — Swift / Kotlin / TS FFI wrappers
//!   check round-trip semantics on a deterministic backend.
//! - **Positron renderer tests** — push synthetic `ViewState` updates
//!   into the renderer's pipeline; the renderer can't tell the
//!   difference between mock + real substrate.
//! - **`continuum-client` integration tests** — roundtrip
//!   `subscribe → emit → drop → unsubscribe` without LAN loopback.
//!
//! ## Shape (matches the audit's elegance bar)
//!
//! Programmable via two narrow surfaces:
//!
//! - `respond_to(command, handler)` — register a closure that maps
//!   request params to a result for that command name. Multiple
//!   registrations per command allowed (FIFO consumed); after the
//!   queue empties the mock falls back to
//!   `ClientError::NotImplemented`.
//! - `emit(class, payload)` — fans `payload` out to every active
//!   subscriber of `class`. Glob-style class matching is NOT
//!   implemented here (substrate side does that); the mock matches on
//!   exact class string. Consumers wanting wildcard tests should
//!   subscribe to the literal classes they want to receive.
//!
//! ## What's NOT in here
//!
//! - No airc, no protocol-crate dependencies. The mock implements
//!   `Transport` directly; no envelope shaping, no peer IDs, no
//!   subscription_id round-trips. Tests that need to exercise the
//!   protocol crate's pure functions use those functions directly.
//! - No persistence. Each `MockTransport` is fresh state.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::error::ClientError;
use crate::event::EventStream;
use crate::transport::{ServeHandler, Transport};

/// Default per-subscription buffer for emitted events. Matches the
/// real `AircIpcTransport` default so a test that fills the buffer
/// observes the same back-pressure shape it would in production.
const DEFAULT_EVENT_BUFFER: usize = 64;

/// A signature a registered command handler matches. The closure
/// receives the params `Value` and returns a result for the round-trip.
///
/// Boxed `dyn Fn` rather than a generic so a single `MockTransport`
/// can hold heterogeneous handlers for different commands.
type CommandHandler = Box<dyn Fn(Value) -> Result<Value, ClientError> + Send + Sync>;

/// One emit-side subscriber: a sender pushing into the consumer's
/// `EventStream`, plus a stable id for diagnostics if a test wants to
/// drop a specific subscriber.
#[derive(Debug)]
struct Subscriber {
    // Stable id for diagnostics (rides the `Debug` impl) + future
    // drop-a-specific-subscriber tests. Not read on the hot path, so dead-code
    // analysis (which ignores Debug-only use) flags it under `-D warnings`;
    // the field is an intentional fixture slot, not dead.
    #[allow(dead_code)]
    id: u64,
    tx: mpsc::Sender<Result<Value, ClientError>>,
}

struct Inner {
    /// FIFO queue per command name. Each subscribe pulls the front
    /// handler; an empty queue means we never registered a response
    /// for this command (and the request returns NotImplemented so
    /// the test failure is obvious).
    handlers: Mutex<HashMap<String, Vec<CommandHandler>>>,
    /// Active subscribers per class (exact-match string keying).
    subscribers: Mutex<HashMap<String, Vec<Subscriber>>>,
    /// Handlers the client PROVIDES (serve side), keyed by command. A test
    /// simulates an inbound routed command via [`MockTransport::dispatch_provided`].
    serve_handlers: Mutex<HashMap<String, Arc<dyn ServeHandler>>>,
    /// Monotonic id source for `Subscriber.id`. Per-instance.
    next_subscriber_id: AtomicU64,
    /// `Transport::close` semantics: idempotent first-call-wins.
    closed: AtomicBool,
}

/// Programmable in-memory `Transport`. See module-level docs.
///
/// Clone-cheap (one `Arc` clone); two clones of the same mock share
/// state — useful for handing the mock to both a CLI under test AND
/// the test's `emit`/`respond_to` callsite.
#[derive(Clone)]
pub struct MockTransport {
    inner: Arc<Inner>,
}

impl MockTransport {
    /// Build an empty mock — no commands programmed, no subscribers.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Inner {
                handlers: Mutex::new(HashMap::new()),
                subscribers: Mutex::new(HashMap::new()),
                serve_handlers: Mutex::new(HashMap::new()),
                next_subscriber_id: AtomicU64::new(0),
                closed: AtomicBool::new(false),
            }),
        }
    }

    /// Register a response for `command`. Each registration consumes
    /// one round-trip in FIFO order. A test wanting to assert N calls
    /// registers N responses (or one closure that increments).
    ///
    /// Returns `&self` so calls chain: `mock.respond_to("a", ...).respond_to("b", ...)`.
    pub fn respond_to<F>(&self, command: impl Into<String>, handler: F) -> &Self
    where
        F: Fn(Value) -> Result<Value, ClientError> + Send + Sync + 'static,
    {
        let mut handlers = self.inner.handlers.lock().expect("handlers lock poisoned");
        handlers
            .entry(command.into())
            .or_default()
            .push(Box::new(handler));
        self
    }

    /// Convenience: register a static response value (no closure needed
    /// when the test doesn't care about params).
    pub fn respond_with(&self, command: impl Into<String>, response: Value) -> &Self {
        let cloned = response.clone();
        self.respond_to(command, move |_params| Ok(cloned.clone()))
    }

    /// Emit a payload to every active subscriber of `class`. Returns
    /// the number of subscribers the payload reached; useful for
    /// tests asserting "exactly N consumers received this event."
    ///
    /// A subscriber whose buffer is full receives nothing — `emit`
    /// uses `try_send` so a slow consumer doesn't deadlock the test.
    /// The dropped count is reflected in the return value (count
    /// returned is *successful* sends only).
    pub fn emit(&self, class: impl AsRef<str>, payload: Value) -> usize {
        let mut subscribers = self
            .inner
            .subscribers
            .lock()
            .expect("subscribers lock poisoned");
        let class = class.as_ref();
        let Some(list) = subscribers.get_mut(class) else {
            return 0;
        };
        let mut delivered = 0;
        // Drop subscribers whose receivers closed — they're already
        // gone. Mirrors the substrate's per-tick cleanup.
        list.retain(|s| !s.tx.is_closed());
        for s in list.iter() {
            if s.tx.try_send(Ok(payload.clone())).is_ok() {
                delivered += 1;
            }
        }
        delivered
    }

    /// Diagnostic: how many active subscribers does `class` have?
    /// Stale (closed) subscribers are pruned at observation time.
    pub fn subscriber_count(&self, class: impl AsRef<str>) -> usize {
        let mut subscribers = self
            .inner
            .subscribers
            .lock()
            .expect("subscribers lock poisoned");
        let class = class.as_ref();
        let Some(list) = subscribers.get_mut(class) else {
            return 0;
        };
        list.retain(|s| !s.tx.is_closed());
        list.len()
    }

    /// Simulate an inbound routed command hitting a handler this client
    /// PROVIDED — the serve-side analog of `emit` for the subscribe side. Looks
    /// up the handler registered via `Transport::provide` and runs it, so a
    /// test can assert "the SDK's provided handler answers" without a live
    /// substrate routing the request. `NotImplemented` if nothing is provided
    /// for `command` (mirrors the request-side's unregistered behaviour).
    pub async fn dispatch_provided(
        &self,
        command: &str,
        params: Value,
    ) -> Result<Value, ClientError> {
        let handler = {
            let handlers = self
                .inner
                .serve_handlers
                .lock()
                .expect("serve_handlers lock poisoned");
            handlers.get(command).map(Arc::clone)
        };
        match handler {
            Some(h) => h.handle(params).await,
            None => Err(ClientError::NotImplemented(
                "MockTransport: no handler provided for command — register via `provide` first",
            )),
        }
    }

    /// Diagnostic: is a handler currently provided for `command`?
    pub fn provides(&self, command: &str) -> bool {
        self.inner
            .serve_handlers
            .lock()
            .expect("serve_handlers lock poisoned")
            .contains_key(command)
    }
}

impl Default for MockTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for MockTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Avoid printing closure addresses; show counts so test logs
        // are useful.
        let handler_count = self
            .inner
            .handlers
            .lock()
            .map(|h| h.values().map(|v| v.len()).sum::<usize>())
            .unwrap_or(0);
        let subscriber_count = self
            .inner
            .subscribers
            .lock()
            .map(|s| s.values().map(|v| v.len()).sum::<usize>())
            .unwrap_or(0);
        f.debug_struct("MockTransport")
            .field("queued_responses", &handler_count)
            .field("active_subscribers", &subscriber_count)
            .field("closed", &self.inner.closed.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl Transport for MockTransport {
    async fn request(&self, command: &str, params: Value) -> Result<Value, ClientError> {
        if self.inner.closed.load(Ordering::Relaxed) {
            return Err(ClientError::Closed);
        }
        let handler = {
            let mut handlers = self.inner.handlers.lock().expect("handlers lock poisoned");
            handlers.get_mut(command).and_then(|q| {
                if q.is_empty() {
                    None
                } else {
                    Some(q.remove(0))
                }
            })
        };
        match handler {
            Some(h) => h(params),
            None => Err(ClientError::NotImplemented(
                "MockTransport: no response registered for command — register via `respond_to` or `respond_with` before exercising the consumer",
            )),
        }
    }

    async fn subscribe(&self, class: &str) -> Result<EventStream, ClientError> {
        if self.inner.closed.load(Ordering::Relaxed) {
            return Err(ClientError::Closed);
        }
        let (tx, rx) = mpsc::channel::<Result<Value, ClientError>>(DEFAULT_EVENT_BUFFER);
        let id = self
            .inner
            .next_subscriber_id
            .fetch_add(1, Ordering::Relaxed);
        {
            let mut subscribers = self
                .inner
                .subscribers
                .lock()
                .expect("subscribers lock poisoned");
            subscribers
                .entry(class.to_string())
                .or_default()
                .push(Subscriber { id, tx });
        }
        Ok(Box::pin(ReceiverStream::new(rx)))
    }

    async fn emit(&self, class: &str, payload: Value) -> Result<(), ClientError> {
        if self.inner.closed.load(Ordering::Relaxed) {
            return Err(ClientError::Closed);
        }
        // Reuse the inherent `emit` (fan-out to subscribers, returns count) via
        // the explicit type path — unambiguously the inherent method, not this
        // trait method (no recursion). The Transport contract returns Result;
        // the inherent's count is the richer test surface.
        let _delivered = MockTransport::emit(self, class, payload);
        Ok(())
    }

    async fn provide(
        &self,
        command: &str,
        handler: Arc<dyn ServeHandler>,
    ) -> Result<(), ClientError> {
        if self.inner.closed.load(Ordering::Relaxed) {
            return Err(ClientError::Closed);
        }
        self.inner
            .serve_handlers
            .lock()
            .expect("serve_handlers lock poisoned")
            .insert(command.to_string(), handler);
        Ok(())
    }

    async fn revoke(&self, command: &str) -> Result<(), ClientError> {
        // Idempotent: revoking an unprovided command is a no-op, not an error.
        self.inner
            .serve_handlers
            .lock()
            .expect("serve_handlers lock poisoned")
            .remove(command);
        Ok(())
    }

    async fn close(&self) -> Result<(), ClientError> {
        if self.inner.closed.swap(true, Ordering::Relaxed) {
            return Err(ClientError::Closed);
        }
        // Drop all senders → any active subscriber stream sees None.
        let mut subscribers = self
            .inner
            .subscribers
            .lock()
            .expect("subscribers lock poisoned");
        subscribers.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;
    use serde_json::json;

    #[tokio::test]
    async fn request_returns_registered_response() {
        let mock = MockTransport::new();
        mock.respond_with("ai/generate", json!({"text": "mocked"}));
        let got = mock
            .request("ai/generate", json!({"prompt": "hi"}))
            .await
            .unwrap();
        assert_eq!(got, json!({"text": "mocked"}));
    }

    #[tokio::test]
    async fn request_handler_sees_params() {
        let mock = MockTransport::new();
        mock.respond_to("echo", Ok);
        let got = mock.request("echo", json!({"x": 1})).await.unwrap();
        assert_eq!(got, json!({"x": 1}));
    }

    #[tokio::test]
    async fn request_fifo_per_command() {
        let mock = MockTransport::new();
        mock.respond_with("c", json!(1));
        mock.respond_with("c", json!(2));
        assert_eq!(mock.request("c", json!({})).await.unwrap(), json!(1));
        assert_eq!(mock.request("c", json!({})).await.unwrap(), json!(2));
    }

    #[tokio::test]
    async fn request_returns_not_implemented_when_unregistered() {
        let mock = MockTransport::new();
        let err = mock.request("nope", json!({})).await.unwrap_err();
        assert!(matches!(err, ClientError::NotImplemented(_)));
    }

    #[tokio::test]
    async fn request_returns_closed_after_close() {
        let mock = MockTransport::new();
        mock.respond_with("c", json!(1));
        mock.close().await.unwrap();
        assert!(matches!(
            mock.request("c", json!({})).await.unwrap_err(),
            ClientError::Closed
        ));
    }

    #[tokio::test]
    async fn close_is_idempotent_then_errs() {
        let mock = MockTransport::new();
        mock.close().await.unwrap();
        assert!(matches!(
            mock.close().await.unwrap_err(),
            ClientError::Closed
        ));
    }

    #[tokio::test]
    async fn subscribe_receives_emitted_events() {
        let mock = MockTransport::new();
        let mut stream = mock.subscribe("persona.response.complete").await.unwrap();
        let delivered = mock.emit("persona.response.complete", json!({"verdict": "respond"}));
        assert_eq!(delivered, 1);
        let event = stream.next().await.unwrap().unwrap();
        assert_eq!(event, json!({"verdict": "respond"}));
    }

    #[tokio::test]
    async fn emit_fans_out_to_all_subscribers_of_same_class() {
        let mock = MockTransport::new();
        let mut s1 = mock.subscribe("c").await.unwrap();
        let mut s2 = mock.subscribe("c").await.unwrap();
        let delivered = mock.emit("c", json!({"v": 1}));
        assert_eq!(delivered, 2);
        assert_eq!(s1.next().await.unwrap().unwrap(), json!({"v": 1}));
        assert_eq!(s2.next().await.unwrap().unwrap(), json!({"v": 1}));
    }

    #[tokio::test]
    async fn emit_to_unrelated_class_delivers_nothing() {
        let mock = MockTransport::new();
        let mut s = mock.subscribe("class.a").await.unwrap();
        let delivered = mock.emit("class.b", json!({}));
        assert_eq!(delivered, 0);
        // The subscriber sees no event; confirm via try_next-style
        // timeout instead of blocking.
        tokio::time::timeout(std::time::Duration::from_millis(10), s.next())
            .await
            .expect_err("no events should arrive");
    }

    #[tokio::test]
    async fn dropped_subscriber_pruned_on_next_emit() {
        let mock = MockTransport::new();
        let s = mock.subscribe("c").await.unwrap();
        assert_eq!(mock.subscriber_count("c"), 1);
        drop(s);
        // emit prunes closed senders before counting
        let _ = mock.emit("c", json!({}));
        assert_eq!(mock.subscriber_count("c"), 0);
    }

    #[tokio::test]
    async fn close_drops_all_subscribers() {
        let mock = MockTransport::new();
        let mut s = mock.subscribe("c").await.unwrap();
        mock.close().await.unwrap();
        // Stream's underlying tx was dropped via subscribers.clear()
        // → stream yields None to signal end-of-stream.
        assert!(s.next().await.is_none());
    }

    #[tokio::test]
    async fn subscriber_count_is_observable() {
        let mock = MockTransport::new();
        assert_eq!(mock.subscriber_count("c"), 0);
        let _s = mock.subscribe("c").await.unwrap();
        assert_eq!(mock.subscriber_count("c"), 1);
    }
}
