//! `code/git/diff` — unified diff of the caller's workspace: by default her work since her
//! branch left its integration branch, or the unstaged or staged changes.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::workspace_root_for;
use crate::code::git_bridge;
use crate::modules::code::CodeState;
use crate::sdk_codegen::CommandError;

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/code/GitDiffParams.ts"
)]
pub struct GitDiffParams {
    /// Which changes: `yourWork` (default: everything since your branch left its
    /// integration branch, including the checkpoints committed for you), `unstaged`, `staged`.
    #[serde(default)]
    pub scope: git_bridge::DiffScope,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/code/GitDiffResult.ts"
)]
pub struct GitDiffResult {
    /// The raw unified diff text.
    pub diff: String,
    /// What the diff was taken against, so an empty diff says what it is empty against.
    pub against: String,
}

crate::action_command! {
    /// Show the git diff for your workspace. By default shows your work since your branch
    /// left its integration branch (committed checkpoints and the working tree); set `scope`
    /// to `unstaged` or `staged` for those alone.
    pub struct CodeGitDiff { state: Arc<CodeState> }
    name: "code/git/diff",
    access: AiSafe,
    native: true,
    aliases: &["git_diff"],
    effect: Observes,
    params: GitDiffParams,
    output: GitDiffResult,
    run(this, ctx, p) => {
        let root = workspace_root_for(&this.state, ctx).await?;
        let diff = git_bridge::git_diff(&root, p.scope).await.map_err(CommandError::Internal)?;
        Ok(GitDiffResult { diff: diff.text, against: diff.against })
    }
}
