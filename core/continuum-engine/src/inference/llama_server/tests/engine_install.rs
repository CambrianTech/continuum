//! Engine install regression fixtures.
#![cfg(test)]
use super::*;
// what this catches (card 7c5f139d): an adopted lane left on last deploy's engine
// forever — the M5 and IntelMac served b10765-965d38a90 (no /train) while the stamp
// moved on. A lane whose /props build names another commit than the installed stamp is
// stale; the same commit in short or full form is current; an unknown on either side
// is never a relaunch.
#[test]
fn a_lane_on_an_older_engine_than_the_installed_one_is_stale() {
    assert!(!lane_runs_installed_engine("b10765-965d38a90", "9733aca6c"), "older engine: relaunch");
    assert!(lane_runs_installed_engine("b10765-965d38a90", "965d38a90"), "same commit");
    assert!(lane_runs_installed_engine("b10765-965d38a90", "965d38a900bc82e28900683f9e30f7333cbf3936"), "short vs full SHA");
    assert!(lane_runs_installed_engine("", "9733aca6c"), "a lane naming no build is not stale");
    assert!(lane_runs_installed_engine("b10765-965d38a90", ""), "no stamp: nothing to compare");
    assert!(lane_runs_installed_engine("b10765-unknown", "9733aca6c"), "a non-SHA build is not evidence");
}

// what this catches: an engine older than --train-dir being handed the flag (it refuses to
// start: the lane goes dark). Only an engine whose help LISTS the option accepts it; a help
// text that merely mentions the words, or none at all, does not.
#[test]
fn train_dir_is_accepted_only_when_the_engines_help_lists_it() {
    use super::engine_help_lists_train_dir;
    let new = "  -lv, --verbosity N\n--train-dir DIR                         enable POST /train\n";
    let old = "  -lv, --verbosity N\n  --lora FNAME   path to LoRA adapter\n";
    let mention = "  --lora FNAME   adapters are not --train-dir aware\n";
    assert!(engine_help_lists_train_dir(new));
    assert!(!engine_help_lists_train_dir(old));
    assert!(!engine_help_lists_train_dir(mention));
    assert!(!engine_help_lists_train_dir(""));
}

// regression for card 8c06f778 (BigMama, 2026-09-26 14:24:55Z): a /slots read that
// timed out mid-restore was read as a dead engine and cost the node an engine
// replacement.
// what this catches: the three outcomes are told apart by the SOCKET, not by
// "did we get a value" — an open socket that never answers is Busy, a socket
// nothing listens on is Unreachable, and a real slots array is Progress.
#[tokio::test]
async fn a_probe_that_times_out_is_busy_and_only_a_missing_socket_is_unreachable() {
    use super::{engine_probe_within, EngineProbe};
    let client = reqwest::Client::new();
    let short = std::time::Duration::from_millis(200);
    // Busy: a listener that accepts and holds the connection without answering.
    let silent = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("test: bind");
    let root = format!("http://{}", silent.local_addr().expect("test: addr"));
    let hold = tokio::spawn(async move {
        let (stream, _) = silent.accept().await.expect("test: accept");
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        drop(stream);
    });
    assert_eq!(engine_probe_within(&root, &client, short).await, EngineProbe::Busy);
    hold.abort();
    // Unreachable: a port that was ours a moment ago and now has no listener.
    let gone = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("test: bind");
    let root = format!("http://{}", gone.local_addr().expect("test: addr"));
    drop(gone);
    assert_eq!(engine_probe_within(&root, &client, short).await, EngineProbe::Unreachable);
    // Progress: a slots array is a fingerprint.
    let app = axum::Router::new().route(
        "/slots",
        axum::routing::get(|| async {
            axum::Json(serde_json::json!([{"id_task": 7, "n_prompt_tokens_processed": 100, "next_token": [{"n_decoded": 5}], "is_processing": true}]))
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("test: bind");
    let root = format!("http://{}", listener.local_addr().expect("test: addr"));
    let server = tokio::spawn(async move { axum::serve(listener, app).await });
    let probe = engine_probe_within(&root, &client, std::time::Duration::from_secs(2)).await;
    assert!(matches!(probe, EngineProbe::Progress(p) if p.any_processing && p.fingerprint == 112), "{probe:?}");
    server.abort();
}

// The service-host's verified engine must win over any older default install.
// A child process isolates the launch environment from parallel tests.
#[test]
fn serving_uses_the_supervisors_engine() {
    const CHILD: &str = "CONTINUUM_ENGINE_CONTRACT_TEST_CHILD";
    const ENGINE: &str = "prepared release/engine-b/llama-server.exe";
    if std::env::var_os(CHILD).is_some() {
        assert_eq!(super::server_bin().unwrap(), ENGINE);
        return;
    }
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "inference::llama_server::tests::engine_install::serving_uses_the_supervisors_engine",
        ])
        .env(CHILD, "1")
        .env("LLAMA_SERVER_BIN", ENGINE);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    assert!(command.status().unwrap().success());
}

// what this catches: a debug llama-server hosting a lane silently (BigMama's
// 5090 served one for a day, 2026-09-04) — the door reads the server's own
// warning line; a release version line passes.
#[test]
fn a_debug_build_is_refused_at_the_door_and_a_release_build_passes() {
    assert!(super::is_debug_build("warning: DEBUG BUILD (asserts enabled) -- performance numbers from this process are not valid\nversion: 10229 (a28ee566c)"));
    assert!(!super::is_debug_build("version: 0.3.0-dev (build 10751, commit 0a637ba22)\nbuilt with AppleClang 21.0.0.21000101 for Darwin arm64"));
}

// The engine-only M5 convergence must preserve the incumbent's actual launch geometry.
#[test]
fn engine_only_convergence_preserves_incumbent_launch_arguments() {
    for (lanes, window, planned_window) in [(2, 187136, 187136), (3, 76032, 76800)] {
        let mut planned = target("convergence-fixture");
        planned.lanes = 1;
        planned.context_window = planned_window;
        let selected = super::super::reconcile::engine_convergence_target(&planned, window, lanes).unwrap();
        let invocation = crate::inference::lane_args::base_invocation(
            Path::new("fixture.gguf"), selected.model_id(), "127.0.0.1", 1,
            selected.parallel_lanes(), selected.served_total_ctx(),
            selected.host_prompt_cache_mib, Path::new("pages"),
        );
        assert_eq!(invocation.value_of("--parallel"), Some(lanes.to_string().as_str()));
        assert_eq!(invocation.value_of("-c"), Some((lanes * window).to_string().as_str()));
    }
    let planned = target("convergence-fixture");
    assert!(super::super::reconcile::engine_convergence_target(&planned, 0, 2).is_err());
    assert!(super::super::reconcile::engine_convergence_target(&planned, 76032, 0).is_err());
}
