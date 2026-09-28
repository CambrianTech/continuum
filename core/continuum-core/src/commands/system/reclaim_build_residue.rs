//! `system/reclaim-build-residue` — reclaim resident citizens' stale cargo build output, on
//! request, never automatically (card 10e6c5e5; Codex's decision on #4528).
//!
//! A resident citizen's workspace is pinned whole, and with it a private `target/` from before
//! every substrate cargo was pointed at the shared cache (the IntelMac, 2026-09-28: 36 GB
//! across three residents, last written Sep 4). Cargo has no lock spanning every build entry
//! path, so pressure relief cannot prove no writer exists; this verb is the explicit,
//! quiescent act instead. Run it when no build is expected in citizens' workspaces.
//!
//! A DRY RUN by default: it judges each tree under the gates
//! ([`crate::system_resources::citizen_workspace_pool`]) and takes nothing. `--apply true`
//! takes each tree that passes them. `Privileged`: it deletes files on the node.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::system_resources::citizen_workspace_pool::{CitizenWorkspacePool, ResidueOutcome};

/// Input for `system/reclaim-build-residue`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/system/ReclaimBuildResidueParams.ts")]
pub struct ReclaimBuildResidueParams {
    /// Take the trees that pass every gate. Absent or false is a dry run that touches nothing.
    #[serde(default)]
    pub apply: bool,
}

/// What the reclaim decided, one line per tree.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/system/ReclaimBuildResidueResult.ts")]
pub struct ReclaimBuildResidueResult {
    pub applied: bool,
    /// Bytes freed (reclaimed trees plus swept leftovers); 0 on a dry run.
    #[ts(type = "number")]
    pub freed_bytes: u64,
    pub outcomes: Vec<ResidueOutcome>,
}

crate::action_command! {
    /// Judge (and with `apply`, reclaim) every resident citizen's stale cargo `target/`: only
    /// a cargo-owned tree, idle past the dormancy window, with no build holding a lock in it
    /// and no solve holding her hands. Her workspace and work are never touched.
    pub struct SystemReclaimBuildResidue;
    name: "system/reclaim-build-residue",
    access: Privileged,
    params: ReclaimBuildResidueParams,
    output: ReclaimBuildResidueResult,
    run(_this, _ctx, p) => {
        let root = crate::commands::benchmark::continuum_home()?.join("citizens");
        let now_ms = crate::persona::trace::now_ms();
        let apply = p.apply;
        let outcomes = tokio::task::spawn_blocking(move || {
            CitizenWorkspacePool::reclaim_build_residue(&root, now_ms, apply)
        })
        .await
        .map_err(|e| crate::sdk_codegen::CommandError::Internal(format!("reclaim task: {e}")))?;
        let freed_bytes = outcomes
            .iter()
            .filter(|o| o.decision == "reclaimed" || o.decision == "swept")
            .map(|o| o.bytes)
            .sum();
        Ok(ReclaimBuildResidueResult { applied: p.apply, freed_bytes, outcomes })
    }
}
