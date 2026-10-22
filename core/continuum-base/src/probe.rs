//! The probe macros: structured tracing events and timing spans at every meaningful seam
//! (docs/architecture/RTOS-DEBUGGER-PROBES.md). Pure `tracing`, so they live in the floor
//! crate and any crate can probe without depending on the core.

/// Structured measurement emit. Routes by the `class` field to the
/// per-class probe stream `airc://<actor>/debug/probes/<class>/stream`.
///
/// Shape mirrors `tracing::info!` for muscle memory:
///
/// ```ignore
/// use continuum_base::probe;
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
    // Preserve an established tracing target when a responsibility moves modules.
    (target: $target:expr, class = $class:expr, $($rest:tt)*) => {
        ::tracing::event!(
            target: $target,
            ::tracing::Level::INFO,
            probe_class = $class,
            $($rest)*
        )
    };
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
/// use continuum_base::time_sync;
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
/// use continuum_base::time_probe;
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
