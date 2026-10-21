#![cfg(test)]
use super::*;

// what this catches: a warm build at normal priority beside a serving core, the
// contention that starved the IntelMac's CPU lane for ~10 h (2026-09-27). A child
// spawned through yield_to_serving must run at the lowest priority, where cargo and
// rustc inherit it.
#[cfg(unix)]
#[test]
fn a_warm_build_runs_at_the_lowest_priority_beside_the_serving_core() {
    let mut cmd = std::process::Command::new("sh");
    cmd.args(["-c", "nice"]);
    yield_to_serving(&mut cmd);
    let out = cmd.output().expect("test: sh runs");
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "19");
}

// what this catches (2026-09-13): a warm build attempted without headroom (starving the
// serving core, Joel 08-23) or without a build definition; and a refusal that does not
// name the number the operator needs.
#[test]
fn a_warm_build_needs_a_script_and_headroom_and_says_why_not() {
    let script = Some(PathBuf::from("/x/start-server.sh"));
    assert!(warm_build_allowed(WARM_BUILD_MIN_FREE_BYTES, script.clone()).is_ok());
    let err = warm_build_allowed(WARM_BUILD_MIN_FREE_BYTES - 1, script).unwrap_err();
    assert!(err.contains("GiB free"), "{err}");
    assert!(
        warm_build_allowed(u64::MAX, None).is_err(),
        "no script = no build definition"
    );
}

// what this catches (2026-09-19, the 5090's consumer): a log handle opened with
// `.append(true)` is FILE_APPEND_DATA-only on Windows, and an MSYS bash handed it
// as stdout exits 1 before running a line — every unattended deploy on the node
// died in one second with an EMPTY log, and the exit code named nothing. The
// handle a child is given must be one bash runs under and writes through, and
// it must still append.
#[cfg(windows)]
#[test]
fn the_deploy_log_handle_is_one_bash_can_run_under() {
    let dir = tempfile::tempdir().expect("tempdir");
    let log = dir.path().join("deploy.log");
    std::fs::write(&log, "seed\n").expect("seed");
    let bash = continuum_base::shell_portable::locate_bash().expect("Git bash is a build prerequisite on Windows");
    let status = std::process::Command::new(bash)
        .args(["-c", "echo from-bash"])
        .stdin(Stdio::null())
        .stdout(Stdio::from(open_log_for_child(&log).expect("open")))
        .stderr(Stdio::from(open_log_for_child(&log).expect("open")))
        .status()
        .expect("spawn");
    assert!(status.success(), "bash under the log handle exited {status}");
    let text = std::fs::read_to_string(&log).expect("read");
    assert_eq!(text.replace("\r", ""), "seed\nfrom-bash\n", "the child wrote through the handle, after the seed");
}

// Regression for card 571d6e0b: an old script's absent receipt, malformed
// output, or failed build cannot turn into teardown followed by a rebuild.
#[tokio::test]
async fn warm_build_receipts_are_bounded_owned_and_fail_closed() {
    let receipt = WarmBuildReceipt::create().unwrap();
    let receipt_path = receipt.0.clone();
    for invalid in ["", "relative/core", "/one\n/two\n", "/one\0two"] {
        std::fs::write(&receipt.0, invalid).unwrap();
        assert!(receipt.artifact().is_err(), "accepted {invalid:?}");
    }
    std::fs::write(&receipt.0, vec![b'x'; 16 * 1024 + 1]).unwrap();
    assert!(receipt.artifact().is_err());
    let artifact = std::env::temp_dir().join("build cache 雪").join("core.exe");
    std::fs::write(&receipt.0, format!("{}\n", artifact.display())).unwrap();
    assert_eq!(receipt.artifact().unwrap(), artifact);
    for failure in [
        PublishedCoreError::Artifact("verified payload hash mismatch".into()),
        PublishedCoreError::CapabilityDeferred("selected artifact lacks capacity protocol".into()),
        PublishedCoreError::ResourceDeferred(continuum_cli_lifecycle::prebuilt_artifact::ResourceDeferral {
            condition: continuum_cli_lifecycle::prebuilt_artifact::ResourceCondition::UnmeasuredFilesystem,
            path: artifact.parent().unwrap().into(), projected_bytes: 123, reason: "quota refusal".into(),
        }),
    ] {
        std::fs::write(&receipt.0, serde_json::to_vec(&failure).unwrap()).unwrap(); // boundary: fixture installer receipt
        assert_eq!(receipt.deployment_failure().unwrap(), Some(failure));
        assert!(receipt.artifact().is_err(), "resource refusal cannot become a selected artifact path");
    }
    std::fs::write(&receipt.0, "{broken").unwrap();
    assert!(receipt.deployment_failure().is_err());
    assert!(matches!(receipt.classify_deployment_failure("installer failed".into()), PublishedCoreError::OperationalBlocked(_)));
    std::fs::write(&receipt.0, "").unwrap();
    assert!(matches!(receipt.classify_deployment_failure("installer failed".into()), PublishedCoreError::OperationalBlocked(_)));
    drop(receipt);
    assert!(!receipt_path.exists());

    let mut failed = std::process::Command::new(locate_bash().unwrap());
    failed.args([
        "-c",
        "printf '%s\\n' /misleading/core > \"$CONTINUUM_BUILD_RECEIPT\"; exit 23",
    ]);
    let error = prepare_warm_build(failed).await.unwrap_err();
    assert!(error.contains("warm build exited"), "{error}");
    assert!(error.contains("leaving the running core untouched"));

    let mut missing = std::process::Command::new(locate_bash().unwrap());
    missing.args(["-c", "exit 0"]);
    assert!(prepare_warm_build(missing)
        .await
        .unwrap_err()
        .contains("did not report one absolute native artifact path"));

    let directory = tempfile::tempdir().unwrap();
    let unusable = directory.path().join("unusable core.exe");
    std::fs::write(&unusable, b"not an executable").unwrap();
    let mut reported = std::process::Command::new(locate_bash().unwrap());
    reported
        .args([
            "-c",
            "printf '%s\\n' \"$1\" > \"$CONTINUUM_BUILD_RECEIPT\"",
            "receipt-fixture",
        ])
        .arg(&unusable);
    let error = prepare_warm_build(reported).await.unwrap_err();
    assert!(error.contains("--build-sha"), "{error}");
}
