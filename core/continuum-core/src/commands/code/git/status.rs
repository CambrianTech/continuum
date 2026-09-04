//! `code/git/status` — branch + changed files for the caller's workspace.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{blocking_git, workspace_root_for};
use crate::code::git_bridge;
use crate::code::types::GitStatusInfo;
use crate::modules::code::CodeState;

/// `code/git/status` takes no input — it reports the caller's workspace.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/code/GitStatusParams.ts"
)]
pub struct GitStatusParams {}

crate::action_command! {
    /// Show git status for your workspace: current branch and the modified, added,
    /// deleted, and untracked files. Use before staging or committing to see what
    /// changed.
    pub struct CodeGitStatus { state: Arc<CodeState> }
    name: "code/git/status",
    access: AiSafe,
    native: true,
    aliases: &["git_status"],
    params: GitStatusParams,
    output: GitStatusInfo,
    run(this, ctx, _p) => {
        let root = workspace_root_for(&this.state, ctx)?;
        blocking_git(move || git_bridge::git_status(&root)).await
    }
}
