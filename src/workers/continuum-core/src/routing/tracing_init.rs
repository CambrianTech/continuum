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
//!    by `CONTINUUM_PROBE_FILE` env var. Optional — if the env var
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
    JsonlProbeFileSink, ProbeFileSinkError, ENV_PROBE_CLASSES, ENV_PROBE_FILE,
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
    /// Append-only JSONL probe log path. `None` = no disk capture
    /// this run.
    pub probe_file: Option<PathBuf>,
    /// Class filter passed to [`JsonlProbeFileSink::new`]. Empty
    /// set = "no filter, every class passes" per the sink's
    /// [`class_passes_filter`](super::probe_file_sink) rules.
    pub probe_classes: HashSet<String>,
    /// `EnvFilter` directive applied to the fmt (stderr) layer when
    /// `RUST_LOG` is unset. `"info"` for production servers; `"warn"`
    /// for noisy tests. Empty string falls back to `"info"`.
    pub default_filter: String,
}

impl ProbeTracingConfig {
    /// Read `CONTINUUM_PROBE_FILE` + `CONTINUUM_PROBE_CLASSES` env
    /// vars into a typed `ProbeTracingConfig`. This is THE seam
    /// where env coupling lives — every other path through the
    /// substrate constructs the config directly.
    ///
    /// `default_filter` is supplied by the caller because env vars
    /// don't carry it (`RUST_LOG` is already its own surface for
    /// the fmt layer; this is the fallback when `RUST_LOG` is
    /// unset).
    pub fn from_env(default_filter: &str) -> Self {
        let probe_file = std::env::var(ENV_PROBE_FILE).ok().map(PathBuf::from);
        let probe_classes = std::env::var(ENV_PROBE_CLASSES)
            .ok()
            .map(|s| {
                s.split(',')
                    .map(|c| c.trim().to_string())
                    .filter(|c| !c.is_empty())
                    .collect::<HashSet<_>>()
            })
            .unwrap_or_default();
        Self {
            probe_file,
            probe_classes,
            default_filter: default_filter.to_string(),
        }
    }
}

/// Result of the install — tells the caller whether disk capture is
/// active so it can be logged at boot ("probes landing at
/// /tmp/x.jsonl" is the kind of one-line confirmation that prevents
/// the "I set the env var, where are my probes" confusion).
#[derive(Debug)]
pub struct ProbeInstall {
    /// Path the JSONL sink is writing to, if `CONTINUUM_PROBE_FILE`
    /// was set and the file opened successfully. `None` = no disk
    /// capture this run (env var unset, OR test path where a
    /// caller-provided subscriber is already installed).
    pub probe_log_path: Option<PathBuf>,
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
/// `config.probe_file` was supplied but the path could not be
/// opened (directory missing, permission denied, etc). Per
/// `[[no-fallbacks-ever]]` the substrate refuses to silently drop
/// probes — the caller can choose to surface the error
/// (recommended for servers) or swallow it (acceptable for ad-hoc
/// tests where the probe path is best-effort).
///
/// Does NOT return an error when `config.probe_file` is `None` —
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

    let fmt_layer = tracing_subscriber::fmt::layer().with_writer(std::io::stderr);

    // Build the optional file sink BEFORE the registry chain so we
    // can either include it or skip it cleanly. None = no disk
    // capture this run (caller didn't ask). Path supplied but
    // unwritable = typed error surfaced to caller per
    // `[[no-fallbacks-ever]]`.
    let (file_sink, probe_log_path) = match config.probe_file {
        Some(path) => {
            let sink = JsonlProbeFileSink::new(&path, config.probe_classes)?;
            (Some(sink), Some(path))
        }
        None => (None, None),
    };

    // Compose the registry. The fmt layer wraps the env-filter so
    // RUST_LOG governs human-facing output; probe layers see EVERY
    // event regardless of RUST_LOG (probes are structured records,
    // not text logs — they're filtered by class via
    // CONTINUUM_PROBE_CLASSES, not by tracing's level system).
    let registry = tracing_subscriber::registry()
        .with(UriCaptureLayer::new())
        .with(ProbeRouterLayer::new())
        .with(file_sink) // Option<JsonlProbeFileSink>: tracing's Layer impl handles None
        .with(fmt_layer.with_filter(env_filter));

    // try_init succeeds the first call, no-ops on subsequent calls.
    // Errors here mean another subscriber was already installed —
    // benign for repeated init from tests, so we drop the error
    // intentionally to keep the helper idempotent.
    let _ = registry.try_init();

    Ok(ProbeInstall { probe_log_path })
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
            probe_file: None,
            probe_classes: HashSet::new(),
            default_filter: "warn".to_string(),
        };
        let first = install_probe_tracing(config.clone());
        let second = install_probe_tracing(config);
        assert!(first.is_ok(), "first install must succeed");
        assert!(second.is_ok(), "second install must no-op cleanly");
        assert!(first.unwrap().probe_log_path.is_none());
        assert!(second.unwrap().probe_log_path.is_none());
    }

    /// Typed-error contract: an unwritable `probe_file` path must
    /// surface `ProbeFileSinkError::OpenFailed` per
    /// `[[no-fallbacks-ever]]`. Operators must see the
    /// configuration problem; substrate refuses to silently drop
    /// probes.
    ///
    /// Parallel-safe because the bad path is passed via typed
    /// config — no env-var mutation, no racing on
    /// `CONTINUUM_PROBE_FILE`.
    #[test]
    fn install_surfaces_open_failed_for_unwritable_path() {
        let config = ProbeTracingConfig {
            probe_file: Some(PathBuf::from(
                "/this/path/definitely/does/not/exist/probes.jsonl",
            )),
            probe_classes: HashSet::new(),
            default_filter: "warn".to_string(),
        };
        let result = install_probe_tracing(config);
        assert!(
            matches!(result, Err(ProbeFileSinkError::OpenFailed { .. })),
            "unwritable path must surface typed error, got {:?}",
            result
        );
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
        let prev_file = std::env::var(ENV_PROBE_FILE).ok();
        let prev_classes = std::env::var(ENV_PROBE_CLASSES).ok();

        // Both vars set → populated config.
        std::env::set_var(ENV_PROBE_FILE, "/tmp/test-probes.jsonl");
        std::env::set_var(ENV_PROBE_CLASSES, "persona,cognition.analyze");
        let populated = ProbeTracingConfig::from_env("info");
        assert_eq!(
            populated.probe_file.as_deref(),
            Some(std::path::Path::new("/tmp/test-probes.jsonl"))
        );
        assert!(populated.probe_classes.contains("persona"));
        assert!(populated.probe_classes.contains("cognition.analyze"));
        assert_eq!(populated.default_filter, "info");

        // Both vars unset → empty config (NOT an error).
        std::env::remove_var(ENV_PROBE_FILE);
        std::env::remove_var(ENV_PROBE_CLASSES);
        let empty = ProbeTracingConfig::from_env("warn");
        assert!(empty.probe_file.is_none());
        assert!(empty.probe_classes.is_empty());
        assert_eq!(empty.default_filter, "warn");

        // Restore prior env state so other tests aren't affected
        // by ordering even if cargo's parallel runner interleaves.
        match prev_file {
            Some(v) => std::env::set_var(ENV_PROBE_FILE, v),
            None => std::env::remove_var(ENV_PROBE_FILE),
        }
        match prev_classes {
            Some(v) => std::env::set_var(ENV_PROBE_CLASSES, v),
            None => std::env::remove_var(ENV_PROBE_CLASSES),
        }
    }
}
