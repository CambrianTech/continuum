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
use std::time::{SystemTime, UNIX_EPOCH};

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
            "continuum-probes.jsonl",
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

        let captured_at_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

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
        if !self.allowed_classes.is_empty()
            && !self.allowed_classes.contains(&probe_event.class)
        {
            return;
        }

        let captured_at_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

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
        class == f || (class.len() > f.len() + 1 && class.starts_with(f) && class[f.len()..].starts_with('.'))
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
mod tests {
    use super::*;
    use serde_json::Value;
    use std::fs::File;
    use std::io::Read;
    use tempfile::tempdir;
    use tracing_subscriber::prelude::*;

    /// Helper: read every JSONL line from the sink path back as
    /// parsed JSON Values. Lets tests assert on field presence /
    /// content without re-implementing the on-disk shape.
    fn read_jsonl(path: &Path) -> Vec<Value> {
        let mut content = String::new();
        File::open(path)
            .unwrap()
            .read_to_string(&mut content)
            .unwrap();
        content
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    #[test]
    fn from_env_returns_envvar_unset_when_dir_missing() {
        // Unset both env vars (saved + restored to avoid bleeding
        // into other tests). The substrate refuses to silently
        // synthesize a default dir — Joel's `[[no-fallbacks-ever]]`.
        let prev_dir = std::env::var(ENV_PROBE_DIR).ok();
        let prev_classes = std::env::var(ENV_PROBE_CLASSES).ok();
        std::env::remove_var(ENV_PROBE_DIR);
        std::env::remove_var(ENV_PROBE_CLASSES);

        let result = JsonlProbeFileSink::from_env();
        assert!(matches!(result, Err(ProbeFileSinkError::EnvVarUnset)));

        if let Some(v) = prev_dir {
            std::env::set_var(ENV_PROBE_DIR, v);
        }
        if let Some(v) = prev_classes {
            std::env::set_var(ENV_PROBE_CLASSES, v);
        }
    }

    #[test]
    fn unfiltered_sink_persists_every_class() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("probes.jsonl");

        let sink = JsonlProbeFileSink::new(&path, HashSet::new()).unwrap();
        let subscriber = tracing_subscriber::registry()
            .with(crate::routing::UriCaptureLayer::new())
            .with(sink);

        tracing::subscriber::with_default(subscriber, || {
            crate::probe!(class = "persona.render.exit", persona = "Paige", "rendered");
            crate::probe!(class = "cognition.analyze.cache_hit", model_used = "test");
        });

        let lines = read_jsonl(&path);
        assert_eq!(lines.len(), 2);

        let classes: Vec<&str> = lines.iter().map(|l| l["class"].as_str().unwrap()).collect();
        assert_eq!(classes, vec!["persona.render.exit", "cognition.analyze.cache_hit"]);

        // The first line should preserve the message + fields the
        // probe! call carried, so an operator reading the log can
        // reconstruct what the breakpoint saw.
        assert_eq!(lines[0]["message"], "rendered");
        assert_eq!(lines[0]["fields"]["persona"], "Paige");
        assert!(lines[0]["captured_at_ms"].as_u64().unwrap() > 0);
    }

    #[test]
    fn class_filter_namespace_prefix_matches_subclasses() {
        // Prefix `persona` should match every persona.* class but
        // NOT `personality.foo` (the dot guard prevents partial-
        // word collisions). Per the docstring's rule 3.
        let dir = tempdir().unwrap();
        let path = dir.path().join("probes.jsonl");

        let mut allowed = HashSet::new();
        allowed.insert("persona".to_string());

        let sink = JsonlProbeFileSink::new(&path, allowed).unwrap();
        let subscriber = tracing_subscriber::registry()
            .with(crate::routing::UriCaptureLayer::new())
            .with(sink);

        tracing::subscriber::with_default(subscriber, || {
            crate::probe!(class = "persona.turn.spoke", "kept");
            crate::probe!(class = "persona.response.render.prompt", "kept");
            crate::probe!(class = "personality.something", "dropped");
            crate::probe!(class = "cognition.analyze.parse", "dropped");
        });

        let lines = read_jsonl(&path);
        assert_eq!(lines.len(), 2, "namespace prefix must drop both non-matching classes");
        let kept_classes: Vec<&str> =
            lines.iter().map(|l| l["class"].as_str().unwrap()).collect();
        assert!(kept_classes.contains(&"persona.turn.spoke"));
        assert!(kept_classes.contains(&"persona.response.render.prompt"));
    }

    #[test]
    fn class_filter_wildcard_matches_every_class() {
        // `*` is the EXPLICIT capture-everything filter — different
        // intent from "empty set means no filter" per the
        // [[no-fallbacks-ever]] doctrine, but operationally equivalent
        // (every class passes). Used when an operator types
        // `CONTINUUM_PROBE_CLASSES=*` to deliberately request the
        // firehose.
        let dir = tempdir().unwrap();
        let path = dir.path().join("probes.jsonl");

        let mut allowed = HashSet::new();
        allowed.insert("*".to_string());

        let sink = JsonlProbeFileSink::new(&path, allowed).unwrap();
        let subscriber = tracing_subscriber::registry()
            .with(crate::routing::UriCaptureLayer::new())
            .with(sink);

        tracing::subscriber::with_default(subscriber, || {
            crate::probe!(class = "persona.turn.spoke", "kept");
            crate::probe!(class = "cognition.analyze.parse", "kept");
            crate::probe!(class = "timing", "kept");
        });

        let lines = read_jsonl(&path);
        assert_eq!(lines.len(), 3, "wildcard must capture every class");
    }

    #[test]
    fn class_filter_combines_exact_and_prefix_in_one_set() {
        // Operator's typical pattern: one specific class for a
        // hard-to-find probe + a whole namespace for the broad
        // picture. The filter must support both shapes in the same
        // HashSet without rule contention.
        let dir = tempdir().unwrap();
        let path = dir.path().join("probes.jsonl");

        let mut allowed = HashSet::new();
        allowed.insert("cognition.analyze.parse".to_string()); // exact
        allowed.insert("persona.turn".to_string()); // namespace

        let sink = JsonlProbeFileSink::new(&path, allowed).unwrap();
        let subscriber = tracing_subscriber::registry()
            .with(crate::routing::UriCaptureLayer::new())
            .with(sink);

        tracing::subscriber::with_default(subscriber, || {
            crate::probe!(class = "cognition.analyze.parse", "kept exact");
            crate::probe!(class = "cognition.analyze.cache_hit", "dropped");
            crate::probe!(class = "persona.turn.spoke", "kept prefix");
            crate::probe!(class = "persona.turn.start", "kept prefix");
            crate::probe!(class = "persona.response.enter", "dropped");
        });

        let lines = read_jsonl(&path);
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn class_passes_filter_pure_function_unit_tests() {
        // Pure-function spec for the helper. Locks the rules from the
        // docstring so future refactors of the per-event Layer code
        // can't drift the matching contract.
        use super::class_passes_filter;

        // Rule 1: empty filter = all pass
        let empty = HashSet::new();
        assert!(class_passes_filter("persona.turn.spoke", &empty));
        assert!(class_passes_filter("anything.at.all", &empty));

        // Rule 2: wildcard = all pass
        let mut wild = HashSet::new();
        wild.insert("*".to_string());
        assert!(class_passes_filter("persona.turn.spoke", &wild));
        assert!(class_passes_filter("cognition.analyze.parse", &wild));

        // Rule 3a: exact match
        let mut exact = HashSet::new();
        exact.insert("persona.turn.spoke".to_string());
        assert!(class_passes_filter("persona.turn.spoke", &exact));
        assert!(!class_passes_filter("persona.turn.silent", &exact));

        // Rule 3b: namespace prefix with dot guard
        let mut ns = HashSet::new();
        ns.insert("persona".to_string());
        assert!(class_passes_filter("persona.turn.spoke", &ns));
        assert!(class_passes_filter("persona.response.enter", &ns));
        assert!(!class_passes_filter("personality.foo", &ns));
        assert!(!class_passes_filter("cognition.analyze.parse", &ns));
        // Exact match against the prefix itself (a probe with
        // class="persona") also matches.
        assert!(class_passes_filter("persona", &ns));
    }

    #[test]
    fn class_filter_drops_unallowed_classes() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("probes.jsonl");

        let mut allowed = HashSet::new();
        allowed.insert("persona.render.exit".to_string());

        let sink = JsonlProbeFileSink::new(&path, allowed).unwrap();
        let subscriber = tracing_subscriber::registry()
            .with(crate::routing::UriCaptureLayer::new())
            .with(sink);

        tracing::subscriber::with_default(subscriber, || {
            crate::probe!(class = "persona.render.exit", "kept");
            crate::probe!(class = "cognition.analyze.cache_hit", "dropped");
            crate::probe!(class = "persona.render.exit", "also kept");
        });

        let lines = read_jsonl(&path);
        assert_eq!(lines.len(), 2, "filter must drop the cache_hit line");
        assert!(lines.iter().all(|l| l["class"] == "persona.render.exit"));
    }

    #[test]
    fn non_probe_tracing_events_are_ignored() {
        // tracing::info!() without a `probe_class` field MUST NOT
        // produce a JSONL line — the sink is a probe-specific
        // consumer, not a generic tracing forwarder. Keeps the
        // probe log signal-rich.
        let dir = tempdir().unwrap();
        let path = dir.path().join("probes.jsonl");

        let sink = JsonlProbeFileSink::new(&path, HashSet::new()).unwrap();
        let subscriber = tracing_subscriber::registry()
            .with(crate::routing::UriCaptureLayer::new())
            .with(sink);

        tracing::subscriber::with_default(subscriber, || {
            tracing::info!("this is a normal log line, not a probe");
            crate::probe!(class = "persona.render.exit", "this IS a probe");
        });

        let lines = read_jsonl(&path);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0]["class"], "persona.render.exit");
    }

    #[test]
    fn sink_captures_uri_chain_when_dispatched_span_active() {
        // The breakpoint mental model: when a probe fires inside a
        // dispatched URI, the on-disk record carries the URI ancestry
        // so operators can trace "this probe fired during THIS
        // command's execution." Mirrors the
        // `probe_event_carries_uri_chain` test in probe_router.
        let dir = tempdir().unwrap();
        let path = dir.path().join("probes.jsonl");

        let sink = JsonlProbeFileSink::new(&path, HashSet::new()).unwrap();
        let subscriber = tracing_subscriber::registry()
            .with(crate::routing::UriCaptureLayer::new())
            .with(sink);

        tracing::subscriber::with_default(subscriber, || {
            let span = tracing::info_span!("cmd", uri = "airc:///cognition/respond");
            let _enter = span.enter();
            crate::probe!(class = "persona.render.exit", "inside dispatch");
        });

        let lines = read_jsonl(&path);
        assert_eq!(lines.len(), 1);
        let chain = lines[0]["uri_chain"].as_array().unwrap();
        assert_eq!(chain.len(), 1);
        assert_eq!(chain[0], "airc:///cognition/respond");
    }

    /// Task #196: `time_sync!` spans must persist as JSONL timing
    /// records when the span closes. Before this fix the sink only
    /// implemented `on_event`, so the entire `time_sync!` /
    /// `time_probe!` macro family was theatrical on disk —
    /// operators tailing `probes.jsonl` saw zero timing records.
    #[test]
    fn time_sync_span_close_persists_timing_to_jsonl() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("probes.jsonl");

        let sink = JsonlProbeFileSink::new(&path, HashSet::new()).unwrap();
        let subscriber = tracing_subscriber::registry()
            .with(crate::routing::UriCaptureLayer::new())
            .with(sink);

        tracing::subscriber::with_default(subscriber, || {
            // Scope so the span is fully dropped (closes) before
            // we read the file.
            let _result: i32 = crate::time_sync!("test_phase", 21 * 2);
        });

        let lines = read_jsonl(&path);
        assert_eq!(lines.len(), 1, "exactly one timing record expected");
        assert_eq!(lines[0]["class"], "timing");
        assert_eq!(lines[0]["fields"]["seam"], "test_phase");
        assert!(
            lines[0]["fields"]["duration_ms"].is_string(),
            "duration_ms must be stringified into fields per on-disk shape"
        );
    }

    /// Same as the sync test but for `time_probe!` (async). Uses
    /// the current-thread tokio runtime so the per-thread subscriber
    /// from `with_default` covers the future's polls.
    #[test]
    fn time_probe_span_close_persists_timing_to_jsonl() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("probes.jsonl");

        let sink = JsonlProbeFileSink::new(&path, HashSet::new()).unwrap();
        let subscriber = tracing_subscriber::registry()
            .with(crate::routing::UriCaptureLayer::new())
            .with(sink);

        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("current-thread runtime");

        async fn produces() -> i32 {
            42
        }

        tracing::subscriber::with_default(subscriber, || {
            let _result: i32 =
                runtime.block_on(async { crate::time_probe!("async_test_phase", produces()) });
        });

        let lines = read_jsonl(&path);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0]["class"], "timing");
        assert_eq!(lines[0]["fields"]["seam"], "async_test_phase");
        assert!(lines[0]["fields"]["duration_ms"].is_string());
    }

    /// Plain `info_span!` calls (no `probe_class`) must NOT produce
    /// JSONL timing records — only `time_sync!` / `time_probe!`
    /// spans (which carry `probe_class = "timing"`) count. Pins the
    /// `[[no-fallbacks-ever]]` doctrine to the on-disk layer.
    #[test]
    fn plain_span_close_does_not_persist_to_jsonl() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("probes.jsonl");

        let sink = JsonlProbeFileSink::new(&path, HashSet::new()).unwrap();
        let subscriber = tracing_subscriber::registry()
            .with(crate::routing::UriCaptureLayer::new())
            .with(sink);

        tracing::subscriber::with_default(subscriber, || {
            let span = tracing::info_span!("plain", some_field = "value");
            let _enter = span.enter();
            drop(_enter);
            drop(span);
        });

        let lines = read_jsonl(&path);
        assert!(
            lines.is_empty(),
            "non-timing spans must not produce JSONL records: {lines:?}"
        );
    }

    /// Composition test (R1 of PR #1541 review): install BOTH
    /// `ProbeRouterLayer` and `JsonlProbeFileSink` in one
    /// subscriber. Fire a `time_sync!`. Assert:
    ///
    /// 1. The router's broadcast subscriber receives a timing
    ///    event with `class = "timing"` and a `seam` field.
    /// 2. The JSONL file persists ONE line with `class = "timing"`
    ///    and the same `seam` field value.
    /// 3. The `duration_ms` reported by both Layers is IDENTICAL
    ///    (load-bearing — proves both Layers read from the same
    ///    `start: Instant` parked by `ensure_probe_meta`, so
    ///    operators can correlate broadcast events with on-disk
    ///    lines exactly).
    ///
    /// Before the shared `probe_span_meta` module each Layer
    /// captured its own `Instant::now()` and the two
    /// `duration_ms` values disagreed by nanoseconds-to-
    /// microseconds. This test pins the invariant going forward.
    #[test]
    fn both_layers_in_one_subscriber_agree_on_duration_ms() {
        use crate::routing::ProbeRouterLayer;

        let dir = tempdir().unwrap();
        let path = dir.path().join("probes.jsonl");

        let sink = JsonlProbeFileSink::new(&path, HashSet::new()).unwrap();
        let router = ProbeRouterLayer::new();
        let mut rx = router.subscribe("timing");

        let subscriber = tracing_subscriber::registry()
            .with(crate::routing::UriCaptureLayer::new())
            .with(router)
            .with(sink);

        tracing::subscriber::with_default(subscriber, || {
            let _result: i32 = crate::time_sync!("composition_phase", 21 * 2);
        });

        // Router side
        let broadcast_event = rx
            .try_recv()
            .expect("subscriber must receive the timing event");
        assert_eq!(broadcast_event.class, "timing");
        assert_eq!(
            broadcast_event.fields.get("seam").map(String::as_str),
            Some("composition_phase")
        );
        let broadcast_duration_ms = broadcast_event
            .fields
            .get("duration_ms")
            .expect("broadcast event must carry duration_ms")
            .clone();

        // Sink side
        let lines = read_jsonl(&path);
        assert_eq!(lines.len(), 1, "exactly one timing line on disk");
        assert_eq!(lines[0]["class"], "timing");
        assert_eq!(lines[0]["fields"]["seam"], "composition_phase");
        let jsonl_duration_ms = lines[0]["fields"]["duration_ms"]
            .as_str()
            .expect("JSONL fields.duration_ms must be a string")
            .to_string();

        // The load-bearing claim — same `Instant::now()` was
        // observed by both Layers.
        assert_eq!(
            broadcast_duration_ms, jsonl_duration_ms,
            "router subscriber and JSONL sink must agree on duration_ms \
             (proves both read the shared `SpanProbeMeta.start` instead \
             of each capturing their own `Instant::now()`)"
        );
    }

    /// The class filter applies to timing spans just as it does to
    /// event-shape probes. An operator running
    /// `CONTINUUM_PROBE_CLASSES=persona.render.exit` should NOT see
    /// stray `timing` lines in the log.
    #[test]
    fn class_filter_applies_to_timing_spans() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("probes.jsonl");

        let mut allowed = HashSet::new();
        allowed.insert("persona.render.exit".to_string());

        let sink = JsonlProbeFileSink::new(&path, allowed).unwrap();
        let subscriber = tracing_subscriber::registry()
            .with(crate::routing::UriCaptureLayer::new())
            .with(sink);

        tracing::subscriber::with_default(subscriber, || {
            let _kept: i32 = crate::time_sync!("dropped_phase", 1);
            crate::probe!(class = "persona.render.exit", "kept");
        });

        let lines = read_jsonl(&path);
        assert_eq!(lines.len(), 1, "timing line must be filtered out");
        assert_eq!(lines[0]["class"], "persona.render.exit");
    }

    /// What this catches: rolling mode writes events into a dated file
    /// inside the target directory, and the public surface (`target()`)
    /// reports the directory not a file. Walks the full lifecycle in
    /// one test per the "less tests with more coverage" doctrine —
    /// construction, install, fire event, observe file on disk.
    ///
    /// Regression here = the structural auto-clean we shipped to close
    /// the disk-bomb gap (per
    /// `[[auto-clean-is-structural-not-operational]]`) silently falls
    /// back to single-file unbounded growth.
    #[test]
    fn new_rolling_writes_to_dated_file_in_target_dir() {
        let dir = tempdir().unwrap();
        let sink = JsonlProbeFileSink::new_rolling(dir.path(), HashSet::new(), 7).unwrap();
        // target() reports the DIRECTORY in rolling mode, not a file.
        assert_eq!(sink.target(), dir.path());

        let subscriber = tracing_subscriber::registry().with(sink);
        tracing::subscriber::with_default(subscriber, || {
            crate::probe!(class = "rolling.test", "hello rolling");
        });

        // Find the rolled file. tracing_appender names the file
        // `continuum-probes.YYYY-MM-DD` for daily rotation; just
        // assert SOMETHING got written with the right prefix, since
        // the test can't know the exact date stamp without re-deriving
        // tracing_appender's internal naming.
        let mut found = None;
        for entry in std::fs::read_dir(dir.path()).unwrap() {
            let entry = entry.unwrap();
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("continuum-probes") {
                found = Some(entry.path());
                break;
            }
        }
        let rolled_path = found.expect("rolling sink must write a continuum-probes.* file");
        let mut content = String::new();
        File::open(&rolled_path)
            .unwrap()
            .read_to_string(&mut content)
            .unwrap();
        // The line is async via tracing_appender's queue — give it a
        // moment if necessary. In practice the BufWriter flush per
        // line happens synchronously here.
        assert!(
            content.contains("\"class\":\"rolling.test\""),
            "expected probe class in rolled file, got: {content:?}"
        );
    }
}
