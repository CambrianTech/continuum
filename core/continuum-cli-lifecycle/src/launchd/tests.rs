    use super::*;

    // what this catches (a1bd8b58): the stage step must write where launchd will exec —
    // the installer's wrapper form today, the bare-binary form after audit item 7. A
    // wrong parse stages a binary launchd never runs and the deploy reports success on
    // the old one (#194's shape, one layer down).
    #[test]
    fn the_slot_is_read_from_either_plist_form_and_never_guessed() {
        let wrapper = r#"<key>ProgramArguments</key>
  <array><string>/bin/bash</string><string>-lc</string><string>export PATH="/x:$PATH"; if [ -f "/Users/j/.continuum/config.env" ]; then set -a; . "/Users/j/.continuum/config.env"; set +a; fi; exec "/Users/j/.continuum/bin/continuum-core-server" "/tmp/continuum-core.sock"</string></array>"#;
        assert_eq!(
            slot_from_plist(wrapper),
            Some(PathBuf::from(
                "/Users/j/.continuum/bin/continuum-core-server"
            ))
        );
        let direct = r#"<key>ProgramArguments</key>
  <array><string>/Users/j/.continuum/bin/continuum-core-server</string><string>/tmp/continuum-core.sock</string></array>"#;
        assert_eq!(
            slot_from_plist(direct),
            Some(PathBuf::from(
                "/Users/j/.continuum/bin/continuum-core-server"
            ))
        );
        assert_eq!(
            slot_from_plist("<key>Label</key><string>x</string>"),
            None,
            "no ProgramArguments = no slot, never a default"
        );
    }

    // what this catches: the receipt's one number. `launchctl print` carries the pid on
    // its own line; a job that is registered but not running has no such line and must
    // read as None, not 0.
    #[test]
    fn the_job_pid_is_read_from_launchctl_print_or_is_none() {
        let running = "com.continuum.core = {\n\tactive count = 1\n\tpath = /x.plist\n\tstate = running\n\n\tpid = 93518\n\tprogram = /bin/bash\n}";
        assert_eq!(pid_from_launchctl_print(running), Some(93518));
        let idle =
            "com.continuum.core = {\n\tstate = not running\n\tlast exit code = (never exited)\n}";
        assert_eq!(pid_from_launchctl_print(idle), None);
    }

    // what this catches (M5 2026-10-02): launchd refused a freshly staged core with
    // OS_REASON_CODESIGNING and the deploy waited five minutes before giving up, with no
    // rollback, so the node stayed dark. The failure is read from launchd's own lines; a
    // job that is running is never a failure, even with a stale exit reason left over.
    // what this catches (review of #4668): a kickstart that launchd refused returned
    // early through `?` after the old core was stopped, skipping the rollback this PR
    // adds and leaving the node dark. Refused kickstart and refused spawn are one road.
    #[test]
    fn a_refused_kickstart_or_spawn_with_nothing_answering_rolls_back() {
        assert_eq!(after_staged_start(Ok(()), true), HandoffNext::Done);
        assert_eq!(
            after_staged_start(
                Err("launchctl kickstart -k system/x failed: Operation not permitted".into()),
                false
            ),
            HandoffNext::RollBack(
                "launchctl kickstart -k system/x failed: Operation not permitted".into()
            )
        );
        assert!(matches!(
            after_staged_start(Err("spawn failed".into()), false),
            HandoffNext::RollBack(_)
        ));
        assert!(matches!(
            after_staged_start(Err("orphan".into()), true),
            HandoffNext::Report(_)
        ));
    }

    // what this catches (review of #4668): the rollback deleted any earlier `.failed`
    // build before keeping the new refused one, destroying what an operator may still be
    // inspecting. Real files: previous restored, refused kept by its own name, an older
    // kept build untouched, and a missing previous refused without touching the slot.
    #[test]
    fn rollback_restores_previous_and_keeps_every_refused_build() {
        let dir = std::env::temp_dir().join(format!("rollback-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let slot = dir.join("continuum-core-server");
        std::fs::write(&slot, b"refused").unwrap();
        std::fs::write(slot.with_extension("prev"), b"serving").unwrap();
        std::fs::write(slot.with_extension("failed"), b"older evidence").unwrap();

        let kept = restore_previous_in(&slot, 42).expect("rollback");
        assert_eq!(std::fs::read(&slot).unwrap(), b"serving");
        assert_eq!(kept, slot.with_extension("failed-42"));
        assert_eq!(std::fs::read(&kept).unwrap(), b"refused");
        assert_eq!(
            std::fs::read(slot.with_extension("failed")).unwrap(),
            b"older evidence"
        );
        assert!(!slot.with_extension("prev").exists());

        let err = restore_previous_in(&slot, 43).unwrap_err();
        assert!(err.contains("no previous build"), "{err}");
        assert_eq!(
            std::fs::read(&slot).unwrap(),
            b"serving",
            "a refused rollback leaves the slot alone"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_spawn_failure_is_read_from_launchctl_print_and_a_running_job_is_not_one() {
        let failed = "com.continuum.core = {\n\tstate = not running\n\truns = 2\n\tlast exit reason = OS_REASON_CODESIGNING\n\tjob state = spawn failed\n}";
        assert_eq!(
            spawn_failed_from_launchctl_print(failed),
            Some(SpawnFailed {
                reason: Some("OS_REASON_CODESIGNING".to_string())
            })
        );
        let running = "com.continuum.core = {\n\tstate = running\n\tpid = 93518\n\tlast exit reason = OS_REASON_CODESIGNING\n\tjob state = spawn failed\n}";
        assert_eq!(spawn_failed_from_launchctl_print(running), None);
        let idle = "com.continuum.core = {\n\tstate = not running\n\tjob state = exited\n}";
        assert_eq!(spawn_failed_from_launchctl_print(idle), None);
    }

    // what this catches (M5 2026-10-03 10:08Z): right after a refused spawn, launchctl print
    // still says `job state = spawn failed` while launchd is about to start the restored
    // build; reading that stale line as the restored start's refusal logged DARK 30 s before
    // the core answered. Only a refusal after launchd spawned again (runs moved) counts.
    #[test]
    fn a_spawn_failure_counts_only_after_launchd_spawned_again() {
        let failed = "com.continuum.core = {\n\tstate = not running\n\truns = 11\n\tlast exit reason = OS_REASON_CODESIGNING\n\tjob state = spawn failed\n}";
        assert_eq!(runs_from_launchctl_print(failed), Some(11));
        assert!(
            !fresh_spawn_attempt(Some(11), Some(11)),
            "the same count is the previous attempt's refusal"
        );
        assert!(
            fresh_spawn_attempt(Some(11), Some(12)),
            "launchd spawned again and was refused again"
        );
        assert!(
            !fresh_spawn_attempt(None, Some(12)),
            "no baseline: not evidence, wait the ceiling"
        );
        assert!(!fresh_spawn_attempt(Some(11), None));

        // The ordering BIGGIEDESK named on #4681: runs read BEFORE the kickstart (11), the
        // kickstart's spawn refused at once (12). Read before, it is a fresh refusal and the
        // wait ends now; read after the kickstart (12 vs 12), it would look stale and the
        // rollback would wait out its five-minute ceiling.
        let pre_start = runs_from_launchctl_print(failed);
        let refused_at_once = failed.replace("runs = 11", "runs = 12");
        let now = runs_from_launchctl_print(&refused_at_once);
        assert!(
            fresh_spawn_attempt(pre_start, now),
            "an instant refusal after a pre-start baseline is this start's"
        );
        assert!(spawn_failed_from_launchctl_print(&refused_at_once).is_some());
        assert!(
            !fresh_spawn_attempt(now, now),
            "a baseline taken after the kickstart hides the instant refusal"
        );

        // The decision wait_owned takes, pinned whole (Cormac on #4681): a refusal counts
        // for this start only with a pre-trigger baseline below the current count.
        let refused = Some(SpawnFailed {
            reason: Some("OS_REASON_CODESIGNING".to_string()),
        });
        assert_eq!(
            refusal_of_this_start(Some(11), &refused_at_once),
            refused,
            "pre-kickstart baseline: this start was refused"
        );
        assert_eq!(
            refusal_of_this_start(Some(12), &refused_at_once),
            None,
            "post-kickstart baseline: the refusal would be hidden"
        );
        assert_eq!(
            refusal_of_this_start(Some(11), failed),
            None,
            "no spawn since the baseline: the previous attempt's line"
        );
        assert_eq!(
            refusal_of_this_start(None, &refused_at_once),
            None,
            "no baseline: wait the ceiling"
        );
    }

    // what this catches (M5 2026-10-03, system log 13:54 and 15:02): the FIRST spawn of every
    // new ad-hoc build is killed by its launch constraint, and the NEXT spawn repairs the LWCR
    // and runs it. Reading that first refusal as final rolled every build back, and the
    // rollback's own kickstart made the repair spawn on the OLD build, so the M5 never advanced
    // past c01065ca6 all day. A codesigning refusal is final only after the repair spawn (which
    // wait_owned triggers on the new build) is refused too; any other refusal stays final at once.
    #[test]
    fn a_launch_constraint_refusal_waits_for_the_repair_spawn() {
        let print = |runs: u64, reason: &str| {
            format!("com.continuum.core = {{\n\tstate = not running\n\truns = {runs}\n\tlast exit reason = {reason}\n\tjob state = spawn failed\n}}")
        };
        let codesigning = SpawnFailed {
            reason: Some("OS_REASON_CODESIGNING".to_string()),
        };
        assert_eq!(
            start_verdict(Some(30), &print(31, "OS_REASON_CODESIGNING")),
            StartVerdict::AwaitingRepairRespawn(codesigning.clone()),
            "first spawn refused by the constraint: launchd's repair respawn is still to come"
        );
        assert_eq!(
            start_verdict(Some(30), &print(32, "OS_REASON_CODESIGNING")),
            StartVerdict::Refused(codesigning),
            "the repair respawn was refused too: the build is refused"
        );
        assert_eq!(
            start_verdict(Some(30), &print(31, "OS_REASON_EXEC")),
            StartVerdict::Refused(SpawnFailed {
                reason: Some("OS_REASON_EXEC".to_string())
            }),
            "a refusal that is not a launch constraint is final on the first spawn"
        );
        assert_eq!(
            start_verdict(Some(31), &print(31, "OS_REASON_CODESIGNING")),
            StartVerdict::Pending,
            "stale line from an earlier attempt"
        );
        let running = "com.continuum.core = {\n\tstate = running\n\tpid = 55819\n\truns = 31\n\tlast exit reason = OS_REASON_CODESIGNING\n\tjob state = running\n}";
        assert_eq!(
            start_verdict(Some(30), running),
            StartVerdict::Pending,
            "the repaired build runs: no refusal"
        );
    }

    // what this catches (2026-09-19 13:32Z, IntelMac): the four states a Mac can be in,
    // and that the one launchd's log named — a gui domain in on-demand-only mode — reads as
    // OWNED-BUT-CANNOT-HEAL, never as SUPERVISED, while the same fact never downgrades a
    // system LaunchDaemon. And the daemon wins when both are registered.
    #[test]
    fn the_verdict_names_the_four_states_and_the_daemon_outranks_the_agent() {
        use SupervisionVerdict::*;
        assert_eq!(
            supervision_verdict(None, None, Some(63839), false),
            Unsupervised
        );
        let gui = Some(Domain::Gui(501));
        assert_eq!(
            supervision_verdict(gui.clone(), Some(93518), Some(93518), true),
            OwnedButCannotHeal {
                domain: Domain::Gui(501)
            },
            "the IntelMac case: launchd owns it and cannot spawn it"
        );
        assert_eq!(
            supervision_verdict(gui.clone(), Some(93518), Some(93518), false),
            Supervised {
                domain: Domain::Gui(501),
                pid: 93518
            }
        );
        assert_eq!(
            supervision_verdict(gui, None, Some(63839), false),
            JobPresentCoreOrphaned {
                job_pid: None,
                core_pid: Some(63839)
            },
            "an agent installed beside an orphan core is not supervision"
        );
        assert_eq!(
            supervision_verdict(Some(Domain::System), Some(7), Some(7), true),
            Supervised {
                domain: Domain::System,
                pid: 7
            },
            "on-demand-only is a gui-domain fact; the daemon is not subject to it"
        );
        assert_eq!(choose_domain(true, true, 501), Some(Domain::System));
        assert_eq!(choose_domain(false, true, 501), Some(Domain::Gui(501)));
        assert_eq!(choose_domain(false, false, 501), None);
        assert!(domain_is_on_demand_only("launchd[1] [gui/501 [100002]:] pending spawn, domain in on-demand-only mode: com.continuum.core"));
        assert!(
            !Unsupervised.is_healthy()
                && Supervised {
                    domain: Domain::System,
                    pid: 1
                }
                .is_healthy()
        );
    }

    // what this catches (audit #4223 item 7): the plist `continuum install` writes runs the
    // BINARY, not a bash wrapper — and the slot it names is the slot `stage` will read back
    // through `slot_from_plist`, so installer and deploy cannot drift. The daemon form
    // carries the operator (never root), the agent form does not; the environment rides in
    // launchd's own dict, XML-escaped, so a `&` in a value cannot break the plist.
    #[test]
    fn the_installed_plist_runs_the_binary_and_round_trips_its_slot() {
        let slot = PathBuf::from("/Volumes/Cold Storage/payloads/j/bin/continuum-core-server");
        let env = vec![
            ("PATH".to_string(), "/a:/b".to_string()),
            ("ORT_DYLIB_PATH".to_string(), "/l/x&y.dylib".to_string()),
        ];
        let spec = |domain: &Domain| {
            render_plist(&PlistSpec {
                domain,
                slot: &slot,
                socket: "/tmp/continuum-core.sock",
                home: Path::new("/Users/j"),
                user: "j",
                env: &env,
            })
        };
        let daemon = spec(&Domain::System);
        assert_eq!(
            slot_from_plist(&daemon),
            Some(slot.clone()),
            "the stage step reads back the slot the installer wrote"
        );
        assert!(
            !daemon.contains("/bin/bash") && !daemon.contains("-lc"),
            "the binary is the command"
        );
        assert!(
            daemon.contains("<key>UserName</key><string>j</string>"),
            "a LaunchDaemon runs as the operator"
        );
        assert!(
            daemon.contains("<key>ORT_DYLIB_PATH</key><string>/l/x&amp;y.dylib</string>"),
            "escaped, in launchd's dict"
        );
        assert!(
            daemon.contains("<key>Crashed</key><true/>")
                && daemon.contains("<key>RunAtLoad</key><true/>")
        );
        assert!(
            daemon.contains("<key>AbandonProcessGroup</key><true/>"),
            "a kickstart or a heal must not SIGKILL the serving lanes in the core's process group (M5: a cold 27B load per deploy)"
        );
        let agent = spec(&Domain::Gui(501));
        assert!(
            !agent.contains("UserName"),
            "an agent already runs as its user"
        );
        assert_eq!(slot_from_plist(&agent), Some(slot));
    }

    // what this catches (the one-verb contract): a converged daemon reads as NOTHING —
    // twice = nothing changed — and each way a Mac can leave the contract is named so the
    // write half changes only that: the script's wrapper plist (two drifts at once), the
    // agent where the daemon was asked for, an owned-but-cannot-heal agent, an orphan.
    #[test]
    fn mac_drift_is_empty_when_converged_and_names_each_departure() {
        let slot = PathBuf::from("/Users/j/.continuum/bin/continuum-core-server");
        let env = vec![("PATH".to_string(), "/a".to_string())];
        let daemon = render_plist(&PlistSpec {
            domain: &Domain::System,
            slot: &slot,
            socket: "/tmp/c.sock",
            home: Path::new("/Users/j"),
            user: "j",
            env: &env,
        });
        assert!(
            mac_drift(
                Some((&Domain::System, &daemon)),
                &Domain::System,
                Some(7),
                Some(7),
                false
            )
            .is_empty(),
            "converged is silent"
        );
        assert_eq!(
            mac_drift(None, &Domain::System, None, Some(3), false),
            vec![MacDrift::Absent]
        );
        let wrapper = "<key>ProgramArguments</key><array><string>/bin/bash</string><string>-lc</string><string>exec \"/x\" \"/s\"</string></array>";
        assert_eq!(
            mac_drift(
                Some((&Domain::Gui(501), wrapper)),
                &Domain::System,
                Some(9),
                Some(9),
                true
            ),
            vec![
                MacDrift::Domain {
                    have: Domain::Gui(501),
                    want: Domain::System
                },
                MacDrift::WrapperCommand,
                MacDrift::KillsLanes,
                MacDrift::CannotHeal,
            ],
            "the script-installed agent on IntelMac, read against the daemon contract"
        );
        let agent = render_plist(&PlistSpec {
            domain: &Domain::Gui(501),
            slot: &slot,
            socket: "/tmp/c.sock",
            home: Path::new("/Users/j"),
            user: "j",
            env: &env,
        });
        assert_eq!(
            mac_drift(
                Some((&Domain::Gui(501), &agent)),
                &Domain::Gui(501),
                Some(9),
                Some(11),
                false
            ),
            vec![MacDrift::NotOwned {
                job_pid: Some(9),
                core_pid: Some(11)
            }],
            "an agent asked for, registered right, beside an orphan core"
        );
        assert!(mac_drift(
            Some((&Domain::Gui(501), &agent)),
            &Domain::Gui(501),
            Some(9),
            Some(9),
            false
        )
        .is_empty());
    }

    // what this catches (Fable on #4232, the same window here): the consent covers the
    // argv, and the elevated half reads the draft AFTER it — so the digest on the argv
    // must change with any byte of the draft, and the system-domain path the elevated
    // half writes is the constant one, never taken from the plan.
    #[test]
    fn the_elevated_half_is_bound_to_the_consented_bytes() {
        let a = sha256_hex(b"<plist>a</plist>");
        let b = sha256_hex(b"<plist>b</plist>");
        assert_eq!(a.len(), 64);
        assert_ne!(a, b, "one byte moved, the digest moved");
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            Domain::System.plist_path(Path::new("/")),
            PathBuf::from("/Library/LaunchDaemons/com.continuum.core.plist")
        );
    }
