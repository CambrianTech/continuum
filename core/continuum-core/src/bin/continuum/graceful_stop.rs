//! The graceful-stop protocol: budget, authority, drain permission and request.

use super::*;

/// `continuum stop` — stop the running core (the detached session started by `continuum start`).
/// How long the core gets to drain, save and join before the CLI stops waiting.
///
/// `Runtime::shutdown` runs three 2s-bounded phases per module in parallel, so a healthy
/// stop is ~6s worst case; the extra room is for the response to travel back. A stop that
/// exceeds this is not assumed dead — it is assumed UNKNOWN, and the caller says so.
///
/// Plus the turn settle's cap (card 32fa22ba): before any module saves, the core lets the
/// citizens' admitted turns finish, for their measured remaining time and never longer than
/// `TURN_SETTLE_CAP`. The same constant the core waits on, so the CLI never gives up on a
/// stop that is still letting a turn finish. An older core does not settle and answers in
/// the first 20 s as before.
pub(super) const GRACEFUL_STOP_BUDGET: std::time::Duration = std::time::Duration::from_secs(
    20 + continuum_core::cognition::turn_ingress::TURN_SETTLE_CAP.as_secs(),
);

/// Say the stop may wait before it does (Fable on #4684): a stop letting turns finish for
/// up to `TURN_SETTLE_CAP` must read as busy, never as hung. Printed before every graceful
/// stop request, so the operator's `stop` and the deploy's reboot say the same thing.
pub(super) fn say_turns_may_settle() {
    println!(
        "▶ stopping: turns already in flight finish first (their measured remaining time, at most {}s), then the core saves",
        continuum_core::cognition::turn_ingress::TURN_SETTLE_CAP.as_secs()
    );
}

/// What the graceful request achieved, if anything.
// Debug: the elevated child has no console an operator can read — its only voice is the
// receipt file it writes beside the plan, and the drain's outcome has to reach that.
#[derive(Debug)]
pub(super) enum GracefulStop {
    /// The core ran the broadcast and every module's state reached disk.
    Durable(String),
    /// The core ran the broadcast and something did NOT save. The message names it.
    /// The process is still stopping; the operator needs the names, not a retry.
    Incomplete(String),
    /// No answer — a core too wedged to answer, or a response that did not arrive inside
    /// the budget. NOT the same as "it stopped": the caller must still tear the process
    /// down, and must not report a clean stop.
    NoAnswer(String),
    /// The running core PREDATES this rail: it has no `system/shutdown` verb, so it was
    /// never going to save and no fix in this binary can change that.
    ///
    /// Distinct from `NoAnswer` on purpose. Both are non-durable, but they are different
    /// facts and an operator needs to tell them apart: this one is the expected, one-time
    /// cost of the FIRST upgrade — the core being replaced was built before the rail
    /// existed — whereas `NoAnswer` is a core that HAS the capability and would not use
    /// it, which is a fault. Collapsing them would make every first rollout look like a
    /// malfunction, and every malfunction look like a rollout.
    LegacyCore(String),
    /// The running core HAS the rail but answered `system/shutdown` with an error and cannot
    /// attest a final checkpoint — e.g. "checkpoint not found". That is a fault in a SUPPORTED
    /// core, not evidence it predates the rail: it must not render as a first rollout.
    ///
    /// Fail-closed on purpose, like `LegacyCore`: a core that cannot attest a final checkpoint
    /// must not be killed mid-upgrade, so `ensure_teardown_supported` refuses reboot for both.
    /// Distinct from `NoAnswer`, which passes the guard — an error from a core that DID answer
    /// is not "no core ever answered".
    CannotAttest(String),
    /// Nothing was listening in the first place. Distinct from `NoAnswer` on purpose: a
    /// core that never ran lost nothing, so `stop` on an already-stopped node must exit 0
    /// rather than announce a data loss that did not happen. The sweep below still runs —
    /// an unanswering socket is not proof that no process survives.
    NothingRunning,
}

impl GracefulStop {
    pub(super) fn require_durable(&self) -> Result<(), String> {
        match self {
            Self::Durable(_) | Self::NothingRunning => Ok(()),
            other => Err(format!("maintenance refused without durable shutdown evidence: {other:?}; scheduler recovery remains required")),
        }
    }

    /// A core that cannot attest a final checkpoint — whether it predates the rail or is
    /// failing inside it — must keep running until the operator can stop it explicitly and
    /// preserve/adopt its selected checkpoint. Both fail-closed outcomes refuse reboot here;
    /// `NoAnswer` deliberately does not (nothing ever answered, so nothing attested a save).
    pub(super) fn ensure_teardown_supported(&self, reboot: bool) -> Result<(), String> {
        if let (true, Self::LegacyCore(reason) | Self::CannotAttest(reason)) = (reboot, self) {
            return Err(format!(
                "keeping the engine refused before teardown (reboot, install handoff or \
                 a plain `stop`): {reason}. The existing core is still running. Use \
                 `continuum stop --engine`, preserve the outgoing checkpoints, and use \
                 `continuum checkpoint inspect` / `continuum checkpoint adopt` where recovery \
                 is needed before running `continuum reboot` again."
            ));
        }
        Ok(())
    }
}

/// Ask the running core to stop ITSELF, so every module's `save_state` runs.
///
/// This is the whole point of the rail. A kill — `taskkill /F` on Windows, a signal-less
/// tree kill elsewhere — runs no module's save, so the citizens' volatile state is lost
/// on every ordinary `stop`, and the CLI reported success because a dead process is
/// indistinguishable from a cleanly stopped one when the only thing you check is whether
/// it is gone.
///
/// The request travels the same socket path `ping` uses, so there is no new transport and
/// no Windows-specific arrangement.
/// Whether a drain may BEGIN, given whether this caller can actually tear the core
/// down afterwards. Pure so the rule is testable without an OS.
///
/// `system/shutdown.rs` deliberately leaves the process alive after its durable
/// receipt — the CLI owns teardown, so a drain whose teardown will be refused
/// strands a drained core still answering ping. Measured 2026-09-22 on Astra's
/// node: `OpenProcess(25040, TERMINATE)` returned NULL with Win32 error 5, and the
/// drain had already run.
pub(super) fn may_begin_shutdown(authority: Result<(), String>) -> Result<(), String> {
    authority.map_err(|why| {
        format!(
            "refusing to DRAIN a core this caller cannot then tear down ({why}). \
             The drain is not reversible: system/shutdown leaves the process alive \
             for the CLI to end, so draining without teardown authority leaves a \
             drained core still answering ping. No state was touched — run \
             `continuum install` at this machine, which may spend one consent to \
             borrow the privilege this caller does not hold"
        )
    })
}

/// Can this caller terminate `pid`? Probed BEFORE any drain. Reports only what the
/// OS answered.
#[cfg(windows)]
pub(super) fn teardown_authority(pid: i32) -> Result<(), String> {
    use windows_sys::Win32::Foundation::{CloseHandle, ERROR_INVALID_PARAMETER};
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_TERMINATE};
    // SAFETY: OpenProcess is a read-only capability query here; the handle is closed
    // immediately and nothing is terminated by this call.
    let handle = unsafe { OpenProcess(PROCESS_TERMINATE, 0, pid as u32) };
    if handle.is_null() {
        let err = std::io::Error::last_os_error();
        // ABSENCE IS NOT REFUSAL, and Windows spells absence differently from unix: a
        // pid that is not in the table answers ERROR_INVALID_PARAMETER here where unix
        // answers ESRCH. Reading it as "no authority" would abandon every stop whose
        // pidfile is merely stale — which is most of them (Astra, 2026-09-22).
        if err.raw_os_error() == Some(ERROR_INVALID_PARAMETER as i32) {
            return Ok(());
        }
        return Err(format!("OpenProcess(pid {pid}, PROCESS_TERMINATE) failed: {err}"));
    }
    // SAFETY: handle came from a successful OpenProcess above and is closed once.
    unsafe { CloseHandle(handle) };
    Ok(())
}

#[cfg(unix)]
pub(super) fn teardown_authority(pid: i32) -> Result<(), String> {
    // SAFETY: signal 0 performs the permission and existence check WITHOUT delivering
    // a signal — the standard capability probe.
    if unsafe { libc::kill(pid, 0) } == 0 {
        return Ok(());
    }
    let err = std::io::Error::last_os_error();
    match err.raw_os_error() {
        Some(libc::EPERM) => Err(format!("signal 0 to pid {pid} refused: {err}")),
        // ESRCH = already gone: nothing to tear down, so nothing to refuse.
        _ => Ok(()),
    }
}

/// The DIRECTORY the supervisor was told to run the core out of — the parent of the
/// `artifact` field in the `ContinuumCore` descriptor.
///
/// A directory and not the file, because a caller that cannot open the process cannot
/// ask the OS which image it is executing (that is the whole reason it is escalating),
/// and the descriptor's file name is routinely NOT that image: after a staging swap the
/// descriptor names the new slot while the surviving core runs from the renamed
/// predecessor beside it. The child proves the rest through its own handle.
#[cfg(windows)]
pub(super) async fn installed_core_dir() -> Result<String, String> {
    let task = PreparedCoreService::query().await?;
    let description: CoreServiceDescription = serde_json::from_str(&task.description)
        .map_err(|e| format!("the {} task's descriptor is unreadable ({e}); rerun the installer before escalating", supervisor_install::CORE_TASK))?;
    Path::new(&description.artifact)
        .parent()
        .map(|d| d.display().to_string())
        .ok_or_else(|| format!("the installed artifact {} has no directory", description.artifact))
}

/// The core pid the pidfile CLAIMS. `None` means no pidfile, or one that names nothing
/// parseable.
pub(super) fn core_pid_for_teardown() -> Option<i32> {
    std::fs::read_to_string(pidfile_for(&socket_path()))
        .ok()
        .and_then(|c| c.trim().parse::<i32>().ok())
}

/// Every core this stop would have to end.
///
/// THE PIDFILE IS NOT SUFFICIENT, and on the node this rail exists for it is already
/// gone: the old `stop` removed it whether or not the kill took, so the file is absent
/// while the process still answers ping (Astra, 2026-09-22). Preflighting only the
/// pidfile would then find nothing to check, pass, and drain a core it cannot end —
/// the exact failure the preflight is for. The process table is the second source, and
/// the one that cannot be deleted by a previous mistake.
pub(super) fn cores_to_tear_down() -> Vec<i32> {
    let mut pids: Vec<i32> = core_pid_for_teardown().into_iter().collect();
    for pid in running_core_pids() {
        if !pids.contains(&pid) {
            pids.push(pid);
        }
    }
    pids
}

/// Proof that the teardown authority was preflighted.
///
/// `request_graceful_stop` takes one, so the drain is not REACHABLE without the check:
/// deleting the preflight does not weaken a test, it stops compiling. That is the
/// answer to "a removed preflight call still passes all three" (Astra, 2026-09-22) —
/// an ordering this load-bearing belongs to the type system, not to a test that has to
/// remember to look for it.
pub(super) struct MayDrain {
    /// The core whose teardown privilege must be BORROWED after the drain. `None` =
    /// this caller can end every core itself.
    pub(super) borrow_authority_for: Option<i32>,
}

impl MayDrain {
    /// The drain is authorised because a TERMINATE handle for the target is HELD OPEN
    /// across it — capability, not intent (Astra, 2026-09-22: "operator_present is
    /// intent, not capability"). Only the elevated child can build one of these, and
    /// only while it is holding the handle it will terminate with.
    #[cfg(windows)]
    pub(super) fn proven_by_held_handle() -> Self {
        Self { borrow_authority_for: None }
    }
}

/// The preflight decision, pure over what the OS answered.
///
/// A refusal with no operator present is final and returns before anything is drained.
/// When the operator is present — `continuum install`, a command a human typed — the
/// un-endable core is carried forward to have its teardown privilege borrowed instead.
pub(super) fn preflight_teardown(
    pids: &[i32],
    operator_present: bool,
    authority: impl Fn(i32) -> Result<(), String>,
) -> Result<MayDrain, String> {
    for pid in pids {
        let Err(why) = authority(*pid) else { continue };
        if operator_present {
            eprintln!(
                "▶ no teardown authority over pid {pid} ({why}); one consent will be asked for after the drain"
            );
            return Ok(MayDrain { borrow_authority_for: Some(*pid) });
        }
        // Err by construction — `may_begin_shutdown` only dresses an Err in the reason
        // the drain is being refused. The `map` is how that flows out as this signature.
        return may_begin_shutdown(Err(why)).map(|()| MayDrain { borrow_authority_for: None });
    }
    Ok(MayDrain { borrow_authority_for: None })
}

/// How long a core gets to leave the process table after being asked to stop and
/// then killed.
pub(super) const TEARDOWN_EXIT_DEADLINE: Duration = Duration::from_secs(30);

/// Wait, bounded, for `pid` to leave the process table.
///
/// NOT an immediate `pid_alive` after the kill: SIGTERM and taskkill are both
/// asynchronous, so a perfectly healthy core is still in the table on the very next
/// line, and an immediate probe would manufacture "the kill was refused" on every
/// ordinary stop (Astra, 2026-09-22). Only failing to go within the deadline is
/// evidence of anything.
pub(super) async fn exited_within(pid: i32, deadline: Duration) -> Result<(), String> {
    let until = std::time::Instant::now() + deadline;
    while pid_alive(pid) {
        if std::time::Instant::now() >= until {
            return Err(format!(
                "core pid {pid} is still running {}s after being asked to stop and then \
                 killed; its pidfile is left in place because the process it names still \
                 exists, and no handoff is issued over a core that did not go",
                deadline.as_secs()
            ));
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    Ok(())
}

pub(super) async fn request_graceful_stop(_authority_preflighted: &MayDrain) -> GracefulStop {
    // Ask whether anything is listening BEFORE spending the stop budget on a socket
    // nobody holds. Without this, `stop` on an already-stopped node waits the full
    // graceful budget and then reports state loss — 20 seconds to be told, wrongly, that
    // a core which never ran failed to save.
    //
    // BUT A PING TIMEOUT IS NOT PROOF OF ABSENCE. `core_is_up` returns false for a core
    // that is running and too wedged to answer — which is the case where state is most
    // likely to be lost, and reporting it as "nothing was running" would exit 0 and tell
    // the operator nothing was at stake. So absence must be corroborated by the pidfile:
    // no answer AND no pidfile is an empty node; no answer WITH a pidfile is a core that
    // would not speak, and that is `NoAnswer`. (The pidfile can also be stale, which is
    // why this decides the LABEL only — the sweep below runs either way.)
    if !core_is_up().await {
        // A MISSING PIDFILE IS NOT PROOF THAT NOTHING IS RUNNING. It was, in the first
        // version of this, and that is the same error as reading a ping timeout as an
        // empty node — one layer down. A core started outside the pidfile's owner, one
        // whose file was removed, or a second core that never wrote one, all present as
        // "no record" while still holding the socket and a citizen's unsaved state.
        //
        // So absence is only claimed when the sweep can also find no process. That is
        // what `running_core_pids` answers, and it is the same enumeration the sweep
        // below acts on — one instrument, so the label and the action cannot disagree.
        //
        // It can still be wrong in the SAFE direction: `processes_named` returning empty
        // because the enumerator itself failed reads as "nothing running". That is worth
        // naming rather than hiding — it is the residual case, and it is why the teardown
        // below runs regardless of what this decides.
        // THREE OUTCOMES, not two. Each piece of evidence can say "yes", "no", or "I
        // could not look", and only the last of those may not be rounded to "no".
        let pidfile = pidfile_for(&socket_path());
        let recorded = match std::fs::read_to_string(&pidfile) {
            Ok(c) => c.trim().parse::<i32>().is_ok(),
            // The ONLY error that means absence. A permission error, a busy file, a
            // path on a filesystem that went away — those mean we did not find out, and
            // `.ok()` used to flatten every one of them into "no record".
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
            Err(e) => {
                return GracefulStop::NoAnswer(format!(
                    "could not read {}: {e} — whether a core is running is unknown, so this                      stop is not being called clean",
                    pidfile
                ));
            }
        };
        // ONE snapshot answers both questions, so they cannot describe different moments.
        let evidence = match core_process_evidence() {
            Ok(evidence) => evidence,
            Err(error) => {
                return GracefulStop::NoAnswer(format!(
                    "could not establish whether a core is running: {error}"
                ));
            }
        };
        let survivors = !evidence.core_pids.is_empty();
        return if recorded || survivors {
            GracefulStop::NoAnswer(
                "a core process is present but did not answer a ping".to_string(),
            )
        } else {
            // Both instruments looked, and both found nothing.
            GracefulStop::NothingRunning
        };
    }
    let conn = connection();
    let cmds = conn.commands();
    let req = cmds.execute_value("system/shutdown", Value::Object(Default::default()));
    say_turns_may_settle();
    match tokio::time::timeout(GRACEFUL_STOP_BUDGET, req).await {
        Ok(Ok(value)) => {
            let durable = value
                .get("state_is_durable")
                .and_then(Value::as_bool)
                // A response whose shape we do not recognise is not a durable stop. An
                // older core answering `system/shutdown` with something else must not be
                // read as a clean save.
                .unwrap_or(false);
            let summary = value
                .get("summary")
                .and_then(Value::as_str)
                .unwrap_or("the core answered without a summary")
                .to_string();
            if durable {
                GracefulStop::Durable(summary)
            } else {
                GracefulStop::Incomplete(summary)
            }
        }
        Ok(Err(e)) => {
            // The verdict must not depend on what the core said — see
            // `classify_shutdown_error`: any error from a reachable core fails closed.
            classify_shutdown_error(&format!("{e}"))
        }
        Err(_) => GracefulStop::NoAnswer(format!(
            "no answer within {}s",
            GRACEFUL_STOP_BUDGET.as_secs()
        )),
    }
}


/// Turn an error answer to `system/shutdown` into a stop outcome — never `NoAnswer`.
///
/// The verdict must not depend on what the core said (#3940): any error from a reachable
/// core fails closed, because such a core cannot attest a final checkpoint and therefore
/// must not be killed mid-upgrade (`ensure_teardown_supported` refuses reboot for both
/// fail-closed outcomes). What the phrasing may still decide is WHICH of them:
/// phrasings that mean "this name is not in the command registry" are positive evidence
/// the core predates the rail (`LegacyCore`, a first rollout); every other error is a fault
/// inside a supported core (`CannotAttest`) and must not be rendered as one.
/// `NoAnswer` keeps its meaning for paths where no core ever answered (pre-check failures
/// and timeouts); an error from a core that DID answer can no longer masquerade as one.
pub(super) fn classify_shutdown_error(rendered: &str) -> GracefulStop {
    let m = rendered.to_lowercase();
    // Recognition now only picks WHICH fail-closed outcome — never whether to fail closed.
    // Only phrasings that specifically mean "this name is not in the command registry" earn
    // the pre-rail label; the old list's weak markers ("not found", bare "unsupported") could
    // mislabel an internal error like `checkpoint not found` as a core from before the rail.
    let predates_rail = m.contains("unknown command") || m.contains("no handler");
    if predates_rail {
        GracefulStop::LegacyCore(format!(
            "the running core has no system/shutdown verb ({rendered}) — it predates this \
             rail, so its modules were never going to save and this stop cannot be called durable"
        ))
    } else {
        // Fail closed (#3940): the old code fell through here to NoAnswer for any phrasing that
        // matched no marker string — and NoAnswer passes the reboot guard, so an un-attested core
        // could be killed mid-upgrade. Any error from a reachable core is now fail-closed, with its
        // own text preserved so the operator can diagnose what it actually said; the outcome says
        // this is a fault in a supported core, not evidence of a first rollout.
        GracefulStop::CannotAttest(format!(
            "the running core answered system/shutdown with an error and cannot attest a final \
             checkpoint ({rendered})"
        ))
    }
}

/// What the operator sees for each outcome, and whether it goes to stderr. Extracted from
/// `stop_with_authority` so a regression test can pin the wording: `CannotAttest` is a fault in a
/// supported core and must never render as a first rollout — that label belongs only to
/// `LegacyCore`, whose phrasing came from a core that positively predates this rail.
pub(super) fn render_graceful_outcome(outcome: &GracefulStop) -> (bool, String) {
    match outcome {
        GracefulStop::Durable(summary) => (false, format!("core stopped gracefully — {summary}")),
        GracefulStop::Incomplete(summary) => (
            true,
            format!("core stopped WITH UNSAVED STATE — {summary}"),
        ),
        GracefulStop::NoAnswer(why) => (
            true,
            format!("no graceful stop ({why}); forcing — modules did not save"),
        ),
        GracefulStop::NothingRunning => (false, "no core is listening; sweeping for survivors".into()),
        GracefulStop::LegacyCore(why) => {
            (true, format!("FIRST UPGRADE — {why}; forcing, and its volatile state is lost"))
        }
        GracefulStop::CannotAttest(why) => (
            true,
            format!(
                "core answered but cannot attest a final checkpoint ({why}); forcing — \
                 its volatile state is lost"
            ),
        )
    }
}
