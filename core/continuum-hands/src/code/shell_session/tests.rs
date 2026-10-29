#![cfg(test)]

    // what this catches: a credential reaching a citizen's shell, and from there a
    // room. Regression for the 2026-09-06 finding — the core is started with
    // `set -a; . config.env; set +a` (install-service.sh:69), config.env defines
    // HF_TOKEN, and Rust's Command inherits the parent env by default, so `env` from
    // any citizen would have returned the live token into tool output that citizens
    // routinely quote into rooms.
    #[test]
    fn secret_shaped_env_names_are_stripped_and_ordinary_ones_are_not() {
        for name in [
            "HF_TOKEN", "hf_token", "ANTHROPIC_API_KEY", "AWS_SECRET_ACCESS_KEY",
            "GH_TOKEN", "DB_PASSWORD", "SOME_CREDENTIAL", "KEY",
        ] {
            assert!(is_secret_env_name(name), "{name} should be stripped");
        }
        // The five NON-secret config.env entries must survive — stripping these
        // would break serving, storage and probe capture on every citizen shell.
        for name in [
            "CONTINUUM_STORAGE_PATH", "HF_HOME", "CONTINUUM_PERSONA_FLOOR",
            "CONTINUUM_PROBE_DIR", "CONTINUUM_SERVING_PLACEMENT",
        ] {
            assert!(!is_secret_env_name(name), "{name} must NOT be stripped");
        }
    }

    // what this catches: an over-broad predicate breaking working citizens. `KEY` as
    // a bare substring would strip MONKEY_DIR and KEYBOARD_LAYOUT; the needles are
    // `_KEY` / `KEY_` (plus exact `KEY`) precisely so it does not. Measured against a
    // live 96-variable Windows environment: zero false positives.
    #[test]
    fn ordinary_environment_names_survive_the_predicate() {
        for name in [
            "PATH", "HOME", "USERPROFILE", "TEMP", "TMP", "SystemRoot", "ComSpec",
            "PATHEXT", "LANG", "CARGO_TARGET_DIR", "MONKEY_DIR", "KEYBOARD_LAYOUT",
            "TURKEY", "AIRC_DEFAULT_ROOM_NAME", "CONTINUUM_SKIP_SELF_BUILD",
        ] {
            assert!(!is_secret_env_name(name), "{name} must NOT be stripped");
        }
    }

    use super::*;
    use std::fs;

    fn setup_workspace() -> (tempfile::TempDir, tokio::runtime::Runtime) {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::write(dir.path().join("src/main.ts"), "console.log('hello');").unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(crate::system_resources::disk_reporters::prepare_writer_fixture());
        (dir, rt)
    }

    #[test]
    fn test_session_creation() {
        let (dir, _rt) = setup_workspace();
        let session = ShellSession::new("test-session", "persona-1", dir.path()).unwrap();

        assert_eq!(session.id(), "test-session");
        assert_eq!(session.persona_id(), "persona-1");
        // cwd and workspace_root are canonicalized (macOS: /var → /private/var)
        let canonical = dir.path().canonicalize().unwrap();
        assert_eq!(session.cwd(), canonical);
        assert_eq!(session.workspace_root(), canonical);
    }

    // what this catches: the shared-build-cache invariant of the citizen-layer
    // economy (Joel 2026-07-10: layers are cheap ONLY if builds share ONE
    // cargo cache). A session must be born with CARGO_TARGET_DIR pointing at
    // the shared cache, so a peer's `cargo build` inside her CoW layer never
    // materializes a per-layer target/ (~10 GB each). An explicit session env
    // set must still override.
    #[test]
    fn session_inherits_the_shared_cargo_target_dir() {
        let (dir, _rt) = setup_workspace();
        let mut session = ShellSession::new("t", "p1", dir.path()).unwrap();
        let target = session
            .env
            .get("CARGO_TARGET_DIR")
            .expect("born with the shared cargo cache");
        assert!(
            target.contains("cargo-target") || std::env::var("CARGO_TARGET_DIR").is_ok(),
            "points at the shared cache: {target}"
        );
        session.set_env("CARGO_TARGET_DIR".into(), "/tmp/override".into());
        assert_eq!(
            session.env.get("CARGO_TARGET_DIR").unwrap(),
            "/tmp/override"
        );
    }

    #[test]
    fn test_cd_within_workspace() {
        let (dir, _rt) = setup_workspace();
        let mut session = ShellSession::new("test", "p1", dir.path()).unwrap();

        let result = session.cd("src");
        assert!(result.is_ok());
        assert!(session.cwd().ends_with("src"));
    }

    #[test]
    fn test_cd_outside_workspace_blocked() {
        let (dir, _rt) = setup_workspace();
        let mut session = ShellSession::new("test", "p1", dir.path()).unwrap();

        let result = session.cd("..");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("outside workspace"));
    }

    #[test]
    fn test_cd_nonexistent_blocked() {
        let (dir, _rt) = setup_workspace();
        let mut session = ShellSession::new("test", "p1", dir.path()).unwrap();

        let result = session.cd("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_execute_quick_command() {
        let (dir, rt) = setup_workspace();
        let mut session = ShellSession::new("test", "p1", dir.path()).unwrap();

        let result = session.execute_and_wait("echo hello", Some(5000), rt.handle());
        assert!(result.is_ok());

        let response = result.unwrap();
        assert_eq!(response.status, ShellExecutionStatus::Completed);
        assert_eq!(response.exit_code, Some(0));
        assert!(response.stdout.unwrap().contains("hello"));
    }

    #[test]
    fn test_execute_failing_command() {
        let (dir, rt) = setup_workspace();
        let mut session = ShellSession::new("test", "p1", dir.path()).unwrap();

        let result = session.execute_and_wait("exit 42", Some(5000), rt.handle());
        assert!(result.is_ok());

        let response = result.unwrap();
        assert_eq!(response.status, ShellExecutionStatus::Failed);
        assert_eq!(response.exit_code, Some(42));

        // Regression for card 08ef15c8: report the shell's chosen pipeline/OR
        // status, never reinterpret a zero exit as validation of every child.
        let handled = session
            .execute_and_wait("false || true", Some(5000), rt.handle())
            .unwrap();
        assert_eq!(handled.status, ShellExecutionStatus::Completed);
        assert_eq!(handled.exit_code, Some(0));
        // The configured Unix shell need not support this Bash/Zsh option.
        let supports_pipefail = session
            .execute_and_wait("set -o pipefail", Some(5000), rt.handle())
            .unwrap()
            .exit_code
            == Some(0);
        if supports_pipefail {
            for (command, status, exit_code) in [
                (
                    "set +o pipefail; false | cat",
                    ShellExecutionStatus::Completed,
                    0,
                ),
                (
                    "set -o pipefail; false | cat",
                    ShellExecutionStatus::Failed,
                    1,
                ),
            ] {
                let response = session
                    .execute_and_wait(command, Some(5000), rt.handle())
                    .unwrap();
                assert_eq!(response.status, status, "{command}");
                assert_eq!(response.exit_code, Some(exit_code), "{command}");
            }
        }
    }

    // Regression for card 08ef15c8: an advertised persistent shell caused callers
    // to reuse a previous command's variables/cwd/options. Only files, handles
    // and explicitly configured session state survive; each shell is fresh.
    #[test]
    fn test_execute_keeps_files_but_not_child_shell_state() {
        let (dir, rt) = setup_workspace();
        let mut session = ShellSession::new("test", "p1", dir.path()).unwrap();
        session.set_env("CONTINUUM_SHELL_CONTRACT_STATE".into(), "session".into());
        let pipeline = "false | cat";
        let initial = session
            .execute_and_wait(pipeline, Some(5000), rt.handle())
            .unwrap();
        assert!(matches!(initial.exit_code, Some(0 | 1)));
        let supports_pipefail = session
            .execute_and_wait("set -o pipefail", Some(5000), rt.handle())
            .unwrap()
            .exit_code
            == Some(0);
        let option = if initial.exit_code == Some(0) {
            "-o"
        } else {
            "+o"
        };
        let mut command = "cd src && export CONTINUUM_SHELL_CONTRACT_STATE=child && \
                           printf kept > shell-contract-proof"
            .to_string();
        if supports_pipefail {
            command.push_str(&format!(" && set {option} pipefail; {pipeline}"));
        }
        let changed = session
            .execute_and_wait(&command, Some(5000), rt.handle())
            .unwrap();
        if supports_pipefail {
            assert_ne!(changed.exit_code, initial.exit_code);
        } else {
            assert_eq!(changed.exit_code, Some(0));
        }
        let fresh = session
            .execute_and_wait(
                "test -f src/shell-contract-proof && \
                 test \"$CONTINUUM_SHELL_CONTRACT_STATE\" = session && printf fresh",
                Some(5000),
                rt.handle(),
            )
            .unwrap();
        assert_eq!(fresh.exit_code, Some(0), "{fresh:?}");
        assert!(fresh.stdout.unwrap().contains("fresh"));
        let repeated = session
            .execute_and_wait(pipeline, Some(5000), rt.handle())
            .unwrap();
        assert_eq!(repeated.status, initial.status);
        assert_eq!(repeated.exit_code, initial.exit_code);
    }

    #[test]
    fn test_execute_with_cwd() {
        let (dir, rt) = setup_workspace();
        let mut session = ShellSession::new("test", "p1", dir.path()).unwrap();

        // cd into src, then run pwd
        session.cd("src").unwrap();
        let result = session.execute_and_wait("pwd", Some(5000), rt.handle());
        assert!(result.is_ok());

        let response = result.unwrap();
        let stdout = response.stdout.unwrap();
        assert!(
            stdout.contains("src"),
            "pwd should show src dir: {}",
            stdout
        );
    }

    #[test]
    fn test_execute_with_env() {
        let (dir, rt) = setup_workspace();
        let mut session = ShellSession::new("test", "p1", dir.path()).unwrap();

        session.set_env("MY_VAR".to_string(), "hello_world".to_string());
        let result = session.execute_and_wait("echo $MY_VAR", Some(5000), rt.handle());
        assert!(result.is_ok());

        let response = result.unwrap();
        assert!(response.stdout.unwrap().contains("hello_world"));
    }

    #[test]
    fn test_poll_pattern() {
        let (dir, rt) = setup_workspace();
        let mut session = ShellSession::new("test", "p1", dir.path()).unwrap();

        // Execute asynchronously
        let exec_id = session
            .execute(
                "echo line1; echo line2; echo line3",
                Some(5000),
                rt.handle(),
            )
            .unwrap();

        // Poll until finished
        // Short handles must round-trip to the full ID and cannot cross sessions.
        let other = ShellSession::new("other", "p2", dir.path()).unwrap();
        assert!(other.get_execution_state(&exec_id[..8]).is_err());
        assert!(other.get_execution_state(&exec_id).is_err());
        assert!(session.get_watch_handles(&exec_id[..8]).is_ok());
        assert_eq!(session.set_sentinel(&exec_id[..8], &[]).unwrap(), 0);
        let mut all_stdout = Vec::new();
        loop {
            std::thread::sleep(Duration::from_millis(50));
            let poll = session.poll(&exec_id[..8]).unwrap();
            assert_eq!(poll.execution_id, exec_id);
            all_stdout.extend(poll.new_stdout);
            if poll.finished {
                assert_eq!(poll.exit_code, Some(0),
                    "terminal poll status={:?} stderr={:?}", poll.status, poll.new_stderr);
                break;
            }
        }

        assert_eq!(all_stdout, vec!["line1", "line2", "line3"]);
    }

    #[test]
    fn test_timeout() {
        let (dir, rt) = setup_workspace();
        let mut session = ShellSession::new("test", "p1", dir.path()).unwrap();

        // Command that sleeps longer than timeout
        let result = session.execute_and_wait("sleep 30", Some(500), rt.handle());
        assert!(result.is_ok());

        let response = result.unwrap();
        assert_eq!(response.status, ShellExecutionStatus::TimedOut);
    }

    #[test]
    fn test_kill_execution() {
        let (dir, rt) = setup_workspace();
        let mut session = ShellSession::new("test", "p1", dir.path()).unwrap();

        // Start a long-running command
        let exec_id = session.execute("sleep 60", None, rt.handle()).unwrap();

        // Give it a moment to start
        std::thread::sleep(Duration::from_millis(200));

        // Kill it
        session.kill(&exec_id[..8]).unwrap();

        // Poll should show killed
        std::thread::sleep(Duration::from_millis(200));
        let poll = session.poll(&exec_id).unwrap();
        assert!(poll.finished);
        assert_eq!(poll.status, ShellExecutionStatus::Killed);

        // what this catches (card ea06701d; Astra's design): a kill that ended only `bash` and left
        // what it started running (a pipeline, a test runner's workers, a backgrounded server).
        // The execution's own task kills the shell's descendants deepest first, then the shell,
        // and retains any unconfirmed descendant identities for later re-check. Unix: the
        // grandchild's pid is written by the shell.
        #[cfg(unix)]
        {
            let pidfile = dir.path().join("grandchild.pid");
            let exec_id = session.execute("sleep 600 & echo $! > grandchild.pid; wait", None, rt.handle()).unwrap();
            // 10 s is this test's bound on a shell writing one file, not a product timeout.
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            let grandchild: u32 = loop {
                if let Some(pid) = std::fs::read_to_string(&pidfile).ok().and_then(|t| t.trim().parse().ok()) {
                    break pid;
                }
                assert!(std::time::Instant::now() < deadline, "the shell never started its grandchild");
                std::thread::sleep(continuum_process::KILL_POLL);
            };
            let identity = crate::system_resources::process_tree::ProcessIdentity {
                pid: grandchild,
                start_time: match crate::inference::engine_residency::probe_process(grandchild) {
                    crate::inference::engine_residency::ProcessProbe::Present(start) => start,
                    other => panic!("the grandchild is running before the kill: {other:?}"),
                },
            };
            session.kill(&exec_id).unwrap();
            let state = session.get_execution_state(&exec_id).unwrap();
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            while !state.lock().unwrap().process_ended {
                assert!(state.lock().unwrap().process_end_error.is_none(), "the shell's reap succeeds");
                assert!(std::time::Instant::now() < deadline, "the killed shell is reaped");
                std::thread::sleep(continuum_process::KILL_POLL);
            }
            // Regression for combined canary 8ea: direct-child reap does not imply that
            // every signalled descendant has already exited at end_tree's snapshot.
            // Observe those retained identities AND the independently witnessed child;
            // never clear custody or send another signal to make this fixture pass.
            let mut observed = state.lock().unwrap().unconfirmed_descendants.clone();
            observed.push(identity);
            loop {
                let surviving = continuum_process::still_present(&observed);
                if surviving.is_empty() { break; }
                assert!(std::time::Instant::now() < deadline,
                    "the grandchild and recorded descendants must die with their shell: {surviving:?}");
                std::thread::sleep(continuum_process::KILL_POLL);
            }
        }
    }

    #[test]
    fn test_session_info() {
        let (dir, _rt) = setup_workspace();
        let session = ShellSession::new("test-session", "persona-1", dir.path()).unwrap();

        let info = session.info();
        assert_eq!(info.session_id, "test-session");
        assert_eq!(info.persona_id, "persona-1");
        assert_eq!(info.active_executions, 0);
        assert_eq!(info.total_executions, 0);
    }

    #[test]
    fn test_gc_moves_to_history() {
        let (dir, rt) = setup_workspace();
        let mut session = ShellSession::new("test", "p1", dir.path()).unwrap();

        // Run a command to completion
        let result = session.execute_and_wait("echo done", Some(5000), rt.handle()).unwrap();

        assert!(session.history().is_empty());

        // GC should move it to history
        session.gc();

        assert_eq!(session.history().len(), 1);
        assert_eq!(session.history()[0].command, "echo done");
        assert_eq!(session.history()[0].exit_code, Some(0));
        // Regression: starting another command must not erase the first output.
        // Explicit ordering avoids millisecond timestamp ties in fast shells.
        session.history[0].finished_at = Some(1);
        session.execute_and_wait("echo next", Some(5000), rt.handle()).unwrap();
        session.gc();
        session.gc();
        assert_eq!(session.history().len(), 2, "index each receipt only once");
        let receipt = session.get_execution_state(&result.execution_id).unwrap();
        assert!(receipt.lock().unwrap().stdout_lines.join("\n").contains("done"));
        assert_eq!(receipt.lock().unwrap().exit_code, Some(0));

        // 5058: bounded history must not own an unresolved process's cache hold.
        let context = crate::identity::hands_context::HandsContext::fresh();
        let admission = Arc::new(crate::identity::hands_gate::admit(context).unwrap());
        let held = Arc::new(());
        let witness = Arc::downgrade(&held);
        receipt.lock().unwrap().unconfirmed_descendants.push(crate::system_resources::process_tree::ProcessIdentity { pid: 4242, start_time: 7 });
        drop(ShellCargoCustody { state: receipt.clone(), admission: admission.clone(), kept: held, completed: true });
        receipt.lock().unwrap().unconfirmed_descendants.clear();
        drop(admission);
        // A direct-child exit alone does not prove output-holding descendants
        // ended when cancellation interrupts the remainder of the shell task.
        let cancelled_context = crate::identity::hands_context::HandsContext::fresh();
        let cancelled_admission = Arc::new(crate::identity::hands_gate::admit(cancelled_context).unwrap());
        let cancelled_hold = Arc::new(());
        let cancelled_witness = Arc::downgrade(&cancelled_hold);
        drop(ShellCargoCustody { state: receipt.clone(), admission: cancelled_admission.clone(), kept: cancelled_hold, completed: false });
        drop(cancelled_admission);
        assert!(!rt.block_on(crate::identity::hands_gate::close(cancelled_context)).unknown.is_empty());
        crate::identity::hands_gate::retain_processes(cancelled_context, Vec::new());
        assert!(cancelled_witness.upgrade().is_some(), "unknown output lifetime is not cleared by an empty known-descendant list");

        // Count eviction keeps only the newest terminal receipt, not running work.
        let running = session.execute("sleep 60", None, rt.handle()).unwrap();
        session.retain_completed(1, usize::MAX);
        assert_eq!(session.history().len(), 1);
        assert!(session.get_execution_state(&result.execution_id).is_err());
        assert!(witness.upgrade().is_some(), "history eviction must not release unresolved process custody");
        crate::identity::hands_gate::retain_processes(context, Vec::new());
        assert!(witness.upgrade().is_none());
        assert!(session.get_execution_state(&running).is_ok());
        let retained_id = session.history()[0].execution_id.clone();
        assert!(session.get_execution_state(&retained_id).is_ok());

        // Byte eviction catches a huge single line and retains exit metadata.
        session.get_execution_state(&retained_id).unwrap().lock().unwrap()
            .stderr_lines.push("x".repeat(4096));
        session.retain_completed(1, 1024);
        assert!(session.get_execution_state(&retained_id).unwrap_err().contains("evicted"));
        assert_eq!(session.history()[0].exit_code, Some(0));
        assert_eq!(session.execution_ids().filter(|id| *id == retained_id).count(), 1);
        assert!(session.get_execution_state(&running).is_ok());
        session.destroy();
    }

    #[test]
    fn test_destroy_kills_running() {
        let (dir, rt) = setup_workspace();
        let mut session = ShellSession::new("test", "p1", dir.path()).unwrap();

        // Start long-running command
        let _exec_id = session.execute("sleep 60", None, rt.handle()).unwrap();

        std::thread::sleep(Duration::from_millis(200));

        // Destroy should kill it
        session.destroy();
        assert!(session.executions.is_empty());
    }

    #[test]
    fn test_multiple_executions() {
        let (dir, rt) = setup_workspace();
        let mut session = ShellSession::new("test", "p1", dir.path()).unwrap();

        // Run multiple sequential commands
        let r1 = session
            .execute_and_wait("echo first", Some(5000), rt.handle())
            .unwrap();
        let r2 = session
            .execute_and_wait("echo second", Some(5000), rt.handle())
            .unwrap();
        let r3 = session
            .execute_and_wait("echo third", Some(5000), rt.handle())
            .unwrap();

        assert_eq!(r1.status, ShellExecutionStatus::Completed);
        assert_eq!(r2.status, ShellExecutionStatus::Completed);
        assert_eq!(r3.status, ShellExecutionStatus::Completed);
        assert!(r1.stdout.unwrap().contains("first"));
        assert!(r2.stdout.unwrap().contains("second"));
        assert!(r3.stdout.unwrap().contains("third"));
    }

    #[test]
    fn test_command_reads_workspace_files() {
        let (dir, rt) = setup_workspace();
        let mut session = ShellSession::new("test", "p1", dir.path()).unwrap();

        let result = session
            .execute_and_wait("cat src/main.ts", Some(5000), rt.handle())
            .unwrap();

        assert_eq!(result.status, ShellExecutionStatus::Completed);
        assert!(result.stdout.unwrap().contains("console.log"));
    }

    #[test]
    fn test_stderr_capture() {
        let (dir, rt) = setup_workspace();
        let mut session = ShellSession::new("test", "p1", dir.path()).unwrap();

        let result = session
            .execute_and_wait("echo error_msg >&2", Some(5000), rt.handle())
            .unwrap();

        assert_eq!(result.status, ShellExecutionStatus::Completed);
        assert!(result.stderr.unwrap().contains("error_msg"));
    }
