//! `continuum stop`: ask the core to stop ITSELF so every module saves (the graceful
//! drain, preflighted against the teardown authority that must follow it), then the
//! teardown — pidfile core, split-brain sweep, owned-orphan reap, serving-lane fate —
//! with the reboot path leaving a healthy lane up for adoption.

#[cfg(windows)]
use std::path::Path;
use std::time::Duration;

#[cfg(windows)]
use super::elevated_teardown;
use continuum_cli_lifecycle::elevated_teardown::StopOptions;
use serde_json::Value;

use super::client::{connection, core_is_up, pidfile_for, socket_path};
use super::process::{core_process_evidence, kill_pid_trees_preserving, pid_alive, reap_owned_orphans, running_core_pids};
#[cfg(windows)]
use super::service::installed_core_dir;

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
const GRACEFUL_STOP_BUDGET: std::time::Duration = std::time::Duration::from_secs(
    20 + continuum_core::cognition::turn_ingress::TURN_SETTLE_CAP.as_secs(),
);

/// Say the stop may wait before it does (Fable on #4684): a stop letting turns finish for
/// up to `TURN_SETTLE_CAP` must read as busy, never as hung. Printed before every graceful
/// stop request, so the operator's `stop` and the deploy's reboot say the same thing.
fn say_turns_may_settle() {
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

    /// A legacy core cannot attest a final checkpoint. Keep it running until the
    /// operator can stop it explicitly and preserve/adopt its selected checkpoint.
    pub(super) fn ensure_teardown_supported(&self, reboot: bool) -> Result<(), String> {
        if let (true, Self::LegacyCore(reason)) = (reboot, self) {
            return Err(format!(
                "reboot refused before teardown: {reason}. The existing core is still running. \
                 Use `continuum stop`, preserve the outgoing checkpoints, and use \
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

/// The core pid the pidfile CLAIMS. `None` means no pidfile, or one that names nothing
/// parseable.
fn core_pid_for_teardown() -> Option<i32> {
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
    fn proven_by_held_handle() -> Self {
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
const TEARDOWN_EXIT_DEADLINE: Duration = Duration::from_secs(30);

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

async fn request_graceful_stop(_authority_preflighted: &MayDrain) -> GracefulStop {
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
            // A core that does not KNOW the verb is not a core that refused it. The
            // command surface answers an unknown name with a "no handler"-shaped error,
            // and during the first upgrade that is exactly what the outgoing core says —
            // it was built before this rail existed.
            let unknown = {
                // `e` is a ClientError, not a String — formatted first so the match is on
                // the rendered message the transport actually produced.
                let m = format!("{e}").to_lowercase();
                m.contains("unknown command")
                    || m.contains("no handler")
                    || m.contains("not found")
                    || m.contains("unsupported")
            };
            if unknown {
                GracefulStop::LegacyCore(format!(
                    "the running core has no system/shutdown verb ({e}) — it predates this                      rail, so its modules were never going to save and this stop cannot be                      called durable"
                ))
            } else {
                GracefulStop::NoAnswer(format!("the core refused the request: {e}"))
            }
        }
        Err(_) => GracefulStop::NoAnswer(format!(
            "no answer within {}s",
            GRACEFUL_STOP_BUDGET.as_secs()
        )),
    }
}

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
    match stop_with(false).await? {
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
    }
}

/// The one teardown, parameterized by lane fate. `keep_lanes: true` is the
/// REBOOT path: a healthy llama-server about to be wanted again by the next
/// core stays up, and boot's serve-or-adopt reconcile adopts it at zero
/// relaunches when the shape matches (regression-pinned:
/// a_past_form_of_ourself_serving_enough_lanes_is_adopted_not_reaped) or
/// honestly reaps+rebuilds when it doesn't. Reaping a 20GB-resident lane just
/// to reload it 60s later made every reboot ~5min; with adoption it is the
/// core swap alone (Joel 2026-08-23: "if it's taking so long we need to fix
/// that first"). The standalone `stop` verb keeps FULL teardown — an operator
/// who says stop means everything.
pub(super) async fn stop_with(keep_lanes: bool) -> Result<GracefulStop, String> {
    // Unattended callers — the deploy consumer above all — NEVER escalate:
    // a consent prompt needs a human at the machine, and one raised from the supervisor
    // would hang the consumer on a dialog nobody can see until the consent times out.
    stop_with_authority(keep_lanes, false).await
}

/// Discover receipt-acknowledged exit without beginning shutdown.
async fn probe_shutdown_target(pids: &[i32]) -> Result<Option<continuum_core::commands::system::shutdown::ShutdownTarget>, String> {
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

async fn commit_graceful_shutdown(target: continuum_core::commands::system::shutdown::ShutdownTarget, require_durable: bool) -> Result<GracefulStop, String> {
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
    // `keep_lanes` IS the reboot flag — see this function's doc: "`keep_lanes: true` is the
    // REBOOT path". Named `reboot` on the guard because that is the property it reasons about,
    // and passed `keep_lanes` because today the two callers are exactly reboot(true)/stop(false).
    // A third caller that wants lanes kept for some OTHER reason must pass the reboot-ness
    // separately rather than reuse this argument.
    graceful.ensure_teardown_supported(keep_lanes)?;
    if require_durable {
        graceful.require_durable()?;
    }
    match &graceful {
        GracefulStop::Durable(summary) => println!("core stopped gracefully — {summary}"),
        GracefulStop::Incomplete(summary) => {
            eprintln!("core stopped WITH UNSAVED STATE — {summary}")
        }
        GracefulStop::NoAnswer(why) => {
            eprintln!("no graceful stop ({why}); forcing — modules did not save")
        }
        GracefulStop::NothingRunning => {
            println!("no core is listening; sweeping for survivors")
        }
        GracefulStop::LegacyCore(why) => {
            eprintln!("FIRST UPGRADE — {why}; forcing, and its volatile state is lost")
        }
    }

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
        Vec::new()
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
        println!("  leaving serving lane(s) up for adoption by the next core (reboot path)");
        let _ = std::fs::remove_file(&socket); // socket cleanup still ours — only the lane fate changed
        return Ok(graceful);
    }
    let mut unconfirmed_lanes = Vec::new();
    for outcome in continuum_core::inference::lane_registry::sweep_all() {
        use continuum_core::inference::lane_registry::SweepOutcome as S;
        match outcome {
            S::ExitUnconfirmed { pid, .. } => unconfirmed_lanes.push(pid),
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
