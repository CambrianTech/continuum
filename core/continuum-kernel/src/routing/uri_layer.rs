//! URI-aware tracing Layer — Slice P keystone.
//!
//! Per `docs/architecture/GRID-ADDRESSING-AND-ROUTING.md`: every
//! dispatched command establishes a `tracing::info_span!("cmd", uri =
//! %command, ...)` and the substrate's tracing Layer captures the
//! recorded `uri` field into a per-thread ancestry stack. The
//! [`stack!`](crate::stack) macro reads that stack and returns the
//! URI frames from outermost dispatch to current scope.
//!
//! ## What this Layer is responsible for
//!
//! - **Capture**: on every new span, if the span's attributes carry a
//!   `uri = ...` field, extract the value and stash a [`UriFrame`] in
//!   the span's extension storage.
//! - **Push/pop**: on span enter/exit, push or pop the URI onto the
//!   thread-local [`URI_STACK`]. `stack!()` simply clones it.
//!
//! ## What this Layer is NOT (yet) responsible for
//!
//! - Probe class routing — `probe!(class = "foo", ...)` events are
//!   emitted as `tracing::event!`s with a `probe_class` field. A
//!   separate Layer (follow-up commit) will route those to per-class
//!   stream consumers reachable at
//!   `airc://<actor>/debug/probes/<class>/stream`.
//! - Timing rollup — `time!("scope", expr)` already creates a span; a
//!   future Layer will aggregate durations into the
//!   `/debug/profile/flamegraph` URI.
//!
//! ## Async + `Span::enter()` held across awaits
//!
//! `tracing` explicitly warns against holding a `_enter` guard across
//! `.await` in async code — the thread-local entry/exit semantics
//! break when tokio moves the task to a different thread mid-await.
//! The substrate's correct pattern is `future.instrument(span).await`
//! which trips `on_enter`/`on_exit` at task suspension boundaries.
//!
//! When code violates that pattern (currently
//! [`CommandExecutor::dispatch`] does), this Layer's URI stack on a
//! given thread may briefly contain entries for spans that have moved
//! to other threads. That's a substrate bug to fix, not a Layer bug to
//! work around. The follow-up commit converts dispatch to use
//! `Instrument` so the stack stays correct across awaits.
//!
//! ## Re-entrancy and isolation
//!
//! [`URI_STACK`] is a `thread_local!` — each thread has its own. Tests
//! that install the Layer once and run multiple span scopes
//! sequentially observe a clean push/pop pattern. Parallel tests with
//! distinct subscribers can't see each other's stacks.

use std::cell::RefCell;
use std::fmt;

use tracing::{
    field::{Field, Visit},
    span, Subscriber,
};
use tracing_subscriber::{layer::Context, registry::LookupSpan, Layer};

/// A URI captured from a span's `uri = ...` field at span creation
/// time. Stored as an extension on the span so the Layer can match
/// up `on_enter` / `on_exit` events to a recorded URI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UriFrame(pub String);

thread_local! {
    /// Per-thread URI ancestry stack. Pushed on `on_enter`, popped on
    /// `on_exit`. `stack!()` clones it.
    ///
    /// Why thread-local: walking the actual `tracing` span tree from
    /// arbitrary call sites requires `dispatcher::get_default` +
    /// `downcast_ref` to the subscriber stack, which doesn't compose
    /// when consumers layer their own subscribers on top. The
    /// thread-local pattern is the standard `tracing-subscriber` way
    /// to expose span context to non-Layer code.
    static URI_STACK: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// Visitor that pulls the `uri` field out of a span's recorded
/// attributes. Handles both `record_str` (string literal fields) and
/// `record_debug` (the `%` Display format used by
/// `info_span!("cmd", uri = %command)`).
#[derive(Default)]
struct UriFieldVisitor {
    uri: Option<String>,
}

impl Visit for UriFieldVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "uri" {
            self.uri = Some(value.to_string());
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        // `uri = %command` records via Display-as-Debug. The wrapper
        // formats the value using `{:?}` which for a Display-via-Debug
        // wrapper produces the unquoted Display output directly.
        if field.name() == "uri" && self.uri.is_none() {
            self.uri = Some(format!("{:?}", value));
        }
    }
}

/// The URI-capturing tracing Layer.
///
/// Install on a `tracing_subscriber::Registry` (or any other Subscriber
/// implementing `LookupSpan`) at substrate boot:
///
/// ```ignore
/// use tracing_subscriber::prelude::*;
/// use continuum_core::routing::UriCaptureLayer;
///
/// tracing_subscriber::registry()
///     .with(UriCaptureLayer::new())
///     .init();
/// ```
///
/// Once installed, `crate::stack!()` returns the URI ancestry from
/// the outermost dispatched span to the current scope. Without the
/// Layer (or without any subscriber), `stack!()` falls back to
/// returning the current span's metadata name — degraded but never
/// panics. See [`UriCaptureLayer`] for the full semantics.
#[derive(Debug, Default, Clone, Copy)]
pub struct UriCaptureLayer;

impl UriCaptureLayer {
    pub fn new() -> Self {
        Self
    }
}

impl<S> Layer<S> for UriCaptureLayer
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
{
    fn on_new_span(&self, attrs: &span::Attributes<'_>, id: &span::Id, ctx: Context<'_, S>) {
        let mut visitor = UriFieldVisitor::default();
        attrs.record(&mut visitor);
        if let Some(uri) = visitor.uri {
            if let Some(span) = ctx.span(id) {
                span.extensions_mut().insert(UriFrame(uri));
            }
        }
    }

    fn on_enter(&self, id: &span::Id, ctx: Context<'_, S>) {
        if let Some(span) = ctx.span(id) {
            if let Some(frame) = span.extensions().get::<UriFrame>() {
                let uri = frame.0.clone();
                URI_STACK.with(|s| s.borrow_mut().push(uri));
            }
        }
    }

    fn on_exit(&self, id: &span::Id, ctx: Context<'_, S>) {
        if let Some(span) = ctx.span(id) {
            if span.extensions().get::<UriFrame>().is_some() {
                URI_STACK.with(|s| {
                    let _ = s.borrow_mut().pop();
                });
            }
        }
    }
}

/// Returns the URI ancestry chain for the current scope: outermost
/// dispatched span first, innermost last.
///
/// This is the function [`stack!`](crate::stack) expands to when the
/// Layer is installed.
///
/// ## Behavior with no subscriber
///
/// Returns an empty `Vec`. Callers that want a degraded fallback
/// (e.g. emit the immediate span name) should layer that decision
/// at the call site — the substrate doesn't fabricate fake frames.
///
/// ## Thread-local semantics
///
/// The chain is per-thread. Async code that holds a `_enter` guard
/// across `.await` and migrates threads will see incorrect chains —
/// the fix is `future.instrument(span).await`, NOT a workaround in
/// this function.
pub fn current_uri_chain() -> Vec<String> {
    URI_STACK.with(|s| s.borrow().clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing_subscriber::prelude::*;

    /// Tests in this module install a dedicated subscriber via
    /// `with_default` so each test sees a clean URI stack. Running
    /// the suite serially within one process is fine; the thread-local
    /// stack is reset at thread creation and each test's subscriber
    /// scope is closed before the next runs.

    fn install_capture<F: FnOnce() -> R, R>(f: F) -> R {
        let subscriber = tracing_subscriber::registry().with(UriCaptureLayer::new());
        tracing::subscriber::with_default(subscriber, f)
    }

    #[test]
    fn chain_empty_outside_any_span() {
        install_capture(|| {
            let chain = current_uri_chain();
            assert!(
                chain.is_empty(),
                "expected empty chain outside any span, got {:?}",
                chain
            );
        });
    }

    #[test]
    fn chain_captures_single_uri_field_with_str() {
        install_capture(|| {
            let span = tracing::info_span!("cmd", uri = "airc:///inference/llm/generate");
            let _enter = span.enter();
            let chain = current_uri_chain();
            assert_eq!(chain, vec!["airc:///inference/llm/generate".to_string()]);
        });
    }

    #[test]
    fn chain_captures_single_uri_field_with_display() {
        install_capture(|| {
            let uri = "airc://maya/inference/llm/generate";
            // `%uri` (Display) is the form CommandExecutor::dispatch uses
            let span = tracing::info_span!("cmd", uri = %uri);
            let _enter = span.enter();
            let chain = current_uri_chain();
            assert_eq!(chain, vec![uri.to_string()]);
        });
    }

    #[test]
    fn chain_walks_nested_spans_in_order() {
        install_capture(|| {
            let outer = tracing::info_span!("cmd", uri = "airc:///inference/llm/generate");
            let _outer_enter = outer.enter();
            let inner = tracing::info_span!("cmd", uri = "airc:///data/list");
            let _inner_enter = inner.enter();
            let chain = current_uri_chain();
            assert_eq!(
                chain,
                vec![
                    "airc:///inference/llm/generate".to_string(),
                    "airc:///data/list".to_string(),
                ],
                "expected outer-to-inner URI ancestry"
            );
        });
    }

    #[test]
    fn chain_pops_on_span_exit() {
        install_capture(|| {
            {
                let span = tracing::info_span!("cmd", uri = "airc:///a");
                let _enter = span.enter();
                assert_eq!(current_uri_chain(), vec!["airc:///a".to_string()]);
            }
            // Span guard dropped — chain should be empty again
            assert!(current_uri_chain().is_empty());
        });
    }

    #[test]
    fn span_without_uri_field_does_not_push_frame() {
        install_capture(|| {
            let span = tracing::info_span!("time", name = "recall_phase");
            let _enter = span.enter();
            // `time` spans have no `uri` field — should not appear in chain
            assert!(current_uri_chain().is_empty());
        });
    }

    #[test]
    fn span_with_uri_intermixed_with_span_without_uri() {
        install_capture(|| {
            let outer = tracing::info_span!("cmd", uri = "airc:///root");
            let _outer = outer.enter();
            {
                // Inner span without uri — should not affect chain
                let timing = tracing::info_span!("time", name = "phase");
                let _timing = timing.enter();
                let inner = tracing::info_span!("cmd", uri = "airc:///child");
                let _inner = inner.enter();
                assert_eq!(
                    current_uri_chain(),
                    vec!["airc:///root".to_string(), "airc:///child".to_string()],
                    "intermediate non-URI span should be transparent to chain"
                );
            }
            assert_eq!(current_uri_chain(), vec!["airc:///root".to_string()]);
        });
    }

    #[test]
    fn no_subscriber_returns_empty_chain() {
        // Pin an explicit NoSubscriber for this thread. The test's premise is
        // "no UriCaptureLayer installed", but other tests in this binary can
        // install a GLOBAL default that includes UriCaptureLayer (e.g.
        // tracing_init's `try_init`), which captured this span and made the
        // test order-dependent. with_default(NoSubscriber) restores the
        // intended no-layer world regardless of what ran before.
        tracing::subscriber::with_default(tracing::subscriber::NoSubscriber::default(), || {
            let span = tracing::info_span!("cmd", uri = "airc:///orphan");
            let _enter = span.enter();
            assert!(
                current_uri_chain().is_empty(),
                "no installed Layer means no captured frames; substrate refuses to fabricate"
            );
        });
    }

    #[test]
    fn layer_default_constructible() {
        let _l1 = UriCaptureLayer;
        let _l2 = UriCaptureLayer::new();
        let _l3 = UriCaptureLayer::default();
    }

    /// The Slice P load-bearing assertion: `.instrument(span).await`
    /// keeps the URI chain correct ACROSS the suspension/resume cycle.
    ///
    /// `CommandExecutor::dispatch` uses this exact shape, and every
    /// `stack!()` call site inside dispatched commands relies on it.
    /// The test installs the Layer, wraps a future that calls
    /// `current_uri_chain()` AFTER `tokio::task::yield_now().await`,
    /// and asserts the chain survives the yield.
    ///
    /// We use a current-thread tokio runtime built INSIDE the
    /// `with_default` scope so the thread-local subscriber stays
    /// attached for the polled future. This is the substrate's
    /// expected boot pattern (subscriber installed once at process
    /// start, runtime polls inside that scope).
    ///
    /// Lives here, not in `runtime::command_executor::tests`, because
    /// that module's other `#[tokio::test]`s spawn multi-thread
    /// runtimes that share cargo test process state and cause flaky
    /// thread-local interactions. The load-bearing property is the
    /// Layer's correctness under `.instrument`, not the dispatch path
    /// specifically; this is where it belongs.
    #[test]
    fn instrument_propagates_chain_across_yield_now() {
        use tracing::Instrument;

        let subscriber = tracing_subscriber::registry().with(UriCaptureLayer::new());
        let mut after_yield: Vec<String> = Vec::new();

        tracing::subscriber::with_default(subscriber, || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("current-thread runtime builds");
            rt.block_on(async {
                let span = tracing::info_span!("cmd", uri = "airc:///chain-test/op");
                async {
                    // Force the suspension/resume cycle.
                    tokio::task::yield_now().await;
                    after_yield = current_uri_chain();
                }
                .instrument(span)
                .await;
            });
        });

        assert_eq!(
            after_yield,
            vec!["airc:///chain-test/op".to_string()],
            "after tokio::task::yield_now().await, the URI chain must still \
             carry the instrumented span's URI. A chain of [] here means \
             tracing's `_enter`-across-`await` anti-pattern crept in — \
             the substrate's dispatch path MUST use .instrument(span) for \
             stack!() to be correct."
        );
    }

    /// Same property under nested instrumented futures — proves the
    /// Layer composes correctly across multiple `.instrument` wrappers,
    /// not just one. Mirror of `chain_walks_nested_spans_in_order` but
    /// across a yield boundary.
    #[test]
    fn instrument_walks_nested_chain_across_yield() {
        use tracing::Instrument;

        let subscriber = tracing_subscriber::registry().with(UriCaptureLayer::new());
        let mut after_yield: Vec<String> = Vec::new();

        tracing::subscriber::with_default(subscriber, || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("current-thread runtime builds");
            rt.block_on(async {
                let outer_span = tracing::info_span!("cmd", uri = "airc:///outer");
                async {
                    let inner_span = tracing::info_span!("cmd", uri = "airc:///inner");
                    async {
                        tokio::task::yield_now().await;
                        after_yield = current_uri_chain();
                    }
                    .instrument(inner_span)
                    .await;
                }
                .instrument(outer_span)
                .await;
            });
        });

        assert_eq!(
            after_yield,
            vec!["airc:///outer".to_string(), "airc:///inner".to_string()],
            "nested .instrument() spans must produce ordered ancestry across yield"
        );
    }
}
