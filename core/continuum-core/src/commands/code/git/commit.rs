//! `code/git/commit` — commit staged changes in the caller's workspace.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{blocking_git, workspace_root_for};
use crate::code::git_bridge;
use crate::modules::code::CodeState;
use crate::sdk_codegen::CommandError;

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/code/GitCommitParams.ts")]
pub struct GitCommitParams {
    /// The commit message.
    pub message: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/code/GitCommitResult.ts")]
pub struct GitCommitResult {
    /// The full SHA of the new commit.
    pub hash: String,
}

crate::action_command! {
    /// Commit the staged changes (`git commit -m`). Returns the new commit's full
    /// SHA. Stage files with `code/git/add` first.
    pub struct CodeGitCommit { state: Arc<CodeState> }
    name: "code/git/commit",
    access: AiSafe,
    native: true,
    aliases: &["git_commit"],
    params: GitCommitParams,
    output: GitCommitResult,
    run(this, ctx, p) => {
        if p.message.trim().is_empty() {
            return Err(CommandError::Invalid(
                "code/git/commit: 'message' is required".into(),
            ));
        }
        let root = workspace_root_for(&this.state, ctx)?;
        let hash = blocking_git(move || git_bridge::git_commit(&root, &p.message))
            .await?
            .map_err(CommandError::Internal)?;
        Ok(GitCommitResult { hash })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{ActionCommand, Ctx};

    // what this catches: a blank/whitespace commit message is REJECTED with a typed
    // Invalid error BEFORE any git call — never shell out `git commit -m ""`, never
    // silently default the message. Fail-loud, the commands-don't-swallow-errors
    // contract for a persona's git hands.
    #[tokio::test]
    async fn blank_commit_message_is_rejected() {
        // The guard rejects before any engine/workspace resolution, so empty
        // engine maps are enough — we never touch them.
        let state = Arc::new(CodeState::new(
            Arc::new(dashmap::DashMap::new()),
            Arc::new(dashmap::DashMap::new()),
            tokio::runtime::Handle::current(),
        ));
        let cmd = CodeGitCommit { state };
        let err = cmd
            .run(&Ctx::default(), GitCommitParams { message: "  ".into() })
            .await
            .unwrap_err();
        assert!(matches!(err, CommandError::Invalid(_)));
    }
}
