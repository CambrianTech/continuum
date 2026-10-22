#![cfg(test)]
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
        let _env = crate::test_env::HomeGuard::preserve_blocking(); // mutates CONTINUUM_PROBE_DIR/CLASSES, which the other env test also mutates (card cb3221a5)
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
            continuum_base::probe!(class = "persona.render.exit", persona = "Paige", "rendered");
            continuum_base::probe!(class = "cognition.analyze.cache_hit", model_used = "test");
        });

        let lines = read_jsonl(&path);
        assert_eq!(lines.len(), 2);

        let classes: Vec<&str> = lines.iter().map(|l| l["class"].as_str().unwrap()).collect();
        assert_eq!(
            classes,
            vec!["persona.render.exit", "cognition.analyze.cache_hit"]
        );

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
            continuum_base::probe!(class = "persona.turn.spoke", "kept");
            continuum_base::probe!(class = "persona.response.render.prompt", "kept");
            continuum_base::probe!(class = "personality.something", "dropped");
            continuum_base::probe!(class = "cognition.analyze.parse", "dropped");
        });

        let lines = read_jsonl(&path);
        assert_eq!(
            lines.len(),
            2,
            "namespace prefix must drop both non-matching classes"
        );
        let kept_classes: Vec<&str> = lines.iter().map(|l| l["class"].as_str().unwrap()).collect();
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
            continuum_base::probe!(class = "persona.turn.spoke", "kept");
            continuum_base::probe!(class = "cognition.analyze.parse", "kept");
            continuum_base::probe!(class = "timing", "kept");
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
            continuum_base::probe!(class = "cognition.analyze.parse", "kept exact");
            continuum_base::probe!(class = "cognition.analyze.cache_hit", "dropped");
            continuum_base::probe!(class = "persona.turn.spoke", "kept prefix");
            continuum_base::probe!(class = "persona.turn.start", "kept prefix");
            continuum_base::probe!(class = "persona.response.enter", "dropped");
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
            continuum_base::probe!(class = "persona.render.exit", "kept");
            continuum_base::probe!(class = "cognition.analyze.cache_hit", "dropped");
            continuum_base::probe!(class = "persona.render.exit", "also kept");
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
            continuum_base::probe!(class = "persona.render.exit", "this IS a probe");
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
            continuum_base::probe!(class = "persona.render.exit", "inside dispatch");
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
            let _result: i32 = continuum_base::time_sync!("test_phase", 21 * 2);
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
                runtime.block_on(async { continuum_base::time_probe!("async_test_phase", produces()) });
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
            let _result: i32 = continuum_base::time_sync!("composition_phase", 21 * 2);
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
            let _kept: i32 = continuum_base::time_sync!("dropped_phase", 1);
            continuum_base::probe!(class = "persona.render.exit", "kept");
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
            continuum_base::probe!(class = "rolling.test", "hello rolling");
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
