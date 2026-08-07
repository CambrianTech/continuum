//! Substrate-wide tracing-subscriber boot composition.
//!
//! Every continuum binary (server, demos, tests-that-care-about-probes)
//! installs the SAME stack of tracing layers in the SAME order, via
//! [`install_probe_tracing`]. Doing it once-per-binary stops the
//! probe-availability accidentally diverging between
//! `continuum-core-server` and the binaries / tests an operator wants
//! to debug. Per Joel 2026-06-06: "Let's perfect debugging as we use
//! it" — a single composition function is the substrate-purist way to
//! keep the JTAG actually on across every entry point.
//!
//! ## What gets installed
//!
//! In order — each layer's docs explain what it does:
//!
//! 1. [`UriCaptureLayer`] — captures the URI ancestry chain at probe
//!    fire time so [`stack!`] returns a meaningful path. Required by
//!    both the broadcast router and the file sink.
//! 2. [`ProbeRouterLayer`] — fans every `probe!` event to per-class
//!    broadcast subscribers. The substrate's `debug/probes/*` URI
//!    consumers read from this.
//! 3. [`JsonlProbeFileSink`] — SIZE-ROTATED JSONL capture on disk,
//!    gated by the `CONTINUUM_PROBE_DIR` env var (a DIRECTORY: the
//!    sink owns the file name so it can rotate). Optional — if the
//!    env var is unset the sink is silently skipped (the operator
//!    intentionally didn't ask). Open failures are LOUD per
//!    `[[no-fallbacks-ever]]` — caller decides whether to surface or
//!    swallow the error.
//! 4. A `fmt` layer reading [`tracing_subscriber::EnvFilter`] from
//!    `RUST_LOG` (with a sensible default so the server still
//!    prints info-level lines when nobody set `RUST_LOG`). Writes
//!    to stderr so stdout stays clean for any caller piping the
//!    server's output.
//!
//! ## Why this lives in `routing`
//!
//! Probe routing IS the substrate's per-class fanout primitive. The
//! tracing-subscriber install is the operational shape that makes
//! the in-tree macros (`probe!`, `time_sync!`, `stack!`) actually
//! reach a consumer. Same module as `probe_router` + `probe_file_sink`
//! + `macros` keeps "the JTAG end-to-end" in one place — anyone
//! grepping for `JsonlProbeFileSink` lands next to the install
//! function.
//!
//! ## Idempotency
//!
//! `try_init()` succeeds the first time and silently no-ops every
//! subsequent call. Safe to call from a `#[tokio::main]` server
//! binary AND from a test harness without coordinating across
//! crates. Tests that want a NEW subscriber (clean capture) can
//! use `tracing::subscriber::with_default(subscriber, || ...)`
//! instead — that path doesn't touch the global default.

use std::collections::HashSet;
use std::path::PathBuf;

use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

use super::probe_file_sink::{
    JsonlProbeFileSink, ProbeFileSinkError, DEFAULT_MAX_LOG_FILES, ENV_PROBE_CLASSES, ENV_PROBE_DIR,
};
use super::probe_router::ProbeRouterLayer;
use super::uri_layer::UriCaptureLayer;

/// Typed boot configuration for `install_probe_tracing`.
///
/// All env-coupling lives in [`ProbeTracingConfig::from_env`] — the
/// library installer takes only typed values. Test code constructs
/// the config directly and passes a tmpdir-relative path; future
/// alternative sources (config file, CLI flags) become additional
/// constructors on this struct without rippling into the install
/// function.
///
/// Per Joel 2026-06-06: env vars are brittle (process-global
/// mutable state, racy under `cargo test`, slated for `unsafe`
/// marking in Rust 2024). The substrate-purist shape keeps env
/// coupling at exactly one seam.
#[derive(Debug, Clone, Default)]
pub struct ProbeTracingConfig {
    /// DIRECTORY the rotating JSONL probe capture writes into.
    /// `None` = no disk capture this run.
    ///
    /// A DIRECTORY, not a file — the sink owns the file name
    /// (`continuum-probes.jsonl`, with `.1`, `.2`, … generations
    /// beside it) precisely so it can rotate. This was `probe_file`
    /// and carried a file path, which is how the highest-volume
    /// writer in the substrate ended up on the unbounded
    /// single-file constructor while the fmt log two lines away was
    /// capped (#341 — 133 MB and climbing at ~32 MB/day, the same
    /// class as the log that once took this host to zero bytes).
    pub probe_dir: Option<PathBuf>,
    /// Class filter passed to [`JsonlProbeFileSink::new_rolling`]. Empty
    /// set = "no filter, every class passes" per the sink's
    /// [`class_passes_filter`](super::probe_file_sink) rules.
    pub probe_classes: HashSet<String>,
    /// `EnvFilter` directive applied to the fmt layer when
    /// `RUST_LOG` is unset. `"info"` for production servers; `"warn"`
    /// for noisy tests. Empty string falls back to `"info"`.
    pub default_filter: String,
    /// Directory the fmt layer's rolling log writer drains to. When
    /// `Some`, the substrate writes its tracing firehose to a
    /// `daily`-rotated file under this directory (max 7 files
    /// retained) — operator never has to redirect stderr, the
    /// substrate manages its own log persistence.
    ///
    /// When `None` (e.g. unit tests where an external subscriber
    /// already captures events), the fmt layer falls back to stderr.
    /// Production callers should always set this.
    ///
    /// Per `[[never-redirect-substrate-stderr]]` (Joel 2026-06-07):
    /// the operator-facing `> /tmp/server.log 2>&1` pattern ate a
    /// host's disk in minutes by capturing the full tracing firehose
    /// at `RUST_LOG=info`. Substrate owns log persistence; shell
    /// redirects are forbidden.
    pub log_dir: Option<PathBuf>,
}

impl ProbeTracingConfig {
    /// Read `CONTINUUM_PROBE_DIR` + `CONTINUUM_PROBE_CLASSES` env
    /// vars into a typed `ProbeTracingConfig`. This is THE seam
    /// where env coupling lives — every other path through the
    /// substrate constructs the config directly.
    ///
    /// `default_filter` is supplied by the caller because env vars
    /// don't carry it (`RUST_LOG` is already its own surface for
    /// the fmt layer; this is the fallback when `RUST_LOG` is
    /// unset).
    pub fn from_env(default_filter: &str) -> Self {
        let probe_dir = std::env::var(ENV_PROBE_DIR).ok().map(PathBuf::from);
        let probe_classes = std::env::var(ENV_PROBE_CLASSES)
            .ok()
            .map(|s| {
                s.split(',')
                    .map(|c| c.trim().to_string())
                    .filter(|c| !c.is_empty())
                    .collect::<HashSet<_>>()
            })
            .unwrap_or_default();
        // log_dir: `CONTINUUM_LOG_DIR` env var overrides; otherwise
        // default to `~/.continuum/logs/`. The substrate owning log
        // persistence is the doctrine — even on a fresh box where
        // the env var is unset, npm start writes to a managed file
        // not stderr-unbounded. Only when neither the env var is set
        // NOR a home directory resolvable (containerized envs with
        // HOME unset, certain CI shapes) does this remain `None` and
        // fmt falls back to stderr — that's an explicit "no managed
        // log dir for this run" outcome, not a silent default.
        let log_dir = std::env::var(ENV_LOG_DIR)
            .ok()
            .map(PathBuf::from)
            .or_else(|| dirs::home_dir().map(|h| h.join(".continuum").join("logs")));
        Self {
            probe_dir,
            probe_classes,
            default_filter: default_filter.to_string(),
            log_dir,
        }
    }
}

/// Env var name for the fmt-layer rolling-log directory. Operator
/// override of the `~/.continuum/logs/` default. Set this in
/// containerized environments where `$HOME` isn't the right place.
pub const ENV_LOG_DIR: &str = "CONTINUUM_LOG_DIR";

/// Result of the install — tells the caller whether disk capture is
/// active so it can be logged at boot ("probes landing at
/// /tmp/x.jsonl" is the kind of one-line confirmation that prevents
/// the "I set the env var, where are my probes" confusion).
pub struct ProbeInstall {
    /// Path the JSONL sink is writing to, if `CONTINUUM_PROBE_DIR`
    /// was set and the file opened successfully. `None` = no disk
    /// capture this run (env var unset, OR test path where a
    /// caller-provided subscriber is already installed).
    pub probe_log_path: Option<PathBuf>,
    /// Directory the fmt-layer rolling-log writer is draining to,
    /// when `config.log_dir` was supplied and writable. The substrate
    /// writes `continuum-core-server.log` there, rotated by SIZE via
    /// [`crate::routing::capped_appender`] — total on disk is capped
    /// arithmetically rather than by a retention count over
    /// unbounded-per-day files. `None` =
    /// fmt layer fell back to stderr (test path with no managed log
    /// dir). Use this to print a "logs landing at <path>" line at
    /// boot.
    pub log_dir: Option<PathBuf>,
    /// `WorkerGuard` for the non-blocking rolling-log writer. Must
    /// be held alive for the process lifetime — dropping it flushes
    /// + shuts down the background writer thread, so the caller
    /// (`main.rs`) stashes this in a `let _guard = ...;` binding at
    /// process scope. Dropping early loses tail-of-process log
    /// lines.
    ///
    /// `None` when fmt fell back to stderr (no rolling writer to
    /// keep alive). Field deliberately not `Debug` — `WorkerGuard`
    /// isn't Debug; the `ProbeInstall` derive is gone above.
    pub fmt_writer_guard: Option<tracing_appender::non_blocking::WorkerGuard>,
}

/// Install the substrate's canonical tracing stack on the GLOBAL
/// default subscriber, given a typed [`ProbeTracingConfig`].
/// Idempotent — calling it from multiple init sites is safe; the
/// second call no-ops via
/// `tracing_subscriber::util::SubscriberInitExt::try_init`.
///
/// The library function takes ONLY typed values — env-coupling
/// lives at [`ProbeTracingConfig::from_env`] and only there. This
/// keeps tests (which would otherwise race `std::env::set_var`
/// across threads) deterministic, and keeps every other call site
/// (config file, CLI flags, hardcoded defaults) on equal footing
/// with the env var path.
///
/// Returns a [`ProbeInstall`] describing what got wired so the
/// caller can log it visibly at boot — the operator-side proof
/// that the probes are landing somewhere.
///
/// ## Errors
///
/// Returns `Err(ProbeFileSinkError::OpenFailed)` if
/// `config.probe_dir` was supplied but the directory could not be
/// opened (directory missing, permission denied, etc). Per
/// `[[no-fallbacks-ever]]` the substrate refuses to silently drop
/// probes — the caller can choose to surface the error
/// (recommended for servers) or swallow it (acceptable for ad-hoc
/// tests where the probe path is best-effort).
///
/// Does NOT return an error when `config.probe_dir` is `None` —
/// that's an intentional "no disk capture this run", not a
/// configuration failure.
pub fn install_probe_tracing(
    config: ProbeTracingConfig,
) -> Result<ProbeInstall, ProbeFileSinkError> {
    let default_filter = if config.default_filter.is_empty() {
        "info"
    } else {
        &config.default_filter
    };
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_filter));

    // Build the fmt-layer writer. When the operator supplied a
    // log_dir (production path: `~/.continuum/logs/` from
    // `ProbeTracingConfig::from_env`), drain through
    // [`CappedAppender`] + `non_blocking`. The `WorkerGuard` MUST be
    // held alive by the caller for the process lifetime; we hand it
    // back in `ProbeInstall`.
    //
    // This used to be `tracing_appender::rolling::DAILY` with
    // `max_log_files(7)`, described here as "disk usage stays
    // bounded". It was not: 7 files times WHATEVER ONE DAY PRODUCES,
    // with nothing constraining the second factor. In practice these
    // logs ran 87–175 MB each (~600 MB/week) against Joel's stated
    // rule of a few MB per file; under the 2026-08-05 wedge, which
    // emitted 1.2 GB/minute, a clock-based rotation would have
    // produced a 172 GB file and rotated it exactly on schedule.
    // Rotating on SIZE is the only form of this that is a bound.
    // See [[one-log-file-reached-172gb-and-took-the-whole-machine-to-zero-bytes]].
    //
    // When log_dir is None (test path), fall back to stderr — the
    // test harness already captures stderr through `cargo test`
    // shape, no rolling-file needed.
    //
    // Per `[[never-redirect-substrate-stderr]]`: the stderr path
    // exists ONLY for tests + explicit operator opt-out. Production
    // boots should always have `log_dir = Some(...)` set so the
    // substrate manages its own log persistence and any operator
    // shell redirect captures an empty stream.
    let (log_dir_out, fmt_writer_guard) = match config.log_dir.as_ref() {
        Some(dir) => {
            // Refuse to silently fall back if the configured dir
            // can't be created. `[[no-fallbacks-ever]]`: surface a
            // typed error so the operator gets the exact path that
            // failed.
            std::fs::create_dir_all(dir).map_err(|e| ProbeFileSinkError::OpenFailed {
                path: dir.clone(),
                source: e,
            })?;
            let file_appender =
                crate::routing::capped_appender::CappedAppender::new(dir, "continuum-core-server.log")
                    .map_err(|source| ProbeFileSinkError::OpenFailed {
                        path: dir.clone(),
                        source,
                    })?;
            let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
            let fmt_layer = tracing_subscriber::fmt::layer().with_writer(non_blocking);
            let probe_file_sink = build_probe_file_sink(&config.probe_dir, config.probe_classes.clone())?;
            let registry = tracing_subscriber::registry()
                .with(UriCaptureLayer::new())
                .with(ProbeRouterLayer::new())
                .with(probe_file_sink)
                .with(fmt_layer.with_filter(env_filter));
            let _ = registry.try_init();
            (Some(dir.clone()), Some(guard))
        }
        None => {
            let fmt_layer = tracing_subscriber::fmt::layer().with_writer(std::io::stderr);
            let probe_file_sink = build_probe_file_sink(&config.probe_dir, config.probe_classes.clone())?;
            let registry = tracing_subscriber::registry()
                .with(UriCaptureLayer::new())
                .with(ProbeRouterLayer::new())
                .with(probe_file_sink)
                .with(fmt_layer.with_filter(env_filter));
            let _ = registry.try_init();
            (None, None)
        }
    };

    let probe_log_path = config.probe_dir;

    Ok(ProbeInstall {
        probe_log_path,
        log_dir: log_dir_out,
        fmt_writer_guard,
    })
}

/// Shared helper — both fmt-writer branches need the same probe
/// file sink built up. Pulled out so the registry-build sites stay
/// readable and so a future test can swap the sink construction
/// independently.
fn build_probe_file_sink(
    probe_dir: &Option<PathBuf>,
    probe_classes: HashSet<String>,
) -> Result<Option<JsonlProbeFileSink>, ProbeFileSinkError> {
    match probe_dir.as_ref() {
        Some(dir) => {
            // ROLLING, not single-file. `JsonlProbeFileSink::new` is append-only and grows
            // without bound; its own docs say "use `new_rolling` for production captures",
            // and `from_env` on the sink is documented as "the ONLY env-based entry point —
            // rolling mode, bounded disk usage". That entry point had ZERO production
            // callers: boot came through here, and here called `new`. So the bounded path
            // was built, correct, tested, and reached by nothing (#341, the [[green-by-
            // every-check-is-not-evidence-of-reachability]] shape) while the probe stream —
            // one JSON line per load-bearing decision across every tokio task, the highest-
            // volume writer in the substrate — accumulated 133 MB at ~32 MB/day. The fmt
            // log built ten lines above this already used `CappedAppender`; the two writers
            // sat side by side with opposite bounding.
            let sink = JsonlProbeFileSink::new_rolling(dir, probe_classes, DEFAULT_MAX_LOG_FILES)
                .map_err(|e| explain_if_dir_is_actually_a_file(e, dir))?;
            Ok(Some(sink))
        }
        None => Ok(None),
    }
}

/// Turn the raw OS error from pointing `CONTINUUM_PROBE_DIR` at an existing FILE into one
/// that names the fix.
///
/// The var has always been called `..._DIR`, but it held a file path on this host for
/// months (`~/.continuum/probes/probes.jsonl`) because the old single-file constructor
/// accepted one. Anyone upgrading past this commit with that value still in `config.env`
/// gets `create_dir_all` failing with a bare `NotADirectory`, which says nothing about
/// what to change. Fail loud AND legible: the substrate refuses to start rather than drop
/// probes ([[fallbacks-are-illegal-fail-loud]]), so the message must carry the remedy.
fn explain_if_dir_is_actually_a_file(err: ProbeFileSinkError, dir: &std::path::Path) -> ProbeFileSinkError {
    if dir.is_file() {
        if let Some(parent) = dir.parent() {
            tracing::error!(
                configured = %dir.display(),
                should_be = %parent.display(),
                "CONTINUUM_PROBE_DIR points at a FILE, but it names a DIRECTORY the rotating \
                 probe sink writes continuum-probes.jsonl into. Set it to the containing \
                 directory; the existing file can stay where it is as a historical capture."
            );
        }
    }
    err
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `install_probe_tracing` accepts a typed config and is
    /// parallel-safe in tests — no env-var mutation, no shared
    /// global state beyond `tracing`'s own default subscriber
    /// (which is exactly the thing we're testing the idempotency
    /// of).
    #[test]
    fn install_is_idempotent_with_no_disk_capture() {
        let config = ProbeTracingConfig {
            probe_dir: None,
            probe_classes: HashSet::new(),
            default_filter: "warn".to_string(),
            log_dir: None,
        };
        let first = install_probe_tracing(config.clone());
        let second = install_probe_tracing(config);
        assert!(first.is_ok(), "first install must succeed");
        assert!(second.is_ok(), "second install must no-op cleanly");
        assert!(first.unwrap().probe_log_path.is_none());
        assert!(second.unwrap().probe_log_path.is_none());
    }

    /// what this catches: THE REGRESSION THIS TEST EXISTS FOR — boot silently taking the
    /// UNBOUNDED single-file constructor. `JsonlProbeFileSink::from_env` is documented as
    /// "the ONLY env-based entry point — rolling mode, bounded disk usage" and had zero
    /// production callers; boot came through `build_probe_file_sink`, which called `new`.
    /// The result was 133 MB of probes at ~32 MB/day, growing beside an fmt log that WAS
    /// capped (#341, the 172 GB class).
    ///
    /// Asserted at the CONSUMER's boundary — the name that appears on disk — not by
    /// checking which constructor was called. `continuum-probes.jsonl` is produced only by
    /// the rolling path (`new` writes to the literal path it is handed), so the file name
    /// IS the proof of which branch ran
    /// ([[a-test-that-asserts-a-field-fires-does-not-assert-it-arrives]]). Reverting this
    /// to `new` puts the file at `<dir>` itself and the assert goes red.
    #[test]
    fn disk_capture_lands_on_the_ROTATING_sink_not_the_unbounded_one() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let sink = build_probe_file_sink(
            &Some(dir.path().to_path_buf()),
            HashSet::new(),
        )
        .expect("rolling sink builds")
        .expect("a dir was supplied, so a sink must exist");
        drop(sink);
        let rolling_file = dir.path().join("continuum-probes.jsonl");
        assert!(
            rolling_file.exists(),
            "rolling sink must own the file name so it can rotate; found {:?}",
            std::fs::read_dir(dir.path())
                .map(|d| d.filter_map(|e| e.ok()).map(|e| e.file_name()).collect::<Vec<_>>())
        );
    }

    /// Typed-error contract: an unwritable `probe_dir` path must
    /// surface `ProbeFileSinkError::OpenFailed` per
    /// `[[no-fallbacks-ever]]`. Operators must see the
    /// configuration problem; substrate refuses to silently drop
    /// probes.
    ///
    /// Parallel-safe because the bad path is passed via typed
    /// config — no env-var mutation, no racing on
    /// `CONTINUUM_PROBE_DIR`.
    #[test]
    fn install_surfaces_open_failed_for_unwritable_path() {
        let config = ProbeTracingConfig {
            probe_dir: Some(PathBuf::from(
                "/this/path/definitely/does/not/exist/probes.jsonl",
            )),
            probe_classes: HashSet::new(),
            default_filter: "warn".to_string(),
            log_dir: None,
        };
        let result = install_probe_tracing(config);
        // Can't `{:?}` the Ok branch — ProbeInstall holds a
        // WorkerGuard which isn't Debug. Match the variant
        // explicitly instead so the assertion error is still
        // informative.
        match &result {
            Err(ProbeFileSinkError::OpenFailed { .. }) => {}
            Err(other) => panic!("expected OpenFailed, got error {other:?}"),
            Ok(_) => panic!("expected Err, got Ok(_)"),
        }
    }

    /// The env-coupling seam. `from_env` is the ONE function that
    /// touches `std::env`; everything downstream takes typed
    /// values. Pin both the populated and empty paths so a future
    /// refactor of the env names can't silently break the
    /// operator-facing surface.
    ///
    /// Serial within itself (env-var mutation needs sequencing) —
    /// scoped to ONE test for the env constructor specifically.
    /// Other tests in this module use direct construction and stay
    /// parallel-safe.
    #[test]
    fn from_env_reads_documented_env_vars() {
        let prev_file = std::env::var(ENV_PROBE_DIR).ok();
        let prev_classes = std::env::var(ENV_PROBE_CLASSES).ok();

        // Both vars set → populated config.
        std::env::set_var(ENV_PROBE_DIR, "/tmp/test-probes-dir");
        std::env::set_var(ENV_PROBE_CLASSES, "persona,cognition.analyze");
        let populated = ProbeTracingConfig::from_env("info");
        assert_eq!(
            populated.probe_dir.as_deref(),
            Some(std::path::Path::new("/tmp/test-probes-dir"))
        );
        assert!(populated.probe_classes.contains("persona"));
        assert!(populated.probe_classes.contains("cognition.analyze"));
        assert_eq!(populated.default_filter, "info");

        // Both vars unset → empty config (NOT an error).
        std::env::remove_var(ENV_PROBE_DIR);
        std::env::remove_var(ENV_PROBE_CLASSES);
        let empty = ProbeTracingConfig::from_env("warn");
        assert!(empty.probe_dir.is_none());
        assert!(empty.probe_classes.is_empty());
        assert_eq!(empty.default_filter, "warn");
        // log_dir defaults to `~/.continuum/logs/` when neither
        // `CONTINUUM_LOG_DIR` nor `dirs::home_dir()` failure forces
        // None. CI runners always have HOME; the only None case is
        // truly homeless environments. Pin the default so future
        // refactors can't silently move logs back to stderr-default.
        let log_dir = empty
            .log_dir
            .as_deref()
            .expect("CI/dev environments have HOME — default must resolve");
        assert!(
            log_dir.ends_with(std::path::PathBuf::from(".continuum/logs")),
            "default log_dir should end with .continuum/logs, got {}",
            log_dir.display()
        );

        // Restore prior env state so other tests aren't affected
        // by ordering even if cargo's parallel runner interleaves.
        match prev_file {
            Some(v) => std::env::set_var(ENV_PROBE_DIR, v),
            None => std::env::remove_var(ENV_PROBE_DIR),
        }
        match prev_classes {
            Some(v) => std::env::set_var(ENV_PROBE_CLASSES, v),
            None => std::env::remove_var(ENV_PROBE_CLASSES),
        }
    }
}
