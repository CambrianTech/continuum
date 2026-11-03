//! `code/git/push` — push the caller's workspace branch to its remote.

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
    export_to = "../../../protocol/typescript/code/GitPushParams.ts"
)]
pub struct GitPushParams {
    /// Remote name (e.g. `origin`). Omit for git's default.
    #[serde(default)]
    pub remote: Option<String>,
    /// Branch to push. Omit for git's default (the current branch).
    #[serde(default)]
    pub branch: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/code/GitPushResult.ts"
)]
pub struct GitPushResult {
    /// Raw `git push` output.
    pub output: String,
    /// The pull request for the pushed branch. A push is not a PR: GitHub's push reply for a
    /// new branch says "Create a pull request ... by visiting <url>", and a resident read that
    /// as done, moved her card to review and said "PR is up" with none open (Aris, the M5,
    /// 2026-11-02). This field says which it is.
    pub pull_request: BranchPullRequest,
}

/// Whether the pushed branch has an open pull request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "state", rename_all = "snake_case")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/code/BranchPullRequest.ts"
)]
pub enum BranchPullRequest {
    /// An open PR carries this branch.
    Open {
        #[ts(type = "number")]
        number: u64,
        url: String,
    },
    /// No PR carries this branch yet: the push alone is not reviewable.
    NoneYet { open_with: String },
    /// GitHub could not be asked (no `gh`, not authenticated, offline); the push itself landed.
    Unknown { why: String },
}

/// The verb that opens a PR from a pushed branch, named where a push has none.
const OPEN_A_PR: &str = "code/github/pr-create";

/// The open PR whose head is `branch`, asked of GitHub once after the push.
async fn pull_request_for(root: &std::path::Path, branch: &str) -> BranchPullRequest {
    let args = [
        "pr",
        "list",
        "--state",
        "open",
        "--head",
        branch,
        "--json",
        "number,url",
        "--jq",
        ".[0] | select(.) | \"\\(.number) \\(.url)\"",
    ]
    .map(str::to_string)
    .to_vec();
    match crate::commands::code::github::run_gh(root.to_path_buf(), args).await {
        Ok(line) => listed(&line),
        Err(err) => BranchPullRequest::Unknown {
            why: err.to_string(),
        },
    }
}

/// `gh pr list`'s answer: `<number> <url>` for an open PR, nothing for none.
fn listed(line: &str) -> BranchPullRequest {
    match line
        .trim()
        .split_once(' ')
        .and_then(|(n, url)| n.parse().ok().map(|number| (number, url)))
    {
        Some((number, url)) => BranchPullRequest::Open {
            number,
            url: url.to_string(),
        },
        None => BranchPullRequest::NoneYet {
            open_with: OPEN_A_PR.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a pushed branch reported as having a PR when GitHub listed none (the
    // resident who said "PR is up" after a push alone), or an open PR read as none.
    #[test]
    fn a_push_names_its_pull_request_or_the_verb_that_opens_one() {
        assert_eq!(
            listed("5718 https://github.com/CambrianTech/continuum/pull/5718\n"),
            BranchPullRequest::Open {
                number: 5718,
                url: "https://github.com/CambrianTech/continuum/pull/5718".into()
            }
        );
        assert_eq!(
            listed(""),
            BranchPullRequest::NoneYet {
                open_with: OPEN_A_PR.into()
            }
        );
    }
}

crate::action_command! {
    /// Push commits to a remote (`git push`). Omit `remote`/`branch` to use git's
    /// defaults. Outward-facing — publishes your commits to the shared remote, so
    /// it's the one git verb that escapes the workspace sandbox. Declared
    /// `Privileged` (the same tier as `code/shell`): a local persona (Trusted) or
    /// the owner runs it; a remote `Provisional` room peer is denied by the grid
    /// ACL — they can't make your citizen push to a shared remote on their say-so.
    pub struct CodeGitPush { state: Arc<CodeState> }
    name: "code/git/push",
    access: Privileged,
    native: true, // a capable citizen's working set (Joel, 2026-09-28: offered, like Claude's)
    aliases: &["git_push"],
    params: GitPushParams,
    output: GitPushResult,
    run(this, ctx, p) => {
        let root = workspace_root_for(&this.state, ctx).await?;
        let remote = p.remote.unwrap_or_default();
        let branch = p.branch.unwrap_or_default();
        let output = git_bridge::git_push(&root, &remote, &branch).await.map_err(CommandError::Internal)?;
        let pushed = match branch.is_empty() {
            true => git_bridge::run_git(&root, &["rev-parse", "--abbrev-ref", "HEAD"]).await.map(|b| b.trim().to_string()),
            false => Ok(branch),
        };
        let pull_request = match pushed {
            Ok(pushed) => pull_request_for(&root, &pushed).await,
            Err(why) => BranchPullRequest::Unknown { why },
        };
        Ok(GitPushResult { output, pull_request })
    }
}
