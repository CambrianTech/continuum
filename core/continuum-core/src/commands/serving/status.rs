//! `serving/status` — the live serving SNAPSHOT (which model is actually up,
//! ready, on what `/v1` url, with which genome layers).
//!
//! Dep-holding: reads the daemon's published
//! [`ServingSnapshot`](crate::inference::llama_server::ServingSnapshot) receiver
//! — a cheap `watch` borrow, no process probe. This is the "did the plan become
//! reality?" view, the inverse of `serving/plan` (the intent).

use serde::{Deserialize, Serialize};
use tokio::sync::watch;
use ts_rs::TS;

use crate::inference::llama_server::ServingSnapshot;

/// Params for `serving/status` — none (the whole snapshot is returned).
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/serving/ServingStatusParams.ts"
)]
pub struct ServingStatusParams {}

crate::action_command! {
    /// The live serving state: which model is actually up, whether `/health` is
    /// green, the `/v1` base url personas point their inference adapter at, and the
    /// LoRA genome layers loaded into the serving catalog. The "did the plan become
    /// reality?" view — read it instead of probing the llama-server process. Read-only.
    pub struct ServingStatus { serving: watch::Receiver<ServingSnapshot> }
    name: "serving/status",
    access: Privileged,
    params: ServingStatusParams,
    output: ServingSnapshot,
    run(this, _ctx, _p) => {
        Ok(this.serving.borrow().clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{ActionCommand, Ctx};

    fn cmd(snapshot: ServingSnapshot) -> ServingStatus {
        let (_tx, rx) = watch::channel(snapshot);
        ServingStatus { serving: rx }
    }

    // what this catches: name/access wiring — serving status is an operator/UI
    // read surface (reveals which model occupies VRAM), Privileged, not a persona
    // toolbelt action.
    #[test]
    fn name_and_access_wired() {
        assert_eq!(ServingStatus::NAME, "serving/status");
        assert!(matches!(
            ServingStatus::ACCESS,
            crate::sdk_codegen::AccessLevel::Privileged
        ));
    }

    // what this catches: the body returns the live snapshot from the captured
    // receiver — a regression that reads a stale/empty value instead of the
    // published one is caught.
    #[tokio::test]
    async fn returns_the_published_snapshot() {
        let mut snap = ServingSnapshot::empty();
        snap.active_model = Some("qwen3-coder".into());
        snap.ready = true;
        let out = cmd(snap.clone())
            .run(&Ctx::default(), ServingStatusParams::default())
            .await
            .expect("status read must succeed");
        assert_eq!(out.active_model.as_deref(), Some("qwen3-coder"));
        assert!(out.ready, "the published readiness flows through");
    }
}
