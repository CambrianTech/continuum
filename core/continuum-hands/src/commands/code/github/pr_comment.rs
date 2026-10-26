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
    native: true, // offered like Claude's (Joel, 2026-09-28); the 9/03 spam cause, must-use pressure, was fixed in #3684
    params: GithubPrCommentParams,
    output: GithubPrCommentResult,
    run(this, ctx, p) => {
        if p.body.trim().is_empty() {
            return Err(CommandError::Invalid("code/github/pr-comment: 'body' is required".into()));
        }
        let root = workspace_root_for(&this.state, ctx).await?;
        let number = p.number.to_string();
        let state = run_gh(root.clone(), vec!["pr".into(), "view".into(), number.clone(), "--json".into(), "state".into(), "--jq".into(), ".state".into()]).await?;
        if let Some(refusal) = refusal_for_state(p.number, &state) {
            return Err(CommandError::Invalid(refusal));
        }
        let args = vec![
            "pr".to_string(), "comment".to_string(), number,
            "--body".to_string(), super::attributed(p.body, ctx),
        ];
        let url = run_gh(root, args).await?;
        Ok(GithubPrCommentResult { url })
    }
}

/// This tool comments only on open PRs: a closed or merged one is answered with its state and
/// nothing is posted, a rule of the tool like the blank-body refusal (card 87f1b05f:
/// four of the seven junk comments residents posted since 09-28 went to PR #1, merged
/// 2025-04-09, each a public comment under the operator's account).
fn refusal_for_state(number: u64, state: &str) -> Option<String> {
    match state.trim() {
        "OPEN" => None,
        "" => Some(format!("code/github/pr-comment: PR #{number}'s state could not be read, so nothing was posted.")),
        other => Some(format!(
            "code/github/pr-comment: PR #{number} is {}, and this tool comments only on open PRs, \
             so nothing was posted.",
            other.to_ascii_lowercase()
        )),
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
            Arc::new(crate::test_acting::TestActing),
        ));
        let cmd = CodeGithubPrComment { state };
        let err = cmd
            .run(&Ctx::default(), GithubPrCommentParams { number: 1, body: "   ".into() })
            .await
            .unwrap_err();
        assert!(matches!(err, CommandError::Invalid(_)));
    }

    // what this catches (card 87f1b05f): a public comment posted on a PR nobody works on. Only
    // an OPEN PR takes one; merged, closed, or anything `gh` reports that is not OPEN is
    // answered with its state and nothing is posted.
    #[test]
    fn only_an_open_pr_takes_a_comment() {
        assert_eq!(refusal_for_state(5270, "OPEN\n"), None);
        let merged = refusal_for_state(1, "MERGED").expect("a merged PR is refused");
        assert!(merged.contains("PR #1 is merged"), "{merged}");
        assert!(refusal_for_state(7, "CLOSED").is_some());
        assert!(refusal_for_state(7, "").is_some(), "an unread state is not OPEN");
    }
}
