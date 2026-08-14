//! Mechanic-grade observability macros — Slice P deliverable 12, 14, 15.
//!
//! Per `docs/architecture/GRID-ADDRESSING-AND-ROUTING.md`:
//!
//! - [`probe!`] — structured measurement emit with per-class routing
//!   (`class = "latency" | "decision" | "state" | "admission" | ...`).
//!   Conventional Rust tracing macros (`debug!`, `info!`, ...) stay
//!   conventional; `probe!` exists only because of features they
//!   don't have: per-class routing, ALWAYS-ON intent, replay
//!   persistence, sample-rate configurable, aggregation-ready.
//! - [`time!`] — explicit-block timing as a one-line macro. Wraps a
//!   `tracing::info_span!` around the supplied block / expression so
//!   the span's duration becomes a timing probe automatically.
//!
//! Joel: "macros are easy and a way of preventing misuse. If coding
//! timing or logging is painful it won't happen." The macro shape
//! mirrors `tracing::*` so developers reach for the familiar idiom;
//! the substrate's tracing subscriber does the URI-tagging + probe
//! routing without any per-call-site wiring.
//!
//! ## Zero-cost-when-disabled property
//!
//! Both macros expand to `tracing::event!` / `tracing::info_span!`
//! calls that inherit `tracing`'s `release_max_level_*` cargo
//! feature gates. When the level is filtered out at build time,
//! the expansion is literally absent from the binary — no
//! formatting, no allocation, no branch. The compiler discipline
//! the design doc names is enforced by sticking to the same
//! `tracing` shape every conventional macro uses.

/// Structured measurement emit. Routes by the `class` field to the
/// per-class probe stream `airc://<actor>/debug/probes/<class>/stream`.
///
/// Shape mirrors `tracing::info!` for muscle memory:
///
/// ```ignore
/// use continuum_core::probe;
///
/// // Minimum: just a class and a message
/// probe!(class = "latency", "turn complete");
///
/// // Typical: class + structured fields + optional message
/// probe!(class = "latency",   turn_id = %id, duration_ms = elapsed, "turn complete");
/// probe!(class = "decision",  action = "evict-lora", target = "typescript-expertise");
/// probe!(class = "state",     working_set_size = ws.len());
/// probe!(class = "admission", lane = 3, verdict = "accepted", "admitted");
/// ```
///
/// Routing key is the `class` field. Substrate's tracing subscriber
/// inspects the event's `class` field and routes the captured
/// record to `airc://<actor>/debug/probes/<class>/stream`. URI-tag
/// for the actor comes from the current span context (the dispatch
/// span the executor establishes).
///
/// Implemented as a tracing event at `Level::INFO` so it composes
/// with the existing tracing pipeline. Future commits gate per-class
/// emission on a cargo feature so production builds compile out
/// disabled classes entirely.
#[macro_export]
macro_rules! probe {
    // probe!(class = "foo", field = val, ..., "message format" args...)
    (class = $class:expr, $($rest:tt)*) => {
        ::tracing::event!(
            ::tracing::Level::INFO,
            probe_class = $class,
            $($rest)*
        )
    };
}

/// Explicit-block timing for **synchronous** code. Wraps a block or
/// expression in an `info_span!` whose duration becomes a timing
/// probe at scope exit.
///
/// **MUST NOT contain `.await`.** Holding the `_enter` span guard
/// across `.await` breaks the URI ancestry stack (the same async
/// anti-pattern the `d1cf19dc5` dispatch fix was specifically about).
/// For async bodies, use [`time_async!`] instead — it wraps the body
/// in `.instrument(span).await` which trips span enter/exit at
/// suspension boundaries correctly.
///
/// Per PR #1529 reviewer 2: the original `time!` was a substrate-wide
/// foot-gun because `time!("infer", run_inference(...).await)` (a
/// common shape) silently broke `URI_STACK` across the await. Split
/// into typed sync/async variants per the dispatch fix doctrine.
///
/// ```ignore
/// use continuum_core::time_sync;
///
/// // Block form — returns the block's value
/// let candidates = time_sync!("recall_phase", {
///     recall_candidates_blocking(&query)
/// });
///
/// // Single-expression form (the most common shape)
/// let result = time_sync!("hash_payload", compute_hash(&payload));
/// ```
///
/// The span name becomes the `seam` field on the timing probe (the
/// same field `time_probe!` uses — both macros agree so `jq` queries
/// like `.fields.seam == "recall_phase"` match either kind). Same
/// subscriber that routes `probe!` events handles span durations and
/// routes to `airc://<actor>/debug/probes/timing/stream`. The
/// dispatch span the executor establishes is the parent — so the
/// timing probe's URI context is whatever URI was being dispatched.
///
/// When `time_sync!` is used outside any active dispatch span (e.g.
/// in bootstrap code before the executor is wired), the span has no
/// URI parent and the timing probe routes to the substrate's
/// `bootstrap` virtual actor.
#[macro_export]
macro_rules! time_sync {
    ($name:expr, $body:expr) => {{
        let __span = ::tracing::info_span!("time", seam = $name, probe_class = "timing",);
        let __enter = __span.enter();
        $body
    }};
}

/// Explicit timing for an **async future** — safe-by-construction
/// companion to [`time_sync!`].
///
/// Per Joel 2026-06-06 `[[refine-tools-as-you-use-them]]`: every
/// async timing site in the cognition path was an
/// `.instrument(info_span!("time", name=..., probe_class="timing"))
/// .await` ceremony. Three lines plus a `use tracing::Instrument`
/// import. Nobody writes those when adding a new seam in a hurry —
/// the result was that async cognition stages stayed untimed. This
/// macro collapses that to one line.
///
/// ```ignore
/// use continuum_core::time_probe;
///
/// // Wrap any future expression:
/// let analysis = time_probe!("cognition.analyze", analyze(input));
///
/// // Inline complex expressions just as cleanly:
/// let response = time_probe!("inference.generate",
///     adapter.generate_text(request));
/// ```
///
/// ## Why this isn't `time_async!`
///
/// The name `time_async!` is taken by `crate::logging::time_async!`
/// which has a DIFFERENT shape (RAII `TimingGuard` with
/// `category` + `operation`, NOT a tracing span). The two
/// macros serve different observability paths — RAII to the
/// logging crate's own logger vs tracing-span to the substrate's
/// probe routing. Keeping distinct names prevents accidental
/// substitution.
///
/// ## Why this is safe across `.await`
///
/// The previous substrate-wide foot-gun (PR #1529 reviewer 2 fix)
/// was a `time!` macro that expanded to `let _enter = span.enter();
/// $body` where `$body` contained `.await`. Holding `_enter` across
/// `.await` broke `URI_STACK`. THIS macro expands to
/// `$future.instrument(span).await` — the future itself enters /
/// exits the span via `Future::poll` boundaries, never holding a
/// scope guard across an await suspension. Same shape
/// `CommandExecutor::dispatch` uses (per the `d1cf19dc5` fix).
///
/// ## Where it lands
///
/// Emits the same `timing` probe class as `time_sync!`. Operators
/// filter both sync + async timings together via
/// `CONTINUUM_PROBE_CLASSES=timing` and see one flat timeline.
///
/// **Substrate gaps tracked separately:** task #196 (probe Layers
/// only implement `on_event`, not `on_close` — span-close events
/// from `time_sync!` + `time_probe!` don't yet reach the JSONL log
/// or broadcast subscribers; the call shape lands here, the
/// routing side follows). Task #197 (flat `timing` class vs the
/// substrate's hierarchical class taxonomy — picking the
/// convention is its own design decision). Don't rely on the
/// timing probe being persistable until #196 lands.
///
/// ## Cost
///
/// Low — but NOT zero. `info_span!` allocates a `Span` struct +
/// attaches the `name` + `probe_class` fields regardless of
/// whether a subscriber is registered. `Instrumented<F>` wraps
/// the future, adding ~24 bytes per call site and one branch on
/// every `poll`. Tracing's `release_max_level_*` cargo features
/// compile out the `event!` body, but the `Instrumented<F>`
/// wrapper persists at runtime even at max-level-off. Acceptable
/// for cognition seams (Qwen inference dominates wall-clock by
/// 4-5 orders of magnitude); audit per task #198 if sprinkled
/// into a hot loop.
///
/// ## Field names
///
/// The span carries `seam = $name` (the seam identifier as a
/// dotted path, e.g. `"cognition.analyze.inference"`) and
/// `probe_class = "timing"`. `seam` is chosen over the more-
/// natural `name` to avoid field-name collision with other
/// probes that already use `name` for different purposes
/// (`probe!(class="state", name="active_persona", ...)`). All
/// `jq` queries should filter on `.fields.seam`.
#[macro_export]
macro_rules! time_probe {
    ($name:expr, $future:expr) => {{
        ::tracing::Instrument::instrument(
            $future,
            ::tracing::info_span!("time", seam = $name, probe_class = "timing",),
        )
        .await
    }};
}

/// Returns the substrate's "call stack" at this point as a
/// `Vec<String>` of URI frames from the dispatch root to here.
///
/// The substrate's spans form a tree; the URI-aware tracing
/// [`UriCaptureLayer`](crate::routing::UriCaptureLayer) walks the
/// current scope's ancestry and emits one frame per entered span
/// carrying a `uri` field. The result is the URI path from the
/// outermost dispatched command (the dispatch span the executor
/// establishes) to whatever scope `stack!()` is called from.
///
/// ```ignore
/// use continuum_core::{stack, probe};
///
/// // Typical pattern: attach the URI stack to an error probe so the
/// // operator looking at /debug/probes/error/stream sees the chain
/// // of dispatches that led to the failure
/// probe!(class = "error", stack = ?stack!(), "engram lookup failed");
/// ```
///
/// ## With the Layer installed (the substrate's wired path)
///
/// Returns the full URI ancestry, outermost first. The Layer must be
/// installed at boot via
/// `tracing_subscriber::registry().with(UriCaptureLayer::new()).init()`.
///
/// ## Without the Layer (bootstrap, third-party tools, tests)
///
/// Returns an empty `Vec` rather than fabricating a fake frame.
/// Consumers handle the empty case (`if stack.is_empty() { ... }`).
/// The substrate refuses to emit invented data — honesty over
/// convenience, per [[no-fallbacks-ever]].
#[macro_export]
macro_rules! stack {
    () => {{
        $crate::routing::current_uri_chain()
    }};
}

#[cfg(test)]
mod tests {
    // The macros expand to tracing::event! / tracing::info_span! calls
    // that the tracing test subscriber can capture. Testing here is
    // verifying that the macros COMPILE and PARSE correctly with the
    // documented call shapes — the actual subscriber-routing logic is
    // exercised by integration tests in a follow-up commit (depends on
    // the URI-aware tracing layer that doesn't exist yet).
    //
    // What's locked here: every call shape the doc claims to support
    // typechecks; future refactors of the macro definitions can't
    // accidentally regress the call surface without a test failure.

    #[test]
    fn probe_with_class_only_message_compiles() {
        crate::probe!(class = "latency", "turn complete");
    }

    #[test]
    fn probe_with_class_and_fields_compiles() {
        let id: u64 = 42;
        let elapsed: u64 = 1697;
        crate::probe!(
            class = "latency",
            turn_id = id,
            duration_ms = elapsed,
            "turn complete"
        );
    }

    #[test]
    fn probe_decision_class_compiles() {
        crate::probe!(
            class = "decision",
            action = "evict-lora",
            target = "typescript-expertise",
            reason = "lru"
        );
    }

    #[test]
    fn probe_state_class_compiles() {
        let working_set_size: usize = 12;
        let recall_candidates: usize = 23;
        crate::probe!(class = "state", working_set_size, recall_candidates);
    }

    #[test]
    fn time_block_returns_block_value() {
        let result = crate::time_sync!("test_phase", {
            let x = 21;
            x * 2
        });
        assert_eq!(result, 42);
    }

    #[test]
    fn time_expression_returns_expression_value() {
        let result = crate::time_sync!("test_phase", 21 * 2);
        assert_eq!(result, 42);
    }

    /// `time_probe!` wraps a future and yields the future's value
    /// at the call site — i.e. the macro is value-transparent so
    /// switching `expr.await` to `time_probe!("seam", expr)` is a
    /// pure observability addition with no shape change.
    ///
    /// Uses `block_on` rather than `#[tokio::test]` to keep the test
    /// dep-light — the macro doesn't care about the executor, just
    /// that the call shape compiles + returns the inner future's
    /// value.
    #[test]
    fn time_probe_returns_inner_future_value() {
        async fn produces_forty_two() -> i32 {
            42
        }
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("current-thread runtime");
        let result =
            runtime.block_on(async { crate::time_probe!("test_phase", produces_forty_two()) });
        assert_eq!(result, 42);
    }

    /// `time_probe!` MUST pass `Result<T, E>` futures through
    /// unchanged — `.instrument()` preserves the inner future's
    /// `Output` type, but a test that uses an infallible future
    /// can't prove that. Pin the error-path explicitly per
    /// `[[no-fallbacks-ever]]`: substrate refuses to silently
    /// swallow errors at any seam.
    #[test]
    fn time_probe_propagates_error_from_inner_future() {
        async fn fails() -> Result<i32, &'static str> {
            Err("intentional")
        }
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("current-thread runtime");
        let result: Result<i32, &str> =
            runtime.block_on(async { crate::time_probe!("test_error_path", fails()) });
        assert_eq!(result, Err("intentional"));
    }

    /// Multiple nested `time_probe!` calls compose — the inner
    /// span becomes a child of the outer span, same as
    /// `time_sync!` nesting. Tests the macro's hygiene + that
    /// the value flows through both layers.
    #[test]
    fn time_probe_nested_compose_and_return_inner_value() {
        async fn doubled(x: i32) -> i32 {
            x * 2
        }
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("current-thread runtime");
        let result = runtime.block_on(async {
            let outer =
                crate::time_probe!("outer", async { crate::time_probe!("inner", doubled(21)) });
            outer
        });
        assert_eq!(result, 42);
    }

    #[test]
    fn stack_returns_vec_of_strings() {
        // Outside any dispatch span and without a subscriber installed
        // the substrate returns an empty Vec — honest reporting per
        // [[no-fallbacks-ever]].
        let s: Vec<String> = crate::stack!();
        assert!(
            s.is_empty(),
            "expected empty stack with no Layer installed, got {:?}",
            s
        );
    }

    /// With the URI-aware tracing Layer installed and a dispatched
    /// span entered, `stack!()` returns the captured URI as a frame.
    /// This is the substrate's wired path that every production caller
    /// will hit.
    #[test]
    fn stack_inside_a_dispatched_span_returns_uri_frame() {
        use tracing_subscriber::prelude::*;

        let subscriber =
            tracing_subscriber::registry().with(crate::routing::UriCaptureLayer::new());

        tracing::subscriber::with_default(subscriber, || {
            let span = tracing::info_span!("cmd", uri = "airc:///inference/llm/generate");
            let _enter = span.enter();
            let chain: Vec<String> = crate::stack!();
            assert_eq!(
                chain,
                vec!["airc:///inference/llm/generate".to_string()],
                "expected captured URI frame from dispatched span"
            );
        });
    }

    /// Multi-frame ancestry — the keystone behavior. Nested dispatched
    /// spans surface as a chain ordered outermost-to-innermost.
    #[test]
    fn stack_walks_nested_dispatched_spans() {
        use tracing_subscriber::prelude::*;

        let subscriber =
            tracing_subscriber::registry().with(crate::routing::UriCaptureLayer::new());

        tracing::subscriber::with_default(subscriber, || {
            let outer = tracing::info_span!("cmd", uri = "airc:///inference/llm/generate");
            let _o = outer.enter();
            let inner = tracing::info_span!("cmd", uri = "airc:///data/list");
            let _i = inner.enter();
            let chain: Vec<String> = crate::stack!();
            assert_eq!(
                chain,
                vec![
                    "airc:///inference/llm/generate".to_string(),
                    "airc:///data/list".to_string(),
                ]
            );
        });
    }

    #[test]
    fn time_nested_inside_probe_compiles() {
        // Mechanic pattern: probe a measurement that itself measures
        // a sub-block. The nested time! span becomes a child of the
        // outer probe's span; the timing flamegraph shows the chain.
        let total = crate::time_sync!("outer", {
            let inner_result = crate::time_sync!("inner", { 21 + 21 });
            crate::probe!(class = "state", inner = inner_result);
            inner_result
        });
        assert_eq!(total, 42);
    }
}
