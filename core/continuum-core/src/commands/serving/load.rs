//! `serving/load` — permit a previously-unloaded model to occupy VRAM again. The
//! inverse of [`serving/unload`](super::unload): a model that can be unloaded must
//! be re-loadable without a reboot.
//!
//! ## Permits, does not force
//!
//! This removes the model from the serving daemon's suppress-set, returning it to
//! the candidate pool. It does **not** force the model into VRAM — the daemon
//! stays the authority on what actually occupies a lane and will serve this model
//! on its next tick only if it is the best fit for the current budget. That is the
//! honest contract: "load" lifts the operator's OFF pin; the autonomic planner
//! decides what wins. (Forcing a specific model regardless of budget would be a
//! different, future verb — a hard pin — not this one.)
//!
//! ## Fail loud
//!
//! Unknown model id ⇒ [`CommandError::NotFound`]. Un-suppressing a model that was
//! not pinned is not an error (idempotent), but the report says so plainly so the
//! caller is not misled into thinking it changed anything.
//!
//! ## Gating
//!
//! `Privileged` — it changes what is eligible to occupy GPU memory on this node.

use std::collections::HashSet;
use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::sync::watch;
use ts_rs::TS;

use crate::inference::llama_server::ServingSnapshot;
use crate::model_registry::live::ModelCatalog;
use crate::sdk_codegen::CommandError;

/// Which model to permit back into the serving candidate pool.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/model_registry/ServingLoadParams.ts"
)]
pub struct ServingLoadParams {
    /// The model id as it appears in `models/list`. Fails loud if unknown.
    pub model_id: String,
}

/// What `serving/load` did: whether an OFF pin was actually lifted, and what the
/// daemon is serving at the moment the pin was lifted (the planner may or may not
/// pick this model on its next tick, by budget).
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/model_registry/LoadReport.ts"
)]
pub struct LoadReport {
    /// True if the model was pinned off and this command lifted the pin; false if
    /// it was never unloaded (nothing changed).
    pub was_suppressed: bool,
    /// What the daemon is serving right now — the planner decides on its next tick
    /// whether the newly-permitted model wins the lane.
    pub now_serving: Option<String>,
    /// Human-readable summary.
    pub detail: String,
}

crate::action_command! {
    /// Permit a previously-unloaded model back into the serving candidate pool — no
    /// reboot. The inverse of serving/unload: lifts the operator's OFF pin so the
    /// planner MAY serve the model when it best fits the budget. Permits, never
    /// forces — the daemon stays the authority on VRAM. Fails loud on an unknown
    /// model id. Idempotent if the model was not pinned.
    pub struct ServingLoad {
        suppress: watch::Sender<Arc<HashSet<String>>>,
        serving: watch::Receiver<ServingSnapshot>,
        catalog: Arc<ModelCatalog>,
    }
    name: "serving/load",
    access: Privileged,
    params: ServingLoadParams,
    output: LoadReport,
    run(this, _ctx, p) => {
        // 1. The model must exist in the live universe — never permit a typo.
        if this.catalog.snapshot().get(&p.model_id).is_none() {
            return Err(CommandError::NotFound(format!(
                "unknown model id '{}' — call models/list to see the live universe",
                p.model_id
            )));
        }

        // 2. Lift the OFF pin if present. Track whether it actually changed.
        let was_suppressed = this.suppress.borrow().contains(&p.model_id);
        if was_suppressed {
            this.suppress.send_modify(|s| {
                Arc::make_mut(s).remove(&p.model_id);
            });
        }

        let now_serving = this.serving.borrow().active_model.clone();
        let detail = if was_suppressed {
            format!(
                "model '{}' permitted back into the candidate pool; the planner will serve it on its next tick if it best fits the budget",
                p.model_id
            )
        } else {
            format!(
                "model '{}' was not unloaded — nothing to lift; no change",
                p.model_id
            )
        };

        Ok(LoadReport {
            was_suppressed,
            now_serving,
            detail,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the wire name mirrors the file path — the routing
    // contract that lets the typed registry dispatch `serving/load` to this command.
    #[test]
    fn name_mirrors_path() {
        use crate::sdk_codegen::ActionCommand;
        assert_eq!(ServingLoad::NAME, "serving/load");
    }
}
