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
//! - `CONTINUUM_PROBE_FILE=/path/to/probes.jsonl` — append-only file.
//!   Unset = sink absent. The directory must exist (sink errors on
//!   open if not; honest failure beats silent drop per
//!   `[[no-fallbacks-ever]]`).
//! - `CONTINUUM_PROBE_CLASSES=persona.render,persona.analyze,timing`
//!   — comma-separated allowed-classes filter. Empty / unset = ALL
//!   classes. Glob/regex deliberately NOT supported in this slice;
//!   exact-match keeps the filter cheap (single HashSet lookup per
//!   event) and the conventional class taxonomy in `persona::probes`
//!   gives operators a discoverable set to choose from.
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
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tracing::field::{Field, Visit};
use tracing::{span::Attributes, Event, Id, Subscriber};
use tracing_subscriber::{layer::Context, registry::LookupSpan, Layer};

use super::current_uri_chain;
use super::probe_router::ProbeEvent;

/// Env var carrying the JSONL file path. Unset = sink disabled.
pub const ENV_PROBE_FILE: &str = "CONTINUUM_PROBE_FILE";

/// Env var carrying the comma-separated class filter. Empty/unset =
/// all classes pass through.
pub const ENV_PROBE_CLASSES: &str = "CONTINUUM_PROBE_CLASSES";

/// JSONL-on-disk consumer for `probe!` events.
///
/// Composes with [`ProbeRouterLayer`](super::probe_router::ProbeRouterLayer)
/// — install both at the registry; the broadcast subscribers stay
/// in-process, this one persists to disk. Both visit the same
/// tracing event independently; neither blocks the other.
pub struct JsonlProbeFileSink {
    path: PathBuf,
    writer: Mutex<BufWriter<File>>,
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
    /// `CONTINUUM_PROBE_FILE` was not set. NOT an error per se —
    /// callers use this to detect "sink isn't configured, skip
    /// installation" and continue silently.
    EnvVarUnset,
    /// `CONTINUUM_PROBE_FILE` was set but the file couldn't be
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
                ENV_PROBE_FILE
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
    /// Construct a sink writing to `path`, filtered to `allowed_classes`.
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
            path,
            writer: Mutex::new(BufWriter::new(file)),
            allowed_classes,
        })
    }

    /// Construct from `CONTINUUM_PROBE_FILE` + `CONTINUUM_PROBE_CLASSES`
    /// env vars.
    ///
    /// Returns `Err(EnvVarUnset)` if `CONTINUUM_PROBE_FILE` is missing —
    /// callers treat that as "sink intentionally disabled, install no
    /// layer." Returns `Err(OpenFailed)` if the path is set but
    /// unwritable (operator must fix).
    pub fn from_env() -> Result<Self, ProbeFileSinkError> {
        let path = std::env::var(ENV_PROBE_FILE).map_err(|_| ProbeFileSinkError::EnvVarUnset)?;
        let allowed_classes = std::env::var(ENV_PROBE_CLASSES)
            .ok()
            .map(|s| {
                s.split(',')
                    .map(|c| c.trim().to_string())
                    .filter(|c| !c.is_empty())
                    .collect::<HashSet<_>>()
            })
            .unwrap_or_default();
        Self::new(path, allowed_classes)
    }

    /// The on-disk path the sink writes to. Useful for tests + boot
    /// logging ("probes landing at /tmp/probes.jsonl").
    pub fn path(&self) -> &Path {
        &self.path
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

/// Per-span data the sink parks in span extensions at
/// `on_new_span` and reads at `on_close` to build the timing
/// JSONL record.
///
/// Mirrors `ProbeRouterLayer::SpanProbeMeta` — duplicated locally
/// (rather than exported and shared) so the sink composes
/// independently per this file's header comment. The shape is
/// trivial; if a third consumer appears we hoist into
/// `routing/mod.rs`.
///
/// Task #196: this is the load-bearing piece that makes
/// `time_sync!` and `time_probe!` actually land in the on-disk
/// JTAG log. Before this, both macros emitted spans the sink
/// never observed — operators tailing `probes.jsonl` saw zero
/// timing records no matter how many `time_sync!` calls fired.
#[derive(Debug, Clone)]
struct FileSinkSpanMeta {
    probe_class: String,
    fields: std::collections::HashMap<String, String>,
    start: Instant,
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
        if !self.allowed_classes.is_empty() && !self.allowed_classes.contains(&class) {
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
    /// `probe_class` attribute. Stash a `FileSinkSpanMeta` so
    /// `on_close` can build the timing JSONL record.
    ///
    /// Spans without `probe_class` (plain `info_span!`,
    /// framework spans) get no extension stored — zero cost
    /// beyond visiting the attrs once.
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        let mut visitor = FileSinkVisitor::default();
        attrs.record(&mut visitor);
        let probe_class = match visitor.probe_class {
            Some(c) => c,
            None => return, // not a timing/probe span
        };
        let Some(span_ref) = ctx.span(id) else {
            return;
        };
        span_ref.extensions_mut().insert(FileSinkSpanMeta {
            probe_class,
            fields: visitor.fields,
            start: Instant::now(),
        });
    }

    /// Span closed — convert the parked `FileSinkSpanMeta` into a
    /// JSONL line on disk. Mirrors the router's `on_close`; the
    /// class filter runs the same check as `on_event` so an
    /// operator running `CONTINUUM_PROBE_CLASSES=timing` sees
    /// `time_sync!` / `time_probe!` durations land alongside
    /// event-shape probes.
    ///
    /// `duration_ms` is injected into `fields` before serialization
    /// so the on-disk record matches the broadcast `ProbeEvent`
    /// shape — same line whether you consume from
    /// `ProbeRouterLayer`'s subscriber or `tail -f`.
    fn on_close(&self, id: Id, ctx: Context<'_, S>) {
        let Some(span_ref) = ctx.span(&id) else {
            return;
        };
        let extensions = span_ref.extensions();
        let Some(meta) = extensions.get::<FileSinkSpanMeta>() else {
            return; // span didn't carry probe_class — not ours
        };

        // Class filter applies to timing spans just as it does to
        // event-shape probes; an operator filtering to
        // `persona.render.exit` shouldn't see timing noise.
        if !self.allowed_classes.is_empty() && !self.allowed_classes.contains(&meta.probe_class) {
            return;
        }

        let duration_ms = meta.start.elapsed().as_millis() as u64;
        let mut fields = meta.fields.clone();
        fields.insert("duration_ms".to_string(), duration_ms.to_string());

        let probe_event = ProbeEvent {
            class: meta.probe_class.clone(),
            uri_chain: current_uri_chain(),
            message: None, // spans don't carry the format-string `message`
            fields,
        };

        // Drop the extensions borrow before write_one — extensions
        // are RwLocked and write_one acquires the sink's writer
        // Mutex; releasing the read guard first keeps the lock
        // hierarchy clean (extensions → writer, never the reverse).
        drop(extensions);

        let captured_at_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        self.write_one(captured_at_ms, &probe_event);
    }
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
    fn from_env_returns_envvar_unset_when_path_missing() {
        // Unset both env vars (saved + restored to avoid bleeding
        // into other tests). The substrate refuses to silently
        // synthesize a default path — Joel's `[[no-fallbacks-ever]]`.
        let prev_file = std::env::var(ENV_PROBE_FILE).ok();
        let prev_classes = std::env::var(ENV_PROBE_CLASSES).ok();
        std::env::remove_var(ENV_PROBE_FILE);
        std::env::remove_var(ENV_PROBE_CLASSES);

        let result = JsonlProbeFileSink::from_env();
        assert!(matches!(result, Err(ProbeFileSinkError::EnvVarUnset)));

        if let Some(v) = prev_file {
            std::env::set_var(ENV_PROBE_FILE, v);
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
}
