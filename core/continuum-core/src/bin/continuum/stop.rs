//! `continuum stop`: stop policies, shutdown commit and orphan reaping.

use super::*;

pub(super) async fn stop(options: StopOptions) -> Result<(), String> {
    // The consented child does ONE thing and returns; it never drains, never sweeps,
    // never asks for a second consent. Everything it is allowed to do is decided by the
    // digest on its own argv.
    if options.elevated {
        let (plan, sha) = match (&options.plan, &options.plan_sha) {
            (Some(p), Some(s)) => (p.clone(), s.clone()),
            // Unreachable: StopOptions::parse binds the three together. Stated as a
            // refusal rather than an unwrap so the invariant fails loud if that changes.
            _ => return Err("stop --elevated: no bound plan".to_string()),
        };
        #[cfg(windows)]
        {
            // The drain is supplied BY THE ROOT, which is where the socket and the
            // `MayDrain` gate live — the module stays a leaf and the token stays where it
            // can only be built by a preflight or by a held handle.
            return elevated_teardown::teardown_elevated(Path::new(&plan), &sha, || async {
                format!(
                    "{:?}",
                    request_graceful_stop(&MayDrain::proven_by_held_handle()).await
                )
            })
            .await;
        }
        #[cfg(not(windows))]
        {
            let _ = (plan, sha);
            return Err("stop --elevated is the Windows consent child; this platform's \
                        teardown needs no borrowed privilege"
                .to_string());
        }
    }
    // The operator verb answers with its EXIT CODE. `stop` returning 0 has meant only
    // "the process is gone"; it now means "the process is gone AND every module's state
    // reached disk", which is the question anyone typing `stop` before an upgrade is
    // actually asking. A stop that could not save is a failure the shell can see.
    // A stop ends the core; the engine stays for the next core to adopt (the reboot path), so
    // a build costs no model reload and nobody's KV. Only `--engine` reaps it.
    match stop_with(!options.engine).await? {
        // Nothing ran, so nothing was lost. Exiting non-zero here would report a data
        // loss that did not happen, and `stop` is used in scripts that would then treat a
        // clean no-op as a failure.
        GracefulStop::Durable(_) | GracefulStop::NothingRunning => Ok(()),
        GracefulStop::Incomplete(summary) => Err(format!(
            "the core stopped but its state is not durable: {summary}"
        )),
        GracefulStop::NoAnswer(why) => Err(format!(
            "the core was forced down without saving ({why}) — volatile state since the last              save-on-write is gone"
        )),
        // Also a failure, and deliberately so: the operator asked for a stop and did not
        // get a durable one. It is EXPECTED once, on the upgrade that installs the rail,
        // and the message says which it is rather than making a rollout look like a fault.
        GracefulStop::LegacyCore(why) => Err(format!(
            "the outgoing core could not stop gracefully ({why}); this is the one-time cost              of installing the shutdown rail, and it is still not a durable stop"
        )),
        // Fail-closed like a first rollout, but NOT one: this is a fault in a supported core.
        // The message must say so — an operator seeing "one-time cost of installing the rail"
        // on every failed checkpoint would learn to ignore it.
        GracefulStop::CannotAttest(why) => Err(format!(
            "the core answered system/shutdown with an error and could not attest a final \
             checkpoint ({why}); this is a fault, not a first rollout — do not reboot until it \
             is resolved"
        )),
    }
}

/// The one teardown, parameterized by lane fate. `keep_lanes: true` keeps the engine for the
/// next core (reboot, the install handoff, a plain `stop`): a healthy llama-server about to be wanted again by the next
/// core stays up, and boot's serve-or-adopt reconcile adopts it at zero
/// relaunches when the shape matches (regression-pinned:
/// a_past_form_of_ourself_serving_enough_lanes_is_adopted_not_reaped) or
/// honestly reaps+rebuilds when it doesn't. Reaping a 20GB-resident lane just
/// to reload it 60s later made every reboot ~5min; with adoption it is the
/// core swap alone (Joel 2026-08-23: "if it's taking so long we need to fix
/// that first"). Only `stop --engine` passes false: the full teardown, for when the engine's
/// memory is wanted back (Joel 2026-10-29: "The engine is supposed to stay up").
pub(super) async fn stop_with(keep_lanes: bool) -> Result<GracefulStop, String> {
    // Unattended callers — the deploy consumer above all — NEVER escalate:
    // a consent prompt needs a human at the machine, and one raised from the supervisor
    // would hang the consumer on a dialog nobody can see until the consent times out.
    stop_with_authority(keep_lanes, false).await
}

/// Discover receipt-acknowledged exit without beginning shutdown.
pub(super) async fn probe_shutdown_target(pids: &[i32]) -> Result<Option<continuum_core::commands::system::shutdown::ShutdownTarget>, String> {
    use continuum_core::commands::system::shutdown::ShutdownTarget;
    let conn = connection();
    let cmds = conn.commands();
    let reply = tokio::time::timeout(Duration::from_secs(3), cmds.execute_value("system/shutdown-target", serde_json::json!({}))).await;
    // Capability discovery does not drain. An older/unreachable core still goes
    // through the existing terminate-authority preflight, never a guessed grant.
    let Ok(Ok(value)) = reply else { return Ok(None) };
    let target: ShutdownTarget = serde_json::from_value(value) // Decode the core process identity received over lifecycle IPC.
        .map_err(|e| format!("invalid shutdown capability: {e}"))?;
    if target.pid > i32::MAX as u32 || target.instance.is_empty() {
        return Err("invalid shutdown process identity".into());
    }
    Ok((pids == [target.pid as i32]).then_some(target))
}

pub(super) async fn commit_graceful_shutdown(target: continuum_core::commands::system::shutdown::ShutdownTarget, require_durable: bool) -> Result<GracefulStop, String> {
    use continuum_core::commands::system::shutdown::{ShutdownCommitParams, ShutdownResult};
    let conn = connection();
    let cmds = conn.commands();
    say_turns_may_settle();
    let drain = tokio::time::timeout(GRACEFUL_STOP_BUDGET,
        cmds.execute_value("system/shutdown-drain", serde_json::to_value(&target).map_err(|e| e.to_string())?)) // Encode the process binding for the lifecycle IPC request.
        .await.map_err(|_| "bound shutdown is still running; no exit acknowledgment sent".to_string())?
        .map_err(|e| format!("bound shutdown failed; no exit acknowledgment sent: {e}"))?;
    let result: ShutdownResult = serde_json::from_value(drain) // Decode the completed save receipt returned over lifecycle IPC.
        .map_err(|e| format!("invalid shutdown receipt; no exit acknowledgment sent: {e}"))?;
    // We HAVE the receipt before sending commit. A successful commit has no RPC
    // reply: the core closes its own process, and OS exit is the completion proof.
    let graceful = if result.receipt.state_is_durable() {
        GracefulStop::Durable(result.receipt.summary())
    } else {
        GracefulStop::Incomplete(result.receipt.summary())
    };
    if require_durable {
        graceful.require_durable()?;
    }
    let pid = target.pid as i32;
    let commit = ShutdownCommitParams {
        target,
        receipt_sha256: continuum_core::commands::system::shutdown::shutdown_receipt_digest(&result.receipt)
            .map_err(|e| e.to_string())?,
    };
    let reply = tokio::time::timeout(TEARDOWN_EXIT_DEADLINE,
        cmds.execute_value("system/shutdown-commit", serde_json::to_value(commit).map_err(|e| e.to_string())?)).await; // Encode the receipt acknowledgment for lifecycle IPC.
    exited_within(pid, TEARDOWN_EXIT_DEADLINE).await.map_err(|why| {
        format!("acknowledged shutdown did not exit: {why}; commit result: {reply:?}")
    })?;
    let evidence = core_process_evidence()
        .map_err(|e| format!("cannot verify processes after acknowledged exit: {e}"))?;
    if !evidence.core_pids.is_empty() {
        return Err("another core is present after acknowledged exit; refusing replacement teardown".into());
    }
    Ok(graceful)
}

/// `stop_with`, plus whether the operator can answer a required consent prompt.
pub(super) async fn stop_with_authority(keep_lanes: bool, operator_present: bool) -> Result<GracefulStop, String> {
    stop_with_policy(keep_lanes, operator_present, false).await
}

// Maintenance may not turn a missing save receipt into permission to force an
// exit. Ordinary stop keeps its existing explicit forced-stop behavior.
pub(super) async fn stop_with_policy(keep_lanes: bool, operator_present: bool, require_durable: bool) -> Result<GracefulStop, String> {
    // ASK BEFORE KILLING. Everything below this point is a kill, and a kill runs no
    // module's `save_state` — so before it, the core gets the chance to stop itself and
    // report what reached disk. The kill still runs afterwards either way: a core that
    // answered is already exiting and the sweep finds nothing, and a core that did not
    // answer still has to go. What changes is that the operator is told which of those
    // happened instead of reading the same success line for both.
    // PREFLIGHT THE TEARDOWN AUTHORITY BEFORE THE DRAIN (Astra, 2026-09-22): the
    // drain is one-way, so a caller that cannot end the process must refuse here
    // rather than leave a drained core answering ping.
    // For older cores, any missing privilege must be obtained BEFORE the drain.
    // New Windows cores can exit after the CLI receives their save receipt.
    // Older builds and multi-core recovery keep the OS-capability/UAC path.
    if !keep_lanes {
        continuum_core::inference::lane_registry::records_checked()
            .map_err(|e| format!("cannot establish lane teardown ownership before drain: {e}"))?;
    }
    let pids = cores_to_tear_down();
    let target = if cfg!(windows) && (keep_lanes || require_durable) {
        probe_shutdown_target(&pids).await?
    } else {
        None
    };
    if require_durable && !pids.is_empty() && target.is_none() {
        return Err("maintenance requires one receipt-bound shutdown generation; legacy or ambiguous cores remain running".into());
    }
    let self_exited = target.as_ref().map(|target| target.pid as i32);
    let graceful = if let Some(target) = target {
        commit_graceful_shutdown(target, require_durable).await?
    } else {
        let may_drain = preflight_teardown(&cores_to_tear_down(), operator_present, teardown_authority)?;
        if require_durable && may_drain.borrow_authority_for.is_some() {
            return Err("maintenance requires owned durable shutdown authority; the legacy elevated stop cannot attest its save receipt".into());
        }
        // BORROW THE PRIVILEGE BEFORE ANYTHING IS DRAINED, not after. `operator_present` is
        // INTENT; a consent can still be declined, time out, or land on a token that does not
        // hold the privilege either — and a drain already spent by then leaves exactly the
        // stranded core this preflight exists to prevent (Astra, 2026-09-22). The consented
        // child therefore owns both halves for its target: it holds a TERMINATE handle open
        // ACROSS its own drain, so every acquisition failure lands BEFORE the drain. The
        // terminate itself can still fail and is still reported — a held handle narrows the
        // window, it does not abolish it. When this returns Ok the core is already down and
        // the request below finds nothing listening, which is the correct reading of that.
        if let Some(pid) = may_drain.borrow_authority_for {
            #[cfg(windows)]
            {
                let install_dir = installed_core_dir().await?;
                elevated_teardown::request_elevated_teardown(pid, &install_dir).await?;
                // Gone is proven the same way every other teardown proves it: the bounded
                // deadline, never a probe on the next line (Astra, 2026-09-22).
                exited_within(pid, TEARDOWN_EXIT_DEADLINE).await?;
            }
            #[cfg(not(windows))]
            {
                return Err(format!(
                    "install: no teardown authority over pid {pid}, and this platform has no \
                     consent boundary to borrow one through — the core runs as another user"
                ));
            }
        }
        request_graceful_stop(&may_drain).await
    };
    // `keep_lanes`: the engine stays for the next core to adopt. Its callers are reboot, the
    // install handoff and a plain `stop`; all three need what the guard checks (a core that
    // can attest its final checkpoint before its engine is handed on), so the one argument
    // serves all three. Only `stop --engine` passes false.
    graceful.ensure_teardown_supported(keep_lanes)?;
    if require_durable {
        graceful.require_durable()?;
    }
    let (to_stderr, line) = render_graceful_outcome(&graceful);
    if to_stderr {
        eprintln!("{line}")
    } else {
        println!("{line}")
    }

    // One existing teardown allowance covers all remaining lane exits, not
    // a fresh grace period for each process or the shell's 100ms kill cadence.
    let teardown_deadline = std::time::Instant::now() + TEARDOWN_EXIT_DEADLINE;
    let socket = socket_path();
    let pidfile = pidfile_for(&socket);
    // Resolve identity BEFORE any tree is killed. live_lane requires a matching
    // Live registry record and a currently observable llama-server image;
    // unreadable or mismatched records are not exclusions.
    let keep: Vec<i32> = if keep_lanes {
        continuum_core::inference::lane_registry::live_lane()
            .map(|r| r.pid as i32)
            .into_iter()
            .collect()
    } else {
        // Registered lanes have one identity-aware termination owner below.
        // Generic tree/orphan cleanup must not signal them first and discard
        // the native result before the registry can bind its exit proof.
        continuum_core::inference::lane_registry::records_checked()
            .map_err(|e| format!("cannot establish lane teardown ownership: {e}"))?
            .into_iter().map(|r| r.pid as i32).collect()
    };

    let mut pidfile_core: Option<i32> = None;
    if let Ok(contents) = std::fs::read_to_string(&pidfile) {
        match contents.trim().parse::<i32>() {
            Ok(pid) => {
                if let Some(exited) = self_exited {
                    if pid != exited {
                        return Err("core PID changed after acknowledged exit; refusing to kill the replacement".into());
                    }
                } else {
                    kill_pid_trees_preserving(&[pid], &keep);
                }
                println!("stopping core (pid {pid})");
                // A PIDFILE IS A CLAIM THAT A PROCESS EXISTS. Removing it while the
                // process exists is what makes every later reader wrong: the sweep
                // below excludes the pid as handled, the handoff proceeds, and the
                // only place the truth surfaced was a timeout blaming a slow exit.
                // So the removal is now CONDITIONAL on the exit being observed, and a
                // core that did not go aborts the stop with its pid still claimed.
                exited_within(pid, TEARDOWN_EXIT_DEADLINE).await?;
                pidfile_core = Some(pid);
                let _ = std::fs::remove_file(&pidfile);
            }
            // A pidfile that parses to nothing names no process, so nothing can
            // survive it — it is stale by construction and safe to clear.
            Err(_) => {
                let _ = std::fs::remove_file(&pidfile);
            }
        }
    }
    let stopped = pidfile_core.is_some();

    // The pidfile names ONE core. It is not evidence that only one is running.
    //
    // This sweep used to be gated behind `if !stopped` — a pidfile present meant
    // "handled, nothing else to look for". Measured 2026-08-14: two cores were
    // alive at once, both with /tmp/continuum-core.sock open (a debug build at
    // 23:51 and the installed release at 23:54, the second having unlinked and
    // re-bound the path). `stop` printed "stopping core (pid 70240)" — singular —
    // reaped that one, and left the other serving. The `reap_owned_orphans` sweep
    // below could not catch it either: ownership there is keyed on
    // `~/.continuum/bin`, and a core built into the project's mandated
    // CARGO_TARGET_DIR is not under that root, so a dev-built core is invisible to
    // it by construction.
    //
    // A second core on this machine is never benign — whichever one the kernel
    // hands a connection to is the one that answers, so a shipped fix can look
    // intermittently broken and an unshipped one intermittently fixed. `stop`
    // must therefore always enumerate, and must be LOUD about a survivor: an
    // extra core is evidence of a lifecycle bug, exactly as an orphan is.
    //
    // (The enumerator itself was `pkill -f continuum-core-server`, which does not
    // exist on Windows: the spawn failed, `.unwrap_or(false)` swallowed it, and
    // stop printed "no running core found" while the core was running — the same
    // shape as the `pgrep` bug documented on `processes_named` above. That fix
    // ported the finder to sysinfo and missed this sibling call site.)
    // Exclude the pidfile core: SIGTERM is asynchronous, so it is very likely
    // still in the process table on the next line. Counting it here would report
    // a SPLIT BRAIN on every ordinary stop — a false alarm on a message whose
    // whole value is that it only fires when something is genuinely wrong.
    let survivors: Vec<i32> = running_core_pids()
        .into_iter()
        .filter(|pid| Some(*pid) != pidfile_core)
        .filter(|pid| pid_alive(*pid))
        .collect();
    if self_exited.is_some() && !survivors.is_empty() {
        return Err("a core appeared after acknowledged exit; refusing replacement teardown".into());
    }
    if survivors.is_empty() {
        if !stopped {
            println!("no running core found");
        }
    } else {
        if stopped {
            println!(
                "  SPLIT BRAIN: {} core(s) still running after the pidfile core was stopped \
                 — reaping (pid(s) {})",
                survivors.len(),
                survivors
                    .iter()
                    .map(|p| p.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            );
        }
        kill_pid_trees_preserving(&survivors, &keep);
        if !stopped {
            println!(
                "stopped continuum-core-server (pid(s) {})",
                survivors
                    .iter()
                    .map(|p| p.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            );
        }
    }

    // Engine children whose parent died without taking them down are invisible
    // to every tree reap above. Sweep them by OWNERSHIP — nothing is left
    // holding a port or VRAM once `stop` returns.
    //
    // EXCEPT the live serving lane on the reboot path. The comment that used
    // to say lanes were invisible to this sweep rotted when llama-server moved
    // under ~/.continuum/bin: measured 2026-09-01/02, EVERY reboot printed
    // "reaping orphaned llama-server — parent gone" here and then "leaving
    // serving lane(s) up for adoption" eleven lines later, over the corpse —
    // a ~15-minute model reload per reboot, the single biggest boot tax. The
    // identity-verified live lane (pidfile + is_llama_server, never a reused
    // pid) is SPARED when keep_lanes; the start rail's adopt_or_reap then
    // health-checks and adopts the warm weights.
    reap_owned_orphans(&keep);

    // The live serving lane may be a core child on Windows or already orphaned,
    // and its executable may live under `~/.continuum/bin`. Both the core-tree
    // reap and ownership sweep above must honor the same exclusion. Until this call
    // existed, `stop` left every `llama-server` running and the registry was
    // swept only on the NEXT boot — measured 2026-08-17 on the M5 as two lanes
    // resident at once (a 19 GB ephemeral 27B beside the live 14B), which
    // starved the planner into serving a 2,816-token window that cannot hold the
    // tool surface. `reboot` could not clear it either: reboot is stop + start,
    // and neither half owned lanes.
    if keep_lanes {
        println!("  leaving the engine up for the next core to adopt (`continuum stop --engine` ends it too)");
        let _ = std::fs::remove_file(&socket); // socket cleanup still ours — only the lane fate changed
        return Ok(graceful);
    }
    let mut unconfirmed_lanes = Vec::new();
    for outcome in continuum_core::inference::lane_registry::sweep_all(teardown_deadline).await? {
        use continuum_core::inference::lane_registry::SweepOutcome as S;
        match outcome {
            S::ExitUnconfirmed { pid, .. } => unconfirmed_lanes.push(format!("pid={pid}: exit deadline elapsed")),
            S::TerminationRefused { pid, started_s, reason } => unconfirmed_lanes.push(format!("pid={pid} started={started_s}: {reason}")),
            S::ReapedLive { pid, port } => {
                println!("  reaping serving lane (pid {pid}, port {port}) — live lane, this core is stopping")
            }
            S::ReapedEphemeral { pid, port } => {
                println!(
                    "  reaping serving lane (pid {pid}, port {port}) — ephemeral lane, owner gone"
                )
            }
            // A record whose pid is dead / recycled / unparseable is bookkeeping,
            // not an event: garbage-collected silently so the loud lines above
            // stay meaningful.
            S::RemovedDead { .. } | S::RemovedReused { .. } | S::RemovedUnparseable { .. } => {}
            // Unreachable under Shutdown (every role is reaped) — but matched
            // explicitly so adding a mode can never silently fall through here.
            S::LeftLive { pid } => {
                println!("  WARNING: serving lane (pid {pid}) left running by a shutdown sweep — report this")
            }
        }
    }

    if !unconfirmed_lanes.is_empty() {
        return Err(format!("serving lane exit not confirmed for {unconfirmed_lanes:?}; ownership records retained, retry stop after resolving termination authority"));
    }
    let _ = std::fs::remove_file(&socket);
    Ok(graceful)
}

/// Kill every owned engine process not descended from `keep`, reporting each
/// by name and pid. Silent when there are none, loud when there are: an orphan
/// is evidence of a lifecycle bug, so it must never be reaped quietly.
pub(super) fn reap_owned_orphans(keep: &[i32]) {
    let orphans = owned_engine_orphans(keep);
    if orphans.is_empty() {
        return;
    }
    for (pid, what) in &orphans {
        println!("  reaping orphaned {what} (pid {pid}) — owned by this install, parent gone");
    }
    let roots: Vec<i32> = orphans.iter().map(|(pid, _)| *pid).collect();
    kill_pid_trees_preserving(&roots, keep);
}
