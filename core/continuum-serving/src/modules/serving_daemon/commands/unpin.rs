//! `serving/unpin` — release the force-serve pin set by [`serving/pin`](super::pin),
//! returning this host to autonomic best-fit. The inverse of the hard pin: a model
//! that can be force-served must be releasable without a reboot.
//!
//! ## Releases to autonomic, does not unload
//!
//! This clears the daemon's force-pin, so its planner is once again free to pick
//! the most-capable model that fits the current budget. It does NOT unload the
//! pinned model — if it is still the best fit, the planner keeps serving it. To
//! free its lane, use [`serving/unload`](super::unload). Clearing a pin that was
//! never set is not an error (idempotent), but the report says so plainly.
//!
//! ## Gating
//!
//! `Privileged` — it changes what dictates GPU residency on this node.

use crate::modules::serving_daemon::ServingIntent;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::modules::serving_pin_store::ServingPinStore;

/// `serving/unpin` takes no parameters — there is at most one force-pin per host.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/model_registry/ServingUnpinParams.ts"
)]
pub struct ServingUnpinParams {}

/// What `serving/unpin` did: which model (if any) was released back to autonomic
/// selection.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/model_registry/UnpinReport.ts"
)]
pub struct UnpinReport {
    /// The model id that was pinned and is now released, or `None` if no pin was
    /// set (nothing changed).
    pub released_model: Option<String>,
    /// Human-readable summary.
    pub detail: String,
}

crate::action_command! {
    /// Release the force-serve pin and return this host to autonomic best-fit —
    /// the inverse of serving/pin. The planner is free again to pick the
    /// most-capable model that fits the budget. Does not unload the formerly
    /// pinned model (use serving/unload for that); if it is still the best fit it
    /// keeps serving. Idempotent when no pin was set.
    pub struct ServingUnpin {
        intent: ServingIntent,
        /// The same store `serving/pin` writes — handed in, never resolved here.
        store: ServingPinStore,
    }
    name: "serving/unpin",
    access: Privileged,
    params: ServingUnpinParams,
    output: UnpinReport,
    run(this, _ctx, _p) => {
        let released_model = this.intent.set_pin(None, true);
        if released_model.is_some() {
            this.store.clear();
        }
        let detail = match &released_model {
            Some(m) => format!(
                "released pin on '{}'; the planner is autonomic again and will serve the best fit on its next tick",
                m
            ),
            None => "no force-pin was set — nothing to release; the planner was already autonomic".to_string(),
        };
        Ok(UnpinReport {
            released_model,
            detail,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{ActionCommand, Ctx};

    // what this catches: the wire name mirrors the file path — the routing
    // contract that lets the typed registry dispatch `serving/unpin` to this command.
    #[test]
    fn name_mirrors_path() {
        assert_eq!(ServingUnpin::NAME, "serving/unpin");
    }

    // what this catches: unpin releases a set pin (the watch flips back to None,
    // which returns the planner to autonomic) and reports which model it freed.
    #[tokio::test]
    async fn release_clears_the_pin_and_names_it() {
        let intent = ServingIntent::new(Some("coder-14b".to_string()));
        let pin_rx = intent.subscribe();
        // The store is the test's own tempdir. Before 2026-09-16 this test removed
        // the REAL machine's pin file on every `cargo test`.
        let dir = tempfile::tempdir().expect("tempdir");
        let store = ServingPinStore::under_home(dir.path());
        store.save("coder-14b");
        let report = ServingUnpin {
            intent,
            store: store.clone(),
        }
        .run(&Ctx::default(), ServingUnpinParams {})
        .await
        .expect("unpin ok");
        assert_eq!(report.released_model.as_deref(), Some("coder-14b"));
        assert!(
            pin_rx.borrow().pinned.is_none(),
            "the pin watch is cleared → autonomic again"
        );
        assert!(
            store.load().is_none(),
            "the persisted pin is dropped from the command's own store"
        );
    }

    // what this catches: unpin with no pin set is idempotent (not an error) and
    // says so plainly rather than misleading the caller.
    #[tokio::test]
    async fn release_with_no_pin_is_idempotent() {
        let intent = ServingIntent::new(None);
        let observed = intent.subscribe();
        let dir = tempfile::tempdir().expect("tempdir");
        let report = ServingUnpin {
            intent,
            store: ServingPinStore::under_home(dir.path()),
        }
        .run(&Ctx::default(), ServingUnpinParams {})
        .await
        .expect("unpin ok");
        assert!(report.released_model.is_none());
        assert_eq!(
            observed.borrow().revision,
            1,
            "explicit unpin supersedes old intent even when already unset"
        );
    }
}
