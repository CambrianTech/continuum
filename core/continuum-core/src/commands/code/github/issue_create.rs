//! `code/github/issue-create` — open a GitHub issue (file work, a bug, a proposal).

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{run_gh, workspace_root_for};
use crate::modules::code::CodeState;
use crate::sdk_codegen::CommandError;

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/code/GithubIssueCreateParams.ts")]
pub struct GithubIssueCreateParams {
    /// The issue title.
    pub title: String,
    /// The issue body (markdown) — describe the bug/task/proposal like a teammate would.
    pub body: String,
    /// Optional comma-separated labels (e.g. "bug,help wanted"). Omit for none.
    #[serde(default)]
    #[ts(optional)]
    pub labels: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/code/GithubIssueCreateResult.ts")]
pub struct GithubIssueCreateResult {
    /// The URL of the opened issue.
    pub url: String,
}

crate::action_command! {
    /// Open a GitHub issue (`gh issue create`) — file a bug, a task, or a proposal for the
    /// team. Write a real body describing it. Returns the issue URL.
    pub struct CodeGithubIssueCreate { state: Arc<CodeState> }
    name: "code/github/issue-create",
    access: AiSafe,
    native: true,
    params: GithubIssueCreateParams,
    output: GithubIssueCreateResult,
    run(this, ctx, p) => {
        if p.title.trim().is_empty() {
            return Err(CommandError::Invalid("code/github/issue-create: 'title' is required".into()));
        }
        let root = workspace_root_for(&this.state, ctx)?;
        let mut args = vec![
            "issue".to_string(), "create".to_string(),
            "--title".to_string(), p.title,
            "--body".to_string(), p.body,
        ];
        if let Some(labels) = p.labels.filter(|s| !s.trim().is_empty()) {
            args.push("--label".to_string());
            args.push(labels);
        }
        let url = run_gh(root, args).await?;
        Ok(GithubIssueCreateResult { url })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{ActionCommand, Ctx};

    // what this catches: a blank issue title is rejected typed BEFORE the `gh` shell-out.
    #[tokio::test]
    async fn blank_title_is_rejected() {
        let state = Arc::new(CodeState::new(
            Arc::new(dashmap::DashMap::new()),
            Arc::new(dashmap::DashMap::new()),
            tokio::runtime::Handle::current(),
        ));
        let cmd = CodeGithubIssueCreate { state };
        let err = cmd
            .run(&Ctx::default(), GithubIssueCreateParams { title: "".into(), body: "x".into(), ..Default::default() })
            .await
            .unwrap_err();
        assert!(matches!(err, CommandError::Invalid(_)));
    }
}
