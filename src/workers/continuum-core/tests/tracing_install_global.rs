//! Integration tests for `install_probe_tracing` — the substrate's
//! GLOBAL tracing subscriber install. These tests MUST live in their
//! own integration-test binary (one process per integration test file
//! in cargo's runner), because `install_probe_tracing` calls
//! `tracing_subscriber::registry().try_init()` which permanently
//! attaches the subscriber for the rest of the process.
//!
//! Per task #203 (`Investigate uri_layer test pollution flake`): when
//! these tests lived under
//! `continuum-core/src/routing/tracing_init.rs::tests`, the global
//! install leaked into the lib-test binary and broke
//! `routing::uri_layer::tests::no_subscriber_returns_empty_chain` —
//! that test asserts an EMPTY chain when no Layer is installed, but
//! whichever lib-test ran first (race-dependent) might already have
//! installed the global UriCaptureLayer through this path.
//!
//! Process isolation is the cure. Each `tests/*.rs` file becomes a
//! separate binary in `cargo test`, so the global subscriber installed
//! here cannot pollute any other test binary's view of the global
//! default.

use std::collections::HashSet;
use std::path::PathBuf;

use continuum_core::routing::probe_file_sink::ProbeFileSinkError;
use continuum_core::routing::tracing_init::{install_probe_tracing, ProbeTracingConfig};

/// `install_probe_tracing` accepts a typed config and is safe to call
/// twice — the second call no-ops cleanly because
/// `tracing_subscriber::registry().try_init()` succeeds the first time
/// and silently no-ops thereafter. Substrate boot paths (main.rs,
/// tests, replay harnesses) rely on this so the install can be
/// re-attempted without checking "did I already call it."
#[test]
fn install_is_idempotent_with_no_disk_capture() {
    let config = ProbeTracingConfig {
        probe_file: None,
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

/// Typed-error contract: an unwritable `probe_file` path must surface
/// `ProbeFileSinkError::OpenFailed` per `[[no-fallbacks-ever]]`.
/// Operators must see the configuration problem; substrate refuses to
/// silently drop probes.
///
/// Lives in the same integration binary as the idempotency test
/// because `install_probe_tracing` is the path under test in both —
/// they share the global-subscriber pollution risk and must share the
/// isolated process.
#[test]
fn install_surfaces_open_failed_for_unwritable_path() {
    let config = ProbeTracingConfig {
        probe_file: Some(PathBuf::from(
            "/this/path/definitely/does/not/exist/probes.jsonl",
        )),
        probe_classes: HashSet::new(),
        default_filter: "warn".to_string(),
        log_dir: None,
    };
    let result = install_probe_tracing(config);
    // Can't `{:?}` the Ok branch — ProbeInstall holds a WorkerGuard
    // which isn't Debug (PR #1547). Match the variant explicitly so
    // the assertion error is still informative on failure.
    match &result {
        Err(ProbeFileSinkError::OpenFailed { .. }) => {}
        Err(other) => panic!("expected OpenFailed, got error {other:?}"),
        Ok(_) => panic!("expected Err, got Ok(_)"),
    }
}
