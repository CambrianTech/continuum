//! `system/shutdown` — ask the RUNNING core to stop itself gracefully and say what
//! reached disk.
//!
//! # Why a resident verb rather than a signal
//!
//! `continuum stop` reached for `taskkill /F` on Windows and a kill tree elsewhere. A
//! forced kill runs no module's `save_state`, so a stop that looked identical to a clean
//! one lost every module's volatile state — and the CLI printed a success line and exited
//! 0 either way, because nothing came back from the kill except "the process is gone".
//!
//! Signals do exist and `main.rs` wires them, but they are not a request/response: the
//! handler cannot hand an exit code or a receipt back to the operator who typed `stop`,
//! and on Windows the console-control arms are best-effort. This verb travels the socket
//! request path every other command already uses, so the answer comes back the same way
//! `ping`'s does — no new transport, no Windows-specific rail.
//!
//! `Privileged`, never `AiSafe`: stopping the node is not a thing a persona reasoning
//! about its own workload should be able to reach for.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::runtime::ShutdownReceipt;

use super::SystemQuery;

/// A process-instance binding, not a grant: all three handoff verbs are Privileged.
/// A recycled PID or a replacement listener cannot accept the old core's handoff.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/system/ShutdownTarget.ts")]
pub struct ShutdownTarget {
    pub pid: u32,
    pub instance: String,
}

fn shutdown_target() -> &'static ShutdownTarget {
    static TARGET: std::sync::OnceLock<ShutdownTarget> = std::sync::OnceLock::new();
    TARGET.get_or_init(|| ShutdownTarget {
        pid: std::process::id(),
        instance: uuid::Uuid::new_v4().to_string(),
    })
}

/// Work in THIS core that a shutdown would kill. The core is the one owner of the
/// answer: it runs these, so the CLI no longer re-derives them from markers and
/// process command lines (card 12047f8e, group C).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/system/LiveWork.ts")]
pub enum LiveWork {
    /// A detached `benchmark/swe-solve`: a task inside the core, killed with no partial result.
    Benchmark { run: String, instance: String },
    /// A `cognition/eval` run whose world names this core as its owner.
    Eval { run: String },
}

impl std::fmt::Display for LiveWork {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LiveWork::Benchmark { run, instance } => write!(f, "benchmark run {run} on {instance}"),
            LiveWork::Eval { run } => write!(f, "eval run {run}"),
        }
    }
}

/// The live work in this process, read from the core's own state.
///
/// Training is not listed. Engine-mode training runs inside the engine lane, which a
/// reboot keeps (`keep_lanes`), and the job board reattaches to it. On the 5090, 119
/// engine-local jobs since 2026-10-06 all reached a terminal claim and none was
/// journaled killed-by-reboot. `forge/train`'s Python `mlx_lm` path is the CLI's
/// remaining guard until card 878f100f decides its fate.
pub fn live_work() -> Vec<LiveWork> {
    let mut live: Vec<LiveWork> = crate::cognition::swe_bench::in_flight_solve_runs()
        .into_iter()
        .map(|(run, instance)| LiveWork::Benchmark { run, instance })
        .collect();
    let me = std::process::id();
    live.extend(
        crate::cognition::eval::in_flight_eval_runs()
            .into_iter()
            .filter(|(_, owner)| *owner == me)
            .map(|(run, _)| LiveWork::Eval { run }),
    );
    live
}

/// The shutdown target plus what stopping it would kill. Flattened, so a CLI that
/// reads only the target keeps working.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/system/ShutdownBinding.ts")]
pub struct ShutdownBinding {
    #[serde(flatten)]
    pub target: ShutdownTarget,
    #[serde(rename = "liveWork")]
    pub live_work: Vec<LiveWork>,
}

/// Drain the bound process. `overLiveWork` is the operator's consent to kill live
/// work (`reboot --force`, `stop`); absent, as from a CLI that predates it, it is
/// no consent.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/system/ShutdownDrainParams.ts")]
pub struct ShutdownDrainParams {
    #[serde(flatten)]
    pub target: ShutdownTarget,
    #[serde(default, rename = "overLiveWork")]
    pub over_live_work: bool,
}

/// A drain that would kill live work refuses by name unless the caller consented.
fn refuse_over_live_work(live: &[LiveWork], over_live_work: bool) -> Result<(), crate::sdk_codegen::CommandError> {
    if over_live_work || live.is_empty() {
        return Ok(());
    }
    let named = live.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ");
    Err(crate::sdk_codegen::CommandError::Invalid(format!(
        "live work would die in this shutdown: {named}. Nothing was drained. Stop it cleanly \
         first (`continuum benchmark/round-stop --run_id <id>` cancels a solve at its next \
         task boundary and keeps every streamed grade), wait for it, or consent to losing it \
         (`continuum reboot --force`)."
    )))
}

fn validate_target(target: &ShutdownTarget, current: &ShutdownTarget) -> Result<(), crate::sdk_codegen::CommandError> {
    if target != current {
        return Err(crate::sdk_codegen::CommandError::Invalid(
            "shutdown target changed; nothing was drained or committed by this request".into(),
        ));
    }
    Ok(())
}

/// The CLI echoes the receipt it has already received. Commit cannot race ahead of
/// save/join, or acknowledge a different shutdown operation.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/system/ShutdownCommitParams.ts")]
pub struct ShutdownCommitParams {
    pub target: ShutdownTarget,
    pub receipt_sha256: String,
}

pub fn shutdown_receipt_digest(receipt: &ShutdownReceipt) -> Result<String, crate::sdk_codegen::CommandError> {
    use sha2::Digest;
    let bytes = serde_json::to_vec(receipt).map_err(|e| crate::sdk_codegen::CommandError::Internal(e.to_string()))?; // Canonical receipt bytes hashed by both sides of lifecycle IPC.
    Ok(format!("{:x}", sha2::Sha256::digest(bytes)))
}

fn validate_commit(params: &ShutdownCommitParams, current: &ShutdownTarget, receipt: Option<&ShutdownReceipt>) -> Result<(), crate::sdk_codegen::CommandError> {
    validate_target(&params.target, current)?;
    if receipt.map(shutdown_receipt_digest).transpose()?.as_deref() != Some(params.receipt_sha256.as_str()) {
        return Err(crate::sdk_codegen::CommandError::Invalid(
            "shutdown commit requires the completed save receipt from this process".into(),
        ));
    }
    Ok(())
}

crate::action_command! {
    /// Discover self-exit support and bind the target before any drain.
    pub struct SystemShutdownTarget;
    name: "system/shutdown-target",
    access: Privileged,
    params: SystemQuery,
    output: ShutdownBinding,
    run(_this, _ctx, _p) => {
        if crate::runtime::signal_runtime().is_none() {
            return Err(crate::sdk_codegen::CommandError::Internal("runtime is not installed; shutdown handoff unavailable".into()));
        }
        Ok(ShutdownBinding { target: shutdown_target().clone(), live_work: live_work() })
    }
}

crate::action_command! {
    /// Drain the bound process through the existing retained shutdown operation.
    pub struct SystemShutdownDrain;
    name: "system/shutdown-drain",
    access: Privileged,
    params: ShutdownDrainParams,
    output: ShutdownResult,
    run(_this, ctx, p) => {
        validate_target(&p.target, shutdown_target())?;
        // Checked here, at the one-way step, not only at the target: work can start
        // between the two (a reboot's warm build runs in between).
        refuse_over_live_work(&live_work(), p.over_live_work)?;
        <SystemShutdown as crate::sdk_codegen::ActionCommand>::run(&SystemShutdown, ctx, SystemQuery {}).await
    }
}

crate::action_command! {
    /// Acknowledge a received save receipt and exit THIS core. Successful commit
    /// closes the connection; the CLI must observe process exit, not expect a reply.
    /// No timer guesses when the preceding drain response reached the caller.
    pub struct SystemShutdownCommit;
    name: "system/shutdown-commit",
    access: Privileged,
    params: ShutdownCommitParams,
    output: SystemQuery,
    run(_this, _ctx, p) => {
        validate_commit(&p, shutdown_target(), crate::runtime::shutdown_receipt().as_ref())?;
        crate::modules::sentinel::shutdown_all_sentinels();
        // Match the signal path: save/join is already complete, and native static
        // destructors are deliberately skipped. No await between validation and exit.
        unsafe { libc::_exit(0) }
    }
}

/// How the core answered a stop request. Carries the receipt so the caller can exit
/// non-zero and NAME what did not save, rather than reporting a success it never
/// established.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/system/ShutdownResult.ts"
)]
pub struct ShutdownResult {
    /// Every module's durable state reached disk. The one field a caller may key an exit
    /// code on.
    pub state_is_durable: bool,
    /// One line naming what did not save, if anything.
    pub summary: String,
    pub receipt: ShutdownReceipt,
}

crate::action_command! {
    /// Drain, save and join every module. Return the retained shutdown receipt before
    /// the calling CLI tears down the process.
    pub struct SystemShutdown;
    name: "system/shutdown",
    access: Privileged,
    params: SystemQuery,
    output: ShutdownResult,
    run(_this, _ctx, _p) => {
        // TRIGGER AND OBSERVE — the operation itself belongs to the runtime.
        //
        // This handler used to run the broadcast inline, and a socket handler is
        // cancelled when its client goes away. A CLI that died or was interrupted
        // mid-request therefore left the node with turn ingress CLOSED and no shutdown:
        // every citizen refusing work, nothing saved, nothing to restart it. Strictly
        // worse than the forced kill this verb exists to replace. `begin_shutdown` runs
        // it in a task no connection owns, and is idempotent, so a signal racing this
        // request joins the same broadcast rather than saving twice over one state.
        // Refuse BEFORE triggering when no runtime is installed. `begin_shutdown`
        // publishes an empty receipt in that case so no observer hangs — but an empty
        // receipt is "all zero modules were durable", which is true and says nothing, and
        // the caller reads a success here as permission to stop hard-killing. A core in
        // its boot window has state at stake and no one to save it; that is a refusal,
        // not a clean stop.
        if crate::runtime::signal_runtime().is_none() {
            return Err(crate::sdk_codegen::CommandError::Internal(
                "no runtime is installed yet — the core cannot stop gracefully, so nothing                  here says its state is durable"
                    .to_string(),
            ));
        }
        // The settle comes first (card 32fa22ba): admitted turns finish, bounded by their
        // measured remaining time and capped by TURN_SETTLE_CAP, before any module saves.
        let settle = crate::cognition::turn_ingress::TURN_SETTLE_CAP;
        let rx = crate::runtime::begin_shutdown(settle);

        // Bounded so a wedged module cannot hold the connection open forever. A timeout
        // here does NOT cancel the shutdown — it is still running, un-cancelled, in the
        // runtime's task — so the honest answer is that durability is unknown, not that
        // it failed.
        // The settle's cap on top of the save's own 15 s, so the turns' wait is never
        // charged against the save.
        let observe = std::time::Duration::from_secs(15) + settle;
        let Some(receipt) = crate::runtime::await_shutdown(rx, observe).await else {
            return Err(crate::sdk_codegen::CommandError::Internal(format!(
                "shutdown is still running after {}s — it was NOT cancelled, and whether                  every module saved is unknown from here",
                observe.as_secs()
            )));
        };

        // The process is deliberately still alive. Every module has drained, saved and
        // joined; what a stopped node is FOR is the caller's decision — the CLI tears it
        // down having read this receipt, and the signal path exits because it must.
        //
        // The previous version exited here on a 400ms timer, which proved nothing about
        // whether this response had reached the wire: a handler that kills its own
        // process cannot also answer, and a timer guessing when its answer flushed is a
        // guess. Handing the teardown back to the caller removes the question instead of
        // estimating it.
        Ok(ShutdownResult {
            state_is_durable: receipt.state_is_durable(),
            summary: receipt.summary(),
            receipt,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{ActionCommand, Ctx};

    // what this catches: the access level. `system/shutdown` on the AiSafe surface would
    // let any persona stop the node it is thinking inside.
    #[test]
    fn stopping_the_node_is_privileged_not_ai_safe() {
        assert_eq!(SystemShutdown::NAME, "system/shutdown");
        assert!(matches!(
            SystemShutdown::ACCESS,
            crate::sdk_codegen::AccessLevel::Privileged
        ));
        for access in [SystemShutdownTarget::ACCESS, SystemShutdownDrain::ACCESS, SystemShutdownCommit::ACCESS] {
            assert!(matches!(access, crate::sdk_codegen::AccessLevel::Privileged));
        }
    }

    // A receipt acknowledgment must never exit a replacement process, or exit
    // before the existing shutdown owner has finished saving and joining.
    #[test]
    fn commit_requires_same_instance_and_completed_received_receipt() {
        let current = ShutdownTarget { pid: 42, instance: "first".into() };
        let receipt = ShutdownReceipt { modules: vec![], total_ms: 19 };
        let mut params = ShutdownCommitParams { target: current.clone(), receipt_sha256: shutdown_receipt_digest(&receipt).unwrap() };
        assert!(validate_commit(&params, &current, None).is_err());
        assert!(validate_commit(&params, &current, Some(&receipt)).is_ok());
        params.target.instance = "previous process with same pid".into();
        assert!(validate_commit(&params, &current, Some(&receipt)).is_err());
        params.target = current.clone();
        params.target.pid += 1;
        assert!(validate_commit(&params, &current, Some(&receipt)).is_err());
        params.target = current.clone();
        params.receipt_sha256.push('0');
        assert!(validate_commit(&params, &current, Some(&receipt)).is_err());
    }

    // Exercise the real exit in an isolated test child, never the serving core.
    // The child must obtain the retained receipt before commit can end it.
    #[tokio::test]
    async fn acknowledged_commit_exits_only_after_receipt() {
        const CHILD: &str = "CONTINUUM_TEST_SHUTDOWN_COMMIT_CHILD";
        if std::env::var_os(CHILD).as_deref() == Some(std::ffi::OsStr::new("1")) {
            crate::runtime::install_signal_shutdown(std::sync::Arc::new(crate::runtime::Runtime::new()));
            let ctx = Ctx::default();
            let target = SystemShutdownTarget.run(&ctx, SystemQuery {}).await.unwrap().target;
            let stale = ShutdownTarget { pid: target.pid, instance: "old instance".into() };
            let drain = |target: ShutdownTarget| ShutdownDrainParams { target, over_live_work: false };
            assert!(SystemShutdownDrain.run(&ctx, drain(stale)).await.is_err());
            assert!(crate::runtime::shutdown_receipt().is_none());
            let result = SystemShutdownDrain.run(&ctx, drain(target.clone())).await.unwrap();
            let params = ShutdownCommitParams { target, receipt_sha256: shutdown_receipt_digest(&result.receipt).unwrap() };
            use std::io::Write;
            println!("received-completed-shutdown-receipt");
            std::io::stdout().flush().unwrap();
            SystemShutdownCommit.run(&ctx, params).await.unwrap();
            panic!("commit returned instead of exiting");
        }
        let mut command = tokio::process::Command::new(std::env::current_exe().unwrap());
        command.args(["--exact", "commands::system::shutdown::tests::acknowledged_commit_exits_only_after_receipt", "--nocapture"])
            .env(CHILD, "1").kill_on_drop(true);
        #[cfg(windows)]
        command.creation_flags(0x08000000);
        let output = tokio::time::timeout(std::time::Duration::from_secs(20), command.output())
            .await.expect("shutdown test child must exit within its deadline").unwrap();
        assert!(output.status.success(), "child failed: {}", String::from_utf8_lossy(&output.stderr));
        assert!(String::from_utf8_lossy(&output.stdout).contains("received-completed-shutdown-receipt"));
    }

    // what this catches: a shutdown that kills a live solve or eval run without
    // consent, and a wire change that breaks the CLI on the other side of a version skew:
    // a reboot's NEW CLI drains the OLD core, and an old CLI may drain a new one.
    #[test]
    fn live_work_refuses_a_drain_without_consent_across_versions() {
        let live = vec![
            LiveWork::Benchmark { run: "r1".into(), instance: "django__django-1".into() },
            LiveWork::Eval { run: "e9".into() },
        ];
        let crate::sdk_codegen::CommandError::Invalid(msg) = refuse_over_live_work(&live, false).unwrap_err() else {
            panic!("live work is a domain precondition, not an internal failure");
        };
        assert!(msg.contains("benchmark run r1 on django__django-1") && msg.contains("eval run e9"), "{msg}");
        assert!(msg.contains("Nothing was drained"), "{msg}");
        assert!(refuse_over_live_work(&live, true).is_ok(), "consent drains");
        assert!(refuse_over_live_work(&[], false).is_ok(), "nothing live drains");

        let target = ShutdownTarget { pid: 7, instance: "i".into() };
        // An old CLI reads the new binding as the bare target it knew.
        let binding = serde_json::to_value(ShutdownBinding { target: target.clone(), live_work: live }).unwrap();
        assert_eq!(serde_json::from_value::<ShutdownTarget>(binding.clone()).unwrap(), target);
        assert_eq!(binding["liveWork"][1], serde_json::json!({ "kind": "eval", "run": "e9" }));
        // An old CLI's bare target is a drain with no consent.
        let old = serde_json::from_value::<ShutdownDrainParams>(serde_json::to_value(&target).unwrap()).unwrap();
        assert!(!old.over_live_work);
        // A new CLI's params are still a target to an old core.
        let new = ShutdownDrainParams { target: target.clone(), over_live_work: true };
        assert_eq!(serde_json::from_value::<ShutdownTarget>(serde_json::to_value(new).unwrap()).unwrap(), target);
    }

    // what this catches: a core with no runtime answering "stopped cleanly". The caller
    // treats a successful response as permission to stop hard-killing, so a success here
    // would be read as "state is safe" about a core that never ran a save.
    #[tokio::test]
    async fn without_a_runtime_it_refuses_rather_than_claiming_a_clean_stop() {
        // `signal_runtime()` is a process-wide OnceLock. In a test binary that has not
        // installed one this is None; if another test in the same process installed it,
        // this assertion would be about a live runtime, so it checks the branch it can.
        if crate::runtime::signal_runtime().is_none() {
            let err = SystemShutdown
                .run(&Ctx::default(), SystemQuery {})
                .await
                .expect_err("no runtime must refuse");
            // Matched on the VARIANT, not on formatted text: `Internal` is the category
            // that says "the node could not do this", and a future refactor that turned
            // this into `Invalid` (a caller error) would be a real behaviour change the
            // test should catch rather than paper over with a substring match.
            let crate::sdk_codegen::CommandError::Internal(msg) = err else {
                panic!("a core that cannot stop gracefully is an Internal failure, not a caller error");
            };
            assert!(
                msg.contains("durable"),
                "the refusal must say what is unestablished, got: {msg}"
            );
        }
    }
}
