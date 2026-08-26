//! `code/github/pr-create` — open a GitHub pull request from the caller's pushed branch.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{run_gh, workspace_root_for};
use crate::modules::code::CodeState;
use crate::sdk_codegen::CommandError;

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/code/GithubPrCreateParams.ts")]
pub struct GithubPrCreateParams {
    /// The PR title.
    pub title: String,
    /// The PR body / description (markdown). Say WHAT changed and WHY, like a teammate would.
    pub body: String,
    /// Base branch to merge INTO. Omit for the repo's default branch (usually `main`).
    #[serde(default)]
    #[ts(optional)]
    pub base: Option<String>,
    /// Head branch carrying your changes. Omit for the current branch — push it first with
    /// `code/git/push`.
    #[serde(default)]
    #[ts(optional)]
    pub head: Option<String>,
    /// Open as a DRAFT PR (work-in-progress, not yet ready for review).
    #[serde(default)]
    #[ts(optional)]
    pub draft: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/code/GithubPrCreateResult.ts")]
pub struct GithubPrCreateResult {
    /// The URL of the opened pull request.
    pub url: String,
}

crate::action_command! {
    /// Open a GitHub pull request (`gh pr create`) from your pushed branch. Commit and
    /// `code/git/push` your branch FIRST, then open the PR. Write a real body — what
    /// changed and why — like a teammate. Returns the PR URL.
    pub struct CodeGithubPrCreate { state: Arc<CodeState> }
    name: "code/github/pr-create",
    access: AiSafe,
    native: true,
    params: GithubPrCreateParams,
    output: GithubPrCreateResult,
    run(this, ctx, p) => {
        if p.title.trim().is_empty() {
            return Err(CommandError::Invalid("code/github/pr-create: 'title' is required".into()));
        }
        let root = workspace_root_for(&this.state, ctx)?;
        let mut args = vec![
            "pr".to_string(), "create".to_string(),
            "--title".to_string(), p.title,
            "--body".to_string(), p.body,
        ];
        if let Some(base) = p.base.filter(|s| !s.trim().is_empty()) {
            args.push("--base".to_string());
            args.push(base);
        }
        if let Some(head) = p.head.filter(|s| !s.trim().is_empty()) {
            args.push("--head".to_string());
            args.push(head);
        }
        if p.draft.unwrap_or(false) { // absent draft flag means a normal PR; boolean option, not a measurement
            args.push("--draft".to_string());
        }
        let url = run_gh(root, args).await?;
        Ok(GithubPrCreateResult { url })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{ActionCommand, Ctx};

    // what this catches: a blank title is rejected with a typed Invalid BEFORE any `gh`
    // shell-out — never open a titleless PR, never silently default it (the same fail-loud
    // contract as code/git/commit).
    #[tokio::test]
    async fn blank_title_is_rejected() {
        let state = Arc::new(CodeState::new(
            Arc::new(dashmap::DashMap::new()),
            Arc::new(dashmap::DashMap::new()),
            tokio::runtime::Handle::current(),
        ));
        let cmd = CodeGithubPrCreate { state };
        let err = cmd
            .run(&Ctx::default(), GithubPrCreateParams { title: "  ".into(), body: "x".into(), ..Default::default() })
            .await
            .unwrap_err();
        assert!(matches!(err, CommandError::Invalid(_)));
    }
}
