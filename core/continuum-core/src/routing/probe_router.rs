//! `ProbeRouterLayer` — the substrate's per-class event fanout.
//!
//! Per `docs/architecture/GRID-ADDRESSING-AND-ROUTING.md` §"Per-class
//! routing": every [`probe!`](crate::probe) call emits a
//! `tracing::Event` carrying a `probe_class` field. This Layer
//! intercepts those events, captures the substrate's URI ancestry via
//! [`current_uri_chain`](crate::routing::current_uri_chain), and
//! forwards a structured [`ProbeEvent`] to whoever is subscribed to
//! that class — sentinels, Ares' dispatcher, the operator running
//! `./jtag airc://maya/debug/probes/decision/stream`.
//!
//! ## Composition with `UriCaptureLayer`
//!
//! Install BOTH Layers at substrate boot:
//!
//! ```ignore
//! use tracing_subscriber::prelude::*;
//! use continuum_core::routing::{ProbeRouterLayer, UriCaptureLayer};
//!
//! let router = ProbeRouterLayer::new();
//! let mut decisions = router.subscribe("decision");
//!
//! tracing_subscriber::registry()
//!     .with(UriCaptureLayer::new())
//!     .with(router)
//!     .init();
//!
//! // ... persona code emits `probe!(class = "decision", ...)` events;
//! // `decisions.recv().await` yields each one in turn.
//! ```
//!
//! The router doesn't depend on `UriCaptureLayer` to compile — it
//! calls `current_uri_chain()` which returns `Vec::new()` if no
//! UriCapture Layer is installed. But in production both are present
//! so each `ProbeEvent` carries the URI ancestry of the dispatch that
//! produced it.
//!
//! ## Why a broadcast channel
//!
//! Multiple consumers per class are the common case: a sentinel
//! watching `latency` for SLO breach AND the operator tailing the
//! same stream during an incident AND a foundry fitness loop scoring
//! every event. `tokio::sync::broadcast` lets all of them see every
//! event without coordination.
//!
//! Lagged consumers (channel full because they fell behind) get
//! `RecvError::Lagged` from `recv()` — that's `tokio::sync::broadcast`
//! signaling honest pressure, not the router losing events silently.
//! Substrate consumers handle Lagged the same way they handle any
//! other backpressure signal (drop oldest, alert, scale capacity).
//!
//! ## Why register classes lazily
//!
//! Probes fire from anywhere — the router can't know up-front which
//! classes the running personas will emit. Subscribing to a class
//! creates the broadcast channel if it doesn't exist yet; firing a
//! probe for a class no one's subscribed to is a cheap HashMap miss,
//! no allocation, no send. That's the substrate's contribution to
//! the [[no-fallbacks-ever]] principle applied to telemetry:
//! unsubscribed classes don't get fabricated consumers, they just
//! don't fire.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tracing::{
    field::{Field, Visit},
    span::Attributes,
    Event, Id, Subscriber,
};
use tracing_subscriber::{layer::Context, registry::LookupSpan, Layer};

use super::current_uri_chain;
use super::probe_span_meta::{
    build_timing_event_from_meta, ensure_probe_meta, span_carries_probe_class,
};

/// A structured probe event the router fans out to subscribers.
///
/// The `class` is the routing key (e.g. `"latency"`, `"decision"`).
/// `uri_chain` is the substrate's URI ancestry at the moment the
/// probe fired (empty if no [`UriCaptureLayer`](super::UriCaptureLayer)
/// is installed). `fields` captures every structured field except
/// `probe_class` and `message` (which get their own dedicated fields).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeEvent {
    /// Routing key, e.g. `"latency"`, `"decision"`, `"state"`,
    /// `"admission"`.
    pub class: String,
    /// The substrate's URI ancestry at fire time, outermost-first.
    pub uri_chain: Vec<String>,
    /// Optional human-readable message (the format-string segment of
    /// the `probe!` invocation, when present).
    pub message: Option<String>,
    /// Every other structured field recorded on the probe event.
    /// Values are stringified at the visitor boundary:
    ///
    /// - `field = "literal_str"` → unquoted (`"value"`)
    /// - `field = some_u64` / `i64` / `bool` → formatted natively
    ///   (`"42"`, `"true"`)
    /// - `field = %something` (Display) / `field = ?something` (Debug)
    ///   → `format!("{:?}", value)` output (Debug-wrapped strings
    ///   surround with quotes)
    ///
    /// Consumers that need typed values parse the strings, or fan out
    /// to per-class typed channels in their own code.
    pub fields: HashMap<String, String>,
}

/// Default broadcast channel capacity per class. The substrate
/// trades memory for headroom — 256 events buffered before lagging
/// consumers see `RecvError::Lagged`. Operators that need tighter
/// or looser bounds can build a router with [`Self::with_capacity`].
pub const DEFAULT_CHANNEL_CAPACITY: usize = 256;

/// The substrate-side `probe!` fanout Layer.
///
/// Clones cheaply (one `Arc<RwLock<...>>` clone). Subscribers and
/// the install site can hold their own clones without coordination.
#[derive(Debug, Clone)]
pub struct ProbeRouterLayer {
    routes: Arc<RwLock<HashMap<String, broadcast::Sender<ProbeEvent>>>>,
    capacity: usize,
}

impl Default for ProbeRouterLayer {
    fn default() -> Self {
        Self::new()
    }
}

impl ProbeRouterLayer {
    /// Build a router with the default per-class channel capacity
    /// ([`DEFAULT_CHANNEL_CAPACITY`]).
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_CHANNEL_CAPACITY)
    }

    /// Build a router with an operator-chosen per-class channel
    /// capacity. Larger = more memory but lagged consumers fall
    /// behind longer before signaling.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            routes: Arc::new(RwLock::new(HashMap::new())),
            capacity,
        }
    }

    /// Subscribe to every probe event in a class. Creates the
    /// broadcast channel for the class if it doesn't exist yet.
    ///
    /// Multiple subscribers per class are supported — each receives
    /// every event the router fans out. Lagged subscribers get
    /// `RecvError::Lagged` from `recv()`.
    pub fn subscribe(&self, class: &str) -> broadcast::Receiver<ProbeEvent> {
        // Fast path: class already has a sender.
        if let Some(sender) = self.routes.read().unwrap().get(class) {
            return sender.subscribe();
        }
        // Slow path: create the channel.
        let mut routes = self.routes.write().unwrap();
        let sender = routes
            .entry(class.to_string())
            .or_insert_with(|| broadcast::channel(self.capacity).0);
        sender.subscribe()
    }

    /// How many classes currently have at least one subscriber (or
    /// have had one at some point — channels persist for the
    /// router's lifetime).
    pub fn known_classes(&self) -> Vec<String> {
        self.routes.read().unwrap().keys().cloned().collect()
    }

    /// Send a [`ProbeEvent`] to the class's broadcast channel if any
    /// subscriber has registered. Returns the number of currently
    /// active receivers (0 if no one is subscribed or the channel
    /// doesn't exist yet — both are "nobody's watching"). Sending to
    /// a channel with no receivers is a HashMap miss + early return,
    /// no allocation.
    fn fan_out(&self, event: ProbeEvent) -> usize {
        let routes = self.routes.read().unwrap();
        let Some(sender) = routes.get(&event.class) else {
            return 0;
        };
        // `send` on a broadcast channel returns Err only when there
        // are no active receivers. That's the same "nobody's watching"
        // semantic we treat as a no-op.
        sender.send(event).unwrap_or(0)
    }
}

impl<S> Layer<S> for ProbeRouterLayer
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        // ZERO-COST GATE (2026-08-23 serialization audit): callsite field sets
        // are static metadata — asking whether `probe_class` exists allocates
        // nothing. Without this, EVERY tracing event in the process and its
        // dependency crates paid a full visitor walk (a String per field into a
        // HashMap) in this layer, then discarded it. The span path got exactly
        // this fix in PR #1541 R2; the event path never did.
        if event.metadata().fields().field("probe_class").is_none() {
            return;
        }
        let mut visitor = ProbeEventVisitor::default();
        event.record(&mut visitor);

        let class = match visitor.probe_class {
            Some(c) => c,
            None => return, // not a probe event, ignore
        };

        let probe_event = ProbeEvent {
            class,
            uri_chain: current_uri_chain(),
            message: visitor.message,
            fields: visitor.fields,
        };

        self.fan_out(probe_event);
    }

    /// Spans created via `time_sync!` / `time_probe!` carry a
    /// `probe_class` attribute. We stash a
    /// [`SpanProbeMeta`](super::probe_span_meta::SpanProbeMeta) in
    /// the span's extensions so `on_close` can build the timing
    /// [`ProbeEvent`].
    ///
    /// Hot-path discipline: the FIRST thing we do is the cheap
    /// static check
    /// [`span_carries_probe_class`](super::probe_span_meta::span_carries_probe_class).
    /// For the vast majority of spans the substrate emits (tokio
    /// executor, framework, plain `info_span!`) this short-circuits
    /// with zero allocation — no visitor constructed, no fields
    /// walked. R2 of PR #1541's review caught the original shape
    /// allocating + walking ALL fields per non-probe span.
    ///
    /// `ensure_probe_meta` is idempotent: if a sibling Layer
    /// (e.g. [`JsonlProbeFileSink`](super::probe_file_sink)) has
    /// already populated the extension for this span, we no-op
    /// without re-visiting. Same `start: Instant` is read by both
    /// Layers at `on_close` → identical `duration_ms` in the
    /// broadcast stream AND on disk.
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        if !span_carries_probe_class(attrs) {
            return; // cheap static check — no allocation
        }
        let Some(span_ref) = ctx.span(id) else {
            return;
        };
        ensure_probe_meta(attrs, &span_ref);
    }

    /// Span closed — convert its parked `SpanProbeMeta` into a
    /// `ProbeEvent` with `duration_ms` and fan out. Pair with
    /// `on_new_span` per task #196.
    ///
    /// `on_close` fires when the span's last clone is dropped.
    /// For `time_sync!` that's the end of the wrapped block; for
    /// `time_probe!` it's when the Instrumented<F> future is
    /// dropped (typically right after `.await` completes).
    ///
    /// We use [`build_timing_event_from_meta`](super::probe_span_meta::build_timing_event_from_meta)
    /// — non-destructive — because a sibling Layer also reads the
    /// same extension at its own `on_close`. The fields clone is
    /// paid ONCE per probe-carrying span close, orders of
    /// magnitude rarer than `on_new_span` firing.
    fn on_close(&self, id: Id, ctx: Context<'_, S>) {
        let Some(span_ref) = ctx.span(&id) else {
            return;
        };
        let Some(probe_event) = build_timing_event_from_meta(&span_ref, current_uri_chain()) else {
            return; // span didn't carry probe_class — not ours
        };
        self.fan_out(probe_event);
    }
}

/// Visitor that pulls `probe_class`, `message`, and every other
/// recorded field off a tracing event.
#[derive(Default)]
struct ProbeEventVisitor {
    probe_class: Option<String>,
    message: Option<String>,
    fields: HashMap<String, String>,
}

impl ProbeEventVisitor {
    fn record_field(&mut self, name: &str, value: String) {
        match name {
            "probe_class" => self.probe_class = Some(value),
            // tracing records the format-string message under the
            // synthetic field name "message"
            "message" => self.message = Some(value),
            _ => {
                self.fields.insert(name.to_string(), value);
            }
        }
    }
}

impl Visit for ProbeEventVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.record_field(field.name(), value.to_string());
    }
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.record_field(field.name(), format!("{:?}", value));
    }
    fn record_i64(&mut self, field: &Field, value: i64) {
        self.record_field(field.name(), value.to_string());
    }
    fn record_u64(&mut self, field: &Field, value: u64) {
        self.record_field(field.name(), value.to_string());
    }
    fn record_bool(&mut self, field: &Field, value: bool) {
        self.record_field(field.name(), value.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::UriCaptureLayer;
    use tracing_subscriber::prelude::*;

    /// Install BOTH layers (UriCapture for ancestry, ProbeRouter for
    /// fanout) inside a `with_default` scope and run the closure.
    fn install<F: FnOnce(ProbeRouterLayer) -> R, R>(f: F) -> R {
        let router = ProbeRouterLayer::new();
        let subscriber = tracing_subscriber::registry()
            .with(UriCaptureLayer::new())
            .with(router.clone());
        tracing::subscriber::with_default(subscriber, || f(router))
    }

    #[test]
    fn router_default_constructible() {
        // Default + new must produce equivalent shapes. We can't
        // compare directly (no PartialEq for the inner Arc) so we
        // just smoke-check that both methods compile and the router
        // has zero classes initially.
        let r1 = ProbeRouterLayer::default();
        assert!(r1.known_classes().is_empty());
        let r2 = ProbeRouterLayer::new();
        assert!(r2.known_classes().is_empty());
        let r3 = ProbeRouterLayer::with_capacity(8);
        assert!(r3.known_classes().is_empty());
    }

    #[test]
    fn subscribed_class_receives_emitted_probe() {
        install(|router| {
            let mut rx = router.subscribe("latency");
            crate::probe!(class = "latency", "turn complete");
            let event = rx.try_recv().expect("subscriber must receive event");
            assert_eq!(event.class, "latency");
            assert_eq!(event.message.as_deref(), Some("turn complete"));
            assert!(
                event.uri_chain.is_empty(),
                "no instrumented span → empty chain"
            );
        });
    }

    #[test]
    fn probe_event_carries_uri_chain() {
        install(|router| {
            let mut rx = router.subscribe("decision");
            let span = tracing::info_span!("cmd", uri = "airc:///inference/llm/generate");
            let _enter = span.enter();
            crate::probe!(
                class = "decision",
                action = "evict-lora",
                target = "typescript-expertise"
            );
            let event = rx.try_recv().expect("subscriber must receive event");
            assert_eq!(event.class, "decision");
            assert_eq!(
                event.uri_chain,
                vec!["airc:///inference/llm/generate".to_string()]
            );
            // String literals reach the visitor via `record_str` and
            // are stored unquoted. The Debug-recorded form would show
            // surrounding quotes; the substrate intentionally keeps
            // the original string content here.
            assert_eq!(
                event.fields.get("action").map(String::as_str),
                Some("evict-lora")
            );
            assert_eq!(
                event.fields.get("target").map(String::as_str),
                Some("typescript-expertise")
            );
        });
    }

    #[test]
    fn unsubscribed_class_is_a_noop_fanout() {
        install(|router| {
            // Don't subscribe to "latency" — probe fires, nobody home.
            crate::probe!(class = "latency", "nobody watching");
            // The router only knows about classes that have ever been
            // subscribed, NOT every class that fires. That's the
            // "no fabricated consumers" property.
            assert!(
                router.known_classes().is_empty(),
                "firing a probe doesn't auto-register the class — only subscribe() does"
            );
        });
    }

    #[test]
    fn multiple_subscribers_each_receive_every_event() {
        install(|router| {
            let mut rx1 = router.subscribe("admission");
            let mut rx2 = router.subscribe("admission");
            crate::probe!(class = "admission", lane = 3i64, verdict = "accepted");
            let e1 = rx1.try_recv().expect("rx1 receives");
            let e2 = rx2.try_recv().expect("rx2 receives");
            assert_eq!(e1.class, e2.class);
            assert_eq!(e1.fields.get("lane").map(String::as_str), Some("3"));
            assert_eq!(e2.fields.get("lane").map(String::as_str), Some("3"));
            assert_eq!(e1.fields.get("verdict"), e2.fields.get("verdict"));
        });
    }

    #[test]
    fn different_classes_routed_independently() {
        install(|router| {
            let mut latency_rx = router.subscribe("latency");
            let mut decision_rx = router.subscribe("decision");

            crate::probe!(class = "latency", duration_ms = 42u64);
            crate::probe!(class = "decision", action = "promote");
            crate::probe!(class = "latency", duration_ms = 99u64);

            // latency channel should get both latency events
            let l1 = latency_rx.try_recv().expect("latency #1");
            let l2 = latency_rx.try_recv().expect("latency #2");
            assert_eq!(l1.fields.get("duration_ms").map(String::as_str), Some("42"));
            assert_eq!(l2.fields.get("duration_ms").map(String::as_str), Some("99"));

            // decision channel only gets the one decision event
            let d1 = decision_rx.try_recv().expect("decision #1");
            assert_eq!(d1.fields.get("action").map(String::as_str), Some("promote"));
            assert!(decision_rx.try_recv().is_err(), "no more decision events");
            assert!(latency_rx.try_recv().is_err(), "no more latency events");
        });
    }

    /// Task #196: `time_sync!` emits a span carrying `probe_class
    /// = "timing"`. When the span closes the Layer must fan out a
    /// `ProbeEvent` with the timing class + a `duration_ms` field.
    /// Before this fix the Layer ignored span close — `time_sync!`
    /// was theatrical.
    #[test]
    fn time_sync_span_close_fans_out_timing_event() {
        install(|router| {
            let mut rx = router.subscribe("timing");
            // Scope so the span is fully dropped (closes) before
            // we check the channel.
            {
                let _result: i32 = crate::time_sync!("test_phase", 21 * 2);
            }
            let event = rx
                .try_recv()
                .expect("subscribed listener must receive the timing event");
            assert_eq!(event.class, "timing");
            assert_eq!(
                event.fields.get("seam").map(String::as_str),
                Some("test_phase")
            );
            // duration_ms is always set on timing events
            assert!(
                event.fields.contains_key("duration_ms"),
                "fields must contain duration_ms: {:?}",
                event.fields
            );
        });
    }

    /// Same as the sync test but for `time_probe!` (async). Uses
    /// the current-thread tokio runtime so the per-thread subscriber
    /// from `with_default` covers the future's polls.
    #[test]
    fn time_probe_span_close_fans_out_timing_event() {
        install(|router| {
            let mut rx = router.subscribe("timing");
            let runtime = tokio::runtime::Builder::new_current_thread()
                .build()
                .expect("current-thread runtime");
            async fn produces() -> i32 {
                42
            }
            let _result: i32 =
                runtime.block_on(async { crate::time_probe!("async_test_phase", produces()) });
            let event = rx.try_recv().expect("subscriber must receive timing event");
            assert_eq!(event.class, "timing");
            assert_eq!(
                event.fields.get("seam").map(String::as_str),
                Some("async_test_phase")
            );
            assert!(event.fields.contains_key("duration_ms"));
        });
    }

    /// Plain `info_span!` calls (no `probe_class`) must NOT trigger
    /// timing fanout — only `time_sync!` / `time_probe!` spans
    /// (which carry `probe_class = "timing"`) count. Pins the
    /// `[[no-fallbacks-ever]]` doctrine: we don't fabricate timing
    /// events for spans nobody asked to time.
    #[test]
    fn span_without_probe_class_does_not_fanout() {
        install(|router| {
            let mut rx = router.subscribe("timing");
            // info_span! with NO probe_class field
            let span = tracing::info_span!("plain", some_field = "value");
            let _enter = span.enter();
            drop(_enter);
            drop(span);
            assert!(
                rx.try_recv().is_err(),
                "non-timing spans must not produce timing fanout"
            );
        });
    }

    #[test]
    fn non_probe_event_does_not_fanout() {
        install(|router| {
            let mut rx = router.subscribe("any");
            // A normal `tracing::info!` has no `probe_class` field;
            // the router must ignore it.
            tracing::info!(some_field = "value", "regular log message");
            assert!(
                rx.try_recv().is_err(),
                "non-probe events must not reach probe subscribers"
            );
        });
    }

    #[test]
    fn subscribe_after_emit_misses_earlier_events() {
        // Documents the (correct) behavior — subscribers see events
        // emitted AFTER they subscribe. Earlier events are gone.
        // This is the broadcast channel's standard semantic; the
        // doctrine is "subscribe at boot, never lose events on the
        // wire by lazy-subscribing mid-flight."
        install(|router| {
            crate::probe!(class = "missed", value = 1i64);
            let mut rx = router.subscribe("missed");
            crate::probe!(class = "missed", value = 2i64);
            let event = rx.try_recv().expect("post-subscribe event arrives");
            assert_eq!(event.fields.get("value").map(String::as_str), Some("2"));
            assert!(
                rx.try_recv().is_err(),
                "the pre-subscribe event #1 is gone — broadcast channels don't backfill"
            );
        });
    }

    #[test]
    fn known_classes_grows_on_subscribe() {
        install(|router| {
            assert!(router.known_classes().is_empty());
            let _rx_a = router.subscribe("alpha");
            let _rx_b = router.subscribe("beta");
            let _rx_a2 = router.subscribe("alpha"); // re-subscribe
            let mut classes = router.known_classes();
            classes.sort();
            assert_eq!(classes, vec!["alpha".to_string(), "beta".to_string()]);
        });
    }

    #[test]
    fn capacity_drives_lag_behavior() {
        // Build a small capacity to make Lagged easy to trigger
        let router = ProbeRouterLayer::with_capacity(2);
        let subscriber = tracing_subscriber::registry()
            .with(UriCaptureLayer::new())
            .with(router.clone());
        tracing::subscriber::with_default(subscriber, || {
            let mut rx = router.subscribe("burst");
            crate::probe!(class = "burst", n = 1i64);
            crate::probe!(class = "burst", n = 2i64);
            crate::probe!(class = "burst", n = 3i64);
            crate::probe!(class = "burst", n = 4i64);
            // First recv should observe Lagged with the count of
            // dropped events.
            let first = rx.try_recv();
            assert!(
                matches!(first, Err(broadcast::error::TryRecvError::Lagged(_))),
                "expected Lagged at capacity boundary, got {first:?}"
            );
            // After Lagged the consumer keeps draining what survives.
            // tokio's broadcast retains the most recent `capacity`
            // events, so the next recv should yield event #3.
            let next = rx.try_recv().expect("post-lag drain succeeds");
            assert_eq!(next.fields.get("n").map(String::as_str), Some("3"));
        });
    }
}
