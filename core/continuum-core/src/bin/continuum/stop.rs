//! `continuum stop`: stop policies, shutdown commit and orphan reaping.

use super::*;

pub(super) async fn stop(options: StopOptions) -> Result<(), String> {
    if options.elevated {
        return Err("elevated termination cannot attest custody; use the receipt-bound stop".into());
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

/// Discover receipt-acknowledged exit without beginning shutdown.
pub(super) async fn probe_shutdown_target(pids: &[i32]) -> Result<Option<continuum_core::commands::system::shutdown::ShutdownTarget>, String> {
    use continuum_core::commands::system::shutdown::ShutdownTarget;
    let conn = connection();
    let cmds = conn.commands();
    let reply = tokio::time::timeout(Duration::from_secs(3), cmds.execute_value("system/shutdown-target", serde_json::json!({}))).await;
    // Capability discovery does not drain. Missing or ambiguous identity refuses stop.
    let Ok(Ok(value)) = reply else { return Ok(None) };
    let target: ShutdownTarget = serde_json::from_value(value) // Decode the core process identity received over lifecycle IPC.
        .map_err(|e| format!("invalid shutdown capability: {e}"))?;
    if target.pid > i32::MAX as u32 || target.instance.is_empty() {
        return Err("invalid shutdown process identity".into());
    }
    Ok((pids == [target.pid as i32]).then_some(target))
}

pub(super) async fn commit_graceful_shutdown(target: continuum_core::commands::system::shutdown::ShutdownTarget) -> Result<GracefulStop, String> {
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
    graceful.require_durable()?;
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

// Every stop preserves unfinished work. Engine teardown is not permission to
// discard a missing or incomplete save receipt.
pub(super) async fn stop_with(keep_lanes: bool) -> Result<GracefulStop, String> {
    // Only the exact core that supplied a durable receipt may end itself.
    // A missing listener is not permission to destroy an engine holding state.
    let pids = cores_to_tear_down();
    if pids.is_empty() {
        if !keep_lanes && !continuum_core::inference::lane_registry::records_checked()
            .map_err(|e| format!("cannot establish retained engine custody: {e}"))?.is_empty() {
            return Err("no core can attest retained engine custody; engines remain running".into());
        }
        return Ok(GracefulStop::NothingRunning);
    }
    let target = probe_shutdown_target(&pids).await?
        .ok_or_else(|| "stop requires one receipt-bound core; missing or ambiguous cores remain running".to_string())?;
    let self_exited = target.pid as i32;
    let graceful = commit_graceful_shutdown(target).await?;
    graceful.require_durable()?;
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
    // Receipt-bound shutdown is the only core exit authority. Never sweep a
    // replacement core or an unregistered engine whose state is unknown.
    if !core_process_evidence().map_err(|e| format!("cannot verify core exit: {e}"))?.core_pids.is_empty() {
        return Err("a core appeared after acknowledged exit; refusing replacement teardown".into());
    }
    if let Ok(contents) = std::fs::read_to_string(&pidfile) {
        let pid = contents.trim().parse::<i32>()
            .map_err(|e| format!("cannot verify shutdown pidfile: {e}"))?;
        if pid != self_exited || pid_alive(pid) {
            return Err("core identity changed after acknowledged exit; pidfile retained".into());
        }
        std::fs::remove_file(&pidfile).map_err(|e| format!("cannot retire stopped core pidfile: {e}"))?;
    }
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
