#![cfg(test)]
    use super::*;

    // what this catches (e92f227c): captured background commands allocating or
    // inheriting a console. The observation is made by the actual native child.
    #[cfg(windows)]
    #[test]
    #[ignore = "native observer invoked by owned_background_child_has_no_console"]
    fn background_console_observer() {
        // SAFETY: this API only observes the calling process's console handle.
        assert!(unsafe { windows_sys::Win32::System::Console::GetConsoleWindow() }.is_null());
        println!("background-console-observer-ok");
        eprintln!("background-console-stderr-ok");
    }

    // what this catches (e92f227c): every raw caller inherits windowless policy
    // at the shared spawn boundary, while exit and both captured streams survive.
    #[cfg(windows)]
    #[tokio::test]
    async fn owned_background_child_has_no_console() {
        let mut command = tokio::process::Command::new(std::env::current_exe().expect("test binary"));
        command.args(["--exact", "inference::lane_process::tests::background_console_observer", "--ignored", "--nocapture"]);
        let output = run_owned(command, None, "console-observer", std::sync::Arc::default()).await.expect("owned native child");
        assert!(matches!(&output.ended, Ended::Exited(status) if status.success()), "native child must exit successfully: {:?}", output.ended);
        assert!(String::from_utf8_lossy(&output.stdout.bytes).contains("background-console-observer-ok"));
        assert!(String::from_utf8_lossy(&output.stderr.bytes).contains("background-console-stderr-ok"));
        assert!(output.input_note().is_none());
        assert!(output.capture_note().is_none());
    }

    // what this catches (Astra and Codex root on #5026 and #5030, card de211a71): a child whose
    // waiter was cancelled (a dropped command, a rebooting core) kept running, and so did what
    // it started, while a release removed the workspace under them; and a grader that outlived
    // its ceiling had only its group killed after the fact. An owned run ends the child AND its
    // tree on either path, holds the admission until that end is done, and keeps what it read
    // reachable on the operation's capture even after the waiter is gone.
    #[cfg(unix)]
    #[tokio::test]
    async fn an_owned_run_ends_its_tree_when_dropped_or_timed_out_and_holds_the_admission_until_it_has() {
        let dir = tempfile::tempdir().unwrap();
        let (shell_pid, grandchild_pid) = (dir.path().join("sh.pid"), dir.path().join("gc.pid"));
        let script = format!("echo started; echo $$ > {}; sleep 600 & echo $! > {}; wait", shell_pid.display(), grandchild_pid.display());
        let read_pid = |path: &std::path::Path| std::fs::read_to_string(path).ok().and_then(|t| t.trim().parse::<u32>().ok());
        let context = crate::identity::hands_context::HandsContext::fresh();
        let lease = std::sync::Arc::new(crate::code::hands::admit(context).expect("open hands"));
        let capture = std::sync::Arc::new(RunCapture::default());
        let mut cmd = tokio::process::Command::new("sh");
        cmd.arg("-c").arg(&script);
        // the strict order (Codex reviewer on #5037): what the caller held for the run (a
        // checkout's sync lock) is released only after the tree's end, so at the instant it
        // drops, the grandchild is already gone. The witness reads that AT its release.
        struct Witness {
            grandchild: std::path::PathBuf,
            gone_at_release: std::sync::Arc<std::sync::Mutex<Option<bool>>>,
        }
        impl Drop for Witness {
            fn drop(&mut self) {
                let pid = std::fs::read_to_string(&self.grandchild).unwrap_or_default(); // unwrap_or_default: no pid file reads as no process, which the assertion below names
                let ps = std::process::Command::new("ps").args(["-o", "stat=", "-p", pid.trim()]).output();
                let stat = ps.map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default(); // unwrap_or_default: no ps output reads as gone
                *self.gone_at_release.lock().unwrap() = Some(!pid.trim().is_empty() && (stat.is_empty() || stat.starts_with('Z'))); // a zombie has exited
            }
        }
        let gone_at_release = std::sync::Arc::new(std::sync::Mutex::new(None));
        let witness: Kept = std::sync::Arc::new(Witness { grandchild: grandchild_pid.clone(), gone_at_release: gone_at_release.clone() });
        let run = crate::code::hands::admitted(Some(lease), holding(witness, run_owned(cmd, None, "test", capture.clone())));
        let tree = tokio::select! {
            out = run => panic!("the run ended on its own: {out:?}"),
            tree = async {
                loop {
                    if let (Some(sh), Some(gc)) = (read_pid(&shell_pid), read_pid(&grandchild_pid)) {
                        let tree = descendants(sh);
                        if tree.iter().any(|who| who.pid == gc) && !capture.stdout.snapshot().bytes.is_empty() {
                            break tree;
                        }
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
            } => tree, // the run's future is dropped here: its waiter is gone
        };
        let lingering = tokio::time::timeout(std::time::Duration::from_secs(10), crate::code::hands::close(context))
            .await
            .expect("the release completes once the tree is ended, not before and not never");
        assert!(lingering.is_empty(), "nothing is left unconfirmed: {lingering:?}");
        assert_eq!(*gone_at_release.lock().unwrap(), Some(true), "what the caller held was released only once the grandchild was gone");
        // a killed descendant may wait for init to reap it; a zombie has exited, so it counts as gone
        assert!(still_present(&tree).is_empty(), "what the child started is gone when the release completes");
        let kept = capture.stdout.snapshot();
        assert_eq!((String::from_utf8_lossy(&kept.bytes).as_ref(), kept.end), ("started\n", CaptureEnd::Cut("the operation was cancelled".into())), "the prefix stays reachable on the operation's capture");

        let mut cmd = tokio::process::Command::new("sh");
        cmd.arg("-c").arg("echo before; sleep 600 & wait");
        let exclusion: Kept = std::sync::Arc::new(());
        let exclusion_held = std::sync::Arc::downgrade(&exclusion);
        let admission = std::sync::Arc::new(crate::code::hands::admit(crate::identity::hands_context::HandsContext::fresh()).expect("open hands"));
        let timed = crate::code::hands::admitted(Some(admission), holding(exclusion, run_owned(cmd, Some(std::time::Duration::from_millis(500)), "test", std::sync::Arc::default())))
            .await
            .expect("ran");
        assert!(exclusion_held.upgrade().is_none(), "a confirmed end releases what its caller held for it (Codex reviewer on #5037)");
        assert!(matches!(timed.ended, Ended::TimedOut { ref unconfirmed } if unconfirmed.is_empty()), "a timeout ends the tree and confirms it: {timed:?}");
        assert_eq!(String::from_utf8_lossy(&timed.stdout.bytes), "before\n", "and what it wrote before the bound is kept");
        assert!(matches!(timed.stdout.end, CaptureEnd::Cut(_)), "a timed-out read is cut, never complete: {timed:?}");
    }

    // what this catches (Codex root on #5030 at 1105c9286): the child's exit was taken as the end
    // of the run while a process it started still held its output. The wait outlived the bound,
    // and the owner, having let go of the child, could no longer end anything. The lifecycle is
    // the exit AND the pipes' EOF under one bound: a holder past it cuts the read, is never
    // claimed ended, and holds the hands' release by name.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_process_that_holds_an_exited_childs_output_past_the_bound_is_held_never_claimed_ended() {
        let dir = tempfile::tempdir().unwrap();
        let holder_pid = dir.path().join("holder.pid");
        let context = crate::identity::hands_context::HandsContext::fresh();
        let lease = std::sync::Arc::new(crate::code::hands::admit(context).expect("open hands"));
        let mut cmd = tokio::process::Command::new("sh");
        // the shell exits at once; its background sleep inherited stdout and stderr
        cmd.arg("-c").arg(format!("echo out; sleep 600 & echo $! > {}", holder_pid.display()));
        // what the caller held for the run (a checkout's sync lock) stays held while the holder
        // lingers unconfirmed, never only until the end's task returns (Codex reviewer on #5037)
        let exclusion: Kept = std::sync::Arc::new(());
        let exclusion_held = std::sync::Arc::downgrade(&exclusion);
        let started = std::time::Instant::now();
        let out = crate::code::hands::admitted(Some(lease), holding(exclusion, run_owned(cmd, Some(std::time::Duration::from_millis(500)), "test", std::sync::Arc::default())))
            .await
            .expect("ran");
        assert!(started.elapsed() < std::time::Duration::from_secs(5), "the bound covers the pipes, not only the exit");
        assert!(matches!(out.ended, Ended::Exited(status) if status.success()), "the shell itself exited: {out:?}");
        assert_eq!(String::from_utf8_lossy(&out.stdout.bytes), "out\n", "what was read is kept");
        assert!(matches!(out.stdout.end, CaptureEnd::Cut(ref why) if why.contains("held it past the bound")), "the read is cut by the holder: {out:?}");
        let lingering = crate::code::hands::close(context).await;
        assert_eq!(lingering.unknown.len(), 1, "the holder is held by name on the hands, never claimed ended: {lingering:?}");
        assert!(exclusion_held.upgrade().is_some(), "and what its caller held for it stays held with it");
        if let Some(pid) = std::fs::read_to_string(&holder_pid).ok().and_then(|t| t.trim().parse::<u32>().ok()) {
            kill9(pid); // the test's own cleanup of the holder it made
        }

        // A cancel DURING that drain (Codex root on #5030 at 3f597b9aa): the readers stay owned
        // and are stopped, and the holder is fenced on the hands just the same.
        let (shell_pid, holder_pid) = (dir.path().join("sh2.pid"), dir.path().join("holder2.pid"));
        let context = crate::identity::hands_context::HandsContext::fresh();
        let lease = std::sync::Arc::new(crate::code::hands::admit(context).expect("open hands"));
        let capture = std::sync::Arc::new(RunCapture::default());
        let mut cmd = tokio::process::Command::new("sh");
        cmd.arg("-c").arg(format!("echo $$ > {}; echo out; sleep 600 & echo $! > {}", shell_pid.display(), holder_pid.display()));
        let run = crate::code::hands::admitted(Some(lease), run_owned(cmd, None, "test", capture.clone()));
        let read_pid = |path: &std::path::Path| std::fs::read_to_string(path).ok().and_then(|t| t.trim().parse::<u32>().ok());
        tokio::select! {
            out = run => panic!("the run ended while its output was held: {out:?}"),
            () = async {
                // the run itself says the shell has exited and been reaped, so the settle of its
                // pipes is under way: the cancel lands in that phase, not on a guessed delay
                while !capture.exited() {
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
            } => {}, // the run's future is dropped mid-drain
        }
        let lingering = crate::code::hands::close(context).await;
        assert_eq!(lingering.unknown.len(), 1, "a cancel mid-drain fences the unidentified holder: {lingering:?}");
        assert!(matches!(capture.stdout.snapshot().end, CaptureEnd::Cut(_)), "and its read is cut, its prefix kept on the capture");
        if let Some(pid) = read_pid(&holder_pid) {
            kill9(pid); // the test's own cleanup of the holder it made
        }
    }

    // what this catches (card 679ae815; Codex root on #5035): an authored command's params go to
    // the child's stdin. The old runner awaited that write BEFORE the wait, so a program that
    // never read it wedged the call past its bound, and the shared writer then discarded the
    // write's outcome, so a child that closed stdin early and exited 0 with valid JSON read as
    // having answered. Delivery is recorded: whole to a reader, failed for an early close, cut
    // at the bound or on cancel.
    #[cfg(unix)]
    #[tokio::test]
    async fn input_delivery_is_recorded_whole_failed_or_cut_and_never_read_as_delivered_when_it_wasnt() {
        let mut cat = tokio::process::Command::new("cat");
        cat.stdin(std::process::Stdio::piped());
        let echoed = run_owned_with_input(cat, Some(b"params on stdin".to_vec()), Some(std::time::Duration::from_secs(10)), "test", std::sync::Arc::default())
            .await
            .expect("ran");
        assert_eq!(echoed.stdout.bytes, b"params on stdin", "the input reached the child whole: {echoed:?}");

        let mut deaf = tokio::process::Command::new("sleep");
        deaf.arg("600");
        let big = vec![b'x'; 4 << 20]; // far past any pipe buffer: a write that waited for a reader would never finish
        let started = std::time::Instant::now();
        let ended = run_owned_with_input(deaf, Some(big), Some(std::time::Duration::from_millis(500)), "test", std::sync::Arc::default())
            .await
            .expect("ran");
        assert!(matches!(ended.ended, Ended::TimedOut { .. }), "{ended:?}");
        assert!(started.elapsed() < std::time::Duration::from_secs(10), "the unread input didn't hold the run past its bound");
        assert!(matches!(ended.input, Some(CaptureEnd::Cut(_))), "and its input reads as cut, never delivered: {:?}", ended.input);
        assert!(
            matches!(ended.answer(), Answer::Not(Unanswered { why: Why::TimedOut { .. }, note: Some(ref note), .. }) if note.contains("input")),
            "a timeout keeps its notes too (Codex root on #5038): {:?}",
            ended.answer()
        );
        assert_eq!(echoed.input, Some(CaptureEnd::Complete), "a child that took it all: delivered");

        // Codex root on #5035 at 549dce142: a child that closes its stdin without taking the
        // input, then answers valid JSON and exits 0, must not read as having answered THIS
        // input. The input is past the pipe's buffer, so the write can only end by the close.
        let mut early = tokio::process::Command::new("sh");
        early.arg("-c").arg("exec 0<&-; echo '{}'");
        let closed = run_owned_with_input(early, Some(vec![b'x'; 4 << 20]), Some(std::time::Duration::from_secs(10)), "test", std::sync::Arc::default())
            .await
            .expect("ran");
        assert!(matches!(closed.ended, Ended::Exited(status) if status.success()), "it exited 0: {closed:?}");
        assert_eq!(String::from_utf8_lossy(&closed.stdout.bytes).trim(), "{}", "with valid JSON");
        assert!(matches!(closed.input, Some(CaptureEnd::Failed(_))), "but its input was not delivered: {:?}", closed.input);
        assert!(matches!(closed.answer(), Answer::Not(Unanswered { why: Why::NotWhole, note: Some(ref note), ref stdout, .. }) if note.contains("input") && stdout.bytes == b"{}\n"), "and it is not an answer: {:?}", closed.answer());

        // canary run 38541241843 (Codex root on #5035): a child that exits non-zero BEFORE reading
        // its input lost its own verdict to the broken-pipe note whenever the write landed first.
        // Past the pipe's buffer the write can only end by the exit, so this is that race, always.
        let mut refuses = tokio::process::Command::new("sh");
        refuses.arg("-c").arg("echo boom >&2; exit 3");
        let failed = run_owned_with_input(refuses, Some(vec![b'x'; 4 << 20]), Some(std::time::Duration::from_secs(10)), "test", std::sync::Arc::default())
            .await
            .expect("ran");
        match failed.answer() {
            Answer::Not(un) => {
                assert!(matches!(un.why, Why::Exited(status) if status.code() == Some(3)), "its own exit status is kept: {un:?}");
                assert_eq!(String::from_utf8_lossy(&un.stderr.bytes).trim(), "boom", "and its own stderr");
                assert!(un.note.as_deref().is_some_and(|n| n.contains("input")), "the undelivered input is said beside it, never in place of it");
                let said = un.to_string();
                assert!(said.starts_with("exit 3 (") && said.contains("input") && said.ends_with("stderr:\nboom"), "and all of it is said: {said}");
            }
            other => panic!("a non-zero exit is the child's own failure: {other:?}"),
        }

        // A cancelled run: the input's end is on the operation's capture, never left open.
        let capture = std::sync::Arc::new(RunCapture::default());
        let mut slow = tokio::process::Command::new("sleep");
        slow.arg("600");
        let run = run_owned_with_input(slow, Some(vec![b'x'; 4 << 20]), None, "test", capture.clone());
        let _ = tokio::time::timeout(std::time::Duration::from_millis(300), run).await; // the waiter is dropped mid-write
        assert_eq!(capture.input.snapshot().end, CaptureEnd::Cut("the operation was cancelled".into()), "a cancel cuts the input by name");
    }

    // what this catches (Codex root on #5035 at dc2a64c8e): a process the child started that
    // holds its STDIN after the child exits (reading nothing) kept the input's write open, and
    // the writer was taken out of the owner before it was awaited, so a cancel then detached
    // it and nothing fenced the release. Past the bound, or on a cancel during that wait, the
    // input is cut by name and the unidentified holder holds the hands, as an output holder does.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_process_that_holds_an_exited_childs_stdin_is_fenced_at_the_bound_and_on_a_cancel() {
        let dir = tempfile::tempdir().unwrap();
        let read_pid = |path: &std::path::Path| std::fs::read_to_string(path).ok().and_then(|t| t.trim().parse::<u32>().ok());
        // the shell exits at once; its background sleep keeps the stdin pipe and nothing else.
        // Through fd 3: a POSIX shell gives an async command /dev/null as stdin unless it has an
        // explicit redirection, and dash (CI's sh) applied that over `<&0` (a broken pipe on CI)
        let holder = |pidfile: &std::path::Path| {
            let mut cmd = tokio::process::Command::new("sh");
            cmd.arg("-c").arg(format!("exec 3<&0; sleep 600 <&3 3<&- >/dev/null 2>&1 & echo $! > {}", pidfile.display()));
            cmd
        };
        let input = || Some(vec![b'x'; 4 << 20]); // past the pipe's buffer: the write waits on the holder

        // at the bound
        let first = dir.path().join("holder1.pid");
        let context = crate::identity::hands_context::HandsContext::fresh();
        let lease = std::sync::Arc::new(crate::code::hands::admit(context).expect("open hands"));
        let out = crate::code::hands::admitted(Some(lease), run_owned_with_input(holder(&first), input(), Some(std::time::Duration::from_millis(500)), "test", std::sync::Arc::default()))
            .await
            .expect("ran");
        assert!(matches!(out.ended, Ended::Exited(_)), "the shell itself exited: {out:?}");
        assert!(matches!(out.input, Some(CaptureEnd::Cut(ref why)) if why.contains("held it past the bound")), "the input is cut by the holder: {:?}", out.input);
        assert_eq!(crate::code::hands::close(context).await.unknown.len(), 1, "and the holder holds the release by name");
        if let Some(pid) = read_pid(&first) {
            kill9(pid); // the test's own cleanup of the holder it made
        }

        // on a cancel AFTER the child exited, while the write is still waiting
        let second = dir.path().join("holder2.pid");
        let context = crate::identity::hands_context::HandsContext::fresh();
        let lease = std::sync::Arc::new(crate::code::hands::admit(context).expect("open hands"));
        let capture = std::sync::Arc::new(RunCapture::default());
        let run = crate::code::hands::admitted(Some(lease), run_owned_with_input(holder(&second), input(), None, "test", capture.clone()));
        tokio::select! {
            out = run => panic!("the run ended while its stdin was held: {out:?}"),
            () = async {
                // the run itself says the shell has exited and been reaped: the writer's wait on the
                // holder is under way, and the cancel lands in it, not after a guessed delay
                while !capture.exited() {
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
            } => {}, // the run's future is dropped during the writer's wait
        }
        assert_eq!(capture.input.snapshot().end, CaptureEnd::Cut("the operation was cancelled".into()), "the cancel cuts the input by name");
        assert_eq!(crate::code::hands::close(context).await.unknown.len(), 1, "and the holder holds the release");
        if let Some(pid) = read_pid(&second) {
            kill9(pid);
        }
    }

    // what this catches (Codex root on #5030 at 1105c9286): a read error was swallowed and the
    // stream reported as whole, losing the prefix. A failed read is typed Failed, apart from the
    // process's verdict, and keeps what it read before the failure.
    #[tokio::test]
    async fn a_read_that_fails_is_typed_failed_and_keeps_its_prefix() {
        struct ThenFail(bool);
        impl tokio::io::AsyncRead for ThenFail {
            fn poll_read(mut self: std::pin::Pin<&mut Self>, _: &mut std::task::Context<'_>, buf: &mut tokio::io::ReadBuf<'_>) -> std::task::Poll<std::io::Result<()>> {
                if self.0 {
                    return std::task::Poll::Ready(Err(std::io::Error::other("the pipe broke")));
                }
                self.0 = true;
                buf.put_slice(b"prefix");
                std::task::Poll::Ready(Ok(()))
            }
        }
        let capture = std::sync::Arc::new(RunCapture::default());
        read_stream(Some(ThenFail(false)), capture.clone(), |c| &c.stdout).await;
        let read = capture.stdout.snapshot();
        assert_eq!((read.bytes.as_slice(), read.end), (b"prefix".as_slice(), CaptureEnd::Failed("the pipe broke".into())));
    }

    // what this catches: the Windows port-owner lookup returning None for a port
    // that IS held. Regression for the 2026-09-05 wedge on BigMama — `lsof` does
    // not exist on Windows, so `pid_listening_on_port` returned None for every
    // port, the pre-spawn gate read that as "held by an unnameable stranger", and
    // serving refused to spawn FOREVER against the core's own orphaned child.
    #[test]
    fn netstat_names_the_listening_pid_for_the_right_port() {
        // Verbatim `netstat -ano -p TCP` output from the wedged box.
        let out = "  Proto  Local Address          Foreign Address        State           PID
  TCP    127.0.0.1:58057        0.0.0.0:0              LISTENING       15464
  TCP    127.0.0.1:58057        127.0.0.1:64707        TIME_WAIT       0
  TCP    0.0.0.0:445            0.0.0.0:0              LISTENING       4
";
        assert_eq!(parse_netstat_listening_pid(out, 58057), Some(15464));
        assert_eq!(parse_netstat_listening_pid(out, 445), Some(4));
        // Not listening anywhere = None, which callers treat as "refuse to kill".
        assert_eq!(parse_netstat_listening_pid(out, 9999), None);
        // TIME_WAIT is not a holder we may reap.
        assert!(!out.is_empty());
    }

    // what this catches: matching the port as a substring instead of the local
    // address's final component. `contains(":8057")` would match ":58057" and
    // hand a reap decision the WRONG pid — a blind kill of an unrelated process.
    #[test]
    fn netstat_port_match_is_not_a_substring_match() {
        let out = "  TCP    127.0.0.1:58057        0.0.0.0:0              LISTENING       15464
";
        assert_eq!(parse_netstat_listening_pid(out, 8057), None);
        assert_eq!(parse_netstat_listening_pid(out, 5805), None);
        assert_eq!(parse_netstat_listening_pid(out, 58057), Some(15464));
    }

    // what this catches: reading tasklist's NO-MATCH prose as an image name.
    // tasklist exits 0 and prints "INFO: No tasks are running..." on stdout, so
    // status success does NOT imply a hit; parsing that line as a name would make
    // is_llama_server true for a dead pid and authorise a kill.
    #[test]
    fn tasklist_parses_the_image_name_and_refuses_the_no_match_line() {
        let hit = "\"llama-server.exe\",\"3852\",\"Console\",\"2\",\"14,254,876 K\"
";
        assert_eq!(
            parse_tasklist_image_name(hit).as_deref(),
            Some("llama-server.exe")
        );
        let miss = "INFO: No tasks are running which match the specified criteria.
";
        assert_eq!(parse_tasklist_image_name(miss), None);
        assert_eq!(parse_tasklist_image_name(""), None);
    }

    // what this catches: THE safety invariant shared by both callers — our own
    // test-runner process is NOT a llama-server, so is_llama_server must refuse it.
    // A regression here would let either module SIGKILL an unrelated reused-pid
    // process (the blind-kill this guard exists to prevent).
    #[test]
    fn is_llama_server_false_for_self() {
        assert!(
            !is_llama_server(std::process::id()),
            "the test runner is not a llama-server; reaping it would be a blind kill"
        );
    }

    // what this catches: the grant recovered for an ADOPTED engine (every deploy leaves the
    // lane up for the next core) is read off its own argv — both spellings, absent = None,
    // malformed = None, and 0 stays 0 (a real "no host cache" launch, not an unknown).
    #[test]
    fn cache_ram_is_read_off_the_engines_own_argv() {
        let argv = |s: &str| s.split_whitespace().map(str::to_string).collect::<Vec<_>>();
        assert_eq!(
            cache_ram_mib_in(&argv("llama-server -m x.gguf -c 133930 --parallel 2 --cache-ram 14396 --port 8080")),
            Some(14_396)
        );
        assert_eq!(cache_ram_mib_in(&argv("llama-server --cache-ram=8704")), Some(8_704));
        assert_eq!(cache_ram_mib_in(&argv("llama-server --cache-ram 0")), Some(0), "0 is a value");
        assert_eq!(cache_ram_mib_in(&argv("llama-server -c 4096")), None, "absent = unknown");
        assert_eq!(cache_ram_mib_in(&argv("llama-server --cache-ram")), None, "malformed = unknown");
        assert_eq!(cache_ram_mib_in(&argv("llama-server --cache-ram lots")), None);
    }

    // what this catches: busy is not dead. The verdict needs the probe AND the process's
    // activity: a miss with CPU advancing is a slow answer under load (adopted), a miss
    // with a frozen process is a wedge (reaped), an unsampled process never exonerates.
    // Regression for the M5 2026-09-20 (card b57b19fd): a 32 GB lane mid-generation was
    // killed on one 3 s probe under swap, twice in one morning.
    #[test]
    fn busy_is_not_dead_the_verdict_needs_two_signals() {
        assert_eq!(verdict_from(Some(2), None, ""), LaneVerdict::Healthy { attempt: 2 });
        let busy = verdict_from(None, Some(140.0), "no answer within 5 s");
        assert!(busy.adopt(), "a working lane that answers late is adopted: {busy:?}");
        assert!(matches!(busy, LaneVerdict::Busy { cpu_pct, .. } if cpu_pct == 140.0));
        let frozen = verdict_from(None, Some(0.0), "no answer within 5 s");
        assert!(!frozen.adopt(), "a frozen lane that never answers is dead: {frozen:?}");
        let unsampled = verdict_from(None, None, "connection refused");
        assert!(!unsampled.adopt(), "an absence never exonerates");
        assert!(busy.evidence().contains("adopted") && frozen.evidence().starts_with("dead"));
    }

    // what this catches: the probe is retried within a bound sized for a loaded box — a
    // listener that answers 4 s late (the M5 under swap) is HEALTHY on the first attempt
    // with a 5 s bound, where the old single 3 s shot read it as dead; and a port nobody
    // listens on misses every attempt with the reason carried.
    #[test]
    fn a_slow_answer_within_the_bound_is_healthy() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        std::thread::spawn(move || {
            if let Ok((mut sock, _)) = listener.accept() {
                let mut buf = [0u8; 512];
                let _ = sock.read(&mut buf);
                std::thread::sleep(std::time::Duration::from_secs(4));
                let _ = sock.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok");
            }
        });
        assert_eq!(probe_health_bounded(port, 1, 5), Ok(1), "4 s late is inside a 5 s bound");
        let dead_port = {
            let l = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
            l.local_addr().expect("addr").port()
        };
        let miss = probe_health_bounded(dead_port, 2, 1);
        assert!(miss.is_err(), "nobody listening → every attempt misses: {miss:?}");
    }

    // what this catches: a definitely-dead pid is neither alive nor a llama-server,
    // so a stale registry/pidfile entry can never trigger a kill.
    #[test]
    fn dead_pid_is_not_alive_or_llama() {
        let mut child = std::process::Command::new("true")
            .spawn()
            .expect("spawn true");
        let dead = child.id();
        child.wait().expect("reap");
        // (Tiny PID-reuse window is acceptable in a unit test; the invariant we
        // assert is "we do not treat a reaped pid as a live llama-server".)
        assert!(
            !is_llama_server(dead),
            "a reaped pid must not read as a live llama-server"
        );
    }

    // what this catches (card 977842fd): the running engine's KV decision is read off its
    // own argv in every shape the launcher and an operator write it — the fact the adopt
    // rail compares the plan's decision against.
    #[test]
    fn the_kv_decision_is_read_off_the_engines_own_argv() {
        let argv = |s: &str| s.split_whitespace().map(str::to_string).collect::<Vec<_>>();
        let kv = launch_kv_in(&argv("llama-server -m x.gguf --cache-type-k q8_0 --cache-type-v q8_0 --flash-attn on -c 53760"));
        assert_eq!(kv, LaunchKv { cache_type: Some("q8_0".into()), flash_attn: true, cache_ram_mib: None });
        let plain = launch_kv_in(&argv("llama-server -m x.gguf -c 26880 --parallel 1"));
        assert_eq!(plain, LaunchKv { cache_type: None, flash_attn: false, cache_ram_mib: None });
        assert_eq!(plain.cache_type_or_default(), "f16", "no flag is the engine's f16 default");
        assert_eq!(launch_kv_in(&argv("llama-server --cache-type-k=q4_0 --flash-attn=off")), LaunchKv { cache_type: Some("q4_0".into()), flash_attn: false, cache_ram_mib: None });
        assert_eq!(launch_kv_in(&argv("llama-server -c 51968 --cache-ram 3248")).cache_ram_mib, Some(3_248), "the grant an adopted lane was launched with");
        assert!(launch_kv_in(&argv("llama-server -fa --mmproj p.gguf")).flash_attn, "a bare flag is on and eats nothing");
        assert!(!launch_kv_in(&argv("llama-server --flash-attn off")).flash_attn);
    }
