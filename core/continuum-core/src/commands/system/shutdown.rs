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
    /// Drain, save and join every module, then exit the process. Returns the receipt
    /// BEFORE exiting, so the caller learns what was saved rather than only that the
    /// process is gone.
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
        let rx = crate::runtime::begin_shutdown();

        // Bounded so a wedged module cannot hold the connection open forever. A timeout
        // here does NOT cancel the shutdown — it is still running, un-cancelled, in the
        // runtime's task — so the honest answer is that durability is unknown, not that
        // it failed.
        const OBSERVE: std::time::Duration = std::time::Duration::from_secs(15);
        let Some(receipt) = crate::runtime::await_shutdown(rx, OBSERVE).await else {
            return Err(crate::sdk_codegen::CommandError::Internal(format!(
                "shutdown is still running after {}s — it was NOT cancelled, and whether                  every module saved is unknown from here",
                OBSERVE.as_secs()
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
