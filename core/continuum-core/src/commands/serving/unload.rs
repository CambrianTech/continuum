//! `serving/unload` — free a model's VRAM lane at runtime. The VRAM-axis "free",
//! counterpart to [`serving/load`](super::load) (and the sibling of the disk-axis
//! [`models/remove`](crate::commands::models::remove)).
//!
//! ## What it does
//!
//! Pins a model OFF in the serving daemon's suppress-set. The daemon is
//! holistically in charge of VRAM, so unloading is not "kill a process" — it is an
//! edit to the exclude list the planner honors: once the id is suppressed, the
//! next plan recompute can no longer choose it, and the reconcile drops it
//! (relaunch to the next-best candidate, or publish empty) — the lane frees live,
//! no reboot. If the model was the one currently being served, this command waits
//! for the published [`ServingSnapshot`] to confirm the lane actually left it
//! before returning, so the caller knows the VRAM is free (e.g. before a
//! `models/remove`).
//!
//! ## Fail loud
//!
//! Unknown model id ⇒ [`CommandError::NotFound`] (a typo'd pin that silently does
//! nothing is a lie). If the lane was serving the model and does not free within
//! the convergence window ⇒ [`CommandError::Internal`] naming that the pin is
//! applied but the daemon did not converge (check `serving/status`) — the pin
//! stays applied (rolling it back would be a worse, hidden failure).
//!
//! ## Gating
//!
//! `Privileged` — it changes what occupies GPU memory on this node.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::sync::watch;
use ts_rs::TS;

use crate::inference::llama_server::ServingSnapshot;
use crate::model_registry::live::ModelCatalog;
use crate::sdk_codegen::CommandError;

/// How long to wait for a served lane to actually free after pinning the model
/// off. The daemon recomputes + reconciles on its tick (5s); a relaunch to the
/// next candidate or a teardown to empty completes within a few ticks. Generous so
/// a healthy daemon always confirms; a timeout means the daemon is unhealthy, not
/// that the window was too tight.
const CONVERGE_TIMEOUT: Duration = Duration::from_secs(20);

/// Which model's VRAM lane to free.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/model_registry/ServingUnloadParams.ts"
)]
pub struct ServingUnloadParams {
    /// The model id as it appears in `models/list`. Fails loud if unknown. May be
    /// pinned off even if it is not the one currently served (a preemptive pin so
    /// the planner will not pick it); if it IS the served model, the command waits
    /// for the lane to free before returning.
    pub model_id: String,
}

/// What `serving/unload` did: whether the model was actually occupying a lane, and
/// what (if anything) the daemon is serving now that the lane is free.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/model_registry/UnloadReport.ts"
)]
pub struct UnloadReport {
    /// True if this model was the one being served when unload was called (its VRAM
    /// was actually reclaimed); false if it was pinned off preemptively.
    pub was_serving: bool,
    /// What the daemon is serving now that the lane freed — `None` if nothing is
    /// live (no other candidate fit the budget, or the node is intentionally idle).
    pub now_serving: Option<String>,
    /// Human-readable summary.
    pub detail: String,
}

crate::action_command! {
    /// Free a model's VRAM lane at runtime — no reboot. Pins the model OFF in the
    /// serving daemon's suppress-set so the planner drops it and the lane frees
    /// (relaunching to the next-best fit, or going idle). If the model was the one
    /// being served, waits for the lane to actually free before returning. The
    /// inverse of serving/load. Fails loud on an unknown model id. Returns whether
    /// VRAM was reclaimed and what is serving now.
    pub struct ServingUnload {
        suppress: watch::Sender<Arc<HashSet<String>>>,
        serving: watch::Receiver<ServingSnapshot>,
        catalog: Arc<ModelCatalog>,
    }
    name: "serving/unload",
    access: Privileged,
    params: ServingUnloadParams,
    output: UnloadReport,
    run(this, _ctx, p) => {
        // 1. The model must exist in the live universe — never pin a typo.
        if this.catalog.snapshot().get(&p.model_id).is_none() {
            return Err(CommandError::NotFound(format!(
                "unknown model id '{}' — call models/list to see the live universe",
                p.model_id
            )));
        }

        // 2. Was it the lane actually in use? (Determines whether we must wait for
        //    real VRAM to free, vs a preemptive pin of something not yet served.)
        let was_serving =
            this.serving.borrow().active_model.as_deref() == Some(p.model_id.as_str());

        // 3. Pin it OFF. Idempotent — re-unloading an already-pinned model is fine.
        this.suppress.send_modify(|s| {
            Arc::make_mut(s).insert(p.model_id.clone());
        });

        // 4. If it held the lane, wait for the daemon's next reconcile to actually
        //    free it before we claim success.
        if was_serving {
            let mut rx = this.serving.clone();
            let freed = tokio::time::timeout(
                CONVERGE_TIMEOUT,
                rx.wait_for(|s| s.active_model.as_deref() != Some(p.model_id.as_str())),
            )
            .await;
            if freed.is_err() {
                return Err(CommandError::Internal(format!(
                    "model '{}' pinned off, but the serving lane did not free within {}s — the serving daemon may be unhealthy; the pin remains applied (check serving/status)",
                    p.model_id,
                    CONVERGE_TIMEOUT.as_secs()
                )));
            }
        }

        let now_serving = this.serving.borrow().active_model.clone();
        let detail = if was_serving {
            match &now_serving {
                Some(next) => format!(
                    "freed model '{}' from VRAM; daemon relaunched to '{next}'",
                    p.model_id
                ),
                None => format!(
                    "freed model '{}' from VRAM; no other candidate fit the budget — node is idle",
                    p.model_id
                ),
            }
        } else {
            format!(
                "model '{}' pinned off (it was not the served lane); planner will not pick it until serving/load",
                p.model_id
            )
        };

        Ok(UnloadReport {
            was_serving,
            now_serving,
            detail,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the wire name mirrors the file path — the routing
    // contract that lets the typed registry dispatch `serving/unload` to this
    // command.
    #[test]
    fn name_mirrors_path() {
        use crate::sdk_codegen::ActionCommand;
        assert_eq!(ServingUnload::NAME, "serving/unload");
    }
}
