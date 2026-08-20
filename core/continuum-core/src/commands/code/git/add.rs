//! `code/git/add` — stage paths in the caller's workspace.

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
    export_to = "../../../protocol/typescript/code/GitAddParams.ts"
)]
pub struct GitAddParams {
    /// Paths to stage, relative to the workspace root. Empty stages nothing.
    #[serde(default)]
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/code/GitAddResult.ts"
)]
pub struct GitAddResult {
    /// Raw `git add` output (usually empty on success).
    pub output: String,
}

crate::action_command! {
    /// Stage files for commit (`git add`). Pass workspace-relative paths; stages
    /// nothing if `paths` is empty.
    pub struct CodeGitAdd { state: Arc<CodeState> }
    name: "code/git/add",
    access: AiSafe,
    aliases: &["git_add"],
    params: GitAddParams,
    output: GitAddResult,
    run(this, ctx, p) => {
        let root = workspace_root_for(&this.state, ctx)?;
        let output = blocking_git(move || {
            let refs: Vec<&str> = p.paths.iter().map(String::as_str).collect();
            git_bridge::git_add(&root, &refs)
        })
        .await?
        .map_err(CommandError::Internal)?;
        Ok(GitAddResult { output })
    }
}
