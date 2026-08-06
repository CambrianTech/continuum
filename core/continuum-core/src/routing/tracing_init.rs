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
//! 3. [`JsonlProbeFileSink`] — append-only JSONL log on disk, gated
//!    by `CONTINUUM_PROBE_DIR` env var. Optional — if the env var
//!    is unset the sink is silently skipped (the operator
//!    intentionally didn't ask). Path-open failures are LOUD per
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
    JsonlProbeFileSink, ProbeFileSinkError, DEFAULT_MAX_LOG_FILES, ENV_PROBE_CLASSES,
    ENV_PROBE_DIR,
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
    /// DIRECTORY the size-rotated JSONL probe stream drains to.
    /// `None` = no disk capture this run.
    ///
    /// A directory, not a file: the sink runs in rolling mode
    /// ([`JsonlProbeFileSink::new_rolling`]) and owns the file names
    /// inside it. This field used to be `probe_file` and fed the
    /// single-file [`JsonlProbeFileSink::new`], which meant the ONE
    /// env var `CONTINUUM_PROBE_DIR` had two contradictory readings:
    /// [`JsonlProbeFileSink::from_env`] took it as a directory and
    /// documented itself as "the ONLY env-based entry point —
    /// bounded disk usage", while THIS path — the one every server
    /// actually boots through — took it as a file and wrote
    /// unbounded. `from_env`/`new_rolling` had zero callers; the
    /// bounded path was built and never wired. Symptom: a peer's
    /// `probes.jsonl` at 86 MB and climbing, under a retention
    /// policy that was never in the boot path. Same failure the fmt
    /// layer above already learned the hard way — see the 172 GB
    /// note — one function further down the same file.
    pub probe_dir: Option<PathBuf>,
    /// Class filter passed to the sink. Empty
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
/// `config.probe_dir` was supplied but the path could not be
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
            // ROLLING, not single-file. The probe stream is this
            // file's own words the "HIGHEST-volume writer in the
            // substrate"; it gets the size-rotated appender for the
            // same reason the fmt log above does. `new()` (unbounded,
            // single file) survives only as the in-code forensic /
            // test constructor its own docs claim it is.
            let sink = JsonlProbeFileSink::new_rolling(dir, probe_classes, DEFAULT_MAX_LOG_FILES)?;
            Ok(Some(sink))
        }
        None => Ok(None),
    }
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

    /// Typed-error contract: an unwritable `probe_dir` path must
    /// surface `ProbeFileSinkError::OpenFailed` per
    /// `[[no-fallbacks-ever]]`. Operators must see the
    /// configuration problem; substrate refuses to silently drop
    /// probes.
    ///
    /// Parallel-safe because the bad path is passed via typed
    /// config — no env-var mutation, no racing on
    /// `CONTINUUM_PROBE_DIR`.
    ///
    /// The bad path is "a directory whose PARENT is an existing
    /// regular file" — `create_dir_all` refuses that on every
    /// platform. The previous literal `/this/path/definitely/does/
    /// not/exist/probes.jsonl` stopped being a negative the moment
    /// rolling mode started creating its directory: on Windows a
    /// leading-slash path is just `C:\this\path\...` and
    /// `create_dir_all` cheerfully succeeds, so the assertion would
    /// have passed on macOS/Linux and failed only on Joel's box.
    #[test]
    fn install_surfaces_open_failed_for_unwritable_path() {
        // A real file — any dir path underneath it is uncreatable.
        let blocker = std::env::temp_dir().join("continuum-probe-dir-blocker.tmp");
        std::fs::write(&blocker, b"not a directory").expect("temp file must be writable");
        let config = ProbeTracingConfig {
            probe_dir: Some(blocker.join("probes")),
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

    /// what this catches: the boot path taking `CONTINUUM_PROBE_DIR`
    /// as a FILE. Regression for 2026-08-06 — arming probes on
    /// BigMama with a directory (the name, and
    /// `JsonlProbeFileSink::from_env`, both say directory) refused
    /// the whole server boot with
    /// `OpenFailed { code: 5, PermissionDenied }`, because Windows
    /// returns access-denied when you open a directory as a file.
    /// The same defect on a peer's Mac was silent and simply wrote
    /// an unbounded 86 MB `probes.jsonl` under a retention policy
    /// that lived in a constructor with zero callers.
    ///
    /// An EXISTING directory must be accepted, and the rolling
    /// appender must own naming inside it.
    #[test]
    fn install_accepts_an_existing_directory_and_rolls_inside_it() {
        let dir = std::env::temp_dir().join("continuum-probe-rolling-accepts");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir must be creatable");

        let config = ProbeTracingConfig {
            probe_dir: Some(dir.clone()),
            probe_classes: HashSet::new(),
            default_filter: "warn".to_string(),
            log_dir: None,
        };
        match install_probe_tracing(config) {
            Ok(install) => assert_eq!(
                install.probe_log_path.as_deref(),
                Some(dir.as_path()),
                "rolling mode reports the DIRECTORY as its target"
            ),
            Err(e) => panic!("an existing directory must be accepted, got {e:?}"),
        }
        // The target stayed a directory — proof we did not go down
        // the single-file path that would have tried to open it.
        assert!(dir.is_dir(), "probe target must remain a directory");

        let _ = std::fs::remove_dir_all(&dir);
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
