use super::*;

// ============================================================================
// Background Command Execution
// ============================================================================
/// Run a shell command asynchronously, streaming output into shared state.
///
/// This function runs in a tokio task. It:
/// 1. Spawns `bash -c "command"` with the session's cwd and env
/// 2. Reads stdout/stderr line-by-line into the shared ExecutionState
/// 3. Handles timeouts by killing the process
/// 4. Detects kill requests by checking the status flag
pub(super) async fn run_shell_command(
    state: Arc<Mutex<ExecutionState>>,
    command: &str,
    cwd: &Path,
    env: &HashMap<String, String>,
    timeout_ms: Option<u64>,
    confinement: Option<&crate::system_resources::confinement::Confinement>,
    who: &str,
    cargo_grant: Option<&crate::system_resources::confinement::CargoTargetGrant>,
) {
    // Resolve a REAL POSIX shell. `TokioCommand::new("bash")` finds
    // C:\Windows\System32\bash.exe on Windows — the WSL launcher, not a shell —
    // and every persona's `code/shell` died with
    //   execvpe(/bin/bash) failed: No such file or directory
    // while code/read, code/search and code/tree kept working, because only
    // execution goes through a shell. Observed live: two citizens cycling
    // through read-and-search all evening and never reaching execution. That
    // reads as an inert persona; it was a severed limb.
    let bash = match continuum_base::shell_portable::locate_bash() {
        Ok(bash) => bash,
        Err(why) => {
            // Fail with the REASON, not with a spawn error from inside a child
            // that the caller cannot interpret.
            if let Ok(mut s) = state.lock() {
                s.status = ShellExecutionStatus::Failed;
                s.stderr_lines.push(format!("no usable shell: {why}"));
                s.process_ended = true; // no child was spawned
                s.finished_at = Some(continuum_base::time::EpochMs::now().millis());
                s.output_notify.notify_one();
            }
            return;
        }
    };
    // Run as the USER'S LOGIN SHELL so every persona inherits the user's COMPLETE
    // environment — their PATH and every program they've installed (uv, cargo, node,
    // pyenv, git…) — exactly like Claude Code runs in the user's own shell. A bare
    // non-login `bash -c` only sees whatever env the CORE happened to launch with and
    // never sources the profile, so a persona could be missing the user's whole
    // toolchain (measured 2026-08-11: a login zsh resolves uv/cargo/node; a non-login
    // shell resolves none). On Unix use `$SHELL` (the user's own shell) as a login
    // shell; fall back to the located POSIX bash. Windows keeps the located bash (the
    // shell_portable WSL-shim guard) — its env model differs and $SHELL is absent.
    //
    // The ENVIRONMENT comes from the user's login shell; the LANGUAGE is always bash.
    // Citizens (and every model) write bash: `echo '===' ; grep … | head`. Running that
    // text in a login zsh broke whole commands on `==`/`=word` expansion ("zsh: ==
    // not found", M5 2026-10-17, three residents, a lost turn each). So the login shell
    // only loads the profile and then execs the located bash on the command, which
    // travels in the environment (`CONTINUUM_SHELL_COMMAND`) so no quoting layer can
    // mangle it.
    let (shell_bin, shell_args): (PathBuf, Vec<std::ffi::OsString>) = if cfg!(windows) {
        (bash, vec!["-c".into()])
    } else {
        match std::env::var("SHELL")
            .ok()
            .filter(|s| !s.is_empty() && Path::new(s).exists())
        {
            Some(user_shell) => (
                PathBuf::from(user_shell),
                vec![
                    "-lc".into(),
                    format!(
                        "exec '{}' -c \"$CONTINUUM_SHELL_COMMAND\"",
                        bash.display().to_string().replace('\'', "'\\''")
                    )
                    .into(),
                ],
            ),
            None => (bash, vec!["-lc".into()]),
        }
    };
    let command_in_env = shell_args.len() == 2;

    // Layer the per-task PATH prefix (the era venv's bin) ON TOP of the login-loaded
    // PATH. A login shell RE-SETS PATH from the profile, so an inherited PATH override
    // is clobbered (measured); instead the caller hands the prefix as
    // CONTINUUM_PATH_PREPEND and we prepend it AFTER the profile loads — the same thing
    // `source venv/bin/activate` does. No prefix → run the command verbatim. This is the
    // ONLY hand on PATH; the base environment is entirely the user's own.
    let effective_command = match env.get("CONTINUUM_PATH_PREPEND") {
        Some(prefix) if !prefix.is_empty() => {
            format!("export PATH=\"{prefix}:$PATH\"; {command}")
        }
        _ => command.to_string(),
    };

    // A citizen's child is confined to her roots at the kernel (card d598c806:
    // eleven citizens ran `find /` as the operator); the operator's is not.
    let mut cmd = crate::system_resources::confinement::command_for(
        confinement,
        who,
        &shell_bin,
        &if command_in_env {
            shell_args.clone()
        } else {
            let mut a = shell_args.clone();
            a.push(effective_command.clone().into());
            a
        },
        Some(cwd),
        cargo_grant,
    );
    cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        // Don't inherit stdin — non-interactive
        .stdin(std::process::Stdio::null());

    // STRIP SECRETS BEFORE THE CHILD EXISTS. A citizen's shell inherits this
    // process's environment (Rust's `Command` does that by default), and the core
    // is started with `set -a; . config.env; set +a` — which puts HF_TOKEN in the
    // ambient environment. A citizen then runs `env` or `printenv` as an ordinary
    // part of exploring its workspace, and quoting tool output into a room is
    // NORMAL behaviour, so the token would reach every enrolled peer across
    // operators. Observed 2026-09-06: a citizen put a partial environment on the
    // wire; the variables it happened to include were harmless, which was luck and
    // not a control.
    //
    // This is a DENYLIST and that choice is deliberate here, against the usual
    // rule: the base environment is intentionally "the user's own" (see the PATH
    // comment above), so an allowlist would refuse unknown-but-needed variables and
    // break working citizens — the wrong failure direction for a shell that must
    // keep running. The floor (not sourcing secrets into the core's ambient
    // environment at all) is card f25f4141; this is the net above it.
    strip_secret_env(&mut cmd);
    utf8_text_env(&mut cmd);

    // Apply session environment variables
    for (k, v) in env {
        cmd.env(k, v);
    }
    // The command text rides in the environment when a login shell hands it to bash
    // (see `command_in_env` above); set last so no session variable can shadow it.
    if command_in_env {
        cmd.env("CONTINUUM_SHELL_COMMAND", &effective_command);
    }

    // Captured shell sessions are noninteractive; share the platform launch policy.
    airc_core::process::configure_background(cmd.as_std_mut());

    // Spawn the child process
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            if let Ok(mut s) = state.lock() {
                s.status = ShellExecutionStatus::Failed;
                s.stderr_lines.push(format!("Failed to spawn bash: {e}"));
                s.finished_at = Some(continuum_base::time::EpochMs::now().millis());
                s.process_ended = true; // nothing started, so nothing is left running
                s.output_notify.notify_one();
            }
            return;
        }
    };

    // Store PID for external kill capability
    if let Some(pid) = child.id() {
        if let Ok(mut s) = state.lock() {
            s.pid = Some(pid);
        }
    }

    // Take stdout/stderr handles
    let (Some(stdout), Some(stderr)) = (child.stdout.take(), child.stderr.take()) else {
        let _ = child.start_kill();
        if let Ok(mut s) = state.lock() {
            s.status = ShellExecutionStatus::Failed;
            s.stderr_lines
                .push("Failed to capture shell output: stdout/stderr pipes unavailable".to_string());
            s.finished_at = Some(continuum_base::time::EpochMs::now().millis());
            s.process_ended = true; // the child was told to die and nothing reads it
            s.output_notify.notify_one();
        }
        return;
    };

    // Spawn line readers (notify watchers on each new line)
    let state_out = state.clone();
    let stdout_task = tokio::spawn(async move {
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if let Ok(mut s) = state_out.lock() {
                if s.status == ShellExecutionStatus::Killed {
                    break;
                }
                s.stdout_lines.push(line);
                // Cap output: drain oldest lines, adjusting cursor
                if s.stdout_lines.len() > MAX_OUTPUT_LINES_PER_EXECUTION {
                    let drain = s.stdout_lines.len() - MAX_OUTPUT_LINES_PER_EXECUTION;
                    s.stdout_lines.drain(..drain);
                    s.stdout_cursor = s.stdout_cursor.saturating_sub(drain);
                }
                s.output_notify.notify_one();
            }
        }
    });

    let state_err = state.clone();
    let stderr_task = tokio::spawn(async move {
        let reader = BufReader::new(stderr);
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if let Ok(mut s) = state_err.lock() {
                if s.status == ShellExecutionStatus::Killed {
                    break;
                }
                s.stderr_lines.push(line);
                if s.stderr_lines.len() > MAX_OUTPUT_LINES_PER_EXECUTION {
                    let drain = s.stderr_lines.len() - MAX_OUTPUT_LINES_PER_EXECUTION;
                    s.stderr_lines.drain(..drain);
                    s.stderr_cursor = s.stderr_cursor.saturating_sub(drain);
                }
                s.output_notify.notify_one();
            }
        }
    });

    // Wait for process completion (with optional timeout and kill detection)
    let state_wait = state.clone();
    let state_for_timed_kill = state.clone();
    let exit_status = if let Some(timeout) = timeout_ms {
        tokio::select! {
            // Branch 1: Process completes
            result = child.wait() => {
                match result {
                    Ok(status) => Some(status),
                    Err(e) => {
                        if let Ok(mut s) = state_wait.lock() {
                            s.stderr_lines.push(format!("Process wait error: {e}"));
                        }
                        None
                    }
                }
            }
            // Branch 2: a kill was asked for (code/shell-kill, a session's end, a release)
            _ = poll_kill_flag(state_for_timed_kill) => None,
            // Branch 3: Timeout fires
            _ = tokio::time::sleep(Duration::from_millis(timeout)) => {
                // Check if already killed
                let already_done = state_wait.lock()
                    .map(|s| s.status != ShellExecutionStatus::Running)
                    .unwrap_or(false);

                if !already_done {
                    if let Ok(mut s) = state_wait.lock() {
                        if s.status == ShellExecutionStatus::Running {
                            s.status = ShellExecutionStatus::TimedOut;
                            s.stderr_lines.push(format!("Timed out after {timeout}ms"));
                            s.finished_at = Some(continuum_base::time::EpochMs::now().millis());
                            s.output_notify.notify_one();
                        }
                    }
                }
                None
            }
        }
    } else {
        // No timeout — wait indefinitely, but check for kill
        let state_for_error = state.clone();
        let state_for_kill = state.clone();
        tokio::select! {
            result = child.wait() => {
                match result {
                    Ok(status) => Some(status),
                    Err(e) => {
                        if let Ok(mut s) = state_for_error.lock() {
                            s.stderr_lines.push(format!("Process wait error: {e}"));
                        }
                        None
                    }
                }
            }
            // Check kill flag periodically
            _ = poll_kill_flag(state_for_kill) => None,
        }
    };

    // PROOF THE DIRECT CHILD EXITED, or why there is none. A `wait` that returned its status is
    // proof. Every other way out of the select (a kill, a timeout, a wait error, or a kill already
    // flagged before the timeout fired, which used to skip the child entirely) signals the child
    // (harmless if it's already dead) and waits once more: only that wait's status proves the
    // reap, and its error is recorded as unproven, never as an exit (Astra on #5005).
    //
    // THE ONE KILL (Astra on ea06701d): only this task, which owns the child, kills it. Everything
    // the shell started is found while the shell is alive and still their parent, and killed
    // deepest first, each only if its pid still names the same process; then the shell itself.
    // What survives is recorded, never assumed gone, and a release re-checks it.
    let crate::system_resources::process_tree::TreeEnd { reaped, survivors, unwalked } = if exit_status.is_some() {
        crate::system_resources::process_tree::TreeEnd { reaped: Ok(()), survivors: Vec::new(), unwalked: None }
    } else {
        continuum_process::end_tree(&mut child).await
    };
    // a tree that couldn't be walked is an unproven end, the same as an unproven reap
    let reaped = match unwalked {
        Some(why) => Err(match reaped {
            Ok(()) => why,
            Err(reap) => format!("{reap}; {why}"),
        }),
        None => reaped,
    };
    if let Ok(mut s) = state.lock() {
        match reaped {
            Ok(()) => s.process_ended = true,
            Err(why) => s.process_end_error = Some(why),
        }
        s.unconfirmed_descendants = survivors;
    }

    // Wait for output readers to drain
    let _ = stdout_task.await;
    let _ = stderr_task.await;

    // Update final state (if not already set by timeout/kill)
    if let Some(status) = exit_status {
        if let Ok(mut s) = state.lock() {
            if s.status == ShellExecutionStatus::Running {
                s.exit_code = status.code();
                s.status = if status.success() {
                    ShellExecutionStatus::Completed
                } else {
                    ShellExecutionStatus::Failed
                };
                s.finished_at = Some(continuum_base::time::EpochMs::now().millis());
                // Wake any blocked watch() calls to deliver final status
                s.output_notify.notify_one();
                // PUSHED COMPLETION (2026-08-24): a handed-back execution's
                // finish previously reached her only if she remembered to
                // poll. Publish the same command:completed shape tracked
                // dispatches use; the dispatch listener folds it into her
                // working memory IF the handle was registered (apply.rs does,
                // on handback).
                if s.handed_back {
                    if let (Some(bus), Ok(handle)) =
                        (SHELL_COMPLETION_BUS.get(), uuid::Uuid::parse_str(&s.id))
                    {
                        let mut lines: Vec<&String> =
                            s.stdout_lines.iter().chain(s.stderr_lines.iter()).collect();
                        let cut = lines.len().saturating_sub(20);
                        let tail: String = lines
                            .split_off(cut)
                            .into_iter()
                            .map(String::as_str)
                            .collect::<Vec<_>>()
                            .join("\n");
                        let ev = crate::runtime::command_events::CommandCompletedEvent {
                            command_name: "code/shell".to_string(),
                            duration_ms: s.finished_at.unwrap_or(0).saturating_sub(s.started_at), // set 3 lines up; 0 → saturates to 0ms, display only
                            success: s.exit_code == Some(0),
                            error: (s.exit_code != Some(0))
                                .then(|| format!("exit {}", s.exit_code.map_or(-1, |c| c))),
                            handle: Some(handle),
                            result: Some(serde_json::json!({
                                "exitCode": s.exit_code,
                                "tail": tail,
                            })),
                        };
                        if let Ok(payload) = serde_json::to_value(&ev) { // bus fan-out boundary — the same encode tracked dispatches pay
                            bus.publish_async_only(
                                crate::runtime::command_events::COMMAND_COMPLETED_TOPIC,
                                payload,
                            );
                        }
                    }
                }

                log_info!(
                    "code",
                    "shell",
                    "Execution {} finished: exit={} cmd={}",
                    &s.id[..8],
                    s.exit_code.unwrap_or(-1), // killed/no-status path reads as conventional -1 in the error line
                    &s.command
                );
            }
        }
    }
}

/// Poll the kill flag on the execution state. Returns when kill is requested.
pub(super) async fn poll_kill_flag(state: Arc<Mutex<ExecutionState>>) {
    loop {
        {
            if let Ok(s) = state.lock() {
                if s.status != ShellExecutionStatus::Running {
                    return;
                }
            }
        }
        tokio::time::sleep(continuum_process::KILL_POLL).await;
    }
}

// ============================================================================
// Tests
// ============================================================================
/// Does this environment variable NAME look like it carries a credential?
///
/// Matched on the NAME only — a value is never inspected, so this can never log
/// or branch on a secret. Case-insensitive substring match against the shapes
/// credentials actually use in this tree and its dependencies.
///
/// VALIDATED against the real thing rather than invented: run over a live 96-variable
/// Windows environment it strips ZERO variables (no false positives), while catching
/// `HF_TOKEN` and passing every non-secret `config.env` entry
/// (CONTINUUM_STORAGE_PATH, HF_HOME, CONTINUUM_PERSONA_FLOOR, CONTINUUM_PROBE_DIR,
/// CONTINUUM_SERVING_PLACEMENT). `_KEY` / `KEY_` rather than bare `KEY` is what keeps
/// MONKEY/KEYBOARD-shaped names out.
/// Remove every credential-shaped variable from a child's inherited environment
/// BEFORE it exists — the one strip for every citizen-facing spawn (the shell here,
/// the snippet runner's python3 / rustc / built binary in `commands/code/run`). Card
/// f25f4141: the core inherits `config.env` (HF_TOKEN) and a snippet reading
/// `os.environ` is as ordinary as a shell running `env`; quoting the output into a
/// room puts it on the wire. Session variables set AFTER this call stay.
pub fn strip_secret_env(cmd: &mut TokioCommand) {
    for (name, _) in std::env::vars() {
        if is_secret_env_name(&name) {
            cmd.env_remove(&name);
        }
    }
}

/// Text a citizen's child prints is UTF-8 whatever the host's code page. On Windows,
/// Python picks the console code page (cp1252) for a piped stdout and raises
/// `UnicodeEncodeError` on the first glyph outside Latin-1 — Kimi's `code/run` exited 1
/// on a `⚙` in a source line she was printing back (2026-09-26 20:06Z, op #6929), and
/// the operator hit the same wall twice in her own tooling on that box. `PYTHONUTF8=1`
/// is Python's own switch (PEP 540); both are no-ops on a host that is already UTF-8.
/// Applied BEFORE the session environment so a citizen who sets her own wins. One place
/// for every citizen-facing spawn, beside [`strip_secret_env`].
pub fn utf8_text_env(cmd: &mut TokioCommand) {
    cmd.env("PYTHONUTF8", "1").env("PYTHONIOENCODING", "utf-8");
}

pub(crate) fn is_secret_env_name(name: &str) -> bool {
    // SEGMENT match, not substring. A substring needle of `KEY_` matches
    // `MONKEY_DIR` and `KEY` matches `KEYBOARD_LAYOUT` / `TURKEY` — stripping
    // those would break working citizens, which is the wrong failure direction for
    // a shell that must keep running. Environment names are `_`-separated, so the
    // segment IS the word. My first draft used substrings and the test below caught
    // it on MONKEY_DIR before it shipped.
    name.to_ascii_uppercase().split('_').any(|seg| {
        matches!(
            seg,
            "TOKEN"
                | "SECRET"
                | "SECRETS"
                | "PASSWORD"
                | "PASSWD"
                | "CREDENTIAL"
                | "CREDENTIALS"
                | "KEY"
                | "KEYS"
                | "APIKEY"
        )
    })
}
