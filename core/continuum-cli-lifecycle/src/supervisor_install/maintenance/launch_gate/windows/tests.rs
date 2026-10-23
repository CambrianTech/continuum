#![cfg(test)]
use super::*;

#[test]
fn legacy_claim_and_operation_refuse_ambiguous_recovery() {
    let temp = tempfile::tempdir().unwrap();
    require_idle_claim(temp.path()).unwrap();
    // The real deploy-claim writer creates this parent before publication.
    std::fs::create_dir_all(temp.path().join("run")).unwrap();
    std::fs::write(crate::deploy_claim::claim_path(temp.path()), b"corrupt").unwrap();
    assert!(require_idle_claim(temp.path()).is_err());
    std::fs::write(
        crate::deploy_claim::claim_path(temp.path()),
        format!(
            r#"{{"pid":{},"started_ms":0,"target_sha":"fixture"}}"#,
            std::process::id()
        ),
    )
    .unwrap();
    assert!(require_idle_claim(temp.path())
        .unwrap_err()
        .contains("existing deploy owner"));
    let mut snapshot = RecoverySnapshot {
        version: 1,
        legacy: None,
        tasks: [
            super::super::super::super::CORE_TASK,
            super::super::super::super::DEPLOY_TASK,
        ]
        .into_iter()
        .map(|name| TaskSnapshot {
            name: name.into(),
            xml: "xml".into(),
            identity_xml: "identity".into(),
            sddl: "security".into(),
            enabled: true,
            state: 0,
        })
        .collect(),
    };
    snapshot.check_operation(None).unwrap();
    let digest = "a".repeat(64);
    snapshot.legacy = Some(LegacyPurpose {
        operation_digest: digest.clone(),
        source: canonical_source(temp.path()).unwrap(),
        serving_expectation: serde_json::json!({"schema": 1, "build": "original"}),
        phase: LegacyPhase::Excluded,
        payload: "payload".into(),
        bootstrap: "bootstrap".into(),
        bootstrap_manifest: "{}".into(),
        active: "active".into(),
        previous: None,
    });
    assert!(snapshot.check_operation(None).is_err());
    assert!(snapshot.check_operation(Some(&"b".repeat(64))).is_err());
    snapshot.check_operation(Some(&digest)).unwrap();
    let restored: RecoverySnapshot =
        serde_json::from_slice(&serde_json::to_vec(&snapshot).unwrap()).unwrap();
    restored.check_operation(Some(&digest)).unwrap();
    std::fs::write(
        temp.path().join(JOURNAL),
        serde_json::to_vec(&snapshot).unwrap(),
    )
    .unwrap();
    assert_eq!(
        LegacySchedulerWindow::recovery_binding(temp.path(), temp.path()).unwrap(),
        digest
    );
    assert_eq!(
        LegacySchedulerWindow::recovery_expectation(temp.path(), temp.path()).unwrap()["build"],
        "original"
    );
    let mut oversized = snapshot.clone();
    oversized.legacy.as_mut().unwrap().serving_expectation = serde_json::json!({"schema": 1, "data": "x".repeat(crate::installed_release::MAX_RECEIPT_BYTES as usize)});
    assert!(oversized.validate().is_err());
    let mut unknown_schema = snapshot.clone();
    unknown_schema.legacy.as_mut().unwrap().serving_expectation = serde_json::json!({"schema": 2});
    assert!(unknown_schema.validate().is_err());
    let other = tempfile::tempdir().unwrap();
    assert!(LegacySchedulerWindow::recovery_binding(temp.path(), other.path()).is_err());
    let mut invalid = snapshot.clone();
    invalid.legacy.as_mut().unwrap().source = "relative".into();
    assert!(invalid.validate().is_err());
    let (_, excluded) = read_snapshot(temp.path()).unwrap();
    assert!(check_restoring_startup(&excluded, &excluded.tasks).is_err());
    let mut restoring = snapshot.clone();
    restoring.legacy.as_mut().unwrap().phase = LegacyPhase::Restoring;
    let journal = temp.path().join(JOURNAL);
    let stage = journal.with_extension("json.restoring");
    let bytes = serde_json::to_vec(&restoring).unwrap();
    std::fs::write(&stage, &bytes[..bytes.len() / 2]).unwrap();
    let (old_pin, _) = read_snapshot(temp.path()).unwrap();
    let pin = publish_restoring(&journal, old_pin, &restoring).unwrap();
    assert!(LegacySchedulerWindow::restoration_pending(temp.path(), temp.path()).unwrap());
    assert_eq!(
        LegacySchedulerWindow::recovery_expectation(temp.path(), temp.path()).unwrap()["build"],
        "original"
    );
    let mut staged_tasks = restoring.tasks.clone();
    staged_tasks[1].enabled = false;
    check_restoring_startup(&restoring, &staged_tasks).unwrap();
    assert!(check_restoring_startup(&restoring, &restoring.tasks).is_err());
    let mut partial = staged_tasks.clone();
    partial[0].enabled = false;
    assert!(check_restoring_startup(&restoring, &partial).is_err());
    partial = staged_tasks.clone();
    partial[1].sddl = "changed".into();
    assert!(check_restoring_startup(&restoring, &partial).is_err());
    drop(pin);
    std::fs::write(&stage, b"unknown").unwrap();
    let (old_pin, _) = read_snapshot(temp.path()).unwrap();
    assert!(publish_restoring(&journal, old_pin, &restoring).is_err());
    assert_eq!(std::fs::read(&journal).unwrap(), bytes);
    let retired_source = temp.path().join("retired-source");
    std::fs::create_dir(&retired_source).unwrap();
    let mut retired = snapshot.clone();
    retired.legacy.as_mut().unwrap().source = canonical_source(&retired_source).unwrap();
    retired.legacy.as_mut().unwrap().phase = LegacyPhase::Restoring;
    std::fs::write(&journal, serde_json::to_vec(&retired).unwrap()).unwrap();
    std::fs::remove_dir(&retired_source).unwrap();
    assert_eq!(
        LegacySchedulerWindow::recovery_binding(temp.path(), &retired_source).unwrap(),
        digest
    );
    assert!(LegacySchedulerWindow::restoration_pending(temp.path(), &retired_source).unwrap());
    assert!(LegacySchedulerWindow::recovery_binding(
        temp.path(),
        &temp.path().join("other-missing")
    )
    .is_err());
    assert!(canonical_source(&temp.path().join("missing-parent").join("source")).is_err());

    std::fs::write(temp.path().join(JOURNAL), b"corrupt").unwrap();
    assert!(LegacySchedulerWindow::recovery_binding(temp.path(), temp.path()).is_err());
}

#[tokio::test]
async fn legacy_own_bootstrap_refuses_before_scheduler_or_release_access() {
    let temp = tempfile::tempdir().unwrap();
    let purpose = LegacyPurpose {
        operation_digest: "a".repeat(64),
        source: canonical_source(temp.path()).unwrap(),
        serving_expectation: serde_json::json!({"schema": 1}),
        phase: LegacyPhase::Excluded,
        payload: temp.path().to_str().unwrap().into(),
        bootstrap: std::env::current_exe().unwrap().to_str().unwrap().into(),
        bootstrap_manifest: "{}".into(),
        active: "invalid unopened receipt".into(),
        previous: None,
    };
    assert!(refuse_own_image(temp.path(), &purpose)
        .await
        .unwrap_err()
        .contains("outside the protected bootstrap"));
    assert!(!temp.path().join(JOURNAL).exists());
}

#[test]
fn bootstrap_policy_and_registered_actions_remain_bound() {
    let script = bootstrap_policy_script().unwrap();
    assert!(!script.contains("$PSScriptRoot"));
    assert!(script.contains("function Assert-CoreSupervisorBootstrap {"));
    assert!(script.contains("function Assert-CoreBootstrapAccess {"));
    assert!(!script.contains("function Install-CoreSupervisorBootstrap {"));
    let envelope = crate::installed_release::Envelope {
        schema: 2,
        active_release: r"C:\home\install-active.json".into(),
        bootstrap: r"C:\protected\continuum.exe".into(),
    };
    let mut task = super::super::super::super::TaskReport {
        present: true,
        actions: 1,
        user_sid: "S-1-5-21-1".into(),
        command: envelope.bootstrap.clone(),
        working_directory: r"C:\protected".into(),
        arguments: envelope.arguments("deploy"),
        ..Default::default()
    };
    assert!(registered_action_matches(
        &task,
        &envelope,
        "deploy",
        "S-1-5-21-1"
    ));
    task.command = r"C:\untrusted\continuum.exe".into();
    assert!(!registered_action_matches(
        &task,
        &envelope,
        "deploy",
        "S-1-5-21-1"
    ));
    task.command = envelope.bootstrap.clone();
    task.arguments = envelope.arguments("core");
    assert!(!registered_action_matches(
        &task,
        &envelope,
        "deploy",
        "S-1-5-21-1"
    ));
}

#[tokio::test]
async fn cancelled_waiter_retains_install_and_evidence_custody_until_worker_exit() {
    let temp = tempfile::tempdir().unwrap();
    let startup = StartupPermit::acquire(temp.path()).await.unwrap();
    let second_startup = StartupPermit::acquire(temp.path()).await.unwrap();
    assert!(launch_gate(temp.path(), true).is_err());
    drop(startup);
    drop(second_startup);
    let maintenance = launch_gate(temp.path(), true).unwrap();
    assert!(StartupPermit::acquire(temp.path()).await.is_err());
    drop(maintenance);
    let lease = take_install_lease(temp.path()).unwrap();
    let journal = temp.path().join(JOURNAL);
    std::fs::write(&journal, b"preserved evidence").unwrap();
    let evidence = OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&journal)
        .unwrap();
    let pins = mutation_pins(&lease, &evidence).unwrap();
    let (release, wait) = std::sync::mpsc::channel::<()>();
    let (exited, exit) = std::sync::mpsc::channel();
    let worker = tokio::task::spawn_blocking(move || {
        let _ = wait.recv();
        drop(pins);
        let _ = exited.send(());
    });
    drop(worker); // Cancelling the wait must not cancel foreground custody.
    drop(lease);
    drop(evidence);
    assert!(take_install_lease(temp.path()).is_err());
    assert!(OpenOptions::new().write(true).open(&journal).is_err());
    drop(release);
    exit.recv().unwrap();
    assert!(take_install_lease(temp.path()).is_ok());
    assert!(OpenOptions::new().write(true).open(&journal).is_ok());
}

// Only the external scheduler object is replaced. The production XML
// reader and mutation script execute unchanged against this object.
const SCHEDULER: &str = r#"
$script:failDeploy=$false
$script:tasks=@{}
foreach ($name in @('ContinuumCore','ContinuumDeploy')) {
    $task=[pscustomobject]@{Name=$name;Value=$true;State=4;Security='O:SYG:SYD:(A;;FA;;;SY)';Extra=''}
    $task | Add-Member ScriptProperty Enabled { $this.Value } {
        param($value)
        if ($script:failDeploy -and $this.Name -eq 'ContinuumDeploy') { throw 'injected interrupted mutation' }
        $this.Value=[bool]$value
    }
    $task | Add-Member ScriptProperty Xml {
        '<Task xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task"><Settings><Enabled>' + $this.Value.ToString().ToLowerInvariant() + '</Enabled></Settings>' + $this.Extra + '</Task>'
    }
    $task | Add-Member ScriptMethod GetSecurityDescriptor { param($flags) if ($flags -ne 7) { throw 'owner/group/DACL required' }; $this.Security }
    $script:tasks[$name]=$task
}
$folder=New-Object PSObject
$folder | Add-Member ScriptMethod GetTask { param($name) $script:tasks[$name] }
"#;

fn isolated(script: &str) -> String {
    let connect = "$scheduler=New-Object -ComObject 'Schedule.Service'; $scheduler.Connect()\n$folder=$scheduler.GetFolder('\\')";
    assert_eq!(
        script.matches(connect).count(),
        1,
        "fixture replaces only the scheduler boundary"
    );
    script.replace(connect, "# disposable scheduler object supplied by fixture")
}

#[tokio::test]
async fn interrupted_scheduler_changes_recover_without_replacing_registration() {
    let reader = isolated(READ);
    let original=super::super::super::super::powershell(
        &format!("{SCHEDULER}\n{reader}\n@((Read-TaskSnapshot 'ContinuumCore'),(Read-TaskSnapshot 'ContinuumDeploy')) | ConvertTo-Json -Depth 4 -Compress"), PROBE).await.unwrap();
    let snapshot = RecoverySnapshot {
        version: 1,
        legacy: None,
        tasks: serde_json::from_str(&original).unwrap(),
    };
    let temp = tempfile::tempdir().unwrap();
    let lease = take_install_lease(temp.path()).unwrap();
    assert!(
        take_install_lease(temp.path()).is_err(),
        "real install lease excludes another owner"
    );
    let journal = temp.path().join(JOURNAL);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&journal)
        .unwrap();
    serde_json::to_writer(&mut file, &snapshot).unwrap();
    file.sync_all().unwrap();
    drop(file);
    assert!(
        StartupPermit::acquire(temp.path()).await.is_err(),
        "durable journal blocks startup even without a live maintenance handle"
    );
    let journal_pin = OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&journal)
        .unwrap();
    assert!(OpenOptions::new().write(true).open(&journal).is_err());
    let mut saved: RecoverySnapshot =
        serde_json::from_slice(&std::fs::read(&journal).unwrap()).unwrap();
    saved.legacy = Some(LegacyPurpose {
        operation_digest: "a".repeat(64),
        source: canonical_source(temp.path()).unwrap(),
        serving_expectation: serde_json::json!({"schema": 1, "build": "original"}),
        phase: LegacyPhase::Restoring,
        payload: temp.path().to_str().unwrap().into(),
        bootstrap: "fixture".into(),
        bootstrap_manifest: "{}".into(),
        active: "fixture".into(),
        previous: None,
    });
    let journal_pin = publish_restoring(&journal, journal_pin, &saved).unwrap();
    let core_only = isolated(&mutation_script(&saved, true, true, &journal).unwrap());
    let disable = isolated(&mutation_script(&saved, false, false, &journal).unwrap());
    let restore = isolated(&mutation_script(&saved, true, false, &journal).unwrap());
    let output_path = temp.path().join("observed.json");
    let output_literal = output_path.to_str().unwrap().replace('\'', "''");
    let script = format!(
        r#"{SCHEDULER}
$script:failDeploy=$true
try {{ {disable} }} catch {{ $firstError=$_.Exception.Message }}
if (!$firstError -or $tasks['ContinuumCore'].Enabled -or !$tasks['ContinuumDeploy'].Enabled) {{ throw 'first disable interruption was not observed' }}
$script:failDeploy=$false
{restore}
{disable}
$script:failDeploy=$true
try {{ {restore} }} catch {{ $secondError=$_.Exception.Message }}
if (!$secondError -or !$tasks['ContinuumCore'].Enabled -or $tasks['ContinuumDeploy'].Enabled) {{ throw 'partial restore was not observed' }}
$script:failDeploy=$false
{restore}
$tasks['ContinuumDeploy'].Extra='<Actions />'
try {{ {disable} }} catch {{ $xmlError=$_.Exception.Message }}
if (!$xmlError -or !$tasks['ContinuumCore'].Enabled) {{ throw 'XML drift changed another task' }}
$tasks['ContinuumDeploy'].Extra=''
$tasks['ContinuumDeploy'].Security='changed'
try {{ {disable} }} catch {{ $securityError=$_.Exception.Message }}
if (!$securityError -or !$tasks['ContinuumCore'].Enabled) {{ throw 'security drift changed another task' }}
$tasks['ContinuumDeploy'].Security='O:SYG:SYD:(A;;FA;;;SY)'
# A failed start/acceptance leaves core enabled and deploy disabled.
{core_only}
if (!$tasks['ContinuumCore'].Enabled -or $tasks['ContinuumDeploy'].Enabled) {{ throw 'restoration did not isolate deployment' }}
# Simulate finish enabling deploy followed by a crash before evidence removal.
{restore}
if (!$tasks['ContinuumDeploy'].Enabled) {{ throw 'finish did not restore original deployment state' }}
# The same saved phase resumes verification without migration or new identity.
{core_only}
if (!$tasks['ContinuumCore'].Enabled -or $tasks['ContinuumDeploy'].Enabled) {{ throw 'interrupted finish could not resume verification' }}
{restore}
$finalJson=@((Read-TaskSnapshot 'ContinuumCore'),(Read-TaskSnapshot 'ContinuumDeploy')) | ConvertTo-Json -Depth 4 -Compress
[IO.File]::WriteAllText('{output_literal}', $finalJson, [Text.UTF8Encoding]::new($false))
"#
    );
    let script_path = temp.path().join("scheduler-fixture.ps1");
    std::fs::write(&script_path, script).unwrap();
    let shell = PathBuf::from(std::env::var_os("SystemRoot").unwrap())
        .join("System32/WindowsPowerShell/v1.0/powershell.exe");
    super::super::super::super::run_installer_script(
        &shell,
        &format!("& '{}'", script_path.to_str().unwrap().replace('\'', "''")),
    )
    .unwrap();
    let final_state = std::fs::read_to_string(output_path).unwrap();
    let final_state: Vec<TaskSnapshot> = serde_json::from_str(&final_state).unwrap();
    assert_eq!(
        final_state, snapshot.tasks,
        "exact original XML/security/enabled state recovered"
    );
    assert_eq!(
        std::fs::read(&journal).unwrap(),
        serde_json::to_vec(&saved).unwrap(),
        "interruption keeps original evidence"
    );
    drop(journal_pin);
    drop(lease);
    assert!(take_install_lease(temp.path()).is_ok());
}

#[tokio::test]
async fn partial_disable_restores_only_generic_custody_and_keeps_both_errors() {
    use std::cell::Cell;
    for legacy in [false, true] {
        for restore_ok in [false, true] {
            let attempts = Cell::new(0);
            let evidence_retained = Cell::new(true);
            let result = scheduler_custody::settle_disable(
                (),
                legacy,
                Err("second task refused after first disabled".into()),
                |()| async {
                    attempts.set(attempts.get() + 1);
                    if restore_ok {
                        evidence_retained.set(false);
                        Ok(())
                    } else {
                        Err("identity changed; evidence retained".into())
                    }
                },
            )
            .await;
            assert!(result
                .as_ref()
                .is_err_and(|error| error.contains("second task refused")));
            assert_eq!(attempts.get(), usize::from(!legacy));
            assert_eq!(evidence_retained.get(), legacy || !restore_ok);
            if !legacy && !restore_ok {
                assert!(result
                    .as_ref()
                    .is_err_and(|error| error.contains("identity changed")
                        && error.contains("maintenance recover")));
            }
        }
    }
    // Successful disable transfers custody without invoking its restoration closure.
    let result = scheduler_custody::settle_disable(42, false, Ok(()), |_| async {
        Err("must not run".into())
    })
    .await;
    assert_eq!(result, Ok(42));
}
