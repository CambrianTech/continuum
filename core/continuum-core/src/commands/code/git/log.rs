//! `code/git/log` — recent commit history for the caller's workspace.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{blocking_git, workspace_root_for};
use crate::code::git_bridge;
use crate::modules::code::CodeState;
use crate::sdk_codegen::CommandError;

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/code/GitLogParams.ts")]
pub struct GitLogParams {
    /// How many recent commits to return. Omit for the last 10.
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/code/GitLogResult.ts")]
pub struct GitLogResult {
    /// The formatted `git log` text.
    pub log: String,
}

crate::action_command! {
    /// Show recent commit history (`git log`) for your workspace. Returns the last
    /// `limit` commits (default 10).
    pub struct CodeGitLog { state: Arc<CodeState> }
    name: "code/git/log",
    access: AiSafe,
    aliases: &["git_log"],
    params: GitLogParams,
    output: GitLogResult,
    run(this, ctx, p) => {
        let root = workspace_root_for(&this.state, ctx)?;
        let count = p.limit.unwrap_or(10);
        let log = blocking_git(move || git_bridge::git_log(&root, count))
            .await?
            .map_err(CommandError::Internal)?;
        Ok(GitLogResult { log })
    }
}
