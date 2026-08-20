//! `code/git/apply` — apply a peer's unified diff to the caller's workspace.
//!
//! The RECEIVING half of diffs-over-the-room: one citizen runs `code/git/diff`
//! and posts the patch; another applies it here. This is the minimal
//! consolidation rail the Conway team asked for live (2026-07-11 — three
//! parallel implementations in three workspaces and no way to merge) before
//! the full branch/merge machinery lands (#49). `check=true` validates
//! without touching files — review before you take a patch.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{blocking_git, workspace_root_for};
use crate::code::git_bridge;
use crate::modules::code::CodeState;
use crate::sdk_codegen::CommandError;

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/code/GitApplyParams.ts"
)]
pub struct GitApplyParams {
    /// The unified diff to apply (the text a peer shared — the output of
    /// `code/git/diff`).
    pub patch: String,
    /// When true, only CHECK that the patch would apply cleanly — no files are
    /// modified. Use this to review a peer's patch before taking it.
    #[serde(default)]
    pub check: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/code/GitApplyResult.ts"
)]
pub struct GitApplyResult {
    /// What happened: applied, or checked-clean.
    pub message: String,
}

crate::action_command! {
    /// Apply a unified diff (a peer's shared patch) to your workspace files.
    /// Pass check=true to validate the patch WITHOUT applying it. Rejects a
    /// patch that does not apply cleanly, naming git's reason.
    pub struct CodeGitApply { state: Arc<CodeState> }
    name: "code/git/apply",
    access: AiSafe,
    native: true,
    aliases: &["git_apply"],
    params: GitApplyParams,
    output: GitApplyResult,
    run(this, ctx, p) => {
        if p.patch.trim().is_empty() {
            return Err(CommandError::Invalid(
                "code/git/apply: 'patch' is required (the unified diff text — the output of code/git/diff)".into(),
            ));
        }
        let root = workspace_root_for(&this.state, ctx)?;
        let check = p.check;
        let message = blocking_git(move || git_bridge::git_apply(&root, &p.patch, check))
            .await?
            .map_err(|e| CommandError::Invalid(format!(
                "patch did not apply cleanly: {e}. Regenerate it against the current \
                 file state (ask the author for a fresh code/git/diff), or apply with \
                 check=true to inspect the conflict."
            )))?;
        Ok(GitApplyResult { message })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::git_bridge;

    // what this catches: the full diffs-over-the-room loop — author repo commits a
    // change, `git_diff` produces the patch, a SECOND repo (same base) applies it
    // via `git_apply` and lands the identical content; a stale/garbage patch is
    // REJECTED loudly, not half-applied. // regression for the consolidation rail
    #[test]
    fn peer_patch_round_trips_between_two_workspaces() {
        let base = std::env::temp_dir().join(format!("git-apply-{}", uuid::Uuid::new_v4()));
        let a = base.join("author");
        let b = base.join("receiver");
        for d in [&a, &b] {
            std::fs::create_dir_all(d).unwrap();
            std::fs::write(d.join("life.rs"), "fn main() {}\n").unwrap();
            git_bridge::git_init_if_needed(d).expect("init");
        }
        // Author edits + stages; diff of the staged change is the shared patch.
        std::fs::write(a.join("life.rs"), "fn main() { println!(\"glider\"); }\n").unwrap();
        let patch = git_bridge::git_diff(&a, false).expect("diff");
        assert!(
            patch.contains("glider"),
            "patch carries the change: {patch}"
        );

        // Receiver checks, then applies.
        git_bridge::git_apply(&b, &patch, true).expect("check passes");
        git_bridge::git_apply(&b, &patch, false).expect("apply");
        let got = std::fs::read_to_string(b.join("life.rs")).unwrap();
        assert!(
            got.contains("glider"),
            "receiver has the author's change: {got}"
        );

        // Garbage is rejected loudly, files untouched.
        let err = git_bridge::git_apply(&b, "not a patch", false).unwrap_err();
        assert!(!err.is_empty());
        let _ = std::fs::remove_dir_all(&base);
    }
}
