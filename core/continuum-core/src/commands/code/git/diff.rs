//! `code/git/diff` — unified diff of the caller's workspace (unstaged or staged).

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{blocking_git, workspace_root_for};
use crate::code::git_bridge;
use crate::modules::code::CodeState;
use crate::sdk_codegen::CommandError;

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/code/GitDiffParams.ts")]
pub struct GitDiffParams {
    /// Show STAGED changes (`--cached`) instead of unstaged working-tree changes.
    #[serde(default)]
    pub staged: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/code/GitDiffResult.ts")]
pub struct GitDiffResult {
    /// The raw unified diff text.
    pub diff: String,
}

crate::action_command! {
    /// Show the git diff for your workspace. By default shows unstaged working-tree
    /// changes; set `staged: true` for the staged (`--cached`) diff.
    pub struct CodeGitDiff { state: Arc<CodeState> }
    name: "code/git/diff",
    access: AiSafe,
    native: true,
    params: GitDiffParams,
    output: GitDiffResult,
    run(this, ctx, p) => {
        let root = workspace_root_for(&this.state, ctx)?;
        let diff = blocking_git(move || git_bridge::git_diff(&root, p.staged))
            .await?
            .map_err(CommandError::Internal)?;
        Ok(GitDiffResult { diff })
    }
}
