//! `code/git/push` — push the caller's workspace branch to its remote.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{blocking_git, workspace_root_for};
use crate::code::git_bridge;
use crate::modules::code::CodeState;
use crate::sdk_codegen::CommandError;

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/code/GitPushParams.ts")]
pub struct GitPushParams {
    /// Remote name (e.g. `origin`). Omit for git's default.
    #[serde(default)]
    pub remote: Option<String>,
    /// Branch to push. Omit for git's default (the current branch).
    #[serde(default)]
    pub branch: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/code/GitPushResult.ts")]
pub struct GitPushResult {
    /// Raw `git push` output.
    pub output: String,
}

crate::action_command! {
    /// Push commits to a remote (`git push`). Omit `remote`/`branch` to use git's
    /// defaults. Outward-facing — publishes your commits to the shared remote, so
    /// it's the one git verb that escapes the workspace sandbox. Declared
    /// `Privileged` (the same tier as `code/shell`): a local persona (Trusted) or
    /// the owner runs it; a remote `Provisional` room peer is denied by the grid
    /// ACL — they can't make your citizen push to a shared remote on their say-so.
    pub struct CodeGitPush { state: Arc<CodeState> }
    name: "code/git/push",
    access: Privileged,
    aliases: &["git_push"],
    params: GitPushParams,
    output: GitPushResult,
    run(this, ctx, p) => {
        let root = workspace_root_for(&this.state, ctx)?;
        let remote = p.remote.unwrap_or_default();
        let branch = p.branch.unwrap_or_default();
        let output = blocking_git(move || git_bridge::git_push(&root, &remote, &branch))
            .await?
            .map_err(CommandError::Internal)?;
        Ok(GitPushResult { output })
    }
}
