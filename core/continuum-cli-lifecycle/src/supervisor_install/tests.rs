#![cfg(test)]
    use super::*;

    #[test]
    fn maintenance_recovery_preserves_registration_and_refuses_drift() {
        use maintenance::{RecoverySnapshot, TaskSnapshot};
        let snapshot = RecoverySnapshot {
            version: 1,
            legacy: None,
            tasks: [CORE_TASK, DEPLOY_TASK].into_iter().map(|name| TaskSnapshot {
                name: name.into(), xml: "original scheduler document".into(),
                identity_xml: "normalized scheduler document".into(),
                sddl: "original scheduler security".into(), enabled: name == CORE_TASK, state: 4,
            }).collect(),
        };
        let bytes = serde_json::to_vec(&snapshot).unwrap();
        let restored: RecoverySnapshot = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(restored.tasks, snapshot.tasks);
        let mut disabled = restored.tasks.clone();
        for task in &mut disabled { task.enabled = false; task.state = 3; }
        assert!(restored.check_identity(&disabled).is_ok(), "disable and process exit do not change registration custody");
        assert!(restored.check_disabled(&disabled).is_ok());
        disabled[0].enabled = true;
        assert!(restored.check_disabled(&disabled).is_err(), "a re-enabled task loses scheduler exclusion");
        disabled[0].enabled = false;
        for field in ["xml", "security", "name"] {
            let mut drifted = disabled.clone();
            match field {
                "xml" => drifted[0].identity_xml.push_str(" changed action"),
                "security" => drifted[0].sddl.push_str(" changed ACL"),
                _ => drifted[0].name = "OtherTask".into(),
            }
            assert!(restored.check_identity(&drifted).is_err(), "{field} drift must refuse recovery");
        }
        let mut invalid = restored.clone();
        invalid.tasks.swap(0, 1);
        assert!(invalid.validate().is_err());
        invalid = restored.clone(); invalid.version += 1;
        assert!(invalid.validate().is_err());
        assert!(serde_json::from_slice::<RecoverySnapshot>(&bytes[..bytes.len()/2]).is_err());
        assert_eq!(snapshot.tasks[0].xml, "original scheduler document");
        assert!(!snapshot.tasks[1].enabled, "pre-existing disabled tasks remain disabled after restore");
    }

    fn plan() -> SupervisorPlan {
        SupervisorPlan {
            shell: r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe".to_string(),
            arguments: r#"-NoProfile -File "C:\slot\run-service-hidden.ps1" -CorePath "C:\slot\continuum-core-server.exe""#.to_string(),
            description: r#"{"artifact":"C:\\slot\\continuum-core-server.exe","cli":"C:\\slot\\continuum.exe"}"#.to_string(),
            user_sid: "S-1-5-21-1-2-3-1004".to_string(),
            cli: r"C:\slot\continuum.exe".to_string(),
        }
    }

    fn converged(spec: &TaskSpec, trigger: &str) -> TaskReport {
        TaskReport {
            working_directory: String::new(),
            present: true,
            description: spec.description.clone(),
            command: spec.command.clone(),
            arguments: spec.arguments.clone(),
            enabled: true,
            state: "Ready".to_string(),
            logon_type: "S4U".to_string(),
            run_level: "Limited".to_string(),
            user_sid: spec.user_sid.clone(),
            triggers: vec![trigger.to_string()],
            actions: 1,
            sddl: format!(
                "D:(A;;0x1200a9;;;{})(A;ID;0x1f019f;;;BA)(A;ID;0x1f019f;;;SY)",
                spec.user_sid
            ),
        }
    }

    // what this catches: the contract itself, as the scheduler will read it — S4U,
    // least privilege, a BOOT trigger for the core (a logon trigger is the 2 h 42 m
    // outage), restart forever for the core and a bounded repeating tick for the
    // consumer; and that a value with XML metacharacters (quotes in the launcher
    // line, `&` in a path) cannot break the document.
    #[test]
    fn task_xml_states_the_session_independent_contract() {
        let p = plan();
        let core = TaskSpec::core(&p).to_xml();
        assert!(core.contains("<LogonType>S4U</LogonType>"));
        assert!(core.contains("<RunLevel>LeastPrivilege</RunLevel>"));
        assert!(
            core.contains("<BootTrigger>"),
            "the supervisor fires at boot, not at logon"
        );
        assert!(!core.contains("LogonTrigger"));
        assert!(core.contains(
            "<RestartOnFailure><Interval>PT1M</Interval><Count>999</Count></RestartOnFailure>"
        ));
        assert!(core.contains("<ExecutionTimeLimit>PT0S</ExecutionTimeLimit>"));
        assert!(core.contains("<MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>"));
        assert!(
            core.contains(
                "<Arguments>-NoProfile -File &quot;C:\\slot\\run-service-hidden.ps1&quot;"
            ),
            "quotes escaped, not dropped"
        );
        assert!(core.contains("<UserId>S-1-5-21-1-2-3-1004</UserId>"));

        let deploy = TaskSpec::deploy(&p, "2026-09-19T12:00:00".to_string()).to_xml();
        assert!(
            deploy.contains("<LogonType>S4U</LogonType>"),
            "the consumer rides the same principal"
        );
        assert!(deploy.contains("<TimeTrigger><StartBoundary>2026-09-19T12:00:00</StartBoundary>"));
        assert!(deploy
            .contains("<Interval>PT10M</Interval><StopAtDurationEnd>false</StopAtDurationEnd>"));
        assert!(deploy.contains("<ExecutionTimeLimit>PT4H</ExecutionTimeLimit>"));
        assert!(
            !deploy.contains("RestartOnFailure"),
            "a failed deploy tick is the ledger's business, not a retry loop"
        );
        assert!(deploy.contains(
            "<Command>C:\\slot\\continuum.exe</Command><Arguments>deploy-consume</Arguments>"
        ));

        let mut odd = p.clone();
        odd.cli = r"C:\a&b\continuum.exe".to_string();
        assert!(TaskSpec::deploy(&odd, String::new())
            .to_xml()
            .contains("C:\\a&amp;b\\continuum.exe"));
    }

    // what this catches: the verdict the elevation is gated on and verified by. The
    // 5090's own task on 2026-09-19 (Interactive + logon trigger) must read as
    // drifted in exactly those two ways; a converged pair reads as nothing; an
    // absent consumer is one drift, not a crash; a task registered for another
    // user or with a deny ACE is never "converged".
    #[test]
    fn drift_names_every_way_a_task_leaves_the_contract() {
        let p = plan();
        let core_spec = TaskSpec::core(&p);
        let deploy_spec = TaskSpec::deploy(&p, String::new());
        let core = converged(&core_spec, "MSFT_TaskBootTrigger");
        let deploy = converged(&deploy_spec, "MSFT_TaskTimeTrigger");
        assert!(drift(&core, &deploy, &p).is_empty(), "converged is silent");

        let mut interactive = core.clone();
        interactive.logon_type = "Interactive".to_string();
        interactive.triggers = vec!["MSFT_TaskLogonTrigger".to_string()];
        assert_eq!(
            drift(&interactive, &deploy, &p),
            vec![
                (CORE_TASK, Drift::LogonType("Interactive".to_string())),
                (
                    CORE_TASK,
                    Drift::Triggers(vec!["MSFT_TaskLogonTrigger".to_string()])
                ),
            ]
        );

        assert_eq!(
            drift(&core, &TaskReport::default(), &p),
            vec![(DEPLOY_TASK, Drift::Absent)]
        );

        let mut theirs = core.clone();
        theirs.user_sid = "S-1-5-21-9-9-9-1001".to_string();
        theirs.sddl = "D:(A;;0x1200a9;;;S-1-5-21-9-9-9-1001)".to_string();
        assert_eq!(
            drift(&theirs, &deploy, &p),
            vec![
                (
                    CORE_TASK,
                    Drift::Principal("S-1-5-21-9-9-9-1001".to_string())
                ),
                (CORE_TASK, Drift::CallerAccess)
            ]
        );

        let mut other_release = core.clone();
        other_release.arguments = "-File other.ps1".to_string();
        assert_eq!(
            drift(&other_release, &deploy, &p),
            vec![(CORE_TASK, Drift::Action)]
        );

        let mut spelled = core.clone();
        spelled.command = spelled.command.to_ascii_uppercase();
        assert!(
            drift(&spelled, &deploy, &p).is_empty(),
            "case and separators are not drift"
        );
        // Fixed supervisor changes selection through its receipt, never through task actions.
        let mut fixed = p.clone();
        fixed.description=serde_json::json!({"schema":2,"activeRelease":"C:/Users/test/.continuum/install-active.json","bootstrap":"C:/Program Files/Continuum/SID/supervisor/continuum.exe"}).to_string();
        let core_spec = TaskSpec::core(&fixed);
        let deploy_spec = TaskSpec::deploy(&fixed, String::new());
        let mut core = converged(&core_spec, "MSFT_TaskBootTrigger");
        let mut deploy = converged(&deploy_spec, "MSFT_TaskTimeTrigger");
        core.working_directory = "C:/Program Files/Continuum/SID/supervisor".into();
        deploy.working_directory = core.working_directory.clone();
        assert!(drift(&core, &deploy, &fixed).is_empty());
        deploy.arguments = "deploy-consume".into();
        assert_eq!(
            drift(&core, &deploy, &fixed),
            vec![(DEPLOY_TASK, Drift::Action)]
        );
        deploy.arguments = deploy_spec.arguments;
        core.working_directory = "C:/Users/test".into();
        assert_eq!(
            drift(&core, &deploy, &fixed),
            vec![(CORE_TASK, Drift::Action)]
        );
    }

    // what this catches: the ACL read the handoff depends on — RX in either SDDL
    // spelling counts, an inherited admin ACE never counts as the caller's, and a
    // deny for the caller fails closed even beside a grant.
    #[test]
    fn caller_access_reads_grants_and_fails_closed_on_deny() {
        let sid = "S-1-5-21-1-2-3-1004";
        assert!(caller_has_read_execute(
            &format!("D:(A;;0x1200a9;;;{sid})(A;ID;FA;;;BA)"),
            sid
        ));
        assert!(caller_has_read_execute(
            &format!("D:(A;;FRFX;;;{sid})"),
            sid
        ));
        assert!(caller_has_read_execute(&format!("D:(A;;FA;;;{sid})"), sid));
        assert!(
            !caller_has_read_execute("D:(A;ID;FA;;;BA)(A;ID;0x1f019f;;;SY)", sid),
            "admins' inherited ACEs are not the caller's grant"
        );
        assert!(
            !caller_has_read_execute(&format!("D:(A;;FR;;;{sid})"), sid),
            "read without execute cannot run the task"
        );
        assert!(
            caller_has_read_execute(&format!("D:(A;;0x1f01ff;;;{sid})"), sid),
            "full control as a mask contains RX"
        );
        assert!(
            !caller_has_read_execute(&format!("D:(A;;0x120089;;;{sid})"), sid),
            "a read-only mask lacks execute"
        );
        assert!(
            !caller_has_read_execute(&format!("D:(D;;GX;;;{sid})(A;;0x1200a9;;;{sid})"), sid),
            "a deny outranks the grant"
        );
        assert!(!caller_has_read_execute("", sid));
    }

    // what this catches: PowerShell's CLIXML stderr (the consent refusal came back
    // as a 600-byte XML blob) reads as its message line; plain stderr is untouched.
    #[test]
    fn clixml_stderr_reads_as_its_message() {
        let raw = "#< CLIXML\n<Objs Version=\"1.1.0.1\"><Obj S=\"progress\"><MS><PR><AV>Preparing modules for first use.</AV></PR></MS></Obj>\
                   <S S=\"Error\">Start-Process : This command cannot be run due to the error: The operation was canceled by the user._x000D__x000A_</S>\
                   <S S=\"Error\">At line:1 char:37_x000D__x000A_</S><S S=\"Error\">+ ... Stop&apos;; $p = Start-Process_x000D__x000A_</S>\
                   <S S=\"Error\">    + CategoryInfo          : InvalidOperation_x000D__x000A_</S></Objs>";
        assert_eq!(
            plain_stderr(raw),
            "Start-Process : This command cannot be run due to the error: The operation was canceled by the user."
        );
        assert_eq!(plain_stderr("  Access is denied.\n"), "Access is denied.");
    }

    // what this catches: the hidden installer child used to return only an exit
    // code, forcing the operator to rerun the install by hand to see the error.
    #[cfg(windows)]
    #[test]
    fn installer_failure_carries_powershell_diagnostics() {
        let shell = std::path::PathBuf::from(std::env::var_os("SystemRoot").expect("Windows root"))
            .join("System32/WindowsPowerShell/v1.0/powershell.exe");
        let error = run_installer_script(&shell, "throw 'installer diagnostic sentinel'")
            .expect_err("the child throws without running an installer");
        assert!(error.contains("installer diagnostic sentinel"), "{error}");
        assert!(error.contains("installer operation failed"), "{error}");
        let silent = run_installer_script(&shell, "exit 17").expect_err("nonzero exit");
        assert!(
            silent.contains("17") && silent.contains("no stderr"),
            "{silent}"
        );
        run_installer_script(&shell, "exit 0").expect("successful child");
    }

    // what this catches (Fable, review of #4232): the elevated child registers only
    // the plan the consent was given for. A plan file rewritten under the child —
    // another principal, another command — hashes differently from the digest the
    // parent put on the argv, and the child refuses before touching the scheduler.
    #[test]
    fn a_plan_rewritten_under_the_elevated_child_is_refused() {
        let dir = std::env::temp_dir().join(format!("plan-bind-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap(); // unwrap: a temp dir the test owns
        let path = dir.join("plan.json");
        let bytes = serde_json::to_vec_pretty(&plan()).unwrap(); // unwrap: the fixture serializes
        std::fs::write(&path, &bytes).unwrap(); // unwrap: the test's own file
        let sha = plan_digest(&bytes);
        assert_eq!(
            read_bound_plan(&path, &sha).unwrap(),
            plan(),
            "the bytes the parent wrote"
        ); // unwrap: the valid case — an Err here IS the failure
        assert!(
            read_bound_plan(&path, &sha.to_ascii_uppercase()).is_ok(),
            "digest case is not identity"
        );
        let mut swapped = plan();
        swapped.user_sid = "S-1-5-21-9-9-9-500".to_string();
        std::fs::write(&path, serde_json::to_vec_pretty(&swapped).unwrap()).unwrap(); // unwrap: the test's own file
        let why = read_bound_plan(&path, &sha).unwrap_err();
        assert!(
            why.contains("not the one the consent was given for"),
            "{why}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // what this catches: a one-element trigger list arriving as a bare string
    // (PowerShell 5.1's ConvertTo-Json) reads the same as the array form.
    #[test]
    fn a_report_reads_one_trigger_as_string_or_array() {
        let one: TaskReport =
            serde_json::from_str(r#"{"present":true,"triggers":"MSFT_TaskBootTrigger"}"#).unwrap(); // unwrap: the fixture is the valid form under test
        let many: TaskReport =
            serde_json::from_str(r#"{"present":true,"triggers":["MSFT_TaskBootTrigger"]}"#)
                .unwrap(); // unwrap: the fixture is the valid form under test
        assert_eq!(one.triggers, many.triggers);
        let absent: TaskReport = serde_json::from_str(r#"{"present":false}"#).unwrap(); // unwrap: the fixture is the valid form under test
        assert!(!absent.present);
    }

    // what this catches (M5 2026-10-02): a node with airc installed but no login
    // supervisor reads as converged, so a reboot takes it off the mesh silently.
    #[test]
    fn airc_without_a_login_supervisor_is_drift_and_absent_airc_is_one_drift() {
        assert_eq!(airc_drift(true, None), Vec::<AircDrift>::new());
        assert_eq!(
            airc_drift(true, Some("airc-join is registered but disabled".into())),
            vec![AircDrift::Unsupervised(
                "airc-join is registered but disabled".into()
            )],
            "a disabled or stale task is drift, with the registrar's reason, not a converged name"
        );
        assert_eq!(
            airc_drift(false, Some("x".into())),
            vec![AircDrift::Missing]
        );
        assert_eq!(airc_drift(false, None), vec![AircDrift::Missing]);
    }

    // what this catches: the elevated child is a one-plan registrar and nothing
    // else; the flags that make it one travel together or not at all.
    #[test]
    fn install_options_pair_elevated_with_a_plan() {
        let parse = |a: &[&str]| InstallOptions::parse(a.iter().map(|s| s.to_string()));
        assert!(parse(&["--supervisor"]).is_ok());
        let bare = parse(&[]).unwrap(); // unwrap: the valid case — an Err here IS the failure
        assert!(
            [Arm::Supervisor, Arm::Core, Arm::Cli, Arm::Airc]
                .iter()
                .all(|a| bare.runs(*a)),
            "bare install runs every arm"
        );
        let airc = parse(&["--airc"]).unwrap(); // unwrap: the valid case — an Err here IS the failure
        assert!(
            airc.runs(Arm::Airc) && !airc.runs(Arm::Core),
            "--airc names only the mesh arm"
        );
        assert!(parse(&["--airc", "--airc"]).is_err());
        let one = parse(&["--cli"]).unwrap(); // unwrap: the valid case — an Err here IS the failure
        assert!(
            one.runs(Arm::Cli) && !one.runs(Arm::Core) && !one.runs(Arm::Supervisor),
            "naming an arm restricts to it"
        );
        assert!(
            parse(&[
                "--cli",
                "--elevated",
                "--plan",
                "x",
                "--plan-sha",
                &"b".repeat(64)
            ])
            .is_err(),
            "the elevated child is the supervisor's only"
        );
        assert!(parse(&["--supervisor", "--elevated"]).is_err());
        assert!(parse(&["--supervisor", "--plan", "x.json"]).is_err());
        assert!(
            parse(&["--supervisor", "--elevated", "--plan", "x.json"]).is_err(),
            "a plan without its digest is unbound"
        );
        let sha = "a".repeat(64);
        let child = parse(&[
            "--supervisor",
            "--elevated",
            "--plan",
            "x.json",
            "--plan-sha",
            &sha,
        ])
        .unwrap(); // unwrap: the valid case — an Err here IS the failure
        assert!(
            child.elevated
                && child.plan.as_deref() == Some(Path::new("x.json"))
                && child.plan_sha.as_deref() == Some(sha.as_str())
        );
        assert!(parse(&[
            "--supervisor",
            "--elevated",
            "--plan",
            "x.json",
            "--plan-sha",
            "not-hex"
        ])
        .is_err());
        assert!(parse(&["--supervisor", "--supervisor"]).is_err());
        assert!(parse(&["--supervisor", "--check"]).unwrap().check); // unwrap: the valid case — an Err here IS the failure
        assert!(
            parse(&["--supervisor", "--check", "--elevated", "--plan", "x.json"]).is_err(),
            "check reads, elevated writes"
        );
    }
