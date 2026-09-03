//! `code/github/pr-comment` — comment on a GitHub pull request (review participation).

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{run_gh, workspace_root_for};
use crate::modules::code::CodeState;
use crate::sdk_codegen::CommandError;

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/code/GithubPrCommentParams.ts")]
pub struct GithubPrCommentParams {
    /// The pull request number to comment on.
    #[ts(type = "number")]
    pub number: u64,
    /// The comment body (markdown) — your review feedback, question, or update.
    pub body: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/code/GithubPrCommentResult.ts")]
pub struct GithubPrCommentResult {
    /// The URL of the posted comment.
    pub url: String,
}

crate::action_command! {
    /// Comment on a GitHub pull request (`gh pr comment`) — respond to review, ask a
    /// question, post an update. This is how you participate in a PR as a teammate.
    /// Returns the comment URL.
    pub struct CodeGithubPrComment { state: Arc<CodeState> }
    name: "code/github/pr-comment",
    access: AiSafe,
    native: false, // reachable BY NAME; never pushed into every turn (placeholder-issue spam, 2026-09-03)
    params: GithubPrCommentParams,
    output: GithubPrCommentResult,
    run(this, ctx, p) => {
        super::require_operator(ctx, "code/github/pr-comment")?;
        if p.body.trim().is_empty() {
            return Err(CommandError::Invalid("code/github/pr-comment: 'body' is required".into()));
        }
        let root = workspace_root_for(&this.state, ctx)?;
        let args = vec![
            "pr".to_string(), "comment".to_string(), p.number.to_string(),
            "--body".to_string(), p.body,
        ];
        let url = run_gh(root, args).await?;
        Ok(GithubPrCommentResult { url })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{ActionCommand, Ctx};

    // what this catches: an empty comment body is rejected typed BEFORE the `gh` shell-out —
    // never post a blank comment.
    #[tokio::test]
    async fn blank_body_is_rejected() {
        let state = Arc::new(CodeState::new(
            Arc::new(dashmap::DashMap::new()),
            Arc::new(dashmap::DashMap::new()),
            tokio::runtime::Handle::current(),
        ));
        let cmd = CodeGithubPrComment { state };
        let err = cmd
            .run(&Ctx::default(), GithubPrCommentParams { number: 1, body: "   ".into() })
            .await
            .unwrap_err();
        assert!(matches!(err, CommandError::Invalid(_)));
    }
}
