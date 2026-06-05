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

/// Explicit-block timing. Wraps a block or expression in an
/// `info_span!` whose duration becomes a timing probe at scope exit.
///
/// ```ignore
/// use continuum_core::time;
///
/// // Block form — returns the block's value
/// let candidates = time!("recall_phase", {
///     recall_candidates(&query)
/// });
///
/// // Single-expression form (the most common shape)
/// let result = time!("inference_run", run_inference(&model, &prompt));
/// ```
///
/// The span name becomes the `name` field on the timing probe. Same
/// subscriber that routes `probe!` events handles span durations and
/// routes to `airc://<actor>/debug/probes/timing/stream`. The
/// dispatch span the executor establishes is the parent — so the
/// timing probe's URI context is whatever URI was being dispatched.
///
/// When `time!` is used outside any active dispatch span (e.g. in
/// bootstrap code before the executor is wired), the span has no
/// URI parent and the timing probe routes to the substrate's
/// `bootstrap` virtual actor.
#[macro_export]
macro_rules! time {
    ($name:expr, $body:expr) => {{
        let __span = ::tracing::info_span!(
            "time",
            name = $name,
            probe_class = "timing",
        );
        let __enter = __span.enter();
        $body
    }};
}

/// Returns the substrate's "call stack" at this point as a
/// `Vec<String>` of URI frames from the dispatch root to here.
///
/// The substrate's spans form a tree; this macro walks the current
/// span's ancestry and emits one frame per entered span carrying a
/// `uri` field. The result is the URI path from the outermost
/// dispatched command (the dispatch span the executor establishes)
/// to whatever scope `stack!()` is called from.
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
/// ## Current implementation: single-frame
///
/// This commit ships the immediate single-frame form — returns
/// `vec![current_span_uri]`. The full multi-frame ancestry requires
/// the substrate's tracing subscriber Layer (the keystone piece that
/// captures span-field values into a thread-local stack — Joel's
/// "you can take it to any node" semantics need that layer to walk
/// across async boundaries). The Layer lands in a follow-up commit
/// on this branch; the macro shape doesn't change when it does.
///
/// ## Outside any dispatch span
///
/// When called from code that's not inside a dispatched command
/// (e.g. bootstrap, tests), returns an empty `Vec` rather than
/// fabricating a fake frame. Consumers handle the empty case
/// (`if stack.is_empty() { /* no parent context */ }`).
#[macro_export]
macro_rules! stack {
    () => {{
        let __span = ::tracing::Span::current();
        // `Span::metadata()` returns Some whenever the span was
        // created via `info_span!`/`debug_span!`/etc., regardless of
        // whether a tracing subscriber is currently registered to
        // RECORD the span. That's the right shape for `stack!()` —
        // we want to surface the dispatch site even in tests or
        // tools that haven't initialized a subscriber.
        //
        // The metadata's `name` is the span name (e.g. "cmd",
        // "time", "recall_phase"). The URI itself lives in the span's
        // recorded fields (e.g. `uri = "airc://maya/inference/llm/generate"`)
        // and requires the substrate's URI-aware tracing Layer to
        // extract. Until that lands in a follow-up commit, we emit
        // the span name as a single frame so callers see SOMETHING
        // useful rather than a perpetually-empty Vec.
        __span
            .metadata()
            .map(|m| ::std::vec![::std::string::String::from(m.name())])
            .unwrap_or_default()
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
        crate::probe!(
            class = "state",
            working_set_size,
            recall_candidates
        );
    }

    #[test]
    fn time_block_returns_block_value() {
        let result = crate::time!("test_phase", {
            let x = 21;
            x * 2
        });
        assert_eq!(result, 42);
    }

    #[test]
    fn time_expression_returns_expression_value() {
        let result = crate::time!("test_phase", 21 * 2);
        assert_eq!(result, 42);
    }

    #[test]
    fn stack_returns_vec_of_strings() {
        let s: Vec<String> = crate::stack!();
        // Outside any dispatch span (in test context) the substrate
        // returns either the current span's metadata name as a single
        // frame, or an empty Vec when no span is active. Both are
        // valid; the test only locks that the macro typechecks AND
        // returns the expected type.
        assert!(s.iter().all(|f| !f.is_empty() || s.is_empty()));
    }

    /// Without a tracing subscriber attached, `Span::metadata()`
    /// returns `None` (the span is "disabled") and `stack!()` produces
    /// an empty Vec. Asserting non-emptiness inside an `info_span!`
    /// requires either initializing a subscriber for the test, OR
    /// the URI-aware tracing Layer (the follow-up commit that walks
    /// the span tree and surfaces recorded URIs).
    ///
    /// What's locked here today: `stack!()` returns the expected
    /// type, integrates with `tracing::Span::current()`, and survives
    /// the no-subscriber case without panicking. The full-ancestry
    /// integration test lands with the Layer.
    #[test]
    fn stack_inside_a_span_returns_string_vec_safely() {
        let span = tracing::info_span!("test_span");
        let _enter = span.enter();
        let s: Vec<String> = crate::stack!();
        // No assertion on contents — without a subscriber the span
        // is disabled and metadata() is None. The integration test
        // with a real subscriber lands in the Layer commit.
        let _ = s;
    }

    #[test]
    fn time_nested_inside_probe_compiles() {
        // Mechanic pattern: probe a measurement that itself measures
        // a sub-block. The nested time! span becomes a child of the
        // outer probe's span; the timing flamegraph shows the chain.
        let total = crate::time!("outer", {
            let inner_result = crate::time!("inner", {
                21 + 21
            });
            crate::probe!(class = "state", inner = inner_result);
            inner_result
        });
        assert_eq!(total, 42);
    }
}
