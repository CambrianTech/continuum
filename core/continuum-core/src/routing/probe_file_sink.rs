//! Offline-debug JSONL sink for `probe!` events.
//!
//! ## Why this layer exists
//!
//! [`ProbeRouterLayer`](super::probe_router::ProbeRouterLayer) fans
//! probe events to in-process broadcast subscribers (the
//! `debug/probes/{class}/stream` URI consumers from Slice P, #177).
//! That's perfect for substrate code that subscribes from another
//! ServiceModule — but it disappears the moment the process exits.
//!
//! Joel's RTOS-debugger framing (2026-06-06,
//! `[[jtag-probes-are-rtos-debugger]]`): a probe is a non-blocking
//! breakpoint with variable inspection + timing. To "hunt down
//! bottlenecks" or "track down stuff like [silence-affordance]
//! bugs" you need the breakpoint history persisted across runs,
//! filterable per-class, and `tail -f`-able from a shell. This
//! layer provides exactly that.
//!
//! ## Shape
//!
//! Plain JSON-Lines on disk. One ProbeEvent per line. Each event
//! gains a `captured_at_ms` field at sink time so log lines are
//! self-dating without the caller threading clocks.
//!
//! ## Configuration
//!
//! Two env vars — operator-friendly, zero-recompile:
//!
//! - `CONTINUUM_PROBE_DIR=/path/to/probes.jsonl` — append-only file.
//!   Unset = sink absent. The directory must exist (sink errors on
//!   open if not; honest failure beats silent drop per
//!   `[[no-fallbacks-ever]]`).
//! - `CONTINUUM_PROBE_CLASSES=persona,cognition.analyze` — comma-
//!   separated allowed-classes filter. Each value is matched against
//!   each probe's class field by ONE of three rules:
//!
//!     1. `*` — wildcard, matches every class. Use when you want to
//!        capture EVERYTHING and filter offline with `jq`.
//!     2. Exact match — `cognition.analyze.parse` matches only
//!        `cognition.analyze.parse`.
//!     3. Namespace prefix — `persona` matches `persona.turn.spoke`,
//!        `persona.response.enter`, etc. (the `persona.` prefix), but
//!        does NOT match a hypothetical `personality.x`. Concretely
//!        the rule is `class == filter || class.starts_with(filter + ".")`.
//!
//!   Same convention as tracing's `RUST_LOG`. Empty / unset = ALL
//!   classes pass (file sink with no filter = full capture). Per
//!   `[[no-fallbacks-ever]]`: `*` is the EXPLICIT all-classes
//!   value; an empty filter set is a deliberate "no filter
//!   configured."
//!
//! ## Output format
//!
//! ```jsonl
//! {"captured_at_ms":1717689000123,"class":"persona.render.exit","uri_chain":["airc:///cognition/respond"],"message":"rendered","fields":{"persona":"Paige","prompt_len":"4823","decision":"Spoke"}}
//! ```
//!
//! Greppable, filterable with `jq`, replay-able by feeding the
//! lines back through `ProbeEvent` deserialization. Same shape the
//! broadcast subscribers see, plus the timestamp.
//!
//! ## Performance
//!
//! Single `Mutex<BufWriter<File>>` per sink. Probes are not on the
//! hot path of cognition (the LLM call dominates by 4-5 orders of
//! magnitude); the lock contention from N persona loops writing
//! one line per turn is dwarfed by the inference cost. If the
//! substrate ever needs lock-free sink the right shape is per-class
//! channels + dedicated writer task — same pattern
//! `ProbeRouterLayer` already uses. Not load-bearing today.

use std::collections::HashSet;
use std::fs::OpenOptions;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::Serialize;
use tracing::field::{Field, Visit};
use tracing::{span::Attributes, Event, Id, Subscriber};
use tracing_subscriber::{layer::Context, registry::LookupSpan, Layer};

use super::current_uri_chain;
use super::probe_router::ProbeEvent;
use super::probe_span_meta::{
    build_timing_event_from_meta, ensure_probe_meta, span_carries_probe_class,
};

/// Env var carrying a directory path for rolling JSONL probe capture.
/// Daily rotation, last `DEFAULT_MAX_LOG_FILES` retained, disk usage
/// bounded by design. The ONLY env-based entry point — per
/// `[[auto-clean-is-structural-not-operational]]` any substrate writer
/// that grows incrementally MUST auto-clean structurally.
///
/// Single-file mode is reachable in-code via
/// [`JsonlProbeFileSink::new`] for tests + explicit forensic capture;
/// it is deliberately NOT exposed as an env var to keep operators on
/// the bounded-by-design path.
pub const ENV_PROBE_DIR: &str = "CONTINUUM_PROBE_DIR";

/// Env var carrying the comma-separated class filter. Empty/unset =
/// all classes pass through.
pub const ENV_PROBE_CLASSES: &str = "CONTINUUM_PROBE_CLASSES";

/// Default retention for the rolling-file mode — last 7 days. Matches
/// the fmt-layer rolling retention so operators learn one number.
pub const DEFAULT_MAX_LOG_FILES: usize = 7;

/// The live probe ledger; the rotation pool that governs its generations names it too.
pub const PROBE_LEDGER_FILE: &str = "continuum-probes.jsonl";

/// JSONL-on-disk consumer for `probe!` events.
///
/// Composes with [`ProbeRouterLayer`](super::probe_router::ProbeRouterLayer)
/// — install both at the registry; the broadcast subscribers stay
/// in-process, this one persists to disk. Both visit the same
/// tracing event independently; neither blocks the other.
pub struct JsonlProbeFileSink {
    /// On-disk target. In single-file mode this is the file path; in
    /// rolling mode this is the directory holding dated rotation files.
    /// Same field surfaces both modes so logging / tests don't need to
    /// branch on the mode.
    target: PathBuf,
    /// `Box<dyn Write + Send>` wraps whichever underlying writer the
    /// constructor chose — `BufWriter<File>` for single-file mode,
    /// `BufWriter<RollingFileAppender>` for rolling mode. Identical
    /// hot-path code regardless of mode.
    writer: Mutex<Box<dyn Write + Send>>,
    /// Empty set = no filter (all classes pass). Non-empty = only
    /// classes in this set get persisted.
    allowed_classes: HashSet<String>,
}

/// Error states for sink construction. Distinct variants because
/// "couldn't open the file" and "the class filter was malformed"
/// are different operator actions — open the right path vs fix
/// the env var.
#[derive(Debug)]
pub enum ProbeFileSinkError {
    /// `CONTINUUM_PROBE_DIR` was not set. NOT an error per se —
    /// callers use this to detect "sink isn't configured, skip
    /// installation" and continue silently.
    EnvVarUnset,
    /// `CONTINUUM_PROBE_DIR` was set but the file couldn't be
    /// opened (directory missing, permissions, etc.). Per
    /// `[[no-fallbacks-ever]]` the substrate refuses to silently
    /// drop probes — operator must fix the path.
    OpenFailed {
        path: PathBuf,
        source: std::io::Error,
    },
}

impl std::fmt::Display for ProbeFileSinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProbeFileSinkError::EnvVarUnset => write!(
                f,
                "{} is not set — probe file sink not configured",
                ENV_PROBE_DIR
            ),
            ProbeFileSinkError::OpenFailed { path, source } => write!(
                f,
                "could not open probe file {}: {}",
                path.display(),
                source
            ),
        }
    }
}

impl std::error::Error for ProbeFileSinkError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ProbeFileSinkError::EnvVarUnset => None,
            ProbeFileSinkError::OpenFailed { source, .. } => Some(source),
        }
    }
}

impl JsonlProbeFileSink {
    /// Construct a sink writing to a single file at `path`.
    /// **Single-file / forensic mode** — append-only, no rotation,
    /// grows unbounded. Use [`new_rolling`](Self::new_rolling) for
    /// production captures per the
    /// `[[auto-clean-is-structural-not-operational]]` doctrine.
    ///
    /// Empty `allowed_classes` = all classes pass. File is opened in
    /// append mode (created if missing). Buffer flushes per line via
    /// `BufWriter::new`'s default + explicit `flush` after each event
    /// (chosen for `tail -f`-ability — operators expect the line to
    /// appear immediately, not after the buffer fills).
    pub fn new<P: AsRef<Path>>(
        path: P,
        allowed_classes: HashSet<String>,
    ) -> Result<Self, ProbeFileSinkError> {
        let path = path.as_ref().to_path_buf();
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|source| ProbeFileSinkError::OpenFailed {
                path: path.clone(),
                source,
            })?;
        Ok(Self {
            target: path,
            writer: Mutex::new(Box::new(BufWriter::new(file))),
            allowed_classes,
        })
    }

    /// Construct a sink writing to a **rotating** JSONL file under
    /// `dir`. **Rolling / production mode.** Daily rotation, retains
    /// last `max_log_files` days (default
    /// [`DEFAULT_MAX_LOG_FILES`] when called via env). Disk usage
    /// bounded by design.
    ///
    /// Implements `[[auto-clean-is-structural-not-operational]]`:
    /// recurring writers MUST auto-clean structurally. Same shape the
    /// fmt-layer rolling uses (`tracing_appender::rolling::Builder`
    /// in `routing/tracing_init.rs`).
    ///
    /// Files land at `<dir>/continuum-probes.jsonl`, with older
    /// generations beside it as `.1`, `.2`, … up to `max_log_files`.
    /// Rotation is by SIZE, not by date: the probe stream's volume is
    /// driven by decision rate, not by the clock, so a per-day file
    /// has no bound at all.
    pub fn new_rolling<P: AsRef<Path>>(
        dir: P,
        allowed_classes: HashSet<String>,
        max_log_files: usize,
    ) -> Result<Self, ProbeFileSinkError> {
        let dir = dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&dir).map_err(|source| ProbeFileSinkError::OpenFailed {
            path: dir.clone(),
            source,
        })?;
        // Size-rotated, not clock-rotated — see [`crate::routing::capped_appender`]. The
        // probe stream is the HIGHEST-volume writer in the substrate (one JSON line per
        // load-bearing decision across every tokio task), so a clock-based "bound" is even
        // less of one here than for the fmt log. `max_log_files` keeps its meaning: how many
        // generations survive. What changes is that a generation now has a size.
        let appender = crate::routing::capped_appender::CappedAppender::with_limits(
            &dir,
            PROBE_LEDGER_FILE,
            crate::routing::capped_appender::MAX_LOG_BYTES,
            max_log_files,
        )
        .map_err(|source| ProbeFileSinkError::OpenFailed {
            path: dir.clone(),
            source,
        })?;
        Ok(Self {
            target: dir,
            writer: Mutex::new(Box::new(BufWriter::new(appender))),
            allowed_classes,
        })
    }

    /// Construct from `CONTINUUM_PROBE_DIR` + `CONTINUUM_PROBE_CLASSES`.
    /// The ONLY env-based entry point — rolling mode, structural
    /// auto-clean, bounded disk usage. Single-file mode is reachable
    /// only via [`new`](Self::new) in-code for tests + forensic capture.
    ///
    /// Returns `Err(EnvVarUnset)` if `CONTINUUM_PROBE_DIR` is missing —
    /// callers treat that as "sink intentionally disabled, install no
    /// layer." Returns `Err(OpenFailed)` if the dir is set but
    /// unwritable.
    pub fn from_env() -> Result<Self, ProbeFileSinkError> {
        let dir = std::env::var(ENV_PROBE_DIR).map_err(|_| ProbeFileSinkError::EnvVarUnset)?;
        let allowed_classes = std::env::var(ENV_PROBE_CLASSES)
            .ok()
            .map(|s| {
                s.split(',')
                    .map(|c| c.trim().to_string())
                    .filter(|c| !c.is_empty())
                    .collect::<HashSet<_>>()
            })
            .unwrap_or_default();
        Self::new_rolling(dir, allowed_classes, DEFAULT_MAX_LOG_FILES)
    }

    /// The on-disk target the sink writes to. In single-file mode this
    /// is the file path; in rolling mode this is the directory holding
    /// the dated rotation files. Useful for tests + boot logging.
    pub fn target(&self) -> &Path {
        &self.target
    }

    /// Write one ProbeEvent as a JSONL line. Flushes immediately so
    /// `tail -f` works. Errors are dropped silently — a sink that
    /// can't write its line to disk SHOULDN'T panic the persona
    /// service loop. The next event tries again; persistent failures
    /// surface via the file's apparent staleness, not a substrate
    /// crash.
    fn write_one(&self, captured_at_ms: u64, ev: &ProbeEvent) {
        #[derive(Serialize)]
        struct OnDiskEnvelope<'a> {
            captured_at_ms: u64,
            class: &'a str,
            uri_chain: &'a [String],
            #[serde(skip_serializing_if = "Option::is_none")]
            message: Option<&'a str>,
            fields: &'a std::collections::HashMap<String, String>,
        }

        let envelope = OnDiskEnvelope {
            captured_at_ms,
            class: &ev.class,
            uri_chain: &ev.uri_chain,
            message: ev.message.as_deref(),
            fields: &ev.fields,
        };

        let Ok(line) = serde_json::to_string(&envelope) else {
            return;
        };
        let Ok(mut guard) = self.writer.lock() else {
            return; // poisoned lock — sink is effectively dead; don't crash callers
        };
        if writeln!(guard, "{line}").is_ok() {
            let _ = guard.flush();
        }
    }
}

impl<S> Layer<S> for JsonlProbeFileSink
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
        // Same visit pattern as ProbeRouterLayer: pull probe_class +
        // message + every other field off the tracing event. The
        // visitor is private here (not exported from probe_router)
        // and the logic is small enough to duplicate; future
        // refactor can pull a shared visitor into probe_router if
        // a third consumer appears.
        let mut visitor = FileSinkVisitor::default();
        event.record(&mut visitor);

        let class = match visitor.probe_class {
            Some(c) => c,
            None => return, // not a probe event, ignore
        };

        // Class filter — early-out before allocating the envelope.
        // Three-rule match per the module docstring: empty set = all
        // pass, `*` = wildcard, otherwise exact or namespace-prefix.
        if !class_passes_filter(&class, &self.allowed_classes) {
            return;
        }

        let probe_event = ProbeEvent {
            class,
            uri_chain: current_uri_chain(),
            message: visitor.message,
            fields: visitor.fields,
        };

        let captured_at_ms = continuum_base::time::EpochMs::now().millis();

        self.write_one(captured_at_ms, &probe_event);
    }

    /// Spans created via `time_sync!` / `time_probe!` carry a
    /// `probe_class` attribute — same lifecycle as
    /// [`ProbeRouterLayer::on_new_span`](super::probe_router::ProbeRouterLayer).
    ///
    /// Hot-path discipline: cheap static
    /// [`span_carries_probe_class`](super::probe_span_meta::span_carries_probe_class)
    /// check FIRST. Non-probe spans (the vast majority) short-
    /// circuit with zero allocation.
    ///
    /// [`ensure_probe_meta`](super::probe_span_meta::ensure_probe_meta)
    /// is idempotent — if `ProbeRouterLayer` already populated
    /// the extension we no-op. Both Layers read the same `start:
    /// Instant` at `on_close`, so the broadcast subscriber and
    /// JSONL log report identical `duration_ms` for the same span.
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        if !span_carries_probe_class(attrs) {
            return; // cheap static check — no allocation
        }
        let Some(span_ref) = ctx.span(id) else {
            return;
        };
        ensure_probe_meta(attrs, &span_ref);
    }

    /// Span closed — build the timing JSONL record from the
    /// shared extension. Same shape as the broadcast event so
    /// `jq` queries on `tail -f probes.jsonl` match subscriber
    /// output line-for-line.
    fn on_close(&self, id: Id, ctx: Context<'_, S>) {
        let Some(span_ref) = ctx.span(&id) else {
            return;
        };
        let Some(probe_event) = build_timing_event_from_meta(&span_ref, current_uri_chain()) else {
            return; // span didn't carry probe_class — not ours
        };

        // Class filter applies to timing spans just as it does to
        // event-shape probes; an operator filtering to
        // `persona.render.exit` shouldn't see timing noise.
        if !self.allowed_classes.is_empty() && !self.allowed_classes.contains(&probe_event.class) {
            return;
        }

        let captured_at_ms = continuum_base::time::EpochMs::now().millis();

        self.write_one(captured_at_ms, &probe_event);
    }
}

/// Decide whether a probe's `class` passes the operator-configured
/// filter set. Pure function — separable for unit testing and so
/// any future per-event optimization (lower-cased lookup, trie,
/// etc.) drops in without touching the Layer impl.
///
/// Rules (in priority order):
///
/// 1. **Empty filter set** = "no filter configured." Every class
///    passes. Used when `CONTINUUM_PROBE_CLASSES` is unset — full
///    capture.
/// 2. **`*` is in the set** = explicit "match every class" wildcard.
///    Different from rule 1 only in intent: `*` is the deliberate
///    capture-everything signal an operator types when they want
///    the firehose. Per `[[no-fallbacks-ever]]` the substrate
///    distinguishes "no filter" from "explicit `*`" so both are
///    honest decisions.
/// 3. **Exact or namespace prefix.** A filter `F` matches a class
///    `C` if `C == F` (exact) OR `C.starts_with(F.to_string() + ".")`
///    (namespace prefix — `persona` matches `persona.turn.spoke`
///    but NOT `personality.x`). Same convention as
///    `tracing_subscriber::EnvFilter`. Keeps comma-separated env
///    var values short — `CONTINUUM_PROBE_CLASSES=persona`
///    captures every `persona.*` class without enumerating.
pub(crate) fn class_passes_filter(class: &str, filter: &HashSet<String>) -> bool {
    if filter.is_empty() {
        return true;
    }
    if filter.contains("*") {
        return true;
    }
    filter.iter().any(|f| {
        class == f
            || (class.len() > f.len() + 1
                && class.starts_with(f)
                && class[f.len()..].starts_with('.'))
    })
}

/// Visitor that pulls `probe_class`, `message`, and every other
/// recorded field off a tracing event. Mirrors the private visitor
/// in `probe_router.rs` — kept local so this layer composes
/// independently.
#[derive(Default)]
struct FileSinkVisitor {
    probe_class: Option<String>,
    message: Option<String>,
    fields: std::collections::HashMap<String, String>,
}

impl FileSinkVisitor {
    fn record_field(&mut self, name: &str, value: String) {
        match name {
            "probe_class" => self.probe_class = Some(value),
            "message" => self.message = Some(value),
            _ => {
                self.fields.insert(name.to_string(), value);
            }
        }
    }
}

impl Visit for FileSinkVisitor {
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
mod tests;
