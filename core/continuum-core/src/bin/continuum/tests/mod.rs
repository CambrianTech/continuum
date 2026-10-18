#![cfg(test)]

use std::path::{Path, PathBuf};
#[cfg(windows)]
use std::process::Stdio;
use continuum_cli_lifecycle::prebuilt_artifact::PublishedCoreError;
use continuum_cli_lifecycle::deploy_provenance::deploy_verdict;

use super::checkout::*;
use super::checkpoint::*;
use super::deploy_consume::*;
#[cfg(windows)]
use super::deploy_log::*;
use super::deploy_verify::*;
use super::desktop::*;
use super::dispatch::*;
use super::launch::*;
use super::prebuilt::*;
use super::process::*;
use super::reboot::*;
use super::runtime_env::*;
use super::service::*;
use super::stop::*;
use super::warm_build::*;
use super::local_help_requested;

// what this catches (M5, 2026-09-26): the start/reboot receipt promised "the web build
// lands in the background" on eight consecutive supervised deploys while no dist was
// configured and none was being built. An unconfigured desktop must be NAMED, with the
// verb that fixes it; the promise is reserved for a configured dist that is still in flight.
#[test]
fn the_desktop_receipt_never_promises_a_build_the_launch_path_is_not_making() {
    let up = desktop_receipt(true, false, 8975);
    assert!(up.contains("http://127.0.0.1:8975/"), "{up}");
    let in_flight = desktop_receipt(false, true, 8975);
    assert!(in_flight.contains("lands in the background"), "{in_flight}");
    let unconfigured = desktop_receipt(false, false, 8975);
    assert!(unconfigured.contains("not configured") && unconfigured.contains("CONTINUUM_UI_DIST"), "{unconfigured}");
    assert!(!unconfigured.contains("lands in the background"), "no promise without a dist: {unconfigured}");
}

// what this catches (2026-09-22, Astra's Windows node): a drain that runs even
// though the teardown after it will be refused. `system/shutdown.rs` deliberately
// leaves the process alive for the CLI to end, and `OpenProcess(25040, TERMINATE)`
// returned NULL/error 5 there — so draining first left a drained core still
// answering ping, with no way to end it. The rule must refuse BEFORE the drain and
// must say that nothing was touched, because an operator who reads "refused" after
// a drain has a different machine in front of them than one who reads it before.
#[test]
fn a_drain_is_refused_when_the_teardown_after_it_would_be() {
    assert!(may_begin_shutdown(Ok(())).is_ok(), "authority present: the stop proceeds");
    let refused = may_begin_shutdown(Err("OpenProcess denied".to_string()))
        .expect_err("no teardown authority must refuse the drain");
    assert!(refused.contains("OpenProcess denied"), "the OS's reason is carried: {refused}");
    assert!(
        refused.contains("No state was touched"),
        "the refusal must say the drain did NOT run: {refused}"
    );
}

// what this catches: a capability probe that refuses on a pid that is merely GONE.
// The pidfile routinely names a core that already exited; if absence read as "no
// authority", every ordinary stop would refuse before doing anything. Only an
// actual permission denial may refuse.
#[cfg(unix)]
#[test]
fn a_pid_that_no_longer_exists_is_not_a_permission_refusal() {
    assert!(
        teardown_authority(std::process::id() as i32).is_ok(),
        "a process this caller owns is tearable down"
    );
    // Max pid + 1 on every supported unix: nothing can be running there.
    assert!(
        teardown_authority(i32::MAX).is_ok(),
        "already gone is nothing to tear down, never a refusal"
    );
}

// what this catches (Astra's review of e275378a2, 2026-09-22): tests that exercise
// helpers instead of the orchestration — "a removed preflight call still passes all
// three". This one is over the decision the stop path ACTUALLY makes, across every
// core it would have to end, and the call itself is now unskippable by construction:
// `request_graceful_stop` takes the `MayDrain` only this function produces, so
// deleting the preflight does not turn a test green, it stops compiling.
#[test]
fn the_drain_is_decided_over_every_core_this_stop_would_have_to_end() {
    let refuse_25040 = |pid: i32| {
        if pid == 25040 { Err("OpenProcess denied".to_string()) } else { Ok(()) }
    };

    // All endable: drain, nothing to borrow.
    let ok = preflight_teardown(&[10, 11], false, refuse_25040).expect("owned cores drain");
    assert!(ok.borrow_authority_for.is_none());

    // THE ONE THAT MATTERS: the un-endable core is not the first in the list. A
    // preflight that checked only the pidfile's claim — the head of this vector —
    // would pass here and drain a core it cannot end.
    let refused = preflight_teardown(&[10, 25040], false, refuse_25040)
        .err()
        .expect("a core this caller cannot end must refuse the drain");
    assert!(refused.contains("OpenProcess denied"), "{refused}");
    assert!(refused.contains("No state was touched"), "{refused}");

    // Same observation, operator present: carried forward to be borrowed AFTER the
    // drain rather than abandoning the stop.
    let escalated = preflight_teardown(&[10, 25040], true, refuse_25040)
        .expect("an operator present proceeds and borrows the privilege later");
    assert_eq!(escalated.borrow_authority_for, Some(25040));

    // Nothing running at all is not a refusal — there is no core to be unable to end.
    assert!(preflight_teardown(&[], false, |_| Err("x".to_string()))
        .expect("no cores, nothing to preflight")
        .borrow_authority_for
        .is_none());
}

// what this catches (Astra's second objection, 2026-09-22): deciding a kill failed
// by probing liveness on the next line. SIGTERM and taskkill are asynchronous, so a
// healthy core is still in the table immediately after — an immediate probe would
// have reported "kill refused" on every successful deploy. The decider is the
// bounded deadline: a pid still present when it expires, and only then.
#[tokio::test]
async fn only_outliving_the_deadline_is_evidence_that_a_kill_did_not_take() {
    use std::time::Duration;
    // Our own pid cannot exit, so it stands in for a core that refused to go.
    let me = std::process::id() as i32;
    let refused = exited_within(me, Duration::from_millis(300))
        .await
        .expect_err("a pid still alive at the deadline is a failed teardown");
    assert!(refused.contains(&me.to_string()), "the surviving pid is named: {refused}");
    assert!(
        refused.contains("pidfile is left in place"),
        "the claim must survive the process it names: {refused}"
    );
    // And a pid that is gone returns without spending the deadline.
    let started = std::time::Instant::now();
    assert!(exited_within(i32::MAX, Duration::from_secs(30)).await.is_ok());
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "an already-dead pid must not wait out the deadline"
    );
}

// what this catches (card 82af11f5, 2026-09-19): the Windows deploy consumer's
// verdict vocabulary — no request = nothing owed; the tip already running (short
// sha as a prefix, git's 7-char floor) = the tracker retires it, never a reboot;
// a dirty checkout is REFUSED (a consumer never stashes an operator's tree); only
// a clean checkout with a different tip deploys. And the option parse: --install
// and --uninstall are exclusive, nothing else is accepted.
#[test]
fn the_deploy_consumer_deploys_only_a_clean_checkout_toward_a_tip_not_running() {
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
    let mut ledger = ConsumeLedger::default();
    for _ in 0..3 {
        ledger.record_failure("254d11e1c");
    }
    ledger.write(&path).unwrap();
    let ledger = ConsumeLedger::read(&path).unwrap();
    assert_eq!(ledger.failures("254d11e1c"), 3);
    assert_eq!(ledger.failures("34e8db15d"), 0, "the older build's failures are not the tip's");
    assert_eq!(
        ledger.admissible(vec!["34e8db15d".into(), "254d11e1c".into(), "5deb01e96".into()], "34e8db15d"),
        vec!["34e8db15d".to_string(), "5deb01e96".to_string()],
        "a build that already failed here is skipped; the tip's and untried ones stay"
    );
    let mut tip_failed = ConsumeLedger::default();
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
    let mut charged = ConsumeLedger::default();
    assert_eq!(charged.charge_failure(Some("254d11e1c"), "34e8db15d"), ("254d11e1c".to_string(), 1));
    assert_eq!(charged.charge_failure(None, "34e8db15d"), ("34e8db15d".to_string(), 1));
    // An older build landing forgets only ITS failures (P2): the exhausted one stays
    // excluded across a reload, and the tip keeps its own count.
    let mut after = ConsumeLedger::default();
    for _ in 0..3 {
        after.record_failure("34e8db15d");
    }
    after.record_failure("254d11e1c");
    after.landed("254d11e1c");
    after.save(&path).unwrap();
    let after = ConsumeLedger::read(&path).unwrap();
    assert_eq!(after.failures("34e8db15d"), 3, "an exhausted build is not made eligible by another landing");
    assert_eq!(after.failures("254d11e1c"), 0);
    assert_eq!(
        after.admissible(vec!["9f1c2d3e4".into(), "34e8db15d".into()], "9f1c2d3e4"),
        vec!["9f1c2d3e4".to_string()],
        "under a newer tip the exhausted build is still skipped"
    );
    let mut emptied = ConsumeLedger::default();
    emptied.record_failure("254d11e1c");
    emptied.landed("254d11e1c");
    emptied.save(&path).unwrap();
    assert!(!path.exists(), "nothing left to remember removes the file");
    std::fs::write(&path, r#"{"failures":3,"tip":"34e8db15db2d543a5bb72ee599b74feef39ae7f8"}"#).unwrap(); // unwrap: test fixture write
    assert_eq!(ConsumeLedger::read(&path).unwrap(), ConsumeLedger::default(), "an old per-request ledger names no build");
    // Resource refusal belongs to the exact artifact, not the changing tracker request.
    let mut blocked = ConsumeLedger::default();
    let condition = continuum_cli_lifecycle::prebuilt_artifact::ResourceDeferral {
        condition: continuum_cli_lifecycle::prebuilt_artifact::ResourceCondition::UnmeasuredFilesystem,
        path: dir.clone(), projected_bytes: 123, reason: "filesystem quota refused while volume admission passed".into(),
    };
    blocked.resource_deferrals.insert("published-build".into(), condition.clone());
    blocked.save(&path).unwrap();
    let mut restarted = ConsumeLedger::read(&path).unwrap();
    assert_eq!(restarted.resource_deferrals.get("published-build"), Some(&condition));
    assert_eq!(restarted.failures("published-build"), 0, "resource deferral does not condemn the build");
    restarted.landed("different-build");
    restarted.save(&path).unwrap();
    assert_eq!(ConsumeLedger::read(&path).unwrap().resource_deferrals.get("published-build"), Some(&condition), "another build landing and a new process preserve the condition");
    let mut pending = Some(("published-build".into(), condition.clone()));
    assert!(matches!(continuum_cli_lifecycle::prebuilt_artifact::recheck_resource_deferral(&mut pending, Ok(())), Some(continuum_cli_lifecycle::prebuilt_artifact::MissingArtifact::Wait(_))));
    assert_eq!(pending.as_ref().map(|(_, value)| value), Some(&condition), "a volume-only success cannot clear quota uncertainty");
    let lease = ConsumeLedger::take_lease(&path).unwrap();
    assert!(ConsumeLedger::take_lease(&path).is_err(), "a concurrent consumer/retry cannot own the ledger");
    let digest = ConsumeLedger::condition_digest(&condition).unwrap();
    let original = std::fs::read(&path).unwrap();
    assert!(ConsumeLedger::retry_resource(&path, "published-build", &digest, true).is_err(), "older live deploy claim refuses retry");
    assert!(ConsumeLedger::retry_resource(&path, "different-build", &digest, false).is_err());
    assert!(ConsumeLedger::retry_resource(&path, "published-build", &"0".repeat(64), false).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), original, "mismatch and active owner preserve exact receipt");
    let mut changed = ConsumeLedger::read(&path).unwrap();
    changed.resource_deferrals.get_mut("published-build").unwrap().projected_bytes += 1;
    changed.write(&path).unwrap();
    assert!(ConsumeLedger::retry_resource(&path, "published-build", &digest, false).is_err(), "stale digest cannot clear changed condition");
    changed.resource_deferrals.insert("another-build".into(), condition.clone());
    changed.record_failure("published-build");
    changed.write(&path).unwrap();
    let current_digest = ConsumeLedger::condition_digest(changed.resource_deferrals.get("published-build").unwrap()).unwrap();
    ConsumeLedger::retry_resource(&path, "published-build", &current_digest, false).unwrap();
    let retried = ConsumeLedger::read(&path).unwrap();
    assert!(!retried.resource_deferrals.contains_key("published-build"));
    assert_eq!(retried.resource_deferrals.get("another-build"), Some(&condition));
    assert_eq!(retried.failures("published-build"), 1, "explicit resource retry never clears engineering failures");
    drop(lease);
    drop(ConsumeLedger::take_lease(&path).unwrap());
    for corrupt in ["{", "{}", r#"{"resource_deferrals":{"build":{"condition":"invented","path":"x","projected_bytes":1,"reason":"unknown"}}}"#] {
        std::fs::write(&path, corrupt).unwrap();
        assert!(ConsumeLedger::read(&path).is_err(), "corrupt custody must not silently authorize retry");
        assert!(ConsumeLedger::retry_resource(&path, "published-build", &digest, false).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), corrupt);
    }
    assert!(blocked.write(&dir.join("missing-parent").join("ledger.json")).is_err(), "persistence failure is visible");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(DeployConsumeOptions::parse(["--install".to_string()].into_iter()).is_err(), "the consumer's task has ONE registrar: continuum install --supervisor");
    assert!(DeployConsumeOptions::parse(std::iter::empty()).is_ok());
    assert!(DeployConsumeOptions::parse(["--protocol".into()].into_iter()).unwrap().protocol);
    assert!(DeployConsumeOptions::parse(["--protocol".into(), "extra".into()].into_iter()).is_err());
    // Mixed-version rollout must fail closed before the saved condition is cleared.
    assert!(retry_launcher(&serde_json::json!({"buildSha":"old"})).is_err());
    assert!(retry_launcher(&serde_json::json!({"deployConsumerProtocol":0,"deployConsumerCli":path})).is_err());
    assert!(retry_launcher(&serde_json::json!({"deployConsumerProtocol":1,"deployConsumerCli":null})).is_err());
    assert!(retry_launcher(&serde_json::json!({"deployConsumerProtocol":1,"deployConsumerCli":"continuum"})).is_err());
    assert_eq!(retry_launcher(&serde_json::json!({"deployConsumerProtocol":1,"deployConsumerCli":path})).unwrap(), path);
    #[cfg(windows)]
    {
        let envelope = continuum_cli_lifecycle::installed_release::Envelope { schema: 2, active_release: "receipt".into(), bootstrap: "bootstrap.exe".into() };
        let mut report = continuum_cli_lifecycle::supervisor_install::TaskReport { present: true, enabled: true, actions: 1, command: envelope.bootstrap.clone(), arguments: envelope.arguments("deploy"), user_sid: "S-1-5-21".into(), ..Default::default() };
        assert!(retry_task_matches(&report, &envelope, "deploy", "S-1-5-21"));
        report.arguments = "deploy-consume".into();
        assert!(!retry_task_matches(&report, &envelope, "deploy", "S-1-5-21"), "legacy direct task cannot satisfy the receipt guard");
        report.arguments = envelope.arguments("deploy");
        report.command = "other.exe".into();
        assert!(!retry_task_matches(&report, &envelope, "deploy", "S-1-5-21"), "changed registration refuses");
    }


    assert!(DeployConsumeOptions::parse(["--now".to_string()].into_iter()).is_err());
    assert!(DeployConsumeOptions::parse(["--retry-resource".into(), "a".repeat(40), "b".repeat(64)].into_iter()).is_ok());
    assert!(DeployConsumeOptions::parse(["--retry-resource".into(), "a".repeat(40)].into_iter()).is_err());
    assert!(DeployConsumeOptions::parse(["--retry-resource".into(), "short".into(), "b".repeat(64)].into_iter()).is_err());
    // The consumer runs on every platform now (the core's actuator launches it); only
    // Windows needs `--service` — macOS promotes a bare reboot to launchd itself, and
    // `reboot` refuses the flag on Linux.
    assert!(consumer_uses_service("windows"));
    assert!(!consumer_uses_service("macos"));
    assert!(!consumer_uses_service("linux"));
}

// Regression for #3929: a first upgrade must leave the legacy core available
// for explicit checkpoint recovery, while normal reboot outcomes may continue.
#[test]
fn legacy_core_requires_explicit_recovery_before_reboot_teardown() {
    let legacy = GracefulStop::LegacyCore("no shutdown handler".into());
    let error = legacy.ensure_teardown_supported(true).unwrap_err();
    assert!(
        legacy.ensure_teardown_supported(false).is_ok(),
        "explicit stop remains available"
    );
    assert!(error.contains("before teardown"));
    assert!(error.contains("checkpoint inspect"));
    assert!(error.contains("checkpoint adopt"));
    // Maintenance must not inherit ordinary stop's permission to force an
    // exit after a missing or incomplete save receipt (5120).
    assert!(legacy.require_durable().is_err());
    for outcome in [
        GracefulStop::NothingRunning,
        GracefulStop::Durable("saved".into()),
        GracefulStop::Incomplete("save failed".into()),
        GracefulStop::NoAnswer("timed out".into()),
    ] {
        assert!(outcome.ensure_teardown_supported(true).is_ok());
        assert_eq!(outcome.require_durable().is_ok(), matches!(outcome,
            GracefulStop::NothingRunning | GracefulStop::Durable(_)));
    }
}

// What this catches (card 9f160b78): missing/truncated process evidence and
// a live PID-file process must never authorize offline memory replacement.
#[test]
fn checkpoint_offline_evidence_keeps_unknown_distinct_from_absent() {
    use std::ffi::OsStr;
    use std::path::Path;
    let own = (41, OsStr::new("continuum"), None);
    let other = (42, OsStr::new("launcher"), None);
    let evidence = CoreProcessEvidence::from_processes(41, [own, other]).unwrap();
    assert!(evidence.ensure_offline(None).is_ok());
    assert!(evidence.ensure_offline(Some(99)).is_ok());
    assert_eq!(
        evidence.ensure_offline(Some(42)).unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    assert!(CoreProcessEvidence::from_processes(41, [other]).is_err());
    let truncated = (43, OsStr::new("continuum-core-"), None);
    assert!(CoreProcessEvidence::from_processes(41, [own, truncated]).is_err());
    let resolved = (
        43,
        OsStr::new("continuum-core-"),
        Some(Path::new("/bin/continuum-core-server")),
    );
    assert!(CoreProcessEvidence::from_processes(41, [own, resolved])
        .unwrap()
        .ensure_offline(None)
        .is_err());
    // The uncertain truncated name must not broaden process termination.
    assert!(!process_matches_fragment(
        truncated.1,
        truncated.2,
        "continuum-core-server"
    ));
    assert!(process_matches_fragment(
        resolved.1,
        resolved.2,
        "continuum-core-server"
    ));
    assert!(
        !process_matches_fragment(
            OsStr::new("python"),
            Some(Path::new("/continuum-core-server-fixtures/python")),
            "continuum-core-server"
        ),
        "parent directories must not turn an unrelated executable into a core kill target"
    );
}

// What this catches (card 9f160b78): inspect --plan <absent volatile.json>
// must not write a valid plan into a Persona's actual memory file.
#[test]
fn inspection_plan_cannot_occupy_checkpoint_or_evidence_storage() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let source_dir = root.join("legacy").join(uuid::Uuid::new_v4().to_string());
    let persona_dir = root.join("personas").join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir_all(&source_dir).unwrap();
    std::fs::create_dir_all(&persona_dir).unwrap();
    let sibling = root.join("legacy").join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir_all(&sibling).unwrap();
    let source = source_dir.join("volatile.json");
    let destination = persona_dir.join("volatile.json");
    for invalid in [
        &source,
        &destination,
        &persona_dir.join(".volatile.lock"),
        &source_dir.join("plan.json"),
        &sibling.join("volatile.json"),
    ] {
        assert!(checkpoint_plan_output(invalid, &source, &destination).is_err());
        assert!(
            !invalid.exists(),
            "inspection must leave managed state absent"
        );
    }
    let outside = root.join("plan.json");
    assert_eq!(
        checkpoint_plan_output(&outside, &source, &destination).unwrap(),
        outside
    );
}

// What this catches (card 9f160b78): a malformed recovery invocation must
// never select/adopt a checkpoint or imply that legacy writers are stopped.
#[test]
fn checkpoint_recovery_requires_explicit_selection_and_offline_precondition() {
    let parse =
        |args: &[&str]| CheckpointCommand::parse(args.iter().map(|arg| arg.to_string()));
    let persona = "68d231fb-1b99-47ea-8615-14538906817a";
    assert!(matches!(
        parse(&[
            "inspect",
            "--source",
            "old/volatile.json",
            "--persona-id",
            persona,
            "--plan",
            "plan.json"
        ])
        .unwrap(),
        CheckpointCommand::Inspect { .. }
    ));
    assert_eq!(
        parse(&["adopt", "--plan", "plan.json", "--legacy-writers-stopped"]).unwrap(),
        CheckpointCommand::Adopt {
            plan: "plan.json".into()
        }
    );
    for args in [
        vec![],
        vec!["adopt", "--plan", "plan.json"],
        vec!["adopt", "--plan", "plan.json", "--force"],
        vec![
            "adopt",
            "--plan",
            "plan.json",
            "--plan",
            "other.json",
            "--legacy-writers-stopped",
        ],
        vec![
            "adopt",
            "--plan",
            "plan.json",
            "--legacy-writers-stopped",
            "--legacy-writers-stopped",
        ],
        vec![
            "inspect",
            "--source",
            "old/volatile.json",
            "--plan",
            "plan.json",
        ],
        vec![
            "inspect",
            "--source",
            "old/volatile.json",
            "--persona-id",
            "invalid",
            "--plan",
            "plan.json",
        ],
        vec![
            "inspect",
            "--source",
            "--persona-id",
            persona,
            "--plan",
            "plan.json",
        ],
    ] {
        assert!(parse(&args).is_err(), "must refuse {args:?}");
    }
}

// what this catches: card 67f53b63 — a missing/misspelled prebuilt path
// must not fall through to a source reboot, and --force only changes leases.
#[test]
fn prebuilt_reboot_options_are_explicit_and_strict() {
    let parse = |args: &[&str]| RebootOptions::parse(args.iter().map(|s| s.to_string()));
    assert_eq!(parse(&[]).unwrap(), RebootOptions::default());
    for args in [
        vec!["--force", "--prebuilt", "release dir/core.exe"],
        vec!["--prebuilt", "release dir/core.exe", "--force"],
    ] {
        let options = parse(&args).unwrap();
        assert!(options.force);
        assert_eq!(
            options.prebuilt.as_deref(),
            Some(std::path::Path::new("release dir/core.exe"))
        );
    }
    for args in [
        vec!["--prebuilt"],
        vec!["--prebuilt", ""],
        vec!["--prebuilt", "--force"],
        vec!["--prebuit", "core.exe"],
        vec!["core.exe"],
        vec!["--force", "--force"],
        vec!["--prebuilt", "one", "--prebuilt", "two"],
        vec!["--force", "--prebuilt", "core.exe", "--from-source"],
        // `--service` ALONE is valid since card 82af11f5 (the warm build supplies the
        // artifact); it is exercised in the valid cases above on Windows.
        vec!["--service", "--service", "--prebuilt", "core.exe"],
        vec!["--validate-only"],
        vec!["--embedding-model", "model.gguf"],
        vec!["--prebuilt", "core.exe", "--embedding-model", "model.gguf"],
        vec![
            "--prebuilt",
            "core.exe",
            "--validate-only",
            "--embedding-model",
        ],
        vec![
            "--prebuilt",
            "core.exe",
            "--validate-only",
            "--embedding-model",
            "one",
            "--embedding-model",
            "two",
        ],
        vec!["--validate-only", "--force", "--prebuilt", "core.exe"],
        vec!["--validate-only", "--service", "--prebuilt", "core.exe"],
        vec![
            "--validate-only",
            "--validate-only",
            "--prebuilt",
            "core.exe",
        ],
    ] {
        assert!(
            parse(&args).is_err(),
            "invalid request must stop at parsing: {args:?}"
        );
    }
    let service = parse(&["--service", "--prebuilt", "core.exe"]);
    let digest = "a".repeat(64);
    let bound_service = parse(&[
        "--service",
        "--prebuilt",
        "core.exe",
        "--service-descriptor-sha",
        &digest,
    ]);
    if cfg!(windows) {
        assert_eq!(
            bound_service.unwrap().service_descriptor_sha.as_deref(),
            Some(digest.as_str())
        );
    } else {
        assert!(bound_service.is_err());
    }
    assert!(parse(&[
        "--prebuilt",
        "core.exe",
        "--validate-only",
        "--service-descriptor-sha",
        &digest
    ])
    .is_err());
    let validate = parse(&["--prebuilt", "core.exe", "--validate-only"]).unwrap();
    assert!(validate.validate_only);
    assert!(!validate.force && !validate.service);
    let embedding = parse(&[
        "--prebuilt",
        "core.exe",
        "--validate-only",
        "--embedding-model",
        "model dir/embed.gguf",
    ])
    .unwrap();
    assert_eq!(
        embedding.embedding_model.as_deref(),
        Some(std::path::Path::new("model dir/embed.gguf"))
    );
    if cfg!(windows) {
        let service = service.unwrap();
        assert!(service.service);
        assert!(!service.force);
    } else {
        assert!(
            service.is_err(),
            "scheduled task deployment is Windows-only"
        );
    }
}

// what this catches: the normal Windows update must refuse a stale task
// BEFORE stopping the live core, even when its description names the new image.
#[test]
fn service_reboot_rejects_stale_action_and_artifact_before_teardown() {
    let tempdir = tempfile::tempdir().unwrap();
    // The directory as an installer receipt names it, BEFORE resolution: on Windows an
    // ordinary path, whose resolved form carries the verbatim `\\?\` prefix.
    let ordinary = tempdir.path().to_path_buf();
    let directory = ordinary.canonicalize().unwrap();
    #[cfg(windows)]
    {
        // Receipt migration and direct reboot share the installer's real
        // exclusive file lease; release permits the next owner to proceed.
        let lease = take_install_lease(&directory).unwrap();
        assert!(take_install_lease(&directory).is_err());
        drop(lease);
        assert!(take_install_lease(&directory).is_ok());
    }
    let artifact = directory.join("continuum-core-server.exe");
    let cli = directory.join("continuum.exe");
    let launcher = directory.join("run-service-hidden.ps1");
    let shell = directory.join("powershell.exe");
    let stale = directory.join("old-core.exe");
    let engine_directory = directory.join("engine-slot");
    std::fs::create_dir(&engine_directory).unwrap();
    let engine = engine_directory.join("llama-server.exe");
    for path in [&artifact, &cli, &launcher, &shell, &stale, &engine] {
        std::fs::write(path, b"installer artifact").unwrap();
    }
    let socket = directory.join("core.sock").display().to_string();
    let description = serde_json::json!({
        "artifact": artifact,
        "socket": socket,
        "cli": cli,
        "launcher": launcher,
        "engine": engine,
        "logDirectory": directory,
    });
    let arguments = format!(
        "-NoProfile -NonInteractive -WindowStyle Hidden -ExecutionPolicy RemoteSigned -File \"{}\" -ExecutablePath \"{}\" -CorePath \"{}\" -SocketPath \"{}\" -EnginePath \"{}\" -LogDirectory \"{}\"",
        launcher.display(), cli.display(), artifact.display(), socket, engine.display(), directory.display(),
    );
    let mut task = CoreServiceTask {
        installed: None,
        working_directory: String::new(),
        description: description.to_string(),
        command: shell.display().to_string(),
        arguments: arguments.clone(),
        enabled: true,
        state: "Ready".to_string(),
    };
    let candidate = PrebuiltCore {
        path: artifact.canonicalize().unwrap(),
        build_sha: "123456789".to_string(),
    };
    task.validate(&candidate, &socket, &shell).unwrap();
    // regression for the 5090 2026-10-06 outage: an engine slot the release names, moved
    // aside, must fail the PRE-stop check (which needs no staged artifact), not only the
    // post-stop `validate`.
    task.runtime_paths_resolve().unwrap();
    let moved = engine_directory.with_file_name("engine-slot.moved");
    std::fs::rename(&engine_directory, &moved).unwrap();
    assert!(task.runtime_paths_resolve().is_err(), "a missing engine refuses before the stop");
    std::fs::rename(&moved, &engine_directory).unwrap();
    // A worker root is part of the registered release, not the scheduler's
    // cwd. Legacy releases omit it; new releases must pass it in the action.
    let mut with_eye = description.clone();
    with_eye["eyeRoot"] = serde_json::json!(directory);
    task.description = with_eye.to_string();
    assert!(task.validate(&candidate, &socket, &shell).is_err());
    task.arguments = format!("{arguments} -EyeRoot \"{}\"", directory.display());
    task.validate(&candidate, &socket, &shell).unwrap();
    with_eye["eyeRoot"] = serde_json::json!("relative/assets");
    task.description = with_eye.to_string();
    assert!(task.validate(&candidate, &socket, &shell).is_err());
    task.description = description.to_string();
    task.arguments = arguments.clone();
    // Engine-only migration can change the selected release without
    // changing its Core artifact; the transfer binds the full descriptor.
    {
        use sha2::{Digest, Sha256};
        let expected = format!("{:x}", Sha256::digest(task.description.as_bytes()));
        task.validate_description_sha(&expected).unwrap();
        let prior = task.description.clone();
        let mut changed: serde_json::Value = serde_json::from_str(&prior).unwrap();
        changed["engine"] = serde_json::json!(directory.join("other-engine.exe"));
        task.description = changed.to_string();
        assert!(task.validate_description_sha(&expected).is_err());
        task.description = prior;
    }
    task.arguments = arguments.replace(
        &artifact.display().to_string(),
        &stale.display().to_string(),
    );
    assert!(task.validate(&candidate, &socket, &shell).is_err());
    task.arguments = arguments;
    assert!(task.validate(&candidate, "different.sock", &shell).is_err());
    task.enabled = false;
    assert!(task.validate(&candidate, &socket, &shell).unwrap_err().contains("disabled"));
    task.enabled = true;
    assert!(task.validate(&candidate, "different.sock", &shell).unwrap_err().contains("different.sock"), "the socket refusal names both sockets");
    // regression for the 5090 2026-10-12 19:31Z (ce84f63ac; Codex traced it): the staged
    // core's path as the installer's receipt names it, RAW, and not already canonical. On
    // Windows that is the ordinary path, whose resolved form carries the verbatim `\\?\`
    // prefix (the failing case); elsewhere a symlinked directory gives the same shape. Once
    // resolved the one way it is the core the descriptor selects; raw, it is refused, and
    // the refusal names both paths.
    let raw = {
        #[cfg(windows)]
        {
            ordinary.join(artifact.file_name().unwrap())
        }
        #[cfg(unix)]
        {
            let link = directory.join("via-link");
            std::os::unix::fs::symlink(&ordinary, &link).unwrap();
            link.join(artifact.file_name().unwrap())
        }
    };
    assert_ne!(raw, resolve_service_path(&raw).unwrap(), "the receipt path is not already canonical");
    let staged = PrebuiltCore { path: resolve_service_path(&raw).unwrap(), build_sha: "123456789".to_string() };
    task.validate(&staged, &socket, &shell).unwrap();
    let unresolved = PrebuiltCore { path: raw.clone(), build_sha: "123456789".to_string() };
    let refused = task.validate(&unresolved, &socket, &shell).unwrap_err();
    assert!(refused.contains("selects artifact") && refused.contains(&raw.display().to_string()), "the artifact refusal names both paths: {refused}");
    let mut old_description = description;
    old_description["artifact"] = serde_json::json!(stale);
    task.description = old_description.to_string();
    assert!(task.validate(&candidate, &socket, &shell).unwrap_err().contains("selects artifact"));
}

// what this catches: card 67f53b63 — provenance is checked before a
// candidate can select the direct launch plan, including on installed nodes.
#[tokio::test]
async fn prebuilt_reboot_rejects_unusable_or_mismatched_candidates() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(
        PrebuiltCore::prepare(&tmp.path().join("absent-core"))
            .await
            .is_err()
    );
    assert!(PrebuiltCore::prepare(tmp.path()).await.is_err());
    let artifact = tmp.path().join("core");
    for report in ["", "unknown", "abc123", "not-a-sha", "def456a"] {
        assert!(PrebuiltCore::from_report(
            artifact.clone(),
            report.into(),
            Some(("abc123f", "git HEAD of this checkout"))
        )
        .is_err());
    }
    for report in ["", "unknown", "abc123", "not-a-sha"] {
        assert!(
            PrebuiltCore::from_report(artifact.clone(), report.into(), None).is_err()
        );
    }
    let ready =
        PrebuiltCore::from_report(artifact, "abc123f0123456789".into(), Some(("abc123f", "git HEAD of this checkout")))
            .unwrap();
    assert_eq!(ready.build_sha, "abc123f0123456789");
}

// Regression for PR3902: failed Git inside a checkout must not become the
// standalone self-anchor. Use real Git metadata and worktree selection;
// process cwd/environment stay local to each command, never global test state.
#[tokio::test]
async fn prebuilt_checkout_lookup_distinguishes_absence_from_failure() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    let standalone = tmp.path().join("standalone");
    let hooks = tmp.path().join("empty-hooks");
    for path in [&repo, &standalone, &hooks] {
        std::fs::create_dir(path).unwrap();
    }
    let git = |cwd: &std::path::Path, args: &[&str]| {
        let output = std::process::Command::new("git")
            .current_dir(cwd)
            .args([
                "-c",
                "user.name=Continuum tests",
                "-c",
                "user.email=tests@example.invalid",
                "-c",
                "commit.gpgsign=false",
                "-c",
                "maintenance.auto=false",
                "-c",
                "gc.auto=0",
            ])
            .arg("-c")
            .arg(format!("core.hooksPath={}", hooks.display()))
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_COMMON_DIR")
            .env_remove("GIT_INDEX_FILE")
            .env_remove("GIT_OBJECT_DIRECTORY")
            .env_remove("GIT_ALTERNATE_OBJECT_DIRECTORIES")
            .args(args)
            .output()
            .expect("Git is required for checkout provenance tests");
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_string()
    };

    git(&repo, &["init", "--quiet"]);
    assert!(
        prebuilt_checkout_sha(&repo, None).await.is_err(),
        "a checkout with no readable HEAD cannot self-anchor"
    );
    git(
        &repo,
        &["commit", "--allow-empty", "--quiet", "-m", "fixture"],
    );
    git(&repo, &["checkout", "--detach", "--quiet", "HEAD"]);
    let head = git(&repo, &["rev-parse", "--short", "HEAD"]);
    assert_eq!(
        prebuilt_checkout_sha(&repo, None).await.unwrap(),
        Some(head.clone())
    );

    git(
        &repo,
        &[
            "worktree",
            "add",
            "--detach",
            "--quiet",
            "../linked",
            "HEAD",
        ],
    );
    let linked = tmp.path().join("linked");
    let nested = linked.join("src");
    std::fs::create_dir(&nested).unwrap();
    assert!(linked.join(".git").is_file());
    assert_eq!(
        prebuilt_checkout_sha(&nested, None).await.unwrap(),
        Some(head.clone())
    );

    // A sibling checkout must not bind a standalone invocation, but an
    // explicit GIT_DIR must bind it, including when that selection is broken.
    let absent = prebuilt_checkout_sha(&standalone, None)
        .await
        .unwrap();
    assert!(absent.is_none());
    assert!(PrebuiltCore::from_report(
        standalone.join("core"),
        "abc123f".into(),
        absent.as_deref().map(|h| (h, "git HEAD of this checkout")),
    )
    .is_ok());
    assert_eq!(
        prebuilt_checkout_sha(&standalone, Some(repo.join(".git").as_os_str()))
            .await
            .unwrap(),
        Some(head),
    );
    assert!(prebuilt_checkout_sha(
        &standalone,
        Some(tmp.path().join("missing").as_os_str())
    )
    .await
    .is_err());

    std::fs::write(linked.join(".git"), "gitdir: missing-checkout\n").unwrap();
    assert!(
        prebuilt_checkout_sha(&nested, None).await.is_err(),
        "a broken worktree gitfile is not a standalone installation"
    );
}

// what this catches (M5, 2026-10-05 01:55Z): a prebuilt core is CI's build of the
// checkout's BUILD KEY, so a docs-only HEAD on top of a core change must accept the
// key's artifact; HEAD itself is the expectation only when no build input was ever
// touched. Checked against HEAD, every docs-only tip refused in two seconds.
#[tokio::test]
async fn a_prebuilt_is_expected_to_be_the_checkouts_build_key_not_its_head() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    let hooks = tmp.path().join("empty-hooks");
    std::fs::create_dir(&repo).unwrap();
    std::fs::create_dir(&hooks).unwrap();
    let git = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .current_dir(&repo)
            .args(["-c", "user.name=t", "-c", "user.email=t@example.invalid", "-c", "commit.gpgsign=false"])
            .arg("-c")
            .arg(format!("core.hooksPath={}", hooks.display()))
            .env_remove("GIT_DIR")
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };
    git(&["init", "-q"]);
    std::fs::create_dir_all(repo.join("core")).unwrap();
    std::fs::write(repo.join("core/lib.rs"), "// core").unwrap();
    git(&["add", "."]);
    git(&["commit", "-qm", "core change"]);
    let key = git(&["rev-parse", "HEAD"]);
    std::fs::write(repo.join("README.md"), "docs only").unwrap();
    git(&["add", "."]);
    git(&["commit", "-qm", "docs only"]);
    let head = git(&["rev-parse", "HEAD"]);
    assert_ne!(key, head);
    let (expected, source) = prebuilt_checkout_build_key(&repo, None, &head);
    assert_eq!(expected, key, "the expectation is the key, not HEAD");
    assert!(source.starts_with("build key"), "{source}");
    // The key's artifact is accepted; HEAD's would not have been built by CI at all.
    assert!(PrebuiltCore::from_report(repo.join("core-bin"), key.clone(), Some((&expected, source))).is_ok());
    assert!(PrebuiltCore::from_report(repo.join("core-bin"), head.clone(), Some((&expected, source))).is_err());
}

// what this catches: card 67f53b63 — an inherited build request, an available
// source script or installed artifact must not redirect an explicit candidate.
// Assert the production Command's path/argv/cwd, not a duplicate launch model.
#[test]
fn prebuilt_reboot_launch_retains_candidate_socket_and_cwd() {
    // The launcher starts media before its service-host child runs. The
    // pre-drain policy must therefore reject elevated/unknown callers,
    // including a stale marker, without attempting recursive relaunch.
    assert!(require_prebuilt_runtime_medium(Ok(0x2000), false).is_ok());
    assert!(require_prebuilt_runtime_medium(Ok(0x2000), true).is_ok());
    assert!(require_prebuilt_runtime_medium(Ok(0x3000), false).unwrap_err().contains("normal-user terminal"));
    assert!(require_prebuilt_runtime_medium(Ok(0x3000), true).is_err());
    assert!(require_prebuilt_runtime_medium(Err("token unavailable".into()), false).is_err());
    let candidate = PrebuiltCore::from_report(
        std::path::PathBuf::from("verified release/core.exe"),
        "abc123f".into(),
        None,
    )
    .unwrap();
    let runtime = continuum_cli_lifecycle::installed_release::Release {
        artifact: "installed/core.exe".into(), socket: "registered socket".into(),
        launcher: "sealed/run-service-hidden.ps1".into(), cli: "sealed/continuum.exe".into(),
        engine: "sealed/engine-c/llama-server.exe".into(), log_directory: "service logs".into(),
        eye_root: Some("installed browser assets".into()),
    };
    for have_script in [false, true] {
        for have_installed in [false, true] {
            for env_from_source in [false, true] {
                // Regression 337b2c67: exercise the SAME command constructor
                // as production, not only the pure plan followed by a
                // disconnected direct-command assertion. Neither inherited
                // source intent nor a discoverable script can build here.
                assert_eq!(plan_launch(LaunchSource::ExplicitPrebuilt(&candidate, Some(&runtime)), env_from_source, have_script, have_installed), LaunchPlan::Prebuilt);
                let command = explicit_prebuilt_command(&candidate, "socket from caller", Some(&runtime), std::path::Path::new("powershell.exe"));
                let args = command.get_args().map(|arg| arg.to_string_lossy().into_owned()).collect::<Vec<_>>();
                assert_eq!(command.get_program(), "powershell.exe");
                for pair in [
                    ("-File", runtime.launcher.as_str()), ("-ExecutablePath", runtime.cli.as_str()),
                    ("-CorePath", "verified release/core.exe"), ("-SocketPath", "socket from caller"),
                    ("-EnginePath", runtime.engine.as_str()), ("-EyeRoot", "installed browser assets"),
                ] { assert!(args.windows(2).any(|args| args == [pair.0, pair.1]), "missing runtime argument {pair:?}: {args:?}"); }
                assert!(!args.iter().any(|arg| arg.contains("start-server") || arg == "cargo"));
                assert_eq!(
                    plan_launch(
                        LaunchSource::Prebuilt(&candidate),
                        env_from_source,
                        have_script,
                        have_installed
                    ),
                    LaunchPlan::Prebuilt,
                );
            }
        }
    }
    let cmd = explicit_prebuilt_command(&candidate, "socket from caller", None, std::path::Path::new("powershell.exe"));
    assert_eq!(cmd.get_program(), candidate.path.as_os_str());
    assert_eq!(
        cmd.get_args().collect::<Vec<_>>(),
        vec![std::ffi::OsStr::new("socket from caller")]
    );
    assert!(
        cmd.get_current_dir().is_none(),
        "inherit repository cwd and its AIRC/workspace scope"
    );
    assert!(deploy_verdict(
        Some("def456a"),
        &candidate.build_sha,
        "prebuilt candidate",
        "replacement core"
    )
    .is_err());
    assert!(deploy_verdict(
        Some("abc123f"),
        &candidate.build_sha,
        "prebuilt candidate",
        "replacement core"
    )
    .is_ok());
}

// Regression for card 25fadb8f: reboot --help stopped a live core. Every
// local verb must take the help return, including when --force is present;
// remote verbs must retain their schema-derived help path.
#[test]
fn lifecycle_help_precedes_actions_and_remote_help_stays_remote() {
    for command in [
        "start",
        "reboot",
        "restart",
        "boot",
        "stop",
        "desktop",
        "ui",
        "orphans",
        "deploy-verify",
        "verify",
        "checkpoint",
    ] {
        for flag in ["-h", "--help"] {
            assert!(local_help_requested(
                command,
                &["--force".into(), flag.into()]
            ));
        }
        assert!(!local_help_requested(command, &["--force".into()]));
    }
    for command in ["help", "-h", "--help"] {
        assert!(local_help_requested(command, &[]));
    }
    assert!(!local_help_requested(
        "serving/status",
        &["--help".into()]
    ));
}

// what this catches: the direct-exec launch path losing the manifest's
// runtime library dirs. On a CUDA Windows node that loss is fatal BEFORE
// main() — STATUS_DLL_NOT_FOUND (0xC0000135), no output, empty start log,
// a core that "just doesn't come up" with nothing to read. Measured on the
// 5090 2026-09-04 with a positive control both ways: the same freshly
// built binary exits 0xC0000135 without the CUDA bin dir on PATH and runs
// with it.
//
// Asserts the SELECTION RULE rather than spawning a process: only
// existing dirs are contributed (a node without CUDA is unaffected), ONE
// CUDA tree's runtime dir is contributed (the one the build links), and
// unrelated directories are never picked up.
#[test]
fn runtime_library_dirs_take_only_real_toolchain_paths() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    let touch = |rel: &str| {
        let f = root.join(rel);
        std::fs::create_dir_all(f.parent().expect("parent")).expect("dir");
        std::fs::write(f, b"").expect("file");
    };
    std::fs::create_dir_all(root.join("tools/cmake/bin")).expect("cmake bin");
    std::fs::create_dir_all(root.join("tools/poppler/Library/bin")).expect("PDF tools");
    // The 5090's trees (2026-10-04): two complete CUDA 12 trees and one complete 13.
    for tree in ["cuda-toolkit", "cuda-env"] {
        let (lib, bin) = if tree == "cuda-toolkit" { ("lib/x64", "bin") } else { ("Library/lib/x64", "Library/bin") };
        touch(&format!("{tree}/{lib}/cuda.lib"));
        touch(&format!("{tree}/{lib}/curand.lib"));
        touch(&format!("{tree}/{bin}/cublas64_12.dll"));
    }
    touch("cuda-13.2/Library/lib/x64/cuda.lib");
    touch("cuda-13.2/Library/lib/x64/curand.lib");
    touch("cuda-13.2/Library/bin/cublas64_13.dll");
    // A newer major with a runtime but no import libs: a build could not link it.
    touch("cuda-14.0/bin/cublas64_14.dll");
    std::fs::create_dir_all(root.join("cuda-build-venv")).expect("empty tree");
    std::fs::create_dir_all(root.join("models")).expect("models");

    let dirs = runtime_library_dirs(root);
    assert!(dirs.contains(&root.join("tools/cmake/bin")), "a provisioned tool's bin/ must be contributed: {dirs:?}");
    assert!(dirs.contains(&root.join("tools/poppler/Library/bin")), "{dirs:?}");
    // what this catches: the multi-tree PATH #4653 re-introduced. Exactly one CUDA dir
    // reaches PATH, and it is the tree the build links (complete, highest major), so the
    // warm build's "CUDA MAJOR MISMATCH" check and the core's loader agree.
    let cuda: Vec<_> = dirs.iter().filter(|d| d.starts_with(root) && d.strip_prefix(root).is_ok_and(|r| r.to_string_lossy().starts_with("cuda-"))).collect();
    assert_eq!(cuda, vec![&root.join("cuda-13.2/Library/bin")], "{dirs:?}");
    assert!(!dirs.iter().any(|d| d.ends_with("models")), "unrelated continuum-root dirs are never runtime library paths: {dirs:?}");

    // A runtime-only node (prebuilt install, nothing to link) still gets its runtime.
    let only = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(only.path().join("cuda-toolkit/bin")).expect("bin");
    std::fs::write(only.path().join("cuda-toolkit/bin/cublas64_12.dll"), b"").expect("dll");
    assert_eq!(declared_cuda_runtime_dir(only.path()), Some(only.path().join("cuda-toolkit/bin")));
}

// what this catches: card d2698cdc — ONNX selection must work without any
// CUDA/toolchain directory and must select the host platform's installed ABI.
#[test]
fn native_launch_selects_platform_ort_without_cuda() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    std::fs::create_dir(root.join("lib")).expect("lib dir");
    for (os, name) in [
        ("windows", "onnxruntime.dll"),
        ("linux", "libonnxruntime.so"),
        ("macos", "libonnxruntime.dylib"),
    ] {
        let library = root.join("lib").join(name);
        std::fs::write(&library, []).expect("installed library fixture");
        let mut cmd = std::process::Command::new("unused");
        cmd.env("ORT_DYLIB_PATH", "");
        cmd.env("PATH", root.join("operator-bin"));
        apply_runtime_library_env_in(&mut cmd, root, os);
        assert_eq!(
            command_env(&cmd, "ORT_DYLIB_PATH"),
            Some(library.into_os_string())
        );
        assert_eq!(
            command_env(&cmd, "PATH"),
            Some(root.join("operator-bin").into_os_string()),
            "ONNX discovery must not require or rewrite a toolchain PATH"
        );
    }
}

// what this catches: config.env overrides live on Command, not in the CLI's
// environment. Preserve both an explicit ORT selection and configured PATH.
#[test]
fn native_launch_preserves_runtime_overrides_and_configured_path() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    let cuda = root.join("cuda-13.2/Library/bin");
    std::fs::create_dir_all(&cuda).expect("cuda bin");
    let explicit_ort = root.join("operator-ort.dll");
    let explicit_path = root.join("operator-bin");
    let mut cmd = std::process::Command::new("unused");
    // Windows preserves spelling but compares environment keys without case.
    // config.env may legally use either spelling; Unix remains case-sensitive.
    let ort_key = if cfg!(windows) {
        "ort_dylib_path"
    } else {
        "ORT_DYLIB_PATH"
    };
    let path_key = if cfg!(windows) { "Path" } else { "PATH" };
    cmd.env(ort_key, &explicit_ort);
    cmd.env(path_key, &explicit_path);
    apply_runtime_library_env_in(&mut cmd, root, "windows");
    assert_eq!(
        command_env(&cmd, "ORT_DYLIB_PATH"),
        Some(explicit_ort.into_os_string()),
        "an explicit override remains authoritative even if the path is absent"
    );
    let actual = command_env(&cmd, "PATH").expect("child PATH");
    assert_eq!(
        std::env::split_paths(&actual).collect::<Vec<_>>(),
        vec![cuda, explicit_path],
        "prepend runtime dirs to the configured child PATH, not the parent's PATH"
    );
    cmd.env_remove(path_key);
    assert!(
        command_env(&cmd, "PATH").is_none(),
        "an explicit removal must not fall through to the parent's environment"
    );
}


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
    let bash = continuum_core::shell_portable::locate_bash().expect("Git bash is a build prerequisite on Windows");
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
use serde_json::json;

fn ptable(pairs: &[(i32, i32)]) -> std::collections::HashMap<i32, i32> {
    pairs.iter().copied().collect()
}

/// The socket a running core is bound to, and the refusal line that names it.
///
/// Measured 2026-09-04 on the Intel Mac: pid 83712 served
/// `~/.continuum/intelmac-core.sock` for an hour while every client-side diagnostic
/// reported `/tmp/continuum-core.sock` — the one path that was not the answer.
mod bound_socket {
    use super::*;

    fn argv(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| s.to_string()).collect()
    }

    /// what this catches: re-deriving `main.rs`'s argument rule instead of calling it.
    /// BOTH spellings of the flag must be peeled — `boot_mode.rs` accepts
    /// `--mode VALUE` as well as `--mode=VALUE`, and the space form is the one the
    /// original heuristic got wrong: "first arg not starting with `-`" returns
    /// **"fail-fast"**, a boot mode reported to the operator as a socket path and
    /// pasted into their remedy line. The equals form alone passes either way, which
    /// is why the first version of this test looked right and proved nothing.
    #[test]
    fn both_flag_spellings_are_peeled_not_just_the_equals_form() {
        assert_eq!(
            socket_from_core_argv(&argv(&[
                "continuum-core-server",
                "--mode",
                "fail-fast",
                "/tmp/a.sock",
            ])),
            Some("/tmp/a.sock".to_string()),
            "the SPACE form's value must never be reported as a socket path"
        );
        assert_eq!(
            socket_from_core_argv(&argv(&[
                "continuum-core-server",
                "--mode=full-citizen",
                "/tmp/a.sock",
            ])),
            Some("/tmp/a.sock".to_string()),
            "a flag before the positional must not be mistaken for the socket"
        );
        assert_eq!(
            socket_from_core_argv(&argv(&["continuum-core-server", "/tmp/a.sock"])),
            Some("/tmp/a.sock".to_string())
        );
    }

    /// what this catches: inventing a socket for a core that passed none. Such a core
    /// resolved the path from ITS environment at launch, which this process cannot read
    /// afterwards — so the honest answer is "unknown", and a `Some(default)` here would
    /// make the hint assert a path the core may well not be on. Both flag spellings,
    /// because the space form is where a "skip the dashes" reading leaks the VALUE.
    #[test]
    fn a_core_with_no_positional_reports_unknown_rather_than_the_default() {
        assert_eq!(
            socket_from_core_argv(&argv(&["continuum-core-server", "--mode=fail-fast"])),
            None
        );
        assert_eq!(
            socket_from_core_argv(&argv(&["continuum-core-server", "--mode", "fail-fast"])),
            None,
            "the space form's value is a boot mode, not an unknown socket"
        );
    }

    /// what this catches: reporting a socket for a core that never bound one. A
    /// malformed `--mode` makes `main.rs` print its error and `exit(2)` BEFORE any
    /// socket is resolved, so such a process provably bound nothing — "unknown" is
    /// the literally correct answer, not a swallowed error. The pid is never hidden
    /// by this: the refusal lists it from `running_core_pids` either way. Only the
    /// unprovable socket claim is withheld, which is this helper's whole contract.
    #[test]
    fn a_malformed_mode_reports_unknown_because_that_core_bound_nothing() {
        assert_eq!(
            socket_from_core_argv(&argv(&[
                "continuum-core-server",
                "--mode",
                "not-a-real-mode",
                "/tmp/a.sock",
            ])),
            None,
            "a core that exits on a bad --mode never reached socket resolution"
        );
    }

    /// what this catches: the hint firing on the ordinary wedged core. A core bound to
    /// the very socket the CLI is using is genuinely not answering; telling that
    /// operator to "reach it elsewhere" would send them chasing a door that is already
    /// open, so the existing message must go out unchanged.
    #[test]
    fn no_hint_when_the_running_core_is_bound_to_the_socket_we_asked_for() {
        let bound = vec![(1, "/tmp/continuum-core.sock".to_string())];
        assert_eq!(
            bound_elsewhere_hint(&bound, "/tmp/continuum-core.sock"),
            None
        );
        assert_eq!(bound_elsewhere_hint(&[], "/tmp/continuum-core.sock"), None);
    }

    /// what this catches: a hint that reports the mismatch without the remedy. The
    /// operator's next keystroke needs the core's ACTUAL path — naming only the
    /// client's is what cost four commands and a wrong "wedged" hypothesis.
    #[test]
    fn hint_names_the_cores_path_and_the_command_that_reaches_it() {
        let bound = vec![(83712, "/home/agent/.continuum/node-core.sock".to_string())];
        let hint = bound_elsewhere_hint(&bound, "/tmp/continuum-core.sock")
            .expect("a core bound elsewhere must produce a hint");
        assert!(
            hint.contains("pid 83712 → /home/agent/.continuum/node-core.sock"),
            "must name which pid is where: {hint}"
        );
        assert!(
            hint.contains("CONTINUUM_CORE_SOCKET=/home/agent/.continuum/node-core.sock"),
            "the remedy must carry the core's path, not the client's: {hint}"
        );
        assert!(
            hint.contains("NOT wedged"),
            "with nothing on our socket, the hint's job is to rule out the wedged case: {hint}"
        );
    }

    /// what this catches: a mixed fleet hiding the outlier — AND the headline denying
    /// the other half of it. With one core on the client's socket and one elsewhere,
    /// only the unreachable core belongs in the list; but the core on our socket IS
    /// genuinely wedged, so leading with an unconditional "NOT wedged" would tell the
    /// operator their real problem does not exist. The earlier version of this test
    /// pinned the right list around exactly that false claim by never asserting on the
    /// headline at all.
    #[test]
    fn mixed_fleet_lists_only_the_outlier_and_does_not_deny_the_wedged_core() {
        let bound = vec![
            (7, "/tmp/continuum-core.sock".to_string()),
            (9, "/home/agent/.continuum/node-core.sock".to_string()),
        ];
        let hint = bound_elsewhere_hint(&bound, "/tmp/continuum-core.sock").expect("hint");
        assert!(
            hint.contains("pid 9"),
            "the unreachable core must appear: {hint}"
        );
        assert!(
            !hint.contains("pid 7"),
            "a core on the asked-for socket is not 'elsewhere': {hint}"
        );
        assert!(
            !hint.contains("NOT wedged"),
            "pid 7 IS on our socket and unresponsive — the hint must not deny it: {hint}"
        );
    }
}

/// The deploy watchdog: is a slow build failing, or just slow?
mod launch_wait {
    use super::*;

    /// what this catches: a WALL-CLOCK bound standing in for a liveness check.
    ///
    /// Both of these are REAL measurements from 2026-09-05 and both deploys SUCCEEDED —
    /// 1,404s for a full CUDA rebuild on the 5090, and an Intel Mac cold build that ran
    /// past the 1,800s ceiling and brought the core up about forty minutes after the CLI
    /// had already reported failure. A verdict keyed on elapsed time calls both of those
    /// dead. Keyed on silence, both are alive, which is the only answer that is correct
    /// on both machines.
    #[test]
    fn a_long_build_that_is_still_talking_is_not_a_failure() {
        let stall = 5 * 60;
        // 23 minutes elapsed means nothing; 10s since the last log line means everything.
        assert_eq!(wait_verdict(10, stall), WaitVerdict::Progressing);
        // Even at the far end of a 40-minute build, recent output is life.
        assert_eq!(wait_verdict(stall - 1, stall), WaitVerdict::Progressing);
    }

    /// what this catches: regression for card 6c91036c — the watchdog killing a HEALTHY
    /// build and reporting it "burned no CPU" while rustc sat at 631% in its own process
    /// group. Reproduced on a real deploy of f49409fa2 on the Intel Mac.
    ///
    /// The readings here are the MEASURED ones, taken 20s apart mid-build: the group total
    /// was ~2589s while a 43-minute rustc was alive, then ~3s once that child finished its
    /// crate and cargo replaced it. Group cputime sums only members alive at that instant,
    /// so a child exiting REMOVES its accumulated time and the total drops.
    ///
    /// THIS TEST MUST DRIVE THE READING DOWN. A test that only ever feeds increasing values
    /// passes against `reading > last` — the buggy comparison — which is presumably how the
    /// defect shipped with the rest of the watchdog well covered. The descent IS the bug.
    #[test]
    fn a_cpu_reading_that_drops_is_progress_because_a_finished_child_leaves_the_group() {
        // The measured pair. Under `>` this returns false and the stall clock never resets
        // again for the life of the build.
        assert!(
            cpu_reading_shows_progress(3, 2589),
            "a group total that FELL because a long rustc finished its crate is a build \
             making progress, not a dead one"
        );
        // Ordinary within-crate accumulation is still progress.
        assert!(cpu_reading_shows_progress(2589, 3));
        // And a build that is genuinely stuck holds the sum CONSTANT — the one reading that
        // must NOT reset the clock, or the watchdog never fires at all.
        assert!(
            !cpu_reading_shows_progress(2589, 2589),
            "an unchanged group total is the signature of a hung exec: it burns no CPU and \
             spawns no children, and it is the only thing this watchdog exists to catch"
        );
        // A climbing total reads as progress, which is what keeps an ordinary build alive.
        // It also means a SPINNING hang is not caught by this watchdog at all: the CPU arm
        // and the log arm reset the same clock, so a spin resets it forever and only
        // MAX_WAIT_SECS bounds it. True of `>` as well — not a regression, just not a
        // guarantee. Asserted so nobody 'fixes' this into an equality check believing the
        // log arm would still cover the spin; it would not.
        assert!(cpu_reading_shows_progress(2590, 2589));
    }

    /// what this catches: the opposite failure — never giving up. A script that has said
    /// nothing for the full stall window has stopped, and the operator needs to be told
    /// that rather than left watching a spinner. The boundary is inclusive so the limit
    /// means "this long is too long", not "this long is fine".
    #[test]
    fn silence_past_the_limit_is_a_stall_and_the_boundary_is_inclusive() {
        let stall = 5 * 60;
        assert_eq!(wait_verdict(stall, stall), WaitVerdict::Stalled);
        assert_eq!(wait_verdict(stall + 1, stall), WaitVerdict::Stalled);
    }

    /// what this catches: misreading `ps -o cputime=`, which is the second progress signal
    /// and the fix for a regression I shipped — a stall clock reset ONLY by log lines
    /// condemns cargo compiling one large crate quietly, which is exactly the cold low-end
    /// build the watchdog protects. Every shape `ps` emits must parse, because a field this
    /// function silently reads as 0 turns a working build back into a false stall.
    #[test]
    fn every_ps_cputime_shape_parses() {
        assert_eq!(cputime_to_secs("0:05"), 5, "mm:ss");
        assert_eq!(cputime_to_secs("12:34"), 12 * 60 + 34);
        assert_eq!(cputime_to_secs("1:02:03"), 3600 + 2 * 60 + 3, "hh:mm:ss");
        assert_eq!(
            cputime_to_secs("2-03:04:05"),
            2 * 86_400 + 3 * 3600 + 4 * 60 + 5,
            "dd-hh:mm:ss — the day field arrives glued to the hour"
        );
        assert_eq!(
            cputime_to_secs("   7:08  "),
            7 * 60 + 8,
            "ps pads its column"
        );
    }

    /// what this catches: a garbage field silently reading as a huge or negative number and
    /// either freezing the stall clock forever or resetting it every tick. Unparseable means
    /// ZERO contribution — the caller then falls back to the log signal, which is the
    /// refuse-to-guess posture the rest of tonight's fixes landed on.
    #[test]
    fn unparseable_cputime_contributes_nothing_rather_than_guessing() {
        assert_eq!(cputime_to_secs(""), 0);
        assert_eq!(cputime_to_secs("   "), 0);
        assert_eq!(cputime_to_secs("?"), 0);
        assert_eq!(cputime_to_secs("not:a:time"), 0);
    }
}

mod launch_policy {
    use super::*;

    /// what this catches: `reboot` — THE deploy path — silently running a
    /// prebuilt artifact instead of the source just edited. Regression for
    /// 7e0c5469a, which made the installed binary the default for BOTH
    /// callers of `launch_core`; the reboot banner still promised "building
    /// fresh binary, then swapping" while exec'ing a month-old binary, so no
    /// edit could reach the running core at all.
    #[test]
    fn reboot_builds_from_source_even_when_an_installed_binary_exists() {
        assert_eq!(
            plan_launch(LaunchSource::FromSource, false, true, true),
            LaunchPlan::Script
        );
    }

    /// what this catches: re-breaking `start` for the no-source-tree user
    /// (the case 7e0c5469a was written for) while fixing reboot. `start`
    /// wants a RUNNING core, not a fresh one — it must never compile when an
    /// artifact is sitting right there.
    #[test]
    fn start_prefers_the_installed_binary_over_a_compile() {
        assert_eq!(
            plan_launch(LaunchSource::Installed, false, true, true),
            LaunchPlan::Installed
        );
    }

    /// what this catches: a reboot on an installed node (no checkout) either
    /// dying with "no start script" or — worse — quietly calling itself a
    /// deploy. It restarts the artifact, and the distinct variant is what
    /// forces the caller to SAY nothing was rebuilt.
    #[test]
    fn reboot_without_a_source_tree_restarts_the_artifact_and_says_so() {
        assert_eq!(
            plan_launch(LaunchSource::FromSource, false, false, true),
            LaunchPlan::InstalledWithoutRebuild
        );
    }

    /// what this catches: substituting a prebuilt binary for an explicit
    /// operator demand to compile. CONTINUUM_FROM_SOURCE asks a specific
    /// question; answering a different one silently is the fallback class
    /// this codebase forbids.
    #[test]
    fn an_explicit_from_source_request_fails_loud_with_no_script() {
        assert_eq!(
            plan_launch(LaunchSource::Installed, true, false, true),
            LaunchPlan::NoLaunchable
        );
    }

    /// what this catches: the env override being ignored on the `start` path
    /// once the policy argument existed — two ways to ask for a source build
    /// and only one honoured.
    #[test]
    fn the_env_override_still_forces_a_source_build_on_start() {
        assert_eq!(
            plan_launch(LaunchSource::Installed, true, true, true),
            LaunchPlan::Script
        );
    }

    /// what this catches: a bare machine (no artifact, no checkout) getting a
    /// launch attempt against nothing instead of one clear error.
    #[test]
    fn nothing_installed_and_no_script_is_a_loud_nothing_to_launch() {
        assert_eq!(
            plan_launch(LaunchSource::Installed, false, false, false),
            LaunchPlan::NoLaunchable
        );
        assert_eq!(
            plan_launch(LaunchSource::FromSource, false, false, false),
            LaunchPlan::NoLaunchable
        );
    }

    /// what this catches: `start` in a fresh checkout before anything is
    /// installed — the fresh-clone front door (#291). Compiling is correct
    /// here; refusing is not.
    #[test]
    fn start_in_a_checkout_with_no_installed_binary_builds() {
        assert_eq!(
            plan_launch(LaunchSource::Installed, false, true, false),
            LaunchPlan::Script
        );
    }
}

/// what this catches: the orphan classifier deciding to KILL a live serving
/// lane. `reboot` reaps every owned engine process not descended from a live
/// core, so a wrong answer here terminates a 21 GB in-service llama-server
/// mid-request. Grandchildren count as descended — engines are spawned
/// through intermediate processes, so a depth-1 check would flag them all.
#[test]
fn in_service_descendants_are_never_orphans() {
    // core 100 -> shim 200 -> engine 300
    let parents = ptable(&[(300, 200), (200, 100)]);
    assert!(
        descends_from(&parents, 300, &[100]),
        "grandchild is in service"
    );
    assert!(descends_from(&parents, 200, &[100]));
    assert!(descends_from(&parents, 100, &[100]), "the core itself");
}

/// what this catches: card d746a1a0 — Windows `/T` on the core killed the
/// recorded gateway before reboot's later orphan exclusion could protect it.
/// Inspect the commands production executes, including the orphan-sweep pass.
#[test]
fn windows_teardown_preserves_only_the_verified_lane_subtree() {
    let parents = ptable(&[
        (100, 1),     // core
        (200, 100),   // gateway launcher, itself expendable
        (8600, 200),  // identity-verified live lane
        (8610, 8600), // helper owned by that lane
        (300, 100),   // eye-node
        (301, 300),   // browser
        (400, 100),   // other worker
        (500, 100),   // stale/unrecorded serving process: not protected
        (501, 500),
        (900, 1), // unrelated process: never a teardown root
    ]);
    let command_args = |plan: Vec<KillStep>| {
        let mut args: Vec<Vec<String>> = plan
            .into_iter()
            .map(|kill| {
                let cmd = kill.command();
                assert_eq!(cmd.get_program(), "taskkill");
                cmd.get_args()
                    .map(|arg| arg.to_str().unwrap().to_owned())
                    .collect()
            })
            .collect();
        args.sort();
        args
    };
    // Duplicate/descendant roots must not produce overlapping tree kills.
    let roots = [300, 200, 100, 100, 301, 500];
    let plan = kill_plan(&roots, &parents, &[8600]);
    assert_eq!(
        plan.first(),
        Some(&KillStep::Process(100)),
        "stop the spawning core first"
    );
    assert_eq!(
        command_args(plan),
        vec![
            vec!["/F", "/PID", "100"],
            vec!["/F", "/PID", "200"],
            vec!["/F", "/T", "/PID", "300"],
            vec!["/F", "/T", "/PID", "400"],
            vec!["/F", "/T", "/PID", "500"],
        ]
    );
    // A surviving launcher ancestor in the owned-orphan pass must ALSO be
    // killed alone, never with /T across the now-adoptable lane beneath it.
    assert_eq!(
        command_args(kill_plan(
            &[200, 300, 301, 400, 500, 501, 8600, 8610],
            &parents,
            &[8600],
        )),
        vec![
            vec!["/F", "/PID", "200"],
            vec!["/F", "/T", "/PID", "300"],
            vec!["/F", "/T", "/PID", "400"],
            vec!["/F", "/T", "/PID", "500"],
        ]
    );
    // Full stop — or no identity-verified lane record — still kills the
    // entire core tree, including the otherwise-preserved gateway.
    assert_eq!(
        command_args(kill_plan(&roots, &parents, &[])),
        vec![vec!["/F", "/T", "/PID", "100"]]
    );
    // An old fixed 64-hop ancestry limit must not turn a deeper protected
    // branch into an apparently unprotected /T target.
    let deep: std::collections::HashMap<i32, i32> =
        (1..=80).map(|pid| (pid, pid - 1)).collect();
    let plan = kill_plan(&[1], &deep, &[80]);
    assert_eq!(plan.len(), 79);
    assert!(plan
        .iter()
        .all(|kill| matches!(kill, KillStep::Process(_))));
}

// what this catches (IntelMac 2026-09-21 11:20Z, card 3ef0986c): the deploy consumer
// is the CORE'S CHILD, so the tree it reaps on a reboot contains itself. With the
// consumer in the keep set (what `keep_with_self` adds at the call site), the core
// gets a `Process` step — it dies alone — and no step names the consumer or its
// branch; without it, `Tree(core)` reaps the hand mid-handoff and the kickstart the
// node is owed is never issued. Both directions: the lane keep still holds beside it.
#[test]
fn the_reaper_is_never_in_its_own_plan() {
    let (core, consumer, lane, eye) = (100, 200, 300, 400);
    let parents = std::collections::HashMap::from([
        (consumer, core),
        (lane, core),
        (eye, core),
        (core, 1),
    ]);
    let plan = kill_plan(&[core], &parents, &keep_with_self_for_test(&[lane], consumer));
    assert_eq!(plan.first(), Some(&KillStep::Process(core)), "the core dies alone, never as a tree");
    assert!(
        !plan.iter().any(|s| matches!(s, KillStep::Tree(p) | KillStep::Process(p) if *p == consumer)),
        "the reaper never appears in its own plan: {plan:?}"
    );
    assert!(!plan.iter().any(|s| matches!(s, KillStep::Tree(p) | KillStep::Process(p) if *p == lane)), "the lane keep still holds");
    assert!(plan.contains(&KillStep::Tree(eye)), "an unprotected sibling is still reaped: {plan:?}");
    // The pre-fix shape, for contrast: with only the lane kept, the consumer IS in the plan.
    let unprotected = kill_plan(&[core], &parents, &[lane]);
    assert!(unprotected.contains(&KillStep::Tree(consumer)), "without the self keep the hand is reaped: {unprotected:?}");
}

/// `keep_with_self` pins the CALLER's pid, which in a test is the test binary; this
/// mirrors it with an explicit self so the plan can be asserted for a chosen pid.
fn keep_with_self_for_test(keep: &[i32], self_pid: i32) -> Vec<i32> {
    let mut with_self = keep.to_vec();
    with_self.push(self_pid);
    with_self
}

/// what this catches: the actual BIGMAMA leak. llama-server pid 37148 ran
/// 24h past its dead parent, holding 127.0.0.1:8090, so every persona that
/// resolved a chat model through that port got an EMBEDDING model. Its
/// parent is absent from the table entirely (dead), which must read as
/// "orphan", not as "unknown, leave it alone".
/// what this catches (card 59052747, 2026-09-20): the Unix executor of the kill plan
/// — until now `let _ = keep;` and a group kill per root, so the plan's `Process(core)`
/// never existed on macOS/Linux and the live lane died with the core on every reboot.
/// Real processes, one group: a leader and its child; a plan that protects the child
/// must leave it alive after the leader is gone, and the same plan with nothing
/// protected must take the whole group (the plain `stop`).
#[cfg(unix)]
#[test]
fn a_protected_child_survives_its_leaders_kill_on_unix() {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};
    fn alive(pid: i32) -> bool {
        unsafe { libc::kill(pid, 0) == 0 }
    }
    fn group(protect: bool) -> (i32, i32) {
        // A leader that execs nothing but waits on one child in ITS group.
        let mut leader = Command::new("sh")
            .args(["-c", "sleep 30 & wait"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn()
            .expect("sh spawns"); // test: the shell exists on every unix CI box
        let leader_pid = leader.id() as i32;
        // Find the child (the sleep) under the leader.
        let child = (0..50)
            .find_map(|_| {
                std::thread::sleep(std::time::Duration::from_millis(50));
                let mut sys = System::new();
                sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
                process_parents(&sys)
                    .into_iter()
                    .find(|(_, parent)| *parent == leader_pid)
                    .map(|(child, _)| child)
            })
            .expect("the leader's child appears"); // test: `sleep` forks within 2.5 s
        let mut sys = System::new();
        sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
        let parents = process_parents(&sys);
        let keep: Vec<i32> = if protect { vec![child] } else { vec![] };
        let plan = kill_plan(&[leader_pid], &parents, &keep);
        assert_eq!(plan, vec![if protect { KillStep::Process(leader_pid) } else { KillStep::Tree(leader_pid) }]);
        for step in plan {
            step.execute();
        }
        let _ = leader.wait();
        std::thread::sleep(std::time::Duration::from_millis(200));
        (leader_pid, child)
    }
    let (leader, child) = group(true);
    assert!(!alive(leader), "the leader is gone");
    assert!(alive(child), "the protected child in the leader's group survives its kill");
    unsafe { libc::kill(child, libc::SIGKILL) };
    let (leader, child) = group(false);
    assert!(!alive(leader));
    assert!(!alive(child), "nothing protected: the group goes with the leader (plain stop)");
}

#[test]
fn a_process_whose_parent_is_gone_is_an_orphan() {
    let parents = ptable(&[(37148, 37856)]); // 37856 itself not present = dead
    assert!(!descends_from(&parents, 37148, &[37920]));
    // And with no core running at all, nothing is in service.
    assert!(!descends_from(&parents, 37148, &[]));
}

/// what this catches: a HANG in `continuum reboot`. A pid table can present
/// a cycle — pid reuse during a racing scan, or a parent pointer into a
/// descendant — and an unbounded ancestor walk would spin forever inside the
/// verb whose entire job is recovering from unknown failures.
#[test]
fn a_cyclic_parent_chain_terminates() {
    let parents = ptable(&[(1, 2), (2, 3), (3, 1)]);
    assert!(
        !descends_from(&parents, 1, &[999]),
        "must terminate, not hang"
    );
    // A cycle that CONTAINS a kept pid still resolves as in-service.
    assert!(descends_from(&parents, 1, &[3]));
}

/// what this catches: pid 0 / self-parent as chain terminators. Both
/// platforms present them, and treating either as a real hop would walk a
/// bogus ancestor or loop.
#[test]
fn root_sentinels_terminate_the_chain() {
    assert!(!descends_from(&ptable(&[(5, 0)]), 5, &[7]));
    assert!(!descends_from(&ptable(&[(5, 5)]), 5, &[7]));
}

// what this catches: the PROCEDURAL param adapter — one generic rule for all
// commands, no per-command switch. Covers the three forms + coercion +
// kebab/snake→camelCase + bare flags. This is the CLI edge of "meet humans/AIs
// in the middle at every interface" (Joel 2026-06-21).
#[test]
fn params_adapt_procedurally_from_args() {
    // No schema available → generic camelCase normalization (pre-schema behavior).
    let no_schema: &[String] = &[];

    // 1. nothing → empty object
    assert_eq!(params_from_args("ping", &[], no_schema).unwrap(), json!({}));

    // 2. positional JSON verbatim (the AI / tool-call path)
    assert_eq!(
        params_from_args("ping", &[r#"{"message":"hi"}"#.to_string()], no_schema).unwrap(),
        json!({ "message": "hi" })
    );

    // 3. --key value with coercion: string stays string, number→number,
    //    bool→bool; keys camelCased from kebab/snake.
    let p = params_from_args(
        "ping",
        &[
            "--message".into(),
            "hi".into(),
            "--round-trip-ms".into(),
            "5".into(),
            "--enabled".into(),
            "true".into(),
        ],
        no_schema,
    )
    .unwrap();
    assert_eq!(
        p,
        json!({ "message": "hi", "roundTripMs": 5, "enabled": true }),
        "coerced + camelCased automatically, one generic rule"
    );

    // bare flag (no value) → true
    assert_eq!(
        params_from_args("ping", &["--verbose".into()], no_schema).unwrap(),
        json!({ "verbose": true })
    );

    // `--key=value` form (muscle memory) — split on first `=`, NOT a junk key.
    assert_eq!(
        params_from_args("ping", &["--filter=data/".into()], no_schema).unwrap(),
        json!({ "filter": "data/" }),
        "--key=value splits correctly (regression: was {{\"filter=data/\": true}})"
    );
    assert_eq!(
        params_from_args("ping", &["--round-trip-ms=5".into()], no_schema).unwrap(),
        json!({ "roundTripMs": 5 }),
        "--key=value coerces + camelCases"
    );

    // a non-flag, non-JSON arg is a clear error (not silently swallowed)
    assert!(params_from_args("ping", &["oops".into()], no_schema).is_err());
}

// what this catches: snake_case Rust-native command fields (e.g. cognition/eval's
// `persona_id`) must be invokable by flag. continuum used to blanket-camelCase every key,
// so `--persona_id` became `personaId` and the server rejected it with
// `missing field persona_id`. With the command's schema known, any spelling of a
// schema field canonicalizes to the exact field name; flags NOT in the schema
// (base fields, schemaless commands) keep the legacy camelCase normalization.
// regression for the 2026-06-25 continuum flag bug.
#[test]
fn flags_canonicalize_to_schema_field_names() {
    let canonical = vec!["persona_id".to_string(), "eval_set".to_string()];
    // every separator/case spelling of a schema field → the exact field name
    for spelling in [
        "--persona_id",
        "--persona-id",
        "--personaId",
        "--PERSONA_ID",
    ] {
        let p = params_from_args("ping", &[spelling.into(), "abc".into()], &canonical).unwrap();
        assert_eq!(p, json!({ "persona_id": "abc" }), "{spelling} → persona_id");
    }
    // `--key=value` form canonicalizes too
    assert_eq!(
        params_from_args("ping", &["--eval-set=x.jsonl".into()], &canonical).unwrap(),
        json!({ "eval_set": "x.jsonl" })
    );
    // a flag NOT in a KNOWN schema is refused, not coerced (see the unknown-flag
    // test below). Identity fields like userId are injected by the connection —
    // no command's schema carries them (349/349 on 2026-09-20) — so there is no
    // base-field carve-out to preserve here.
    assert!(
        params_from_args("ping", &["--room-id".into(), "r1".into()], &canonical).is_err(),
        "non-schema flag against a known schema → refused"
    );
    // with no schema at all, everything is legacy camelCase
    assert_eq!(
        params_from_args("ping", &["--persona-id".into(), "abc".into()], &[]).unwrap(),
        json!({ "personaId": "abc" }),
        "no schema → legacy camelCase (pre-schema behavior preserved)"
    );
}

// what this catches: a typo'd flag being silently coerced into a junk param the
// command ignores, so the caller gets a successful-looking answer to a question
// they did not ask. Measured 2026-09-05: `continuum ping --nonsense-flag`
// returned a healthy pong, exit 0. The ladder is the point: refuse ONLY when the
// schema is known; with no schema we cannot prove a flag is unknown, so the
// camelCase guess must stand or every schemaless command becomes uncallable.
// regression for #3724.
#[test]
fn an_unknown_flag_is_refused_by_name_only_when_the_schema_is_known() {
    let schema = &["message".to_string()];
    let err = params_from_args("ping", &["--nonsense-flag".into()], schema)
        .expect_err("a flag the schema does not have must be refused, not coerced");
    assert!(err.contains("--nonsense-flag"), "names the offending flag: {err}");
    assert!(err.contains("`ping`"), "names the command: {err}");
    assert!(err.contains("--message"), "lists what IS accepted: {err}");
    // every arg shape refuses the same way: `--k=v` and `--k v`, not just bare
    assert!(params_from_args("ping", &["--nonsense=1".into()], schema).is_err());
    assert!(params_from_args("ping", &["--nonsense".into(), "1".into()], schema).is_err());

    // A real field still parses, and still canonicalizes across separators.
    assert_eq!(
        params_from_args("ping", &["--message".into(), "hi".into()], schema).unwrap(),
        json!({ "message": "hi" })
    );

    // NO schema → cannot prove unknown → the pre-schema behaviour is preserved.
    let no_schema: &[String] = &[];
    assert_eq!(
        params_from_args("ping", &["--nonsense-flag".into()], no_schema).unwrap(),
        json!({ "nonsenseFlag": true }),
        "without a schema an unrecognised flag must NOT be refused"
    );
}

#[test]
fn camel_case_normalizes_kebab_and_snake() {
    assert_eq!(to_camel_case("round-trip-ms"), "roundTripMs");
    assert_eq!(to_camel_case("round_trip_ms"), "roundTripMs");
    assert_eq!(to_camel_case("message"), "message");
    // round-trips with the display direction
    assert_eq!(camel_to_kebab("roundTripMs"), "round-trip-ms");
    assert_eq!(camel_to_kebab("message"), "message");
}

// what this catches: the CLI help adapter renders a command's manual in the
// bash paradigm from the SAME schema the AI adapter reads as a tool spec — flags
// (camelCase property → --kebab), types, descriptions, required markers. "The
// instructions manual matches the paradigm" (Joel 2026-06-21), single source.
#[test]
fn help_renders_schema_as_bash_flags() {
    let info = json!({
        "name": "ping",
        "description": "Health check.",
        "paramsSchema": {
            "type": "object",
            "properties": {
                "message": { "type": "string", "description": "Echoed back." },
                "roundTripMs": { "type": "integer" }
            },
            "required": ["message"]
        }
    });
    let help = render_cli_help("ping", &info);
    assert!(help.contains("ping — Health check."), "header: {help}");
    assert!(help.contains("--message"), "flag from property: {help}");
    assert!(help.contains("Echoed back."), "prop description: {help}");
    assert!(help.contains("(required)"), "required marker: {help}");
    assert!(help.contains("--round-trip-ms"), "camel→kebab flag: {help}");
    assert!(help.contains("<integer>"), "type label: {help}");
}

// what this catches (#194): the artifact resolution ORDER is a shared contract with
// tools/scripts/install-service.sh::resolve_core_bin — installed locations before the
// cargo target dir (release before debug), default target dir under ~/.continuum/cache.
// A drift here means the CLI verifies a different binary than the service installer
// deploys — and NOTHING may resolve "next to the CLI exe" (the guess that printed
// "could not locate continuum-core-server" while a 2-day-old core kept serving).
#[test]
fn artifact_resolution_order_matches_install_service() {
    let c = core_artifact_candidates(Path::new("/home/u/.continuum/cache/cargo-target"), Path::new("/home/u/.continuum"));
    let shown: Vec<String> = c.iter().map(|p| p.display().to_string()).collect();
    #[cfg(not(windows))]
    assert_eq!(
        shown,
        vec![
            "/usr/local/bin/continuum-core-server",
            "/home/u/.continuum/bin/continuum-core-server",
            "/home/u/.continuum/cache/cargo-target/release/continuum-core-server",
            "/home/u/.continuum/cache/cargo-target/debug/continuum-core-server",
        ],
        "order is the install-service.sh contract"
    );
    #[cfg(windows)]
    {
        assert!(shown
            .iter()
            .all(|p| p.ends_with("continuum-core-server.exe")));
        assert!(
            !shown.iter().any(|p| p.starts_with("/usr/local/bin")),
            "no unix-only install location on Windows"
        );
    }
    // explicit CARGO_TARGET_DIR overrides the default cache location
    let c = core_artifact_candidates(Path::new("/tgt"), Path::new("/cold/payload"));
    assert!(c.iter().any(|p| p.starts_with("/cold/payload/bin")));
    assert!(!c.iter().any(|p| p.starts_with("/home/u/.continuum/bin")));
    assert!(
        c.iter().any(|p| p.starts_with("/tgt/release"))
            && c.iter().any(|p| p.starts_with("/tgt/debug")),
        "CARGO_TARGET_DIR is honored: {c:?}"
    );
}
