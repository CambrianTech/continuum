#![cfg(test)]
use super::*;
use serde_json::json;

// Card 0f6c4872: an older published candidate's Unknown verdict must not be
// attributed to a newer request. Actual adoption resolves only that incident;
// handoff alone and unrelated adoption do not, and newer-request debt survives.
#[test]
fn adopted_candidate_resolves_source_wait_without_satisfying_newer_request()
    -> Result<(), Box<dyn std::error::Error>> {
    use continuum_cli_lifecycle::deploy_health::{self, DeployHealth, DeploySelection, RefusalKind};
    let request = "5a765f7a14546557736f8ccd33cd8c3e67e6e8d2";
    let candidate = "5e01cca3970382b9588c422b71a2b0e8986df614";
    let root = tempfile::tempdir()?;
    let state = root.path().join("state");
    std::fs::create_dir(&state)?;
    std::fs::write(state.join("deploy-request.json"), serde_json::to_vec(&json!({
        "tip_sha": request, "requested_ms": 1
    }))?)?;
    let path = state.join("deploy-consume-attempts.json");
    let mut ledger = super::ConsumeLedger {
        selection: Some(DeploySelection { request: request.into(), build: request.into() }),
        ..Default::default()
    };
    ledger.record_refusal(request, candidate, RefusalKind::SourceAuthorization, "checks Unknown");
    ledger.save(&path)?;
    let mut restarted = super::ConsumeLedger::read(&path)?;
    assert!(matches!(deploy_health::read(root.path(), "4a2e3a004"),
        DeployHealth::Blocked { refusal } if refusal.kind == RefusalKind::SourceAuthorization
            && refusal.build == candidate));
    restarted.landed(candidate);
    assert!(restarted.refusal.is_some(), "handoff is not observed adoption");
    assert!(!restarted.observe_running("4a2e3a004"));
    assert!(restarted.observe_running("5e01cca39"));
    assert_eq!(restarted.selection, ledger.selection);
    assert_eq!(restarted.failures, ledger.failures);
    assert_eq!(restarted.resource_deferrals, ledger.resource_deferrals);
    restarted.save(&path)?;
    assert!(matches!(deploy_health::read(root.path(), "5e01cca39"), DeployHealth::Pending { .. }));
    assert!(matches!(deploy_health::read(root.path(), "5a765f7a1"), DeployHealth::Running));
    // Existing unrelated failure memory must not be forgiven by incident recovery.
    restarted.record_failure(request);
    restarted.record_refusal(request, candidate, RefusalKind::SourceAuthorization, "checks Unknown");
    assert!(restarted.observe_running(candidate));
    assert_eq!(restarted.failures(request), 1);
    Ok(())
}

// Health must read evidence saved by the consumer, including receipts with no
// failed attempts or resource deferrals. Handwritten JSON misses save's cleanup.
#[test]
fn consumer_save_preserves_selection_and_refusal_for_health() -> Result<(), Box<dyn std::error::Error>> {
    use continuum_cli_lifecycle::deploy_health::{self, DeployHealth, DeploySelection, RefusalKind};
    let root = tempfile::tempdir()?;
    let state = root.path().join("state");
    std::fs::create_dir(&state)?;
    std::fs::write(state.join("deploy-request.json"), serde_json::to_vec(&json!({
        "tip_sha": "abcdef1234567", "requested_ms": 1
    }))?)?;
    let path = state.join("deploy-consume-attempts.json");
    let mut ledger = super::ConsumeLedger::default();
    ledger.record_refusal("abcdef1234567", "abcdef1234567", RefusalKind::CheckoutDirty, "owner edits");
    ledger.save(&path)?;
    assert_eq!(super::ConsumeLedger::read(&path)?, ledger);
    assert!(matches!(deploy_health::read(root.path(), "123456789abcd"),
        DeployHealth::Blocked { refusal } if refusal.kind == RefusalKind::CheckoutDirty));

    let ledger = super::ConsumeLedger {
        selection: Some(DeploySelection { request: "abcdef1234567".into(), build: "7654321abcdef".into() }),
        ..Default::default()
    };
    ledger.save(&path)?;
    assert_eq!(super::ConsumeLedger::read(&path)?, ledger);
    assert!(matches!(deploy_health::read(root.path(), "123456789abcd"), DeployHealth::Pending { .. }));
    assert!(matches!(deploy_health::read(root.path(), "7654321abcdef"), DeployHealth::Running));

    super::ConsumeLedger::default().save(&path)?;
    assert!(!path.exists(), "only a wholly empty ledger is retired");
    Ok(())
}

// what this catches (card 82af11f5, 2026-09-19): the Windows deploy consumer's
// verdict vocabulary — no request = nothing owed; the tip already running (short
// sha as a prefix, git's 7-char floor) = the tracker retires it, never a reboot;
// a dirty checkout is REFUSED (a consumer never stashes an operator's tree); only
// a clean checkout with a different tip deploys. And the option parse: --install
// and --uninstall are exclusive, nothing else is accepted.
#[test]
fn the_deploy_consumer_deploys_only_a_clean_checkout_toward_a_tip_not_running() {
    use super::{consume_verdict, ConsumeVerdict, DeployConsumeOptions};
    let v = |tip: Option<&str>, running: Option<&str>, dirty: bool| consume_verdict(tip, None, running, dirty, false, 0);
    assert_eq!(v(None, Some("6d8fc04de"), false), ConsumeVerdict::NothingOwed);
    assert_eq!(v(Some("6d8fc04de"), Some("6d8fc04de1234567"), false), ConsumeVerdict::AlreadyRunning);
    assert_eq!(v(Some("6d8fc04de1234567"), Some("6d8fc04de"), false), ConsumeVerdict::AlreadyRunning, "either spelling as the prefix");
    assert_eq!(
        v(Some("6d8fc0"), Some("6d8fc04de"), false),
        ConsumeVerdict::Deploy,
        "under git's 7-char floor a prefix is a coincidence, not a match"
    );
    assert_eq!(v(Some("abc1234"), Some("6d8fc04de"), true), ConsumeVerdict::RefuseDirty);
    assert_eq!(v(Some("abc1234"), Some("6d8fc04de"), false), ConsumeVerdict::Deploy);
    assert_eq!(v(Some("abc1234"), None, false), ConsumeVerdict::Deploy, "no core answering = deploy, the request stands");
    // A live deploy claim = a build in flight from an earlier tick: NEVER a second
    // reboot into it (the 10-min task vs a 50-min build). It outranks dirty and the
    // ledger because nothing about this tick should act at all.
    assert_eq!(consume_verdict(Some("abc1234"), None, None, false, true, 0), ConsumeVerdict::BuildInFlight);
    assert_eq!(consume_verdict(Some("abc1234"), None, None, true, true, 9), ConsumeVerdict::BuildInFlight);
    // A tip that would not land here is tried CONSUME_MAX_ATTEMPTS times, then left to
    // deploy.stranded — never a rebuild loop every tick until the claim ages out.
    assert_eq!(consume_verdict(Some("abc1234"), None, None, false, false, 2), ConsumeVerdict::Deploy);
    assert_eq!(consume_verdict(Some("abc1234"), None, None, false, false, 3), ConsumeVerdict::GaveUp);
    // The ledger is per BUILD tried (card f7826d97, the 5090 on 2026-10-13): an older
    // published core that failed here three times is skipped while the tip's is
    // awaited, and is never charged to the tip; the tip's own build still gives up
    // at the bound. An old per-request ledger cannot be attributed and reads as none.
    let dir = std::env::temp_dir().join(format!("consume-ledger-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap(); // unwrap: test fixture — a temp dir that cannot be made fails the test loudly
    let path = dir.join("deploy-consume-attempts.json");
    let mut ledger = super::ConsumeLedger::default();
    for _ in 0..3 {
        ledger.record_failure("254d11e1c");
    }
    ledger.write(&path).unwrap();
    let ledger = super::ConsumeLedger::read(&path).unwrap();
    assert_eq!(ledger.failures("254d11e1c"), 3);
    assert_eq!(ledger.failures("34e8db15d"), 0, "the older build's failures are not the tip's");
    assert_eq!(
        ledger.admissible(vec!["34e8db15d".into(), "254d11e1c".into(), "5deb01e96".into()], "34e8db15d"),
        vec!["34e8db15d".to_string(), "5deb01e96".to_string()],
        "a build that already failed here is skipped; the tip's and untried ones stay"
    );
    let mut tip_failed = super::ConsumeLedger::default();
    for _ in 0..3 {
        tip_failed.record_failure("34e8db15d");
    }
    assert_eq!(
        tip_failed.admissible(vec!["34e8db15d".into()], "34e8db15d"),
        vec!["34e8db15d".to_string()],
        "the tip's own build is never filtered here: giving it up is the verdict's call"
    );
    // A pass that selected an older build and failed before its checkout (Astra review
    // of #5011, P1) is charged to that build; one that selected nothing is the tip's.
    let mut charged = super::ConsumeLedger::default();
    assert_eq!(charged.charge_failure(Some("254d11e1c"), "34e8db15d"), ("254d11e1c".to_string(), 1));
    assert_eq!(charged.charge_failure(None, "34e8db15d"), ("34e8db15d".to_string(), 1));
    // An older build landing forgets only ITS failures (P2): the exhausted one stays
    // excluded across a reload, and the tip keeps its own count.
    let mut after = super::ConsumeLedger::default();
    for _ in 0..3 {
        after.record_failure("34e8db15d");
    }
    after.record_failure("254d11e1c");
    after.landed("254d11e1c");
    after.save(&path).unwrap();
    let after = super::ConsumeLedger::read(&path).unwrap();
    assert_eq!(after.failures("34e8db15d"), 3, "an exhausted build is not made eligible by another landing");
    assert_eq!(after.failures("254d11e1c"), 0);
    assert_eq!(
        after.admissible(vec!["9f1c2d3e4".into(), "34e8db15d".into()], "9f1c2d3e4"),
        vec!["9f1c2d3e4".to_string()],
        "under a newer tip the exhausted build is still skipped"
    );
    let mut emptied = super::ConsumeLedger::default();
    emptied.record_failure("254d11e1c");
    emptied.landed("254d11e1c");
    emptied.save(&path).unwrap();
    assert!(!path.exists(), "nothing left to remember removes the file");
    std::fs::write(&path, r#"{"failures":3,"tip":"34e8db15db2d543a5bb72ee599b74feef39ae7f8"}"#).unwrap(); // unwrap: test fixture write
    assert_eq!(super::ConsumeLedger::read(&path).unwrap(), super::ConsumeLedger::default(), "an old per-request ledger names no build");
    // Resource refusal belongs to the exact artifact, not the changing tracker request.
    let mut blocked = super::ConsumeLedger::default();
    let condition = continuum_cli_lifecycle::prebuilt_artifact::ResourceDeferral {
        condition: continuum_cli_lifecycle::prebuilt_artifact::ResourceCondition::UnmeasuredFilesystem,
        path: dir.clone(), projected_bytes: 123, reason: "filesystem quota refused while volume admission passed".into(),
    };
    blocked.resource_deferrals.insert("published-build".into(), condition.clone());
    blocked.save(&path).unwrap();
    let mut restarted = super::ConsumeLedger::read(&path).unwrap();
    assert_eq!(restarted.resource_deferrals.get("published-build"), Some(&condition));
    assert_eq!(restarted.failures("published-build"), 0, "resource deferral does not condemn the build");
    restarted.landed("different-build");
    restarted.save(&path).unwrap();
    assert_eq!(super::ConsumeLedger::read(&path).unwrap().resource_deferrals.get("published-build"), Some(&condition), "another build landing and a new process preserve the condition");
    let mut pending = Some(("published-build".into(), condition.clone()));
    assert!(matches!(continuum_cli_lifecycle::prebuilt_artifact::recheck_resource_deferral(&mut pending, Ok(())), Some(continuum_cli_lifecycle::prebuilt_artifact::MissingArtifact::AdmissionWait(_))));
    assert_eq!(pending.as_ref().map(|(_, value)| value), Some(&condition), "a volume-only success cannot clear quota uncertainty");
    let lease = super::ConsumeLedger::take_lease(&path).unwrap();
    assert!(super::ConsumeLedger::take_lease(&path).is_err(), "a concurrent consumer/retry cannot own the ledger");
    let digest = super::ConsumeLedger::condition_digest(&condition).unwrap();
    let original = std::fs::read(&path).unwrap();
    assert!(super::ConsumeLedger::retry_resource(&path, "published-build", &digest, true).is_err(), "older live deploy claim refuses retry");
    assert!(super::ConsumeLedger::retry_resource(&path, "different-build", &digest, false).is_err());
    assert!(super::ConsumeLedger::retry_resource(&path, "published-build", &"0".repeat(64), false).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), original, "mismatch and active owner preserve exact receipt");
    let mut changed = super::ConsumeLedger::read(&path).unwrap();
    changed.resource_deferrals.get_mut("published-build").unwrap().projected_bytes += 1;
    changed.write(&path).unwrap();
    assert!(super::ConsumeLedger::retry_resource(&path, "published-build", &digest, false).is_err(), "stale digest cannot clear changed condition");
    changed.resource_deferrals.insert("another-build".into(), condition.clone());
    changed.record_failure("published-build");
    changed.write(&path).unwrap();
    let current_digest = super::ConsumeLedger::condition_digest(changed.resource_deferrals.get("published-build").unwrap()).unwrap();
    super::ConsumeLedger::retry_resource(&path, "published-build", &current_digest, false).unwrap();
    let retried = super::ConsumeLedger::read(&path).unwrap();
    assert!(!retried.resource_deferrals.contains_key("published-build"));
    assert_eq!(retried.resource_deferrals.get("another-build"), Some(&condition));
    assert_eq!(retried.failures("published-build"), 1, "explicit resource retry never clears engineering failures");
    drop(lease);
    drop(super::ConsumeLedger::take_lease(&path).unwrap());
    for corrupt in ["{", "{}", r#"{"resource_deferrals":{"build":{"condition":"invented","path":"x","projected_bytes":1,"reason":"unknown"}}}"#] {
        std::fs::write(&path, corrupt).unwrap();
        assert!(super::ConsumeLedger::read(&path).is_err(), "corrupt custody must not silently authorize retry");
        assert!(super::ConsumeLedger::retry_resource(&path, "published-build", &digest, false).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), corrupt);
    }
    assert!(blocked.write(&dir.join("missing-parent").join("ledger.json")).is_err(), "persistence failure is visible");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(DeployConsumeOptions::parse(["--install".to_string()].into_iter()).is_err(), "the consumer's task has ONE registrar: continuum install --supervisor");
    assert!(DeployConsumeOptions::parse(std::iter::empty()).is_ok());
    assert!(DeployConsumeOptions::parse(["--protocol".into()].into_iter()).unwrap().protocol);
    assert!(DeployConsumeOptions::parse(["--protocol".into(), "extra".into()].into_iter()).is_err());
    // Mixed-version rollout must fail closed before the saved condition is cleared.
    assert!(super::retry_launcher(&serde_json::json!({"buildSha":"old"})).is_err());
    assert!(super::retry_launcher(&serde_json::json!({"deployConsumerProtocol":0,"deployConsumerCli":path})).is_err());
    assert!(super::retry_launcher(&serde_json::json!({"deployConsumerProtocol":1,"deployConsumerCli":null})).is_err());
    assert!(super::retry_launcher(&serde_json::json!({"deployConsumerProtocol":1,"deployConsumerCli":"continuum"})).is_err());
    assert_eq!(super::retry_launcher(&serde_json::json!({"deployConsumerProtocol":1,"deployConsumerCli":path})).unwrap(), path);
    #[cfg(windows)]
    {
        let envelope = continuum_cli_lifecycle::installed_release::Envelope { schema: 2, active_release: "receipt".into(), bootstrap: "bootstrap.exe".into() };
        let mut report = super::supervisor_install::TaskReport { present: true, enabled: true, actions: 1, command: envelope.bootstrap.clone(), arguments: envelope.arguments("deploy"), user_sid: "S-1-5-21".into(), ..Default::default() };
        assert!(super::retry_task_matches(&report, &envelope, "deploy", "S-1-5-21"));
        report.arguments = "deploy-consume".into();
        assert!(!super::retry_task_matches(&report, &envelope, "deploy", "S-1-5-21"), "legacy direct task cannot satisfy the receipt guard");
        report.arguments = envelope.arguments("deploy");
        report.command = "other.exe".into();
        assert!(!super::retry_task_matches(&report, &envelope, "deploy", "S-1-5-21"), "changed registration refuses");
    }


    assert!(DeployConsumeOptions::parse(["--now".to_string()].into_iter()).is_err());
    assert!(DeployConsumeOptions::parse(["--retry-resource".into(), "a".repeat(40), "b".repeat(64)].into_iter()).is_ok());
    assert!(DeployConsumeOptions::parse(["--retry-resource".into(), "a".repeat(40)].into_iter()).is_err());
    assert!(DeployConsumeOptions::parse(["--retry-resource".into(), "short".into(), "b".repeat(64)].into_iter()).is_err());
    // The consumer runs on every platform now (the core's actuator launches it); only
    // Windows needs `--service` — macOS promotes a bare reboot to launchd itself, and
    // `reboot` refuses the flag on Linux.
    assert!(super::consumer_uses_service("windows"));
    assert!(!super::consumer_uses_service("macos"));
    assert!(!super::consumer_uses_service("linux"));
}

// what this catches (card 9beab2da, Codex on #5551): a training hold charged as a handoff
// failure (three of them give up on the build) or kept as a durable resource deferral that waits
// for an explicit retry; it releases on its own, so it is rechecked on the next pass with nothing
// charged. The other classes keep their charges.
#[test]
fn a_held_handoff_charges_no_attempt_and_keeps_no_deferral() {
    use continuum_cli_lifecycle::prebuilt_artifact::ResourceCondition;
    let core = std::path::PathBuf::from("prepared-core");
    let held = "the engine is held for live work (genome-train:8b46862c on ggml-org/Qwen3.8-27B-GGUF (engine pid 37904, port 58057))";
    assert_eq!(super::handoff_refusal(PublishedCoreError::Held(held.into()), core.clone()), super::HandoffRefusal::Blocked(held.into()));
    assert_eq!(super::handoff_refusal(PublishedCoreError::SourcePending("pending".into()), core.clone()), super::HandoffRefusal::Blocked("pending".into()));
    assert_eq!(super::handoff_refusal(PublishedCoreError::Artifact("refused".into()), core.clone()), super::HandoffRefusal::Failed("refused".into()));
    assert!(matches!(
        super::handoff_refusal(PublishedCoreError::OperationalBlocked("installer".into()), core),
        super::HandoffRefusal::Deferred(d) if d.condition == ResourceCondition::OperationalUncertainty && d.reason == "installer"
    ));
}
